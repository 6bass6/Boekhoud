//! Bank statements: CAMT.053 (ISO 20022 XML, offered by all Dutch banks) and CSV exports
//! (ING, Rabobank, bunq and similar layouts, recognised by their column names).

use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;

use crate::text::{amount_cents, date, decode, find, norm};
use crate::{ImportError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BankTransaction {
    pub booking_date: Option<NaiveDate>,
    pub value_date: Option<NaiveDate>,
    /// Positive = money in, negative = money out.
    pub amount_cents: i64,
    pub currency: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub description: String,
    pub end_to_end_id: Option<String>,
    /// Unique reference assigned by the bank, when the format provides one.
    pub bank_reference: Option<String>,
    /// Own account (CSV formats that list it per row).
    pub account_iban: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Balance {
    pub date: NaiveDate,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BankStatement {
    pub format: String,
    pub account_iban: Option<String>,
    pub currency: Option<String>,
    pub opening_balance: Option<Balance>,
    pub closing_balance: Option<Balance>,
    pub transactions: Vec<BankTransaction>,
}

pub fn parse(bytes: &[u8]) -> Result<BankStatement> {
    let text = decode(bytes);
    let trimmed = text.trim_start();
    if trimmed.starts_with('<') {
        if text.contains("BkToCstmrStmt") {
            return parse_camt053(&text);
        }
        return Err(ImportError::UnknownFormat(
            "XML-bestand is geen CAMT.053-afschrift".into(),
        ));
    }
    parse_csv(&text)
}

// ---------------------------------------------------------------------------
// CAMT.053
// ---------------------------------------------------------------------------

#[derive(Default)]
struct EntryState {
    tx: BankTransaction,
    credit: Option<bool>,
    unstructured: Vec<String>,
    structured_ref: Option<String>,
    additional: Option<String>,
    debtor_name: Option<String>,
    creditor_name: Option<String>,
    debtor_iban: Option<String>,
    creditor_iban: Option<String>,
}

#[derive(Default)]
struct BalanceState {
    code: String,
    amount: Option<i64>,
    credit: bool,
    date: Option<NaiveDate>,
}

fn path_ends(path: &[String], suffix: &[&str]) -> bool {
    path.len() >= suffix.len()
        && path[path.len() - suffix.len()..]
            .iter()
            .zip(suffix)
            .all(|(a, b)| a == b)
}

fn in_path(path: &[String], name: &str) -> bool {
    path.iter().any(|p| p == name)
}

fn parse_camt053(text: &str) -> Result<BankStatement> {
    let mut reader = Reader::from_str(text);
    let mut path: Vec<String> = Vec::new();
    let mut st = BankStatement {
        format: "CAMT.053".into(),
        ..Default::default()
    };
    let mut entry: Option<EntryState> = None;
    let mut bal: Option<BalanceState> = None;
    let mut amount_ccy: Option<String> = None;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Err(e) => return Err(ImportError::Other(format!("XML-fout: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "Amt" {
                    amount_ccy = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.local_name().as_ref() == b"Ccy")
                        .map(|a| String::from_utf8_lossy(&a.value).to_string());
                }
                path.push(name.clone());
                buf.clear();
                if name == "Ntry" {
                    entry = Some(EntryState::default());
                }
                if name == "Bal" && entry.is_none() {
                    bal = Some(BalanceState::default());
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                let v = std::mem::take(&mut buf).trim().to_string();
                if !v.is_empty() {
                    handle_value(&path, v, &mut entry, &mut bal, &mut st, &amount_ccy)?;
                }
                if name == "Ntry" {
                    if let Some(mut s) = entry.take() {
                        let credit = s.credit.unwrap_or(true);
                        if !credit {
                            s.tx.amount_cents = -s.tx.amount_cents;
                        }
                        let (name, iban) = if credit {
                            (s.debtor_name, s.debtor_iban)
                        } else {
                            (s.creditor_name, s.creditor_iban)
                        };
                        s.tx.counterparty_name = name;
                        s.tx.counterparty_iban = iban;
                        let mut desc = s.unstructured.join(" ");
                        if desc.is_empty() {
                            desc = s.structured_ref.clone().or(s.additional.clone()).unwrap_or_default();
                        } else if let Some(r) = &s.structured_ref {
                            desc = format!("{desc} {r}");
                        }
                        s.tx.description = desc.trim().to_string();
                        if s.tx.currency.is_empty() {
                            s.tx.currency = st.currency.clone().unwrap_or_else(|| "EUR".into());
                        }
                        st.transactions.push(s.tx);
                    }
                }
                if name == "Bal" && entry.is_none() {
                    if let Some(b) = bal.take() {
                        if let (Some(a), Some(d)) = (b.amount, b.date) {
                            let balance = Balance {
                                date: d,
                                amount_cents: if b.credit { a } else { -a },
                            };
                            match b.code.as_str() {
                                "OPBD" | "PRCD" => {
                                    if st.opening_balance.is_none() {
                                        st.opening_balance = Some(balance);
                                    }
                                }
                                "CLBD" => st.closing_balance = Some(balance),
                                _ => {}
                            }
                        }
                    }
                }
                path.pop();
            }
            Ok(Event::Text(t)) => {
                buf.push_str(&t.decode().map_err(|e| ImportError::Other(e.to_string()))?);
            }
            Ok(Event::GeneralRef(r)) => {
                let name = r.decode().map_err(|e| ImportError::Other(e.to_string()))?;
                let resolved = match name.as_ref() {
                    "amp" => "&".to_string(),
                    "lt" => "<".to_string(),
                    "gt" => ">".to_string(),
                    "quot" => "\"".to_string(),
                    "apos" => "'".to_string(),
                    other => r
                        .resolve_char_ref()
                        .ok()
                        .flatten()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| format!("&{other};")),
                };
                buf.push_str(&resolved);
            }
            Ok(_) => {}
        }
    }
    if st.transactions.is_empty() && st.closing_balance.is_none() {
        return Err(ImportError::Empty);
    }
    Ok(st)
}

fn handle_value(
    path: &[String],
    v: String,
    entry: &mut Option<EntryState>,
    bal: &mut Option<BalanceState>,
    st: &mut BankStatement,
    amount_ccy: &Option<String>,
) -> Result<()> {
    if let Some(s) = entry.as_mut() {
        if path_ends(path, &["Ntry", "Amt"]) {
            s.tx.amount_cents = amount_cents(&v).ok_or_else(|| ImportError::Other(format!("ongeldig bedrag {v}")))?;
            s.tx.currency = amount_ccy.clone().unwrap_or_default();
        } else if path_ends(path, &["Ntry", "CdtDbtInd"]) {
            s.credit = Some(v == "CRDT");
        } else if path_ends(path, &["BookgDt", "Dt"]) || path_ends(path, &["BookgDt", "DtTm"]) {
            s.tx.booking_date = date(&v[..v.len().min(10)], false);
        } else if path_ends(path, &["ValDt", "Dt"]) || path_ends(path, &["ValDt", "DtTm"]) {
            s.tx.value_date = date(&v[..v.len().min(10)], false);
        } else if path_ends(path, &["Ntry", "AcctSvcrRef"])
            || (path_ends(path, &["Refs", "AcctSvcrRef"]) && s.tx.bank_reference.is_none())
        {
            s.tx.bank_reference = Some(v);
        } else if path_ends(path, &["Refs", "EndToEndId"]) && v != "NOTPROVIDED" {
            s.tx.end_to_end_id = Some(v);
        } else if path_ends(path, &["RmtInf", "Ustrd"]) {
            s.unstructured.push(v);
        } else if path_ends(path, &["CdtrRefInf", "Ref"]) {
            s.structured_ref = Some(v);
        } else if path_ends(path, &["Ntry", "AddtlNtryInf"]) {
            s.additional = Some(v);
        } else if path_ends(path, &["Nm"]) && in_path(path, "RltdPties") {
            if in_path(path, "Dbtr") && !in_path(path, "UltmtDbtr") {
                s.debtor_name = Some(v);
            } else if in_path(path, "Cdtr") && !in_path(path, "UltmtCdtr") {
                s.creditor_name = Some(v);
            }
        } else if path_ends(path, &["Id", "IBAN"]) && in_path(path, "RltdPties") {
            if in_path(path, "DbtrAcct") {
                s.debtor_iban = Some(v);
            } else if in_path(path, "CdtrAcct") {
                s.creditor_iban = Some(v);
            }
        }
    } else if let Some(b) = bal.as_mut() {
        if path_ends(path, &["CdOrPrtry", "Cd"]) {
            b.code = v;
        } else if path_ends(path, &["Bal", "Amt"]) {
            b.amount = amount_cents(&v);
        } else if path_ends(path, &["Bal", "CdtDbtInd"]) {
            b.credit = v == "CRDT";
        } else if path_ends(path, &["Bal", "Dt", "Dt"]) || path_ends(path, &["Bal", "Dt", "DtTm"]) {
            b.date = date(&v[..v.len().min(10)], false);
        }
    } else if path_ends(path, &["Stmt", "Acct", "Id", "IBAN"]) && st.account_iban.is_none() {
        st.account_iban = Some(v);
    } else if path_ends(path, &["Stmt", "Acct", "Ccy"]) {
        st.currency = Some(v);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

fn delimiter(header_line: &str) -> u8 {
    let counts = [
        (b';', header_line.matches(';').count()),
        (b',', header_line.matches(',').count()),
        (b'\t', header_line.matches('\t').count()),
    ];
    counts.iter().max_by_key(|(_, n)| *n).map(|(d, _)| *d).unwrap_or(b',')
}

fn clean_iban(s: &str) -> Option<String> {
    let v: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn parse_csv(text: &str) -> Result<BankStatement> {
    let first = text.lines().find(|l| !l.trim().is_empty()).ok_or(ImportError::Empty)?;
    let delim = delimiter(first);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| ImportError::Other(e.to_string()))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    let normed: Vec<String> = headers.iter().map(|h| norm(h)).collect();

    let c_date = find(
        &headers,
        &["Datum", "Date", "Boekdatum", "Transactiedatum", "Boekingsdatum"],
    )
    .ok_or_else(|| ImportError::MissingColumn("Datum".into()))?;
    let c_value = find(&headers, &["Rentedatum", "Interest Date", "Valutadatum"]);
    let c_amount = find(&headers, &["Bedrag (EUR)", "Bedrag", "Amount", "Transactiebedrag"])
        .ok_or_else(|| ImportError::MissingColumn("Bedrag".into()))?;
    let c_sign = find(&headers, &["Af Bij", "Af/Bij", "Debet/Credit", "Credit/Debet"]);
    let c_name = find(
        &headers,
        &[
            "Naam tegenpartij",
            "Naam / Omschrijving",
            "Naam/Omschrijving",
            "Name",
            "Counterparty name",
            "Tegenpartij",
        ],
    );
    let c_iban = find(
        &headers,
        &[
            "Tegenrekening IBAN/BBAN",
            "Tegenrekening",
            "Counterparty",
            "Tegenrekening IBAN",
            "Counterparty IBAN",
        ],
    );
    let c_own = find(
        &headers,
        &["IBAN/BBAN", "Rekening", "Account", "Rekeningnummer", "IBAN"],
    );
    let c_cur = find(&headers, &["Munt", "Currency", "Valuta"]);
    let c_ref = find(&headers, &["Transactiereferentie", "Volgnr", "Referentie"]);
    let c_e2e = find(&headers, &["Betalingskenmerk", "End-to-end ID"]);
    let desc_cols: Vec<usize> = normed
        .iter()
        .enumerate()
        .filter(|(_, h)| h.starts_with("omschrijving") || *h == "mededelingen" || *h == "description")
        .filter(|(i, _)| Some(*i) != c_name)
        .map(|(i, _)| i)
        .collect();
    let is_ing = c_sign.is_some() && find(&headers, &["Mededelingen"]).is_some();
    let format = if is_ing {
        "ING CSV"
    } else if find(&headers, &["Volgnr"]).is_some() {
        "Rabobank CSV"
    } else {
        "CSV"
    };

    let mut st = BankStatement {
        format: format.into(),
        ..Default::default()
    };
    for (i, rec) in rdr.records().enumerate() {
        let n = i + 2;
        let rec = rec.map_err(|e| ImportError::Row {
            line: n,
            message: e.to_string(),
        })?;
        let get = |c: usize| rec.get(c).unwrap_or("").trim();
        if get(c_date).is_empty() && get(c_amount).is_empty() {
            continue;
        }
        let err = |m: &str| ImportError::Row {
            line: n,
            message: m.into(),
        };
        let booking = date(get(c_date), false).ok_or_else(|| err("ongeldige datum"))?;
        let mut amount = amount_cents(get(c_amount)).ok_or_else(|| err("ongeldig bedrag"))?;
        if let Some(c) = c_sign {
            let s = get(c).to_lowercase();
            if s.starts_with("af") || s.starts_with("debet") || s == "d" {
                amount = -amount.abs();
            } else {
                amount = amount.abs();
            }
        }
        let description = desc_cols
            .iter()
            .map(|c| get(*c))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let tx = BankTransaction {
            booking_date: Some(booking),
            value_date: c_value.and_then(|c| date(get(c), false)),
            amount_cents: amount,
            currency: c_cur
                .map(|c| get(c).to_uppercase())
                .filter(|c| c.len() == 3)
                .unwrap_or_else(|| "EUR".into()),
            counterparty_name: c_name.map(|c| get(c).to_string()).filter(|s| !s.is_empty()),
            counterparty_iban: c_iban.and_then(|c| clean_iban(get(c))),
            description,
            end_to_end_id: c_e2e.map(|c| get(c).to_string()).filter(|s| !s.is_empty()),
            bank_reference: c_ref.map(|c| get(c).to_string()).filter(|s| !s.is_empty()),
            account_iban: c_own.and_then(|c| clean_iban(get(c))),
        };
        if st.account_iban.is_none() {
            st.account_iban = tx.account_iban.clone();
        }
        st.transactions.push(tx);
    }
    if st.transactions.is_empty() {
        return Err(ImportError::Empty);
    }
    st.currency = Some(st.transactions[0].currency.clone());
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAMT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.02">
  <BkToCstmrStmt>
    <Stmt>
      <Id>FICTIEF-1</Id>
      <Acct><Id><IBAN>NL00BANK0123456789</IBAN></Id><Ccy>EUR</Ccy></Acct>
      <Bal><Tp><CdOrPrtry><Cd>OPBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">1000.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-11-01</Dt></Dt></Bal>
      <Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">2089.20</Amt><CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-11-30</Dt></Dt></Bal>
      <Ntry>
        <Amt Ccy="EUR">1210.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts>BOOK</Sts>
        <BookgDt><Dt>2026-11-10</Dt></BookgDt><ValDt><Dt>2026-11-10</Dt></ValDt>
        <AcctSvcrRef>REF-0001</AcctSvcrRef>
        <NtryDtls><TxDtls>
          <Refs><EndToEndId>E2E-77</EndToEndId></Refs>
          <RltdPties><Dbtr><Nm>Stichting Voorbeeld</Nm></Dbtr><DbtrAcct><Id><IBAN>NL11OTHR0000000001</IBAN></Id></DbtrAcct></RltdPties>
          <RmtInf><Ustrd>Factuur 2026-0001</Ustrd></RmtInf>
        </TxDtls></NtryDtls>
      </Ntry>
      <Ntry>
        <Amt Ccy="EUR">120.80</Amt><CdtDbtInd>DBIT</CdtDbtInd>
        <BookgDt><Dt>2026-11-12</Dt></BookgDt>
        <AcctSvcrRef>REF-0002</AcctSvcrRef>
        <NtryDtls><TxDtls>
          <RltdPties><Cdtr><Nm>Hosting &amp; Co</Nm></Cdtr><CdtrAcct><Id><IBAN>NL22HOST0000000002</IBAN></Id></CdtrAcct></RltdPties>
          <RmtInf><Strd><CdtrRefInf><Ref>H-001</Ref></CdtrRefInf></Strd></RmtInf>
        </TxDtls></NtryDtls>
      </Ntry>
    </Stmt>
  </BkToCstmrStmt>
</Document>"#;

    #[test]
    fn parses_camt053() {
        let s = parse(CAMT.as_bytes()).unwrap();
        assert_eq!(s.format, "CAMT.053");
        assert_eq!(s.account_iban.as_deref(), Some("NL00BANK0123456789"));
        assert_eq!(
            s.opening_balance,
            Some(Balance {
                date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                amount_cents: 100_000
            })
        );
        assert_eq!(s.closing_balance.as_ref().unwrap().amount_cents, 208_920);
        assert_eq!(s.transactions.len(), 2);
        let a = &s.transactions[0];
        assert_eq!(
            (a.amount_cents, a.counterparty_name.as_deref(), a.description.as_str()),
            (121_000, Some("Stichting Voorbeeld"), "Factuur 2026-0001")
        );
        assert_eq!(a.counterparty_iban.as_deref(), Some("NL11OTHR0000000001"));
        assert_eq!(a.bank_reference.as_deref(), Some("REF-0001"));
        let b = &s.transactions[1];
        assert_eq!(
            (b.amount_cents, b.counterparty_name.as_deref(), b.description.as_str()),
            (-12_080, Some("Hosting & Co"), "H-001")
        );
    }

    #[test]
    fn parses_ing_csv() {
        let csv = "\"Datum\";\"Naam / Omschrijving\";\"Rekening\";\"Tegenrekening\";\"Code\";\"Af Bij\";\"Bedrag (EUR)\";\"Mutatiesoort\";\"Mededelingen\"\n\
\"20261110\";\"Stichting Voorbeeld\";\"NL00INGB0001234567\";\"NL11OTHR0000000001\";\"GT\";\"Bij\";\"1210,00\";\"Online bankieren\";\"Factuur 2026-0001\"\n\
\"20261112\";\"Hosting en Co\";\"NL00INGB0001234567\";\"NL22HOST0000000002\";\"IC\";\"Af\";\"120,80\";\"Incasso\";\"Kenmerk H-001\"\n";
        let s = parse(csv.as_bytes()).unwrap();
        assert_eq!(s.format, "ING CSV");
        assert_eq!(s.account_iban.as_deref(), Some("NL00INGB0001234567"));
        assert_eq!(s.transactions[0].amount_cents, 121_000);
        assert_eq!(s.transactions[1].amount_cents, -12_080);
        assert_eq!(s.transactions[1].counterparty_name.as_deref(), Some("Hosting en Co"));
        assert_eq!(s.transactions[1].description, "Kenmerk H-001");
    }

    #[test]
    fn parses_rabobank_csv() {
        let csv = "\"IBAN/BBAN\",\"Munt\",\"BIC\",\"Volgnr\",\"Datum\",\"Rentedatum\",\"Bedrag\",\"Saldo na trn\",\"Tegenrekening IBAN/BBAN\",\"Naam tegenpartij\",\"Naam uiteindelijke partij\",\"Naam initiërende partij\",\"BIC tegenpartij\",\"Code\",\"Batch ID\",\"Transactiereferentie\",\"Machtigingskenmerk\",\"Incassant ID\",\"Betalingskenmerk\",\"Omschrijving-1\",\"Omschrijving-2\",\"Omschrijving-3\"\n\
\"NL00RABO0123456789\",\"EUR\",\"RABONL2U\",\"000000000000000001\",\"2026-11-10\",\"2026-11-10\",\"+1210,00\",\"+2210,00\",\"NL11OTHR0000000001\",\"Stichting Voorbeeld\",\"\",\"\",\"\",\"cb\",\"\",\"TR-1\",\"\",\"\",\"\",\"Factuur 2026-0001\",\"\",\"\"\n\
\"NL00RABO0123456789\",\"EUR\",\"RABONL2U\",\"000000000000000002\",\"2026-11-12\",\"2026-11-12\",\"-120,80\",\"+2089,20\",\"NL22HOST0000000002\",\"Hosting en Co\",\"\",\"\",\"\",\"id\",\"\",\"TR-2\",\"\",\"\",\"\",\"Kenmerk\",\"H-001\",\"\"\n";
        let s = parse(csv.as_bytes()).unwrap();
        assert_eq!(s.format, "Rabobank CSV");
        assert_eq!(s.transactions.len(), 2);
        assert_eq!(s.transactions[1].amount_cents, -12_080);
        assert_eq!(s.transactions[1].description, "Kenmerk H-001");
        assert_eq!(s.transactions[0].bank_reference.as_deref(), Some("TR-1"));
    }

    #[test]
    fn rejects_unknown_files() {
        assert!(parse(b"<html></html>").is_err());
        assert!(matches!(parse(b"foo;bar\n1;2"), Err(ImportError::MissingColumn(_))));
    }
}
