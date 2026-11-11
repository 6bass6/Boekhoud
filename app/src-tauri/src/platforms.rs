//! Commands for importing and booking App Store / Google Play reports.

use base64::Engine;
use boekhoud_core::{Currency, Money};
use boekhoud_db::{CurrencyAmount, EurAmount, PlatformReportInput, PlatformReportView, ProductAmount};
use boekhoud_import::{apple, google};
use serde::Deserialize;
use tauri::State;

use crate::{with_ledger, AppState, CmdResult};

/// Recognise the report type from its content and convert it to the platform-neutral form.
pub fn parse_platform_file(bytes: &[u8]) -> CmdResult<PlatformReportInput> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4000)]).to_lowercase();
    let head_utf16 = if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        "utf16".to_string()
    } else {
        String::new()
    };
    if head.contains("extended partner share") {
        let r = apple::parse(bytes).map_err(|e| format!("Apple-rapport: {e}"))?;
        return Ok(PlatformReportInput {
            platform: "apple".into(),
            period_start: r.period_start,
            period_end: r.period_end,
            currencies: r
                .currencies
                .iter()
                .map(|c| CurrencyAmount {
                    currency: c.currency.clone(),
                    units: c.units,
                    amount_cents: c.proceeds_cents,
                })
                .collect(),
            products: r
                .products
                .iter()
                .map(|p| ProductAmount {
                    product: p.title.clone(),
                    currency: p.currency.clone(),
                    units: p.units,
                    amount_cents: p.proceeds_cents,
                })
                .collect(),
            breakdown: serde_json::json!({ "rows": r.rows }),
        });
    }
    if head.contains("merchant currency") || !head_utf16.is_empty() {
        let r = google::parse(bytes).map_err(|e| format!("Google Play-rapport: {e}"))?;
        if r.merchant_currency != "EUR" {
            return Err(format!(
                "Uitbetalingsvaluta is {}; alleen EUR wordt ondersteund.",
                r.merchant_currency
            ));
        }
        let charges: i64 = r.products.iter().map(|p| p.charges).sum();
        return Ok(PlatformReportInput {
            platform: "google".into(),
            period_start: r.period_start,
            period_end: r.period_end,
            currencies: vec![CurrencyAmount {
                currency: "EUR".into(),
                units: charges,
                amount_cents: r.net_cents,
            }],
            products: r
                .products
                .iter()
                .map(|p| ProductAmount {
                    product: p.product.clone(),
                    currency: "EUR".into(),
                    units: p.charges,
                    amount_cents: p.amount_cents,
                })
                .collect(),
            breakdown: serde_json::json!({ "types": r.by_type, "rows": r.rows }),
        });
    }
    Err("Onbekend rapport. Gebruik het financiële rapport van App Store Connect of het earnings report van Google Play (CSV).".into())
}

#[tauri::command]
pub fn import_platform_file(
    state: State<AppState>,
    filename: String,
    data_base64: String,
) -> CmdResult<PlatformReportView> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| "Bestand kon niet worden gelezen.".to_string())?;
    let input = parse_platform_file(&bytes)?;
    with_ledger(&state, |l| {
        let id = l.import_platform_report(&input, &bytes, &filename)?;
        l.platform_report(&id)
    })
}

#[tauri::command]
pub fn platform_reports(state: State<AppState>) -> CmdResult<Vec<PlatformReportView>> {
    with_ledger(&state, |l| l.platform_reports())
}

#[derive(Deserialize)]
pub struct EurForm {
    currency: String,
    eur: String,
}

#[tauri::command]
pub fn book_platform_report(state: State<AppState>, id: String, amounts: Vec<EurForm>) -> CmdResult<String> {
    let mut eur = Vec::new();
    for a in amounts {
        let cents = Money::parse(a.eur.trim(), ',', Currency::EUR)
            .map_err(|_| format!("Ongeldig euro-bedrag voor {}: {}", a.currency, a.eur))?
            .cents;
        eur.push(EurAmount {
            currency: a.currency,
            eur_cents: cents,
        });
    }
    with_ledger(&state, |l| l.book_platform_report(&id, &eur))
}

#[tauri::command]
pub fn delete_platform_report(state: State<AppState>, id: String) -> CmdResult<()> {
    with_ledger(&state, |l| l.delete_platform_report(&id))
}

#[tauri::command]
pub fn platform_revenue_by_product(state: State<AppState>, year: i32) -> CmdResult<Vec<boekhoud_db::ProductRevenue>> {
    with_ledger(&state, |l| l.platform_revenue_by_product(year))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_report_types() {
        let apple = "Start Date\tEnd Date\tVendor Identifier\tQuantity\tPartner Share\tExtended Partner Share\tPartner Share Currency\tSale or Return\tTitle\n10/01/2026\t10/31/2026\tx\t2\t1.00\t2.00\tEUR\tS\tApp\n";
        assert_eq!(parse_platform_file(apple.as_bytes()).unwrap().platform, "apple");
        let google = "Description,Transaction Date,Transaction Type,Product Title,Merchant Currency,Amount (Merchant Currency)\nx,\"Oct 1, 2026\",Charge,App,EUR,1.00\n";
        let g = parse_platform_file(google.as_bytes()).unwrap();
        assert_eq!((g.platform.as_str(), g.currencies[0].amount_cents), ("google", 100));
        assert!(parse_platform_file(b"iets anders").is_err());
    }
}
