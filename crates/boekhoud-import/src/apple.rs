//! App Store Connect *financial reports* (tab-separated, "All Countries or Regions" single file or
//! per-region files). Amounts are in the local currency of each storefront ("Partner Share
//! Currency"); the euro amount Apple pays comes from the Payments page and is entered separately.
//!
//! Field reference: App Store Connect Help → Financial report fields.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::text::{amount_cents, date, decode, find, require};
use crate::{ImportError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleCurrencyTotal {
    pub currency: String,
    pub units: i64,
    /// Sum of Extended Partner Share in `currency`, returns subtracted.
    pub proceeds_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleProductTotal {
    pub title: String,
    pub vendor_identifier: String,
    pub currency: String,
    pub units: i64,
    pub proceeds_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleReport {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub currencies: Vec<AppleCurrencyTotal>,
    pub products: Vec<AppleProductTotal>,
    pub rows: usize,
}

pub fn parse(bytes: &[u8]) -> Result<AppleReport> {
    let text = decode(bytes);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header_line = lines.next().ok_or(ImportError::Empty)?;
    let headers: Vec<String> = header_line.split('\t').map(|h| h.trim().to_string()).collect();
    if headers.len() < 5 {
        return Err(ImportError::UnknownFormat(
            "geen tab-gescheiden Apple financieel rapport".into(),
        ));
    }
    let c_start = require(&headers, &["Start Date"])?;
    let c_end = require(&headers, &["End Date"])?;
    let c_qty = require(&headers, &["Quantity"])?;
    let c_ext = require(&headers, &["Extended Partner Share"])?;
    let c_cur = require(&headers, &["Partner Share Currency"])?;
    let c_sr = find(&headers, &["Sale or Return"]);
    let c_title = find(&headers, &["Title"]);
    let c_vendor = find(&headers, &["Vendor Identifier"]);

    let mut start: Option<NaiveDate> = None;
    let mut end: Option<NaiveDate> = None;
    let mut currencies: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut products: BTreeMap<(String, String, String), (i64, i64)> = BTreeMap::new();
    let mut rows = 0;
    for (i, line) in lines.enumerate() {
        let n = i + 2;
        let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
        let first = cells.first().copied().unwrap_or("");
        // Footer rows ("Total_Rows", "Total_Amount", ...) and repeated headers are skipped.
        if first.starts_with("Total_") || first == headers[0] || cells.len() <= c_cur.max(c_ext) {
            continue;
        }
        let row_err = |m: &str| ImportError::Row {
            line: n,
            message: m.to_string(),
        };
        let s = date(cells[c_start], true).ok_or_else(|| row_err("ongeldige startdatum"))?;
        let e = date(cells[c_end], true).ok_or_else(|| row_err("ongeldige einddatum"))?;
        start = Some(start.map_or(s, |x| x.min(s)));
        end = Some(end.map_or(e, |x| x.max(e)));
        let qty: i64 = cells[c_qty].parse().map_err(|_| row_err("ongeldig aantal"))?;
        let mut ext = amount_cents(cells[c_ext]).ok_or_else(|| row_err("ongeldig bedrag (Extended Partner Share)"))?;
        // Returns must reduce proceeds, whichever sign convention the file uses.
        let is_return = c_sr.map(|c| cells.get(c).copied() == Some("R")).unwrap_or(false);
        if is_return && ext > 0 {
            ext = -ext;
        }
        let units = if is_return { -qty.abs() } else { qty };
        let currency = cells[c_cur].to_uppercase();
        if currency.len() != 3 {
            return Err(row_err("ongeldige valuta"));
        }
        let ct = currencies.entry(currency.clone()).or_default();
        ct.0 += units;
        ct.1 += ext;
        let title = c_title.and_then(|c| cells.get(c)).unwrap_or(&"").to_string();
        let vendor = c_vendor.and_then(|c| cells.get(c)).unwrap_or(&"").to_string();
        let pt = products.entry((title, vendor, currency)).or_default();
        pt.0 += units;
        pt.1 += ext;
        rows += 1;
    }
    let (period_start, period_end) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        _ => return Err(ImportError::Empty),
    };
    Ok(AppleReport {
        period_start,
        period_end,
        currencies: currencies
            .into_iter()
            .map(|(currency, (units, proceeds_cents))| AppleCurrencyTotal {
                currency,
                units,
                proceeds_cents,
            })
            .collect(),
        products: products
            .into_iter()
            .map(
                |((title, vendor_identifier, currency), (units, proceeds_cents))| AppleProductTotal {
                    title,
                    vendor_identifier,
                    currency,
                    units,
                    proceeds_cents,
                },
            )
            .collect(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fictional report in the documented column layout.
    pub const SAMPLE: &str = "Start Date\tEnd Date\tUPC\tISRC/ISBN\tVendor Identifier\tQuantity\tPartner Share\tExtended Partner Share\tPartner Share Currency\tSale or Return\tApple Identifier\tArtist/Show/Developer/Author\tTitle\tLabel/Studio/Network/Developer/Publisher\tGrid\tProduct Type Identifier\tISAN/Other Identifier\tCountry Of Sale\tPre-order Flag\tPromo Code\tCustomer Price\tCustomer Currency
09/28/2026\t10/31/2026\t\t\tvoorbeeld.app\t10\t0.70\t7.00\tEUR\tS\t1234567890\tVoorbeeld Apps\tVoorbeeld App\t\t\t1F\t\tNL\t\t\t0.99\tEUR
09/28/2026\t10/31/2026\t\t\tvoorbeeld.app\t-1\t0.70\t-0.70\tEUR\tR\t1234567890\tVoorbeeld Apps\tVoorbeeld App\t\t\t1F\t\tDE\t\t\t0.99\tEUR
09/28/2026\t10/31/2026\t\t\tvoorbeeld.pro\t3\t2.10\t6.30\tUSD\tS\t1234567891\tVoorbeeld Apps\tVoorbeeld Pro\t\t\tIA1\t\tUS\t\t\t2.99\tUSD
Total_Rows\t3
Total_Amount\t12.60
";

    #[test]
    fn parses_currencies_products_and_returns() {
        let r = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(r.rows, 3);
        assert_eq!(r.period_start, NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
        assert_eq!(r.period_end, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
        assert_eq!(
            r.currencies,
            vec![
                AppleCurrencyTotal {
                    currency: "EUR".into(),
                    units: 9,
                    proceeds_cents: 630
                },
                AppleCurrencyTotal {
                    currency: "USD".into(),
                    units: 3,
                    proceeds_cents: 630
                },
            ]
        );
        assert_eq!(r.products.len(), 2);
    }

    #[test]
    fn positive_return_amounts_are_subtracted() {
        let s = SAMPLE.replace("-1\t0.70\t-0.70", "1\t0.70\t0.70");
        let r = parse(s.as_bytes()).unwrap();
        assert_eq!(r.currencies[0].proceeds_cents, 630);
        assert_eq!(r.currencies[0].units, 9);
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(parse(b"a,b,c\n1,2,3"), Err(ImportError::UnknownFormat(_))));
        assert!(matches!(parse(b""), Err(ImportError::Empty)));
    }
}
