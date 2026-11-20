//! Proposals for a purchase invoice read from its document (UBL exactly, PDF/image via local OCR).
//! Only a proposal: the screen fills empty fields and the user checks everything before booking.

use boekhoud_db::{Contact, TaxCode, VatRegime};
use boekhoud_import::invoice::{from_text, from_ubl, InvoiceProposal};
use serde::Serialize;
use tauri::State;

use crate::{ocr, with_ledger, AppState, CmdResult};

#[derive(Debug, Serialize)]
pub struct PurchaseSuggestion {
    /// "ubl" (exact) or "text" (OCR, check carefully).
    pub source: String,
    /// Existing supplier recognised by VAT number, KvK number or name.
    pub contact_id: Option<String>,
    /// What the document says about the supplier (to create a new contact).
    pub supplier_name: Option<String>,
    pub supplier_vat: Option<String>,
    pub supplier_kvk: Option<String>,
    pub supplier_iban: Option<String>,
    pub supplier_reference: Option<String>,
    pub invoice_date: Option<String>,
    pub due_date: Option<String>,
    pub net_cents: Option<i64>,
    pub vat_cents: Option<i64>,
    pub total_cents: Option<i64>,
    pub vat_rate_bp: Option<i64>,
    /// Amounts are in this other currency and were left empty (owner looks up the euro amount).
    pub foreign_currency: Option<String>,
    /// Proposed VAT code and cost account (supplier defaults first).
    pub tax_code: Option<String>,
    pub account: Option<String>,
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

fn find_contact<'a>(contacts: &'a [Contact], p: &InvoiceProposal, text: &str) -> Option<&'a Contact> {
    let suppliers = || contacts.iter().filter(|c| !c.archived);
    if let Some(vat) = &p.vat_number {
        if let Some(c) = suppliers().find(|c| c.vat_number.as_deref().map(norm).as_deref() == Some(vat.as_str())) {
            return Some(c);
        }
    }
    if let Some(kvk) = &p.kvk_number {
        if let Some(c) = suppliers().find(|c| c.kvk_number.as_deref().map(norm).as_deref() == Some(kvk.as_str())) {
            return Some(c);
        }
    }
    // By name: the longest supplier name that occurs in the text (at least 4 letters, to avoid noise).
    let haystack = text.to_lowercase();
    suppliers()
        .filter(|c| c.is_supplier)
        .filter(|c| {
            let name = c.name.trim().to_lowercase();
            name.chars().filter(|ch| ch.is_alphanumeric()).count() >= 4 && haystack.contains(&name)
        })
        .max_by_key(|c| c.name.len())
}

/// The purchase VAT rate (from the VAT codes valid on the invoice date) that turns the net amount into the
/// VAT amount, within a cent. For when OCR mangled the printed percentage ("210/0" for "21%").
fn rate_from_amounts(codes: &[TaxCode], p: &InvoiceProposal) -> Option<i64> {
    let (net, vat) = (p.net_cents?, p.vat_cents?);
    if net <= 0 {
        return None;
    }
    codes
        .iter()
        .filter(|t| t.kind == "purchase")
        .filter_map(|t| t.rate_bp)
        .find(|bp| (net * bp - vat * 10_000).abs() < 10_000)
}

