//! Extract purchase-invoice data as a *proposal*: exactly from UBL e-invoices, heuristically from
//! plain text (OCR output). Nothing here decides anything; the user checks and confirms every field.

use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

use crate::text::amount_cents;
use crate::{ImportError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct InvoiceProposal {
    /// "ubl" (exact) or "text" (read from OCR text, may be wrong).
    pub source: String,
    pub supplier_name: Option<String>,
    /// Normalised: upper case, no spaces or dots.
    pub vat_number: Option<String>,
    pub kvk_number: Option<String>,
    pub iban: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub net_cents: Option<i64>,
    pub vat_cents: Option<i64>,
    pub total_cents: Option<i64>,
    /// VAT rate in basis points (2100 = 21%) when a single rate was found.
    pub vat_rate_bp: Option<i64>,
}

impl InvoiceProposal {
    /// Fill missing amounts from the other two and drop a set that does not add up.
    fn complete_amounts(&mut self) {
        match (self.net_cents, self.vat_cents, self.total_cents) {
            (Some(n), Some(v), None) => self.total_cents = Some(n + v),
            (None, Some(v), Some(t)) if v <= t => self.net_cents = Some(t - v),
            (Some(n), None, Some(t)) if n <= t => self.vat_cents = Some(t - n),
            (None, None, Some(t)) => {
                if let Some(bp) = self.vat_rate_bp {
                    // net = total / (1 + rate), rounded half away from zero (integers only).
                    let net = div_round(t * 10_000, 10_000 + bp);
                    self.net_cents = Some(net);
                    self.vat_cents = Some(t - net);
                }
            }
            (Some(n), Some(v), Some(t)) if n + v != t => {
                // Inconsistent: trust total and VAT (usually printed most prominently).
                if v <= t {
                    self.net_cents = Some(t - v);
                } else {
                    self.net_cents = None;
                    self.vat_cents = None;
                }
            }
            _ => {}
        }
    }
}

fn div_round(n: i64, d: i64) -> i64 {
    let q = n / d;
    let r = n % d;
    if 2 * r.abs() >= d {
        q + n.signum()
    } else {
        q
    }
}

// ---------------------------------------------------------------------------
// UBL (Peppol BIS / NLCIUS) e-invoices
// ---------------------------------------------------------------------------

/// Read the fields of a UBL `Invoice` (or `CreditNote`) document.
pub fn from_ubl(bytes: &[u8]) -> Result<InvoiceProposal> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut path: Vec<String> = Vec::new();
    let mut p = InvoiceProposal {
        source: "ubl".into(),
        ..Default::default()
    };
    let mut buf = Vec::new();
    let mut root_seen = false;
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if path.is_empty() {
                    if name != "Invoice" && name != "CreditNote" {
                        return Err(ImportError::UnknownFormat("geen UBL-factuur".into()));
                    }
                    root_seen = true;
                }
                path.push(name);
                text.clear();
            }
            Ok(Event::Text(t)) => {
                text.push_str(&t.decode().map_err(|e| ImportError::Other(e.to_string()))?);
            }
            Ok(Event::GeneralRef(r)) => {
                let name = r.decode().map_err(|e| ImportError::Other(e.to_string()))?;
                text.push_str(match name.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                });
            }
            Ok(Event::End(_)) => {
                let value = text.trim().to_string();
                text.clear();
                if !value.is_empty() {
                    ubl_field(&mut p, &path, value);
                }
                path.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ImportError::Other(format!("XML: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    if !root_seen {
        return Err(ImportError::UnknownFormat("geen UBL-factuur".into()));
    }
    if p.vat_rate_bp == Some(-1) {
        p.vat_rate_bp = None;
    }
    p.complete_amounts();
    Ok(p)
}

fn ubl_field(p: &mut InvoiceProposal, path: &[String], value: String) {
    let joined = path.join("/");
    let tail = |suffix: &str| joined.ends_with(suffix);
    let depth = path.len();
    if depth == 2 && tail("/ID") {
        p.invoice_number.get_or_insert(value);
    } else if depth == 2 && tail("/IssueDate") {
        p.invoice_date = p.invoice_date.or(iso_date(&value));
    } else if (depth == 2 && tail("/DueDate")) || tail("PaymentMeans/PaymentDueDate") {
        p.due_date = p.due_date.or(iso_date(&value));
    } else if joined.contains("AccountingSupplierParty") {
        if tail("PartyName/Name") || tail("PartyLegalEntity/RegistrationName") {
            p.supplier_name.get_or_insert(value);
        } else if tail("PartyTaxScheme/CompanyID") {
            p.vat_number.get_or_insert(normalise_id(&value));
        } else if tail("PartyLegalEntity/CompanyID") {
            p.kvk_number.get_or_insert(value);
        }
    } else if tail("PayeeFinancialAccount/ID") {
        p.iban.get_or_insert(normalise_id(&value));
    } else if tail("LegalMonetaryTotal/TaxExclusiveAmount") {
        p.net_cents = p.net_cents.or(amount_cents(&value));
    } else if tail("LegalMonetaryTotal/PayableAmount") {
        p.total_cents = amount_cents(&value).or(p.total_cents);
    } else if tail("LegalMonetaryTotal/TaxInclusiveAmount") {
        p.total_cents = p.total_cents.or(amount_cents(&value));
    } else if depth == 3 && tail("TaxTotal/TaxAmount") {
        p.vat_cents = p.vat_cents.or(amount_cents(&value));
    } else if tail("TaxSubtotal/TaxCategory/Percent") {
        let bp = percent_bp(&value);
        p.vat_rate_bp = match (p.vat_rate_bp, bp) {
            (None, b) => b,
            (Some(a), Some(b)) if a == b => Some(a),
            // Several rates: no single rate to propose.
            _ => Some(-1),
        };
    }
}

fn iso_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn percent_bp(s: &str) -> Option<i64> {
    let t = s.trim().trim_end_matches('%').trim().replace(',', ".");
    let (i, f) = t.split_once('.').unwrap_or((&t, ""));
    let f = format!("{:0<2}", f.get(..2.min(f.len()))?);
    Some(i.parse::<i64>().ok()? * 100 + f.parse::<i64>().ok()?)
}

fn normalise_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

// ---------------------------------------------------------------------------
// Plain text (OCR)
// ---------------------------------------------------------------------------

static VAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([A-Z]{2})[ .]?([0-9][0-9A-Z .]{6,13}[0-9A-Z])\b").unwrap());
static NL_VAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bNL[\s.]?(\d{4}[\s.]?\d{2}[\s.]?\d{3})[\s.]?B[\s.]?(\d{2})\b").unwrap());
static IBAN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([A-Z]{2}\d{2}(?: ?[A-Z0-9]{4}){2,7}(?: ?[A-Z0-9]{1,3})?)\b").unwrap());
static KVK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:kvk|k\.v\.k\.|kamer van koophandel|coc|chamber of commerce)[^0-9]{0,20}(\d{8})\b").unwrap()
});
static AMOUNT_RE: LazyLock<Regex> = LazyLock::new(|| {
    // 1.234,56 | 1,234.56 | 1234,56 | 12.10 (two decimals required, so dates and numbers are skipped)
    Regex::new(r"(-?\d{1,3}(?:[.,\s]\d{3})*[.,]\d{2}|-?\d+[.,]\d{2})(?:\D|$)").unwrap()
});
static PERCENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2}(?:[.,]\d{1,2})?)\s?%").unwrap());
static NUMERIC_DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2})[-/.](\d{1,2})[-/.](\d{4}|\d{2})\b|\b(\d{4})-(\d{2})-(\d{2})\b").unwrap());
static WORD_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d{1,2})\.?\s+([a-z]{3,10})\.?\s+(\d{4})\b|\b([a-z]{3,10})\.?\s+(\d{1,2}),?\s+(\d{4})\b")
        .unwrap()
});

