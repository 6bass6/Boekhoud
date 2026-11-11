//! Commands for contacts, sales invoices (incl. PDF), purchase invoices and documents.

use base64::Engine;
use boekhoud_core::invoice::{PurchaseLine, SalesLine};
use boekhoud_core::{Currency, Money};
use boekhoud_db::{
    Company, Contact, DocumentInfo, IssueRequirements, Ledger, PurchaseInput, PurchaseInvoiceDetail,
    PurchaseInvoiceSummary, SalesInvoiceDetail, SalesInvoiceInput, SalesInvoiceSummary, VatRegime,
};
use boekhoud_pdf::{InvoicePdf, Party, PdfLine};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{format, parse_date, with_ledger, AppState, CmdResult};

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn amount(s: &str, what: &str) -> CmdResult<i64> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(0);
    }
    Money::parse(t, ',', Currency::EUR)
        .map(|m| m.cents)
        .map_err(|_| format!("Ongeldig bedrag bij {what}: {s}"))
}

fn opt_date(s: &Option<String>) -> CmdResult<Option<NaiveDate>> {
    s.as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(parse_date)
        .transpose()
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn contacts(state: State<AppState>, include_archived: bool) -> CmdResult<Vec<Contact>> {
    with_ledger(&state, |l| l.contacts(include_archived))
}

#[tauri::command]
pub fn save_contact(state: State<AppState>, contact: Contact) -> CmdResult<String> {
    let c = Contact {
        contact_person: clean(contact.contact_person),
        email: clean(contact.email),
        address: clean(contact.address),
        postal_code: clean(contact.postal_code),
        city: clean(contact.city),
        vat_number: clean(contact.vat_number),
        kvk_number: clean(contact.kvk_number),
        default_account: clean(contact.default_account),
        default_tax_code: clean(contact.default_tax_code),
        ..contact
    };
    with_ledger(&state, |l| l.save_contact(&c))
}

// ---------------------------------------------------------------------------
// Sales invoices
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SalesLineForm {
    description: String,
    quantity: String,
    unit_price: String,
    tax_code: String,
    account: String,
}

#[derive(Deserialize)]
pub struct SalesInvoiceForm {
    contact_id: String,
    invoice_date: String,
    supply_date: Option<String>,
    due_date: Option<String>,
    reference: Option<String>,
    notes: Option<String>,
    lines: Vec<SalesLineForm>,
}

fn sales_input(f: SalesInvoiceForm) -> CmdResult<SalesInvoiceInput> {
    if f.contact_id.is_empty() {
        return Err("Kies een klant.".into());
    }
    let mut lines = Vec::new();
    for (i, l) in f.lines.into_iter().enumerate() {
        if l.description.trim().is_empty() && l.unit_price.trim().is_empty() {
            continue;
        }
        let n = i + 1;
        let quantity_milli =
            format::parse_quantity(&l.quantity).ok_or_else(|| format!("Regel {n}: ongeldig aantal."))?;
        lines.push(SalesLine {
            description: l.description.trim().to_string(),
            quantity_milli,
            unit_price_cents: amount(&l.unit_price, &format!("regel {n}"))?,
            tax_code: l.tax_code,
            account: if l.account.is_empty() { "8100".into() } else { l.account },
        });
    }
    Ok(SalesInvoiceInput {
        contact_id: f.contact_id,
        invoice_date: parse_date(&f.invoice_date)?,
        supply_date: opt_date(&f.supply_date)?,
        due_date: opt_date(&f.due_date)?,
        reference: clean(f.reference),
        notes: clean(f.notes),
        lines,
    })
}

#[tauri::command]
pub fn sales_invoices(state: State<AppState>, year: Option<i32>) -> CmdResult<Vec<SalesInvoiceSummary>> {
    with_ledger(&state, |l| l.sales_invoices(year))
}

#[tauri::command]
pub fn sales_invoice(state: State<AppState>, id: String) -> CmdResult<SalesInvoiceDetail> {
    with_ledger(&state, |l| l.sales_invoice(&id))
}

#[tauri::command]
pub fn save_sales_invoice(state: State<AppState>, id: Option<String>, invoice: SalesInvoiceForm) -> CmdResult<String> {
    let input = sales_input(invoice)?;
    with_ledger(&state, |l| match id.filter(|i| !i.is_empty()) {
        Some(id) => l.update_sales_invoice(&id, &input).map(|_| id),
        None => l.create_sales_invoice(&input),
    })
}

#[tauri::command]
pub fn delete_sales_invoice(state: State<AppState>, id: String) -> CmdResult<()> {
    with_ledger(&state, |l| l.delete_draft_sales_invoice(&id))
}

#[tauri::command]
pub fn sales_issue_requirements(state: State<AppState>, id: String) -> CmdResult<IssueRequirements> {
    with_ledger(&state, |l| l.sales_issue_requirements(&id))
}

/// Issue the invoice and archive its PDF as a document.
#[tauri::command]
pub fn issue_sales_invoice(state: State<AppState>, id: String) -> CmdResult<SalesInvoiceSummary> {
    with_ledger(&state, |l| {
        let summary = l.issue_sales_invoice(&id)?;
        let detail = l.sales_invoice(&id)?;
        let pdf = boekhoud_pdf::render_invoice(&invoice_pdf(l, &detail)?);
        let name = format!(
            "{} {}.pdf",
            if detail.summary.is_credit_note {
                "Creditnota"
            } else {
                "Factuur"
            },
            summary.invoice_number.clone().unwrap_or_default()
        );
        let doc = l.add_document(&pdf, &name, "sales_invoice")?;
        l.set_sales_invoice_document(&id, &doc.id)?;
        Ok(summary)
    })
}

#[tauri::command]
pub fn create_credit_note(state: State<AppState>, id: String, date: String) -> CmdResult<String> {
    let date = parse_date(&date)?;
    with_ledger(&state, |l| l.create_credit_note(&id, date))
}

#[tauri::command]
pub fn register_sales_payment(
    state: State<AppState>,
    id: String,
    date: String,
    amount_text: String,
) -> CmdResult<String> {
    let date = parse_date(&date)?;
    let cents = amount(&amount_text, "betaling")?;
    with_ledger(&state, |l| l.register_sales_payment(&id, date, cents, "handmatig"))
}

/// Open the invoice PDF: the archived document for issued invoices, a preview for drafts.
#[tauri::command]
pub fn open_sales_invoice_pdf(state: State<AppState>, id: String) -> CmdResult<()> {
    let (bytes, name) = with_ledger(&state, |l| {
        let detail = l.sales_invoice(&id)?;
        if let Some(doc) = &detail.document_id {
            let (info, bytes) = l.document_bytes(doc)?;
            return Ok((bytes, info.original_filename));
        }
        let pdf = boekhoud_pdf::render_invoice(&invoice_pdf(l, &detail)?);
        Ok((pdf, "Concept factuur.pdf".to_string()))
    })?;
    open_temp_file(&state, &name, &bytes)
}

fn address_lines(
    address: &Option<String>,
    postal: &Option<String>,
    city: &Option<String>,
    country: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(a) = address {
        out.extend(a.lines().map(str::to_string));
    }
    let pc = [postal.as_deref(), city.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if !pc.trim().is_empty() {
        out.push(pc);
    }
    if country != "NL" && !country.is_empty() {
        out.push(country.to_string());
    }
    out
}

/// Compose the printable invoice from stored data. Issued invoices use their frozen snapshots.
fn invoice_pdf(l: &Ledger, d: &SalesInvoiceDetail) -> boekhoud_db::Result<InvoicePdf> {
    let company: Company = match &d.seller {
        Some(c) => c.clone(),
        None => l.company()?.unwrap_or_default(),
    };
    let buyer: Contact = match &d.buyer {
        Some(c) => c.clone(),
        None => l.contact(&d.summary.contact_id)?,
    };
    let codes = l.tax_codes(d.summary.invoice_date)?;
    let rate_of = |code: &str| codes.iter().find(|c| c.code == code).and_then(|c| c.rate_bp);
    let draft = d.summary.status == "draft";
    let credit = d.summary.is_credit_note;

    let mut seller_lines = address_lines(&company.address, &company.postal_code, &company.city, &company.country);
    if let Some(e) = &company.email {
        seller_lines.push(e.clone());
    }
    let mut buyer_lines = Vec::new();
    if let Some(p) = &buyer.contact_person {
        buyer_lines.push(format!("t.a.v. {p}"));
    }
    buyer_lines.extend(address_lines(
        &buyer.address,
        &buyer.postal_code,
        &buyer.city,
        &buyer.country,
    ));
    if let Some(v) = &buyer.vat_number {
        buyer_lines.push(format!("Btw-id: {v}"));
    }

    let mut meta = vec![
        (
            if credit { "Creditnotanummer" } else { "Factuurnummer" }.to_string(),
            d.summary.invoice_number.clone().unwrap_or_else(|| "CONCEPT".into()),
        ),
        ("Factuurdatum".to_string(), format::date(d.summary.invoice_date)),
    ];
    if let Some(s) = d.supply_date {
        meta.push(("Leverdatum".into(), format::date(s)));
    }
    if !credit {
        meta.push(("Vervaldatum".into(), format::date(d.summary.due_date)));
    }
    if let Some(n) = &d.credits_invoice_number {
        meta.push(("Betreft factuur".into(), n.clone()));
    }
    if let Some(r) = d.reference.as_ref().filter(|_| !credit) {
        meta.push(("Uw referentie".into(), r.clone()));
    }

    let lines = d
        .lines
        .iter()
        .map(|line| {
            let net = line.net_cents().unwrap_or(0);
            PdfLine {
                description: line.description.clone(),
                quantity: format::quantity(line.quantity_milli),
                unit_price: format::euro(line.unit_price_cents),
                vat: match rate_of(&line.tax_code) {
                    Some(bp)
                        if d.vat_regime == VatRegime::Regular
                            && !matches!(line.tax_code.as_str(), "V0EU" | "V0NEU") =>
                    {
                        format::rate(bp)
                    }
                    _ => "—".into(),
                },
                amount: format::euro(net),
            }
        })
        .collect();

    let mut totals = Vec::new();
    let mut notices = Vec::new();
    if let Some(t) = &d.totals {
        totals.push(("Subtotaal".to_string(), format::euro(t.subtotal_cents)));
        for g in &t.groups {
            if g.vat_cents != 0 {
                let r = g.rate_bp.map(format::rate).unwrap_or_default();
                totals.push((
                    format!("Btw {r} over {}", format::euro(g.base_cents)),
                    format::euro(g.vat_cents),
                ));
            }
            if g.reverse_charge {
                notices.push(format!(
                    "Btw verlegd — VAT reverse charged. Btw-id afnemer: {}.",
                    buyer.vat_number.clone().unwrap_or_default()
                ));
            }
            if g.code == "VKOR" {
                notices.push("Vrijgesteld van btw op grond van de kleineondernemersregeling (KOR).".into());
            }
        }
        totals.push(("Totaal".to_string(), format::euro(t.total_cents)));
    }
    notices.dedup();

    let payment_text = if credit {
        Some("Het bedrag van deze creditnota wordt verrekend of aan u terugbetaald.".into())
    } else {
        company.iban.as_ref().map(|iban| {
            format!(
                "Graag het totaalbedrag vóór {} overmaken op {} t.n.v. {}, onder vermelding van factuurnummer {}.",
                format::date(d.summary.due_date),
                iban,
                company.legal_name,
                d.summary.invoice_number.clone().unwrap_or_else(|| "(volgt)".into())
            )
        })
    };

    let mut footer = vec![company.legal_name.clone()];
    if let Some(k) = &company.kvk_number {
        footer.push(format!("KvK {k}"));
    }
    if let Some(v) = &company.vat_number {
        if d.vat_regime == VatRegime::Regular {
            footer.push(format!("Btw-id {v}"));
        }
    }
    if let Some(i) = &company.iban {
        footer.push(format!("IBAN {i}"));
    }

    let title = match (draft, credit) {
        (true, true) => "CONCEPT CREDITNOTA",
        (true, false) => "CONCEPT FACTUUR",
        (false, true) => "CREDITNOTA",
        (false, false) => "FACTUUR",
    };
    Ok(InvoicePdf {
        title: title.into(),
        seller: Party {
            name: company.trade_name.clone().unwrap_or_else(|| company.legal_name.clone()),
            lines: seller_lines,
        },
        buyer: Party {
            name: buyer.name.clone(),
            lines: buyer_lines,
        },
        meta,
        lines,
        totals,
        notices,
        payment_text,
        notes: d.notes.clone(),
        footer: footer.join(" · "),
        document_title: format!(
            "{} {}",
            if credit { "Creditnota" } else { "Factuur" },
            d.summary.invoice_number.clone().unwrap_or_default()
        ),
    })
}

// ---------------------------------------------------------------------------
// Purchase invoices
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PurchaseLineForm {
    description: Option<String>,
    account: String,
    net: String,
    tax_code: String,
    vat: String,
}

#[derive(Deserialize)]
pub struct PurchaseForm {
    contact_id: String,
    supplier_reference: Option<String>,
    invoice_date: String,
    due_date: Option<String>,
    description: String,
    paid_privately: bool,
    document_id: Option<String>,
    lines: Vec<PurchaseLineForm>,
}

#[tauri::command]
pub fn register_purchase_invoice(state: State<AppState>, invoice: PurchaseForm) -> CmdResult<PurchaseInvoiceSummary> {
    if invoice.contact_id.is_empty() {
        return Err("Kies een leverancier.".into());
    }
    let mut lines = Vec::new();
    for (i, l) in invoice.lines.into_iter().enumerate() {
        if l.account.is_empty() && l.net.trim().is_empty() {
            continue;
        }
        let n = i + 1;
        if l.account.is_empty() {
            return Err(format!("Regel {n}: kies een kostenrekening."));
        }
        lines.push(PurchaseLine {
            description: clean(l.description),
            account: l.account,
            net_cents: amount(&l.net, &format!("regel {n}"))?,
            tax_code: l.tax_code,
            vat_cents: amount(&l.vat, &format!("btw regel {n}"))?,
        });
    }
    let input = PurchaseInput {
        contact_id: invoice.contact_id,
        supplier_reference: clean(invoice.supplier_reference),
        invoice_date: parse_date(&invoice.invoice_date)?,
        due_date: opt_date(&invoice.due_date)?,
        description: invoice.description,
        lines,
        paid_privately: invoice.paid_privately,
        document_id: clean(invoice.document_id),
    };
    with_ledger(&state, |l| l.register_purchase_invoice(&input))
}

#[tauri::command]
pub fn purchase_invoices(state: State<AppState>, year: Option<i32>) -> CmdResult<Vec<PurchaseInvoiceSummary>> {
    with_ledger(&state, |l| l.purchase_invoices(year))
}

#[tauri::command]
pub fn purchase_invoice(state: State<AppState>, id: String) -> CmdResult<PurchaseInvoiceDetail> {
    with_ledger(&state, |l| l.purchase_invoice(&id))
}

#[tauri::command]
pub fn attach_purchase_document(state: State<AppState>, id: String, document_id: String) -> CmdResult<()> {
    with_ledger(&state, |l| l.attach_purchase_document(&id, &document_id))
}

#[tauri::command]
pub fn register_purchase_payment(
    state: State<AppState>,
    id: String,
    date: String,
    amount_text: String,
) -> CmdResult<String> {
    let date = parse_date(&date)?;
    let cents = amount(&amount_text, "betaling")?;
    with_ledger(&state, |l| l.register_purchase_payment(&id, date, cents, "handmatig"))
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn upload_document(
    state: State<AppState>,
    filename: String,
    data_base64: String,
    document_type: String,
) -> CmdResult<DocumentInfo> {
    let bytes = b64()
        .decode(data_base64.as_bytes())
        .map_err(|_| "Bestand kon niet worden gelezen.".to_string())?;
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("Bestand is groter dan 50 MB.".into());
    }
    with_ledger(&state, |l| l.add_document(&bytes, &filename, &document_type))
}

#[derive(Serialize)]
pub struct DocumentContent {
    info: DocumentInfo,
    data_base64: String,
}

#[tauri::command]
pub fn document_content(state: State<AppState>, id: String) -> CmdResult<DocumentContent> {
    let (info, bytes) = with_ledger(&state, |l| l.document_bytes(&id))?;
    Ok(DocumentContent {
        info,
        data_base64: b64().encode(bytes),
    })
}

#[tauri::command]
pub fn open_document(state: State<AppState>, id: String) -> CmdResult<()> {
    let (info, bytes) = with_ledger(&state, |l| l.document_bytes(&id))?;
    open_temp_file(&state, &info.original_filename, &bytes)
}

/// Write a copy to the app's temp folder and open it with the default application.
/// The archived original is never handed out for editing.
pub fn open_temp_file(state: &State<AppState>, name: &str, bytes: &[u8]) -> CmdResult<()> {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " .-_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dir = state.data_dir.join("tmp");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Tijdelijke map: {e}"))?;
    let path = dir.join(safe);
    std::fs::write(&path, bytes).map_err(|e| format!("Bestand schrijven mislukt: {e}"))?;
    open_with_default_app(&path)
}

