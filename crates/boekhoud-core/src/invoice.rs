//! Invoice arithmetic and the journal lines an invoice produces. Pure functions, no I/O.
//!
//! VAT on sales invoices is calculated per VAT code over the summed line totals and rounded
//! half away from zero (see docs/accounting-rules.md §4).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::journal::EntryLine;
use crate::money::{div_round_half_away, Money};

/// What a VAT code means for posting. Built from the `tax_code` table valid on the invoice date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatRule {
    pub code: String,
    pub rate_bp: Option<i64>,
    pub payable_account: Option<String>,
    pub deductible_account: Option<String>,
    pub reverse_charge: bool,
}

impl VatRule {
    /// VAT charged to the customer on a sales invoice (zero for reverse charge, KOR, exempt).
    fn charges_vat(&self) -> bool {
        !self.reverse_charge && self.payable_account.is_some() && self.rate_bp.unwrap_or(0) > 0
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InvoiceError {
    #[error("invoice has no lines")]
    NoLines,
    #[error("line {0}: unknown VAT code {1}")]
    UnknownVatCode(usize, String),
    #[error("line {0}: VAT code {1} is not a sales code")]
    NotSalesCode(usize, String),
    #[error("line {0}: VAT code {1} cannot carry VAT on a purchase")]
    NoDeductibleAccount(usize, String),
    #[error("line {0}: description is empty")]
    EmptyDescription(usize),
    #[error("amount overflow")]
    Overflow,
    #[error("invoice total is zero")]
    ZeroTotal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesLine {
    pub description: String,
    /// Quantity in thousandths: 1 = 1000, 1.5 = 1500. Negative on credit notes.
    pub quantity_milli: i64,
    pub unit_price_cents: i64,
    pub tax_code: String,
    pub account: String,
}

impl SalesLine {
    pub fn net_cents(&self) -> Result<i64, InvoiceError> {
        let v = div_round_half_away(
            self.quantity_milli as i128 * self.unit_price_cents as i128,
            1000,
        );
        i64::try_from(v).map_err(|_| InvoiceError::Overflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatGroup {
    pub code: String,
    pub rate_bp: Option<i64>,
    pub base_cents: i64,
    pub vat_cents: i64,
    pub reverse_charge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceTotals {
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub total_cents: i64,
    pub groups: Vec<VatGroup>,
}

fn rule<'a>(rules: &'a [VatRule], line: usize, code: &str) -> Result<&'a VatRule, InvoiceError> {
    rules
        .iter()
        .find(|r| r.code == code)
        .ok_or_else(|| InvoiceError::UnknownVatCode(line, code.to_string()))
}

pub fn sales_totals(lines: &[SalesLine], rules: &[VatRule]) -> Result<InvoiceTotals, InvoiceError> {
    if lines.is_empty() {
        return Err(InvoiceError::NoLines);
    }
    let mut bases: BTreeMap<&str, i64> = BTreeMap::new();
    for (i, l) in lines.iter().enumerate() {
        if l.description.trim().is_empty() {
            return Err(InvoiceError::EmptyDescription(i + 1));
        }
        let r = rule(rules, i + 1, &l.tax_code)?;
        if r.deductible_account.is_some() && r.payable_account.is_none() {
            return Err(InvoiceError::NotSalesCode(i + 1, l.tax_code.clone()));
        }
        let e = bases.entry(&l.tax_code).or_default();
        *e = e
            .checked_add(l.net_cents()?)
            .ok_or(InvoiceError::Overflow)?;
    }
    let mut groups = Vec::new();
    let (mut subtotal, mut vat) = (0i64, 0i64);
    for (code, base) in bases {
        let r = rule(rules, 0, code)?;
        let v = if r.charges_vat() {
            Money::eur(base)
                .apply_basis_points(r.rate_bp.unwrap_or(0))
                .map_err(|_| InvoiceError::Overflow)?
                .cents
        } else {
            0
        };
        subtotal = subtotal.checked_add(base).ok_or(InvoiceError::Overflow)?;
        vat = vat.checked_add(v).ok_or(InvoiceError::Overflow)?;
        groups.push(VatGroup {
            code: code.to_string(),
            rate_bp: r.rate_bp,
            base_cents: base,
            vat_cents: v,
            reverse_charge: r.reverse_charge,
        });
    }
    let total = subtotal.checked_add(vat).ok_or(InvoiceError::Overflow)?;
    if total == 0 && subtotal == 0 {
        return Err(InvoiceError::ZeroTotal);
    }
    Ok(InvoiceTotals {
        subtotal_cents: subtotal,
        vat_cents: vat,
        total_cents: total,
        groups,
    })
}

/// Journal lines for an issued sales invoice: debit receivable, credit revenue per account and
/// VAT code, credit VAT payable per code. Credit notes (negative totals) produce the mirror image.
pub fn sales_entry_lines(
    lines: &[SalesLine],
    rules: &[VatRule],
    receivable_account: &str,
) -> Result<Vec<EntryLine>, InvoiceError> {
    let totals = sales_totals(lines, rules)?;
    let mut out = vec![EntryLine {
        account: receivable_account.into(),
        amount: Money::eur(totals.total_cents),
        tax_code: None,
        description: None,
    }];
    let mut revenue: BTreeMap<(&str, &str), i64> = BTreeMap::new();
    for l in lines {
        *revenue.entry((&l.account, &l.tax_code)).or_default() += l.net_cents()?;
    }
    for ((account, code), net) in revenue {
        if net != 0 {
            out.push(EntryLine {
                account: account.into(),
                amount: Money::eur(-net),
                tax_code: Some(code.into()),
                description: None,
            });
        }
    }
    for g in &totals.groups {
        if g.vat_cents != 0 {
            let r = rule(rules, 0, &g.code)?;
            let acc = r
                .payable_account
                .as_deref()
                .expect("charges_vat implies payable account");
            out.push(EntryLine {
                account: acc.into(),
                amount: Money::eur(-g.vat_cents),
                tax_code: Some(g.code.clone()),
                description: None,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseLine {
    pub description: Option<String>,
    pub account: String,
    pub net_cents: i64,
    pub tax_code: String,
    /// VAT as stated on the supplier's invoice (or the self-assessed VAT for reverse charge).
    pub vat_cents: i64,
}

/// Suggested VAT for a purchase line (the user may adjust it to match the supplier's invoice).
pub fn purchase_vat_suggestion(net_cents: i64, rule: &VatRule) -> i64 {
    match (rule.deductible_account.is_some(), rule.rate_bp) {
        (true, Some(bp)) => Money::eur(net_cents)
            .apply_basis_points(bp)
            .map(|m| m.cents)
            .unwrap_or(0),
        _ => 0,
    }
}

/// Totals of a purchase invoice: what is owed to the supplier excludes reverse-charged VAT.
pub fn purchase_totals(
    lines: &[PurchaseLine],
    rules: &[VatRule],
) -> Result<(i64, i64, i64), InvoiceError> {
    if lines.is_empty() {
        return Err(InvoiceError::NoLines);
    }
    let (mut net, mut vat_owed_to_supplier) = (0i64, 0i64);
    for (i, l) in lines.iter().enumerate() {
        let r = rule(rules, i + 1, &l.tax_code)?;
        if l.vat_cents != 0 && r.deductible_account.is_none() {
            return Err(InvoiceError::NoDeductibleAccount(i + 1, l.tax_code.clone()));
        }
        net = net.checked_add(l.net_cents).ok_or(InvoiceError::Overflow)?;
        if !r.reverse_charge {
            vat_owed_to_supplier = vat_owed_to_supplier
                .checked_add(l.vat_cents)
                .ok_or(InvoiceError::Overflow)?;
        }
    }
    let total = net
        .checked_add(vat_owed_to_supplier)
        .ok_or(InvoiceError::Overflow)?;
    if total == 0 && net == 0 {
        return Err(InvoiceError::ZeroTotal);
    }
    Ok((net, vat_owed_to_supplier, total))
}

/// Journal lines for a purchase invoice: debit expense per line, debit deductible VAT,
/// credit self-assessed VAT for reverse charge, credit the payable account for the total.
pub fn purchase_entry_lines(
    lines: &[PurchaseLine],
    rules: &[VatRule],
    payable_account: &str,
) -> Result<Vec<EntryLine>, InvoiceError> {
    let (_, _, total) = purchase_totals(lines, rules)?;
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let r = rule(rules, i + 1, &l.tax_code)?;
        if l.net_cents != 0 {
            out.push(EntryLine {
                account: l.account.clone(),
                amount: Money::eur(l.net_cents),
                tax_code: Some(l.tax_code.clone()),
                description: l.description.clone(),
            });
        }
        if l.vat_cents != 0 {
            let ded = r
                .deductible_account
                .as_deref()
                .ok_or_else(|| InvoiceError::NoDeductibleAccount(i + 1, l.tax_code.clone()))?;
            out.push(EntryLine {
                account: ded.into(),
                amount: Money::eur(l.vat_cents),
                tax_code: Some(l.tax_code.clone()),
                description: None,
            });
            if r.reverse_charge {
                let pay = r
                    .payable_account
                    .as_deref()
                    .ok_or_else(|| InvoiceError::NoDeductibleAccount(i + 1, l.tax_code.clone()))?;
                out.push(EntryLine {
                    account: pay.into(),
                    amount: Money::eur(-l.vat_cents),
                    tax_code: Some(l.tax_code.clone()),
                    description: None,
                });
            }
        }
    }
    out.push(EntryLine {
        account: payable_account.into(),
        amount: Money::eur(-total),
        tax_code: None,
        description: None,
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn rules() -> Vec<VatRule> {
        let r =
            |code: &str, bp: Option<i64>, pay: Option<&str>, ded: Option<&str>, rc: bool| VatRule {
                code: code.into(),
                rate_bp: bp,
                payable_account: pay.map(Into::into),
                deductible_account: ded.map(Into::into),
                reverse_charge: rc,
            };
        vec![
            r("V21", Some(2100), Some("1510"), None, false),
            r("V9", Some(900), Some("1511"), None, false),
            r("V0EU", None, None, None, true),
            r("VKOR", None, None, None, false),
            r("I21", Some(2100), None, Some("1500"), false),
            r("IEU", Some(2100), Some("1515"), Some("1500"), true),
            r("GEEN", None, None, None, false),
        ]
    }

    fn line(desc: &str, qty: i64, price: i64, code: &str) -> SalesLine {
        SalesLine {
            description: desc.into(),
            quantity_milli: qty,
            unit_price_cents: price,
            tax_code: code.into(),
            account: "8100".into(),
        }
    }

    fn sum(lines: &[EntryLine]) -> i64 {
        lines.iter().map(|l| l.amount.cents).sum()
    }

    #[test]
    fn talk_invoice_21_percent() {
        let lines = [
            line("Lezing", 1000, 100_000, "V21"),
            line("Reiskosten", 120_500, 23, "V21"),
        ];
        // 120.5 km * 0.23 = 27.715 -> 27.72
        assert_eq!(lines[1].net_cents().unwrap(), 2_772);
        let t = sales_totals(&lines, &rules()).unwrap();
        assert_eq!(t.subtotal_cents, 102_772);
        assert_eq!(t.vat_cents, 21_582); // 21% of 1027.72 = 215.8212
        assert_eq!(t.total_cents, 124_354);
        let e = sales_entry_lines(&lines, &rules(), "1300").unwrap();
        assert_eq!(sum(&e), 0);
        assert_eq!(e[0].amount.cents, 124_354);
        assert!(e.iter().any(|l| l.account == "1510"
            && l.amount.cents == -21_582
            && l.tax_code.as_deref() == Some("V21")));
    }

    #[test]
    fn reverse_charge_and_kor_carry_no_vat() {
        for code in ["V0EU", "VKOR"] {
            let t = sales_totals(&[line("Lezing", 1000, 50_000, code)], &rules()).unwrap();
            assert_eq!((t.vat_cents, t.total_cents), (0, 50_000));
            let e =
                sales_entry_lines(&[line("Lezing", 1000, 50_000, code)], &rules(), "1300").unwrap();
            assert_eq!(e.len(), 2);
        }
    }

    #[test]
    fn credit_note_mirrors_invoice() {
        let e = sales_entry_lines(
            &[line("Creditering lezing", -1000, 100_000, "V21")],
            &rules(),
            "1300",
        )
        .unwrap();
        assert_eq!(e[0].amount.cents, -121_000);
        assert_eq!(sum(&e), 0);
    }

    #[test]
    fn invoice_errors() {
        assert_eq!(sales_totals(&[], &rules()), Err(InvoiceError::NoLines));
        assert!(matches!(
            sales_totals(&[line("x", 1000, 1, "ZZ")], &rules()),
            Err(InvoiceError::UnknownVatCode(1, _))
        ));
        assert!(matches!(
            sales_totals(&[line("x", 1000, 1, "I21")], &rules()),
            Err(InvoiceError::NotSalesCode(1, _))
        ));
        assert!(matches!(
            sales_totals(&[line(" ", 1000, 1, "V21")], &rules()),
            Err(InvoiceError::EmptyDescription(1))
        ));
    }

    #[test]
    fn purchase_with_reverse_charge() {
        // Scenario S6 + S7 on one invoice.
        let lines = [
            PurchaseLine {
                description: None,
                account: "4000".into(),
                net_cents: 10_000,
                tax_code: "I21".into(),
                vat_cents: 2_100,
            },
            PurchaseLine {
                description: None,
                account: "4010".into(),
                net_cents: 10_000,
                tax_code: "IEU".into(),
                vat_cents: 2_100,
            },
        ];
        assert_eq!(
            purchase_totals(&lines, &rules()).unwrap(),
            (20_000, 2_100, 22_100)
        );
        let e = purchase_entry_lines(&lines, &rules(), "1600").unwrap();
        assert_eq!(sum(&e), 0);
        assert!(e
            .iter()
            .any(|l| l.account == "1515" && l.amount.cents == -2_100));
        assert_eq!(
            e.iter()
                .filter(|l| l.account == "1500")
                .map(|l| l.amount.cents)
                .sum::<i64>(),
            4_200
        );
        assert_eq!(e.last().unwrap().amount.cents, -22_100);
    }

    #[test]
    fn purchase_vat_on_code_without_deduction_is_rejected() {
        let lines = [PurchaseLine {
            description: None,
            account: "4300".into(),
            net_cents: 1_000,
            tax_code: "GEEN".into(),
            vat_cents: 210,
        }];
        assert!(matches!(
            purchase_totals(&lines, &rules()),
            Err(InvoiceError::NoDeductibleAccount(1, _))
        ));
        assert_eq!(purchase_vat_suggestion(10_000, &rules()[4]), 2_100);
        assert_eq!(purchase_vat_suggestion(10_000, &rules()[6]), 0);
    }
}
