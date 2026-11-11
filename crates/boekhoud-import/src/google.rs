//! Google Play *earnings reports* (CSV, one row per transaction). "Amount (Merchant Currency)" is
//! what Google credits or debits you, so its sum is the payout for the month.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::text::{amount_cents, date, decode, find, require};
use crate::{ImportError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoogleTypeTotal {
    /// "Charge", "Google fee", "Charge refund", "Google fee refund", "Tax", ...
    pub transaction_type: String,
    pub rows: usize,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoogleProductTotal {
    pub product: String,
    pub charges: i64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoogleReport {
    pub merchant_currency: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub by_type: Vec<GoogleTypeTotal>,
    pub products: Vec<GoogleProductTotal>,
    /// Sum of all amounts in merchant currency = net payout.
    pub net_cents: i64,
    pub rows: usize,
}

pub fn parse(bytes: &[u8]) -> Result<GoogleReport> {
    let text = decode(bytes);
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| ImportError::Other(e.to_string()))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    let c_date = require(&headers, &["Transaction Date"])?;
    let c_type = require(&headers, &["Transaction Type"])?;
    let c_amount = require(&headers, &["Amount (Merchant Currency)"])?;
    let c_cur = require(&headers, &["Merchant Currency"])?;
    let c_product = find(&headers, &["Product Title", "Product id"]);

    let mut types: BTreeMap<String, (usize, i64)> = BTreeMap::new();
    let mut products: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let (mut start, mut end, mut currency): (Option<NaiveDate>, Option<NaiveDate>, Option<String>) = (None, None, None);
    let mut net = 0i64;
    let mut rows = 0;
    for (i, rec) in rdr.records().enumerate() {
        let n = i + 2;
        let rec = rec.map_err(|e| ImportError::Row {
            line: n,
            message: e.to_string(),
        })?;
        let get = |c: usize| rec.get(c).unwrap_or("").trim();
        if get(c_type).is_empty() && get(c_amount).is_empty() {
            continue;
        }
        let err = |m: &str| ImportError::Row {
            line: n,
            message: m.into(),
        };
        let d = date(get(c_date), true).ok_or_else(|| err("ongeldige transactiedatum"))?;
        let amount = amount_cents(get(c_amount)).ok_or_else(|| err("ongeldig bedrag"))?;
        let cur = get(c_cur).to_uppercase();
        match &currency {
            None => currency = Some(cur.clone()),
            Some(c) if *c != cur => return Err(err("meerdere uitbetalingsvaluta's in één rapport")),
            _ => {}
        }
        start = Some(start.map_or(d, |x| x.min(d)));
        end = Some(end.map_or(d, |x| x.max(d)));
        let t = types.entry(get(c_type).to_string()).or_default();
        t.0 += 1;
        t.1 += amount;
        if let Some(c) = c_product {
            let p = products.entry(get(c).to_string()).or_default();
            if get(c_type) == "Charge" {
                p.0 += 1;
            }
            p.1 += amount;
        }
        net += amount;
        rows += 1;
    }
    let (period_start, period_end, merchant_currency) = match (start, end, currency) {
        (Some(s), Some(e), Some(c)) => (s, e, c),
        _ => return Err(ImportError::Empty),
    };
    Ok(GoogleReport {
        merchant_currency,
        period_start,
        period_end,
        by_type: types
            .into_iter()
            .map(|(transaction_type, (rows, amount_cents))| GoogleTypeTotal {
                transaction_type,
                rows,
                amount_cents,
            })
            .collect(),
        products: products
            .into_iter()
            .map(|(product, (charges, amount_cents))| GoogleProductTotal {
                product,
                charges,
                amount_cents,
            })
            .collect(),
        net_cents: net,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = "Description,Transaction Date,Transaction Time,Tax Type,Transaction Type,Refund Type,Product Title,Product id,Product Type,Sku Id,Hardware,Buyer Country,Buyer State,Buyer Postal Code,Buyer Currency,Amount (Buyer Currency),Currency Conversion Rate,Merchant Currency,Amount (Merchant Currency)
GPA.0000-0000-0000-00001,\"Oct 1, 2026\",10:00:00 PDT,,Charge,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,NL,,,EUR,4.99,1.000000,EUR,4.12
GPA.0000-0000-0000-00001,\"Oct 1, 2026\",10:00:00 PDT,,Google fee,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,NL,,,EUR,-0.62,1.000000,EUR,-0.62
GPA.0000-0000-0000-00002,\"Oct 20, 2026\",11:00:00 PDT,,Charge,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,US,CA,,USD,4.99,0.920000,EUR,4.59
GPA.0000-0000-0000-00002,\"Oct 20, 2026\",11:00:00 PDT,,Google fee,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,US,CA,,USD,-0.75,0.920000,EUR,-0.69
GPA.0000-0000-0000-00001,\"Oct 25, 2026\",09:00:00 PDT,,Charge refund,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,NL,,,EUR,-4.99,1.000000,EUR,-4.12
GPA.0000-0000-0000-00001,\"Oct 25, 2026\",09:00:00 PDT,,Google fee refund,,Voorbeeld App (nl.voorbeeld.app),nl.voorbeeld.app,inapp,pro,phone,NL,,,EUR,0.62,1.000000,EUR,0.62
";

    #[test]
    fn sums_net_payout_and_types() {
        let r = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(r.merchant_currency, "EUR");
        assert_eq!(r.net_cents, 412 - 62 + 459 - 69 - 412 + 62);
        assert_eq!(r.rows, 6);
        assert_eq!(r.period_start, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        assert_eq!(r.period_end, NaiveDate::from_ymd_opt(2026, 10, 25).unwrap());
        let fee = r.by_type.iter().find(|t| t.transaction_type == "Google fee").unwrap();
        assert_eq!(fee.amount_cents, -131);
        assert_eq!(r.products[0].charges, 2);
    }

    #[test]
    fn utf16_file_is_accepted() {
        let bytes: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(SAMPLE.encode_utf16().flat_map(|u| u.to_le_bytes()))
            .collect();
        assert_eq!(parse(&bytes).unwrap().rows, 6);
    }

    #[test]
    fn missing_columns_are_reported() {
        assert!(matches!(
            parse(b"Description,Amount\nx,1"),
            Err(ImportError::MissingColumn(_))
        ));
    }
}