const MONTHS: [(&str, u32); 24] = [
    ("jan", 1),
    ("feb", 2),
    ("mrt", 3),
    ("maa", 3),
    ("mar", 3),
    ("apr", 4),
    ("mei", 5),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("okt", 10),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
    ("januari", 1),
    ("februari", 2),
    ("maart", 3),
    ("juni", 6),
    ("juli", 7),
    ("augustus", 8),
    ("oktober", 10),
    ("march", 3),
];

fn month_from(word: &str) -> Option<u32> {
    let w = word.to_lowercase();
    MONTHS
        .iter()
        .find(|(m, _)| w == *m)
        .or_else(|| MONTHS.iter().find(|(m, _)| m.len() == 3 && w.starts_with(m)))
        .map(|(_, n)| *n)
}

/// All dates on a line, in order.
fn dates_in(line: &str) -> Vec<NaiveDate> {
    let mut found: Vec<(usize, NaiveDate)> = Vec::new();
    for c in NUMERIC_DATE_RE.captures_iter(line) {
        let pos = c.get(0).map_or(0, |m| m.start());
        let d = if let Some(y) = c.get(4) {
            NaiveDate::from_ymd_opt(
                y.as_str().parse().ok().unwrap_or(0),
                c[5].parse().unwrap_or(0),
                c[6].parse().unwrap_or(0),
            )
        } else {
            let mut y: i32 = c[3].parse().unwrap_or(0);
            if y < 100 {
                y += 2000;
            }
            NaiveDate::from_ymd_opt(y, c[2].parse().unwrap_or(0), c[1].parse().unwrap_or(0))
        };
        if let Some(d) = d {
            found.push((pos, d));
        }
    }
    for c in WORD_DATE_RE.captures_iter(line) {
        let pos = c.get(0).map_or(0, |m| m.start());
        let d = if let Some(day) = c.get(1) {
            month_from(&c[2]).and_then(|m| NaiveDate::from_ymd_opt(c[3].parse().ok()?, m, day.as_str().parse().ok()?))
        } else {
            month_from(&c[4]).and_then(|m| NaiveDate::from_ymd_opt(c[6].parse().ok()?, m, c[5].parse().ok()?))
        };
        if let Some(d) = d {
            found.push((pos, d));
        }
    }
    found.sort_by_key(|(p, _)| *p);
    found.into_iter().map(|(_, d)| d).collect()
}