fn open_with_default_app(path: &std::path::Path) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    result.map(|_| ()).map_err(|e| format!("Openen mislukt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use boekhoud_core::invoice::SalesLine;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// Builds a complete fictional invoice and renders it; writes the PDF to the temp dir for inspection.
    #[test]
    fn issued_invoice_pdf_contains_required_details() {
        let mut l = Ledger::open_in_memory().unwrap();
        l.save_company(&Company {
            legal_name: "Voorbeeld Apps".into(),
            kvk_number: Some("12345678".into()),
            vat_number: Some("NL000099998B57".into()),
            address: Some("Voorbeeldstraat 1".into()),
            postal_code: Some("1234 AB".into()),
            city: Some("Utrecht".into()),
            iban: Some("NL00BANK0123456789".into()),
            email: Some("info@voorbeeld.invalid".into()),
            ..Default::default()
        })
        .unwrap();
        let c = l
            .save_contact(&Contact {
                name: "Stichting Voorbeeld".into(),
                contact_person: Some("J. Jansen".into()),
                address: Some("Laan 2".into()),
                postal_code: Some("5678 CD".into()),
                city: Some("Zwolle".into()),
                country: "NL".into(),
                is_customer: true,
                ..Default::default()
            })
            .unwrap();
        let id = l
            .create_sales_invoice(&SalesInvoiceInput {
                contact_id: c,
                invoice_date: d(2026, 11, 11),
                supply_date: Some(d(2026, 11, 5)),
                due_date: None,
                reference: Some("PO-77".into()),
                notes: Some("Bedankt voor de prettige samenwerking.".into()),
                lines: vec![
                    SalesLine {
                        description: "Lezing ‘Apps bouwen als eenmanszaak’ op 5 november".into(),
                        quantity_milli: 1000,
                        unit_price_cents: 95_000,
                        tax_code: "V21".into(),
                        account: "8100".into(),
                    },
                    SalesLine {
                        description: "Reiskosten (km)".into(),
                        quantity_milli: 120_500,
                        unit_price_cents: 23,
                        tax_code: "V21".into(),
                        account: "8100".into(),
                    },
                ],
            })
            .unwrap();
        l.issue_sales_invoice(&id).unwrap();
        let detail = l.sales_invoice(&id).unwrap();
        let pdf = invoice_pdf(&l, &detail).unwrap();
        assert_eq!(pdf.title, "FACTUUR");
        assert!(pdf.meta.iter().any(|(k, v)| k == "Factuurnummer" && v == "2026-0001"));
        assert!(pdf.meta.iter().any(|(k, _)| k == "Leverdatum"));
        assert!(pdf.footer.contains("KvK 12345678") && pdf.footer.contains("Btw-id NL000099998B57"));
        assert_eq!(pdf.totals.last().unwrap().1, "€ 1.183,04");
        let bytes = boekhoud_pdf::render_invoice(&pdf);
        std::fs::write(std::env::temp_dir().join("boekhoud-voorbeeldfactuur.pdf"), bytes).unwrap();
    }
}