#[tauri::command]
pub async fn extract_purchase(state: State<'_, AppState>, document_id: String) -> CmdResult<PurchaseSuggestion> {
    let (info, bytes, own_ids) = with_ledger(&state, |l| {
        let (info, bytes) = l.document_bytes(&document_id)?;
        let mut own = Vec::new();
        if let Some(c) = l.company()? {
            own.extend(
                [Some(c.legal_name), c.trade_name, c.vat_number, c.kvk_number, c.iban]
                    .into_iter()
                    .flatten()
                    .map(|s| norm(&s)),
            );
        }
        for b in l.bank_accounts()? {
            own.push(norm(&b.iban));
        }
        Ok((info, bytes, own))
    })?;

    let is_xml = info.mime_type.contains("xml") || info.original_filename.to_lowercase().ends_with(".xml");
    let (proposal, text) = if is_xml {
        let p = from_ubl(&bytes).map_err(|e| format!("Dit XML-bestand is geen leesbare e-factuur (UBL): {e}"))?;
        let name = p.supplier_name.clone().unwrap_or_default();
        (p, name)
    } else {
        let mime = info.mime_type.clone();
        let text = tauri::async_runtime::spawn_blocking(move || ocr::recognise(&bytes, &mime))
            .await
            .map_err(|e| format!("Tekstherkenning mislukt: {e}"))??;
        if text.trim().is_empty() {
            return Err("Er is geen tekst in dit bestand herkend. Vul de gegevens zelf in.".into());
        }
        (from_text(&text, &own_ids), text)
    };

    with_ledger(&state, |l| {
        let contacts = l.contacts(false)?;
        let contact = find_contact(&contacts, &proposal, &text);
        let date = proposal
            .invoice_date
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        let regime = l.vat_regime_on(date)?;
        let codes = l.tax_codes(date)?;
        let vat_rate_bp = proposal.vat_rate_bp.or_else(|| rate_from_amounts(&codes, &proposal));
        let tax_code = contact.and_then(|c| c.default_tax_code.clone()).or_else(|| {
            // A domestic purchase code with the rate found on the invoice, valid on its date and regime.
            let rate = vat_rate_bp?;
            let foreign = proposal.vat_number.as_deref().is_some_and(|v| !v.starts_with("NL"));
            if foreign || regime == VatRegime::Kor {
                return None;
            }
            codes
                .iter()
                .find(|t| {
                    t.kind == "purchase"
                        && !t.reverse_charge
                        && t.rate_bp == Some(rate)
                        && t.regime.is_none_or(|r| r == regime)
                })
                .map(|t| t.code.clone())
        });
        Ok(PurchaseSuggestion {
            source: proposal.source.clone(),
            contact_id: contact.map(|c| c.id.clone()),
            supplier_name: proposal.supplier_name.clone(),
            supplier_vat: proposal.vat_number.clone(),
            supplier_kvk: proposal.kvk_number.clone(),
            supplier_iban: proposal.iban.clone(),
            supplier_reference: proposal.invoice_number.clone(),
            invoice_date: proposal.invoice_date.map(|d| d.to_string()),
            due_date: proposal.due_date.map(|d| d.to_string()),
            net_cents: proposal.net_cents,
            vat_cents: proposal.vat_cents,
            total_cents: proposal.total_cents,
            vat_rate_bp,
            foreign_currency: proposal.foreign_currency.clone(),
            tax_code,
            account: contact.and_then(|c| c.default_account.clone()),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: &str, name: &str, vat: Option<&str>) -> Contact {
        Contact {
            id: id.into(),
            name: name.into(),
            country: "NL".into(),
            vat_number: vat.map(Into::into),
            is_supplier: true,
            ..Default::default()
        }
    }

    #[test]
    fn rate_is_derived_from_net_and_vat() {
        let code = |code: &str, kind: &str, bp: i64| TaxCode {
            code: code.into(),
            valid_from: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            valid_until: None,
            name: code.into(),
            kind: kind.into(),
            rate_bp: Some(bp),
            rubric: None,
            icp: false,
            reverse_charge: false,
            regime: None,
            note: None,
        };
        let codes = [
            code("V21", "sales", 2100),
            code("I21", "purchase", 2100),
            code("I9", "purchase", 900),
        ];
        let p = |net, vat| InvoiceProposal {
            net_cents: Some(net),
            vat_cents: Some(vat),
            ..Default::default()
        };
        assert_eq!(rate_from_amounts(&codes, &p(1_080, 227)), Some(2100));
        assert_eq!(rate_from_amounts(&codes, &p(1_000, 90)), Some(900));
        assert_eq!(rate_from_amounts(&codes, &p(1_000, 150)), None);
        assert_eq!(rate_from_amounts(&codes, &p(0, 0)), None);
    }

    #[test]
    fn supplier_is_found_by_vat_number_then_name() {
        let contacts = vec![
            contact("a", "Voorbeeld Hosting", Some("NL 8765.43.210.B01")),
            contact("b", "Kantoor", None),
            contact("c", "Kantoorartikelen Voorbeeld", None),
        ];
        let p = InvoiceProposal {
            vat_number: Some("NL876543210B01".into()),
            ..Default::default()
        };
        assert_eq!(find_contact(&contacts, &p, "").map(|c| c.id.as_str()), Some("a"));
        let none = InvoiceProposal::default();
        let text = "Factuur van KANTOORARTIKELEN VOORBEELD B.V.";
        assert_eq!(find_contact(&contacts, &none, text).map(|c| c.id.as_str()), Some("c"));
        assert!(find_contact(&contacts, &none, "iets anders").is_none());
    }
}