fn amounts_in(line: &str) -> Vec<i64> {
    AMOUNT_RE
        .captures_iter(line)
        .filter_map(|c| amount_cents(&c[1].replace(' ', "")))
        .collect()
}

/// IBAN check digits (ISO 13616, mod 97).
fn iban_valid(iban: &str) -> bool {
    if iban.len() < 15 || iban.len() > 34 {
        return false;
    }
    let rearranged = format!("{}{}", &iban[4..], &iban[..4]);
    let mut rem: u32 = 0;
    for c in rearranged.chars() {
        let v = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'A'..='Z' => c as u32 - 'A' as u32 + 10,
            _ => return false,
        };
        for digit in v.to_string().chars() {
            rem = (rem * 10 + (digit as u32 - '0' as u32)) % 97;
        }
    }
    rem == 1
}

fn lower_has(line: &str, words: &[&str]) -> bool {
    let l = line.to_lowercase();
    words.iter().any(|w| l.contains(w))
}

/// The value after a label on the same line ("Factuurnummer: 2026-041"), or the next line's first token.
fn value_after_label(lines: &[&str], labels: &[&str]) -> Option<String> {
    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        for label in labels {
            if let Some(pos) = lower.find(label) {
                let rest = &line[pos + label.len()..];
                let rest = rest.trim_start_matches(|c: char| c == ':' || c == '.' || c == '#' || c.is_whitespace());
                let token = rest.split_whitespace().next().map(|t| t.trim_end_matches([',', ';']));
                let token = match token {
                    Some(t) if t.chars().any(|c| c.is_ascii_digit()) => Some(t.to_string()),
                    _ => lines
                        .get(i + 1)
                        .and_then(|n| n.split_whitespace().next())
                        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
                        .map(str::to_string),
                };
                if token.is_some() {
                    return token;
                }
            }
        }
    }
    None
}

/// Read a proposal from invoice text. `own_ids` are the owner's own VAT number, KvK number and IBANs
/// (normalised), which also appear on received invoices and must not be taken for the supplier's.
pub fn from_text(text: &str, own_ids: &[String]) -> InvoiceProposal {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let own = |id: &str| own_ids.iter().any(|o| o == id);
    let mut p = InvoiceProposal {
        source: "text".into(),
        ..Default::default()
    };

    // Identifiers. Dutch VAT numbers first (most reliable pattern), then other EU formats.
    for c in NL_VAT_RE.captures_iter(text) {
        let id = format!("NL{}B{}", normalise_id(&c[1]), &c[2]);
        if !own(&id) {
            p.vat_number = Some(id);
            break;
        }
    }
    if p.vat_number.is_none() {
        for line in lines
            .iter()
            .filter(|l| lower_has(l, &["btw", "vat", "tva", "ust", "iva"]))
        {
            for c in VAT_RE.captures_iter(line) {
                let id = format!("{}{}", c[1].to_uppercase(), normalise_id(&c[2]));
                if id.chars().skip(2).filter(|ch| ch.is_ascii_digit()).count() >= 7 && !own(&id) && !iban_valid(&id) {
                    p.vat_number = Some(id);
                    break;
                }
            }
            if p.vat_number.is_some() {
                break;
            }
        }
    }
    for c in IBAN_RE.captures_iter(text) {
        let id = normalise_id(&c[1]);
        if iban_valid(&id) && !own(&id) {
            p.iban = Some(id);
            break;
        }
    }
    for c in KVK_RE.captures_iter(text) {
        if !own(&c[1]) {
            p.kvk_number = Some(c[1].to_string());
            break;
        }
    }

    p.invoice_number = value_after_label(
        &lines,
        &[
            "factuurnummer",
            "factuurnr",
            "factuur nr",
            "factuur nummer",
            "notanummer",
            "invoice number",
            "invoice no",
            "invoice nr",
            "invoice #",
            "rechnungsnummer",
        ],
    );

    // Dates: by label first, the first date in the text as fallback for the invoice date.
    for (i, line) in lines.iter().enumerate() {
        let on_line_or_next = || {
            let d = dates_in(line);
            if d.is_empty() {
                lines.get(i + 1).map(|n| dates_in(n)).unwrap_or_default()
            } else {
                d
            }
        };
        if p.due_date.is_none()
            && lower_has(
                line,
                &[
                    "vervaldatum",
                    "uiterlijk",
                    "betalen voor",
                    "due date",
                    "payment due",
                    "fällig",
                ],
            )
        {
            p.due_date = on_line_or_next().first().copied();
        } else if p.invoice_date.is_none()
            && lower_has(
                line,
                &[
                    "factuurdatum",
                    "datum factuur",
                    "invoice date",
                    "date of issue",
                    "rechnungsdatum",
                ],
            )
        {
            p.invoice_date = on_line_or_next().first().copied();
        }
    }
    if p.invoice_date.is_none() {
        p.invoice_date = lines
            .iter()
            .filter(|l| !lower_has(l, &["vervaldatum", "uiterlijk", "due", "periode", "period"]))
            .find_map(|l| dates_in(l).first().copied());
    }

    // Amounts: the last amount on a labelled line. Labels from most to least specific.
    let last_amount = |labels: &[&str], exclude: &[&str]| -> Option<i64> {
        for label in labels {
            for line in lines.iter().rev() {
                if lower_has(line, &[label]) && !lower_has(line, exclude) {
                    if let Some(a) = amounts_in(line).last() {
                        return Some(*a);
                    }
                }
            }
        }
        None
    };
    p.total_cents = last_amount(
        &[
            "totaal te betalen",
            "te betalen",
            "totaal incl",
            "total due",
            "amount due",
            "total incl",
            "totaalbedrag",
            "total amount",
            "totaal",
            "total",
        ],
        &["excl", "subtota", "btw-nummer", "btw nummer"],
    );
    p.net_cents = last_amount(
        &[
            "totaal excl",
            "subtotaal",
            "subtotal",
            "excl. btw",
            "excl btw",
            "total excl",
            "net amount",
            "netto",
        ],
        &[],
    );
    p.vat_cents = last_amount(
        &["btw", "vat", "tax", "mwst", "tva"],
        &["excl", "incl", "nummer", "number", "nr", "id", "total", "totaal"],
    )
    .or_else(|| {
        last_amount(
            &["btw", "vat"],
            &["excl", "nummer", "number", "nr", "te betalen", "due"],
        )
    });
    let rates: Vec<i64> = lines
        .iter()
        .filter(|l| lower_has(l, &["btw", "vat", "tax", "mwst", "tva"]))
        .flat_map(|l| {
            PERCENT_RE
                .captures_iter(l)
                .filter_map(|c| percent_bp(&c[1]))
                .collect::<Vec<_>>()
        })
        .collect();
    if let Some(first) = rates.first() {
        if rates.iter().all(|r| r == first) {
            p.vat_rate_bp = Some(*first);
        }
    }
    p.complete_amounts();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    const OWN: &str = "NL000099998B57";

    #[test]
    fn reads_a_dutch_invoice_text() {
        // Fictional OCR output (line breaks and spacing as OCR tends to produce them).
        let text = "Voorbeeld Hosting B.V.\nHostingstraat 5, 1234 AB Utrecht\nKvK: 87654321\n\
                    Btw-nr: NL 8765.43.210 B01\nIBAN: NL91 ABNA 0417 1643 00\n\
                    Aan: Voorbeeld Apps, btw NL000099998B57\n\
                    Factuurnummer: VH-2026-0417\nFactuurdatum: 3 november 2026\nVervaldatum 17-11-2026\n\
                    Omschrijving   Bedrag\nVPS november  € 40,00\nBackup  € 10,00\n\
                    Subtotaal € 50,00\nBtw 21% € 10,50\nTotaal te betalen € 60,50\n";
        let p = from_text(text, &[OWN.to_string()]);
        assert_eq!(p.vat_number.as_deref(), Some("NL876543210B01"));
        assert_eq!(p.kvk_number.as_deref(), Some("87654321"));
        assert_eq!(p.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(p.invoice_number.as_deref(), Some("VH-2026-0417"));
        assert_eq!(p.invoice_date, Some(d(2026, 11, 3)));
        assert_eq!(p.due_date, Some(d(2026, 11, 17)));
        assert_eq!(
            (p.net_cents, p.vat_cents, p.total_cents),
            (Some(5_000), Some(1_050), Some(6_050))
        );
        assert_eq!(p.vat_rate_bp, Some(2100));
    }

    #[test]
    fn reads_an_english_invoice_and_skips_own_ids() {
        let text = "Example Software Ltd\nVAT number IE1234567T\nInvoice # 10045\n\
                    Date of issue: November 2, 2026\nBill to: Voorbeeld Apps NL000099998B57\n\
                    Subtotal 1,000.00\nVAT (0%) 0.00\nAmount due EUR 1,000.00\n";
        let p = from_text(text, &[OWN.to_string()]);
        assert_eq!(p.vat_number.as_deref(), Some("IE1234567T"));
        assert_eq!(p.invoice_number.as_deref(), Some("10045"));
        assert_eq!(p.invoice_date, Some(d(2026, 11, 2)));
        assert_eq!(
            (p.net_cents, p.vat_cents, p.total_cents),
            (Some(100_000), Some(0), Some(100_000))
        );
        assert_eq!(p.vat_rate_bp, Some(0));
    }

    #[test]
    fn total_with_rate_only_gives_net_and_vat() {
        let p = from_text("Bon 12-10-2026\nTotaal 12,10\nIncl. 21% btw\n", &[]);
        assert_eq!(p.invoice_date, Some(d(2026, 10, 12)));
        assert_eq!(
            (p.net_cents, p.vat_cents, p.total_cents),
            (Some(1_000), Some(210), Some(1_210))
        );
    }

    #[test]
    fn nothing_found_is_empty_not_wrong() {
        let p = from_text("Bedankt voor uw bezoek", &[]);
        assert_eq!(
            p,
            InvoiceProposal {
                source: "text".into(),
                ..Default::default()
            }
        );
    }

    #[test]
    fn iban_checksum() {
        assert!(iban_valid("NL91ABNA0417164300"));
        assert!(!iban_valid("NL92ABNA0417164300"));
    }

    #[test]
    fn reads_a_ubl_invoice() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"
  xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2"
  xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>F-2026-77</cbc:ID>
  <cbc:IssueDate>2026-11-05</cbc:IssueDate>
  <cbc:DueDate>2026-12-05</cbc:DueDate>
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyName><cbc:Name>Voorbeeld Kantoor B.V.</cbc:Name></cac:PartyName>
    <cac:PartyTaxScheme><cbc:CompanyID>NL876543210B01</cbc:CompanyID></cac:PartyTaxScheme>
    <cac:PartyLegalEntity><cbc:RegistrationName>Voorbeeld Kantoor B.V.</cbc:RegistrationName>
      <cbc:CompanyID>87654321</cbc:CompanyID></cac:PartyLegalEntity>
  </cac:Party></cac:AccountingSupplierParty>
  <cac:AccountingCustomerParty><cac:Party>
    <cac:PartyTaxScheme><cbc:CompanyID>NL000099998B57</cbc:CompanyID></cac:PartyTaxScheme>
  </cac:Party></cac:AccountingCustomerParty>
  <cac:PaymentMeans><cac:PayeeFinancialAccount><cbc:ID>NL91 ABNA 0417 1643 00</cbc:ID></cac:PayeeFinancialAccount></cac:PaymentMeans>
  <cac:TaxTotal>
    <cbc:TaxAmount currencyID="EUR">21.00</cbc:TaxAmount>
    <cac:TaxSubtotal><cbc:TaxableAmount currencyID="EUR">100.00</cbc:TaxableAmount>
      <cbc:TaxAmount currencyID="EUR">21.00</cbc:TaxAmount>
      <cac:TaxCategory><cbc:ID>S</cbc:ID><cbc:Percent>21</cbc:Percent></cac:TaxCategory></cac:TaxSubtotal>
  </cac:TaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:TaxExclusiveAmount currencyID="EUR">100.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount currencyID="EUR">121.00</cbc:TaxInclusiveAmount>
    <cbc:PayableAmount currencyID="EUR">121.00</cbc:PayableAmount>
  </cac:LegalMonetaryTotal>
</Invoice>"#;
        let p = from_ubl(xml.as_bytes()).unwrap();
        assert_eq!(p.source, "ubl");
        assert_eq!(p.supplier_name.as_deref(), Some("Voorbeeld Kantoor B.V."));
        assert_eq!(p.vat_number.as_deref(), Some("NL876543210B01"));
        assert_eq!(p.kvk_number.as_deref(), Some("87654321"));
        assert_eq!(p.iban.as_deref(), Some("NL91ABNA0417164300"));
        assert_eq!(p.invoice_number.as_deref(), Some("F-2026-77"));
        assert_eq!(
            (p.invoice_date, p.due_date),
            (Some(d(2026, 11, 5)), Some(d(2026, 12, 5)))
        );
        assert_eq!(
            (p.net_cents, p.vat_cents, p.total_cents),
            (Some(10_000), Some(2_100), Some(12_100))
        );
        assert_eq!(p.vat_rate_bp, Some(2100));
        assert!(from_ubl(b"<Other/>").is_err());
    }
}
