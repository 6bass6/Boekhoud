//! Income-tax estimate (IB-prognose) for an entrepreneur under AOW age.
//!
//! Pure integer arithmetic; the yearly parameters are data (table `tax_parameter`) and are passed in.
//! Amounts are cents, rates micro-units (35,75% = 357_500). Every multiplication rounds half away
//! from zero to whole cents. See `docs/accounting-rules.md` §IB-prognose for the calculation order.

use serde::Serialize;

use crate::money::div_round_half_away;

/// Box 1 bracket: `rate_micro` applies to income up to `upto_cents` (None = no upper limit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bracket {
    pub upto_cents: Option<i64>,
    pub rate_micro: i64,
}

/// One segment of a tax credit (heffingskorting): for income above the previous segment's limit and up
/// to `upto_cents`, the credit is `base_cents + rate_micro × (income − previous limit)`. Negative rates
/// phase the credit out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreditSegment {
    pub upto_cents: Option<i64>,
    pub base_cents: i64,
    pub rate_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeTaxParams {
    pub brackets: Vec<Bracket>,
    pub zelfstandigenaftrek_cents: i64,
    pub startersaftrek_cents: i64,
    pub mkb_rate_micro: i64,
    /// Maximum rate at which ondernemersaftrek + mkb-winstvrijstelling reduce tax (37,56% in 2026).
    pub deduction_rate_cap_micro: i64,
    pub algemene_heffingskorting: Vec<CreditSegment>,
    pub arbeidskorting: Vec<CreditSegment>,
    pub zvw_rate_micro: i64,
    pub zvw_max_income_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IncomeTaxInput {
    /// Accounting profit of the year (treated as fiscal profit).
    pub profit_cents: i64,
    /// Urencriterium met (≥ 1,225 hours).
    pub hours_met: bool,
    /// Entitled to startersaftrek (owner's statement).
    pub starter: bool,
    /// Salary from employment (loon) for the year.
    pub salary_cents: i64,
    /// Loonheffing already withheld on that salary.
    pub withheld_cents: i64,
    /// Months left in the year including the current one; 0 = year is over (no reserve advice).
    pub remaining_months: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BracketLine {
    pub upto_cents: Option<i64>,
    pub rate_micro: i64,
    pub income_cents: i64,
    pub tax_cents: i64,
}

/// Every intermediate step, so the UI can show the calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeTaxEstimate {
    pub profit_cents: i64,
    pub zelfstandigenaftrek_cents: i64,
    pub startersaftrek_cents: i64,
    pub ondernemersaftrek_cents: i64,
    pub mkb_vrijstelling_cents: i64,
    pub taxable_profit_cents: i64,
    pub salary_cents: i64,
    pub box1_income_cents: i64,
    pub brackets: Vec<BracketLine>,
    pub bracket_tax_cents: i64,
    /// Deductions that fell in the top bracket, added back at (top rate − cap rate).
    pub rate_cap_base_cents: i64,
    pub rate_cap_cents: i64,
    pub tax_cents: i64,
    pub algemene_heffingskorting_cents: i64,
    pub arbeidskorting_cents: i64,
    /// Sum of both credits, limited to `tax_cents`.
    pub credits_applied_cents: i64,
    pub tax_after_credits_cents: i64,
    pub zvw_base_cents: i64,
    pub zvw_cents: i64,
    pub total_cents: i64,
    pub withheld_cents: i64,
    /// Negative = expected refund.
    pub to_pay_cents: i64,
    /// `to_pay ÷ remaining months`, None when the year is over or nothing is to be paid.
    pub monthly_reserve_cents: Option<i64>,
}

fn apply_rate(cents: i64, rate_micro: i64) -> i64 {
    div_round_half_away(cents as i128 * rate_micro as i128, 1_000_000) as i64
}

fn credit(segments: &[CreditSegment], income: i64) -> i64 {
    let income = income.max(0);
    let mut lower = 0;
    for s in segments {
        if s.upto_cents.is_none_or(|u| income <= u) {
            return (s.base_cents + apply_rate(income - lower, s.rate_micro)).max(0);
        }
        lower = s.upto_cents.unwrap_or(lower);
    }
    0
}

pub fn estimate(p: &IncomeTaxParams, i: IncomeTaxInput) -> IncomeTaxEstimate {
    // §2.2 Ondernemersaftrek. Startersaftrek raises the zelfstandigenaftrek, so it needs the urencriterium
    // too; with startersaftrek the deduction is not capped at profit (it may create a loss).
    let starter = i.hours_met && i.starter;
    let zsa = match (i.hours_met, starter) {
        (false, _) => 0,
        (true, true) => p.zelfstandigenaftrek_cents,
        (true, false) => p.zelfstandigenaftrek_cents.min(i.profit_cents.max(0)),
    };
    let sa = if starter { p.startersaftrek_cents } else { 0 };
    let oa = zsa + sa;
    // §2.3 Mkb-winstvrijstelling; on a loss it is negative (reduces the loss).
    let mkb = apply_rate(i.profit_cents - oa, p.mkb_rate_micro);
    let taxable_profit = i.profit_cents - oa - mkb;
    // §2.4 Box 1. shortcut: a negative box 1 income is taxed as 0 (no loss carry-back/forward).
    let box1 = (taxable_profit + i.salary_cents).max(0);
    let mut lower = 0;
    let brackets: Vec<BracketLine> = p
        .brackets
        .iter()
        .map(|b| {
            let top = b.upto_cents.map_or(box1, |u| u.min(box1));
            let income = (top - lower).max(0);
            lower = b.upto_cents.unwrap_or(lower).max(lower);
            BracketLine {
                upto_cents: b.upto_cents,
                rate_micro: b.rate_micro,
                income_cents: income,
                tax_cents: apply_rate(income, b.rate_micro),
            }
        })
        .collect();
    let bracket_tax: i64 = brackets.iter().map(|b| b.tax_cents).sum();
    // §2.5 Rate cap: deductions that would otherwise have been taxed in the top bracket.
    let top = p.brackets.last();
    let top_lower = p.brackets.iter().rev().nth(1).and_then(|b| b.upto_cents).unwrap_or(0);
    let deductions = (oa + mkb).max(0);
    let cap_base = deductions.min((box1 + deductions - top_lower).max(0));
    let cap_rate = top.map_or(0, |b| (b.rate_micro - p.deduction_rate_cap_micro).max(0));
    let rate_cap = apply_rate(cap_base, cap_rate);
    let tax = bracket_tax + rate_cap;
    // §2.6 Heffingskortingen, together at most the tax.
    let ahk = credit(&p.algemene_heffingskorting, box1);
    let ak = credit(&p.arbeidskorting, i.salary_cents + taxable_profit);
    let credits = (ahk + ak).min(tax);
    let tax_after = tax - credits;
    // §2.7 Zvw on profit, up to the maximum income minus salary (salary already bears Zvw via employer).
    let zvw_base = taxable_profit.min(p.zvw_max_income_cents - i.salary_cents).max(0);
    let zvw = apply_rate(zvw_base, p.zvw_rate_micro);
    let total = tax_after + zvw;
    // §2.8
    let to_pay = total - i.withheld_cents;
    let reserve = (i.remaining_months > 0 && to_pay > 0)
        .then(|| div_round_half_away(to_pay as i128, i.remaining_months as i128) as i64);
    IncomeTaxEstimate {
        profit_cents: i.profit_cents,
        zelfstandigenaftrek_cents: zsa,
        startersaftrek_cents: sa,
        ondernemersaftrek_cents: oa,
        mkb_vrijstelling_cents: mkb,
        taxable_profit_cents: taxable_profit,
        salary_cents: i.salary_cents,
        box1_income_cents: box1,
        brackets,
        bracket_tax_cents: bracket_tax,
        rate_cap_base_cents: cap_base,
        rate_cap_cents: rate_cap,
        tax_cents: tax,
        algemene_heffingskorting_cents: ahk,
        arbeidskorting_cents: ak,
        credits_applied_cents: credits,
        tax_after_credits_cents: tax_after,
        zvw_base_cents: zvw_base,
        zvw_cents: zvw,
        total_cents: total,
        withheld_cents: i.withheld_cents,
        to_pay_cents: to_pay,
        monthly_reserve_cents: reserve,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn seg(upto: Option<i64>, base: i64, rate: i64) -> CreditSegment {
        CreditSegment {
            upto_cents: upto,
            base_cents: base,
            rate_micro: rate,
        }
    }

    /// The 2026 values from migration 0008 (belastingdienst.nl, verified 2026-11-12).
    fn params_2026() -> IncomeTaxParams {
        IncomeTaxParams {
            brackets: vec![
                Bracket {
                    upto_cents: Some(3_888_300),
                    rate_micro: 357_500,
                },
                Bracket {
                    upto_cents: Some(7_842_600),
                    rate_micro: 375_600,
                },
                Bracket {
                    upto_cents: None,
                    rate_micro: 495_000,
                },
            ],
            zelfstandigenaftrek_cents: 120_000,
            startersaftrek_cents: 212_300,
            mkb_rate_micro: 127_000,
            deduction_rate_cap_micro: 375_600,
            algemene_heffingskorting: vec![
                seg(Some(2_973_600), 311_500, 0),
                seg(Some(7_842_600), 311_500, -63_980),
                seg(None, 0, 0),
            ],
            arbeidskorting: vec![
                seg(Some(1_196_500), 0, 83_240),
                seg(Some(2_584_500), 99_600, 310_090),
                seg(Some(4_559_200), 530_000, 19_500),
                seg(Some(13_292_000), 568_500, -65_100),
                seg(None, 0, 0),
            ],
            zvw_rate_micro: 48_500,
            zvw_max_income_cents: 7_940_900,
        }
    }

    #[test]
    fn starter_with_hours_in_bracket_1() {
        // Profit 30.000,00, urencriterium met, starter, no salary, 3 months left.
        // Ondernemersaftrek     1.200,00 + 2.123,00                     = 3.323,00
        // Mkb  12,7% × (30.000,00 − 3.323,00) = 12,7% × 26.677,00 = 3.387,979 → 3.387,98
        // Taxable profit  30.000,00 − 3.323,00 − 3.387,98               = 23.289,02 = box 1 income
        // Bracket 1  35,75% × 23.289,02 = 8.325,82465                    → 8.325,82
        // Rate cap: box 1 below bracket 3                                = 0
        // AHK: 23.289,02 ≤ 29.736                                        = 3.115,00
        // AK: 996 + 31,009% × (23.289,02 − 11.965) = 996 + 3.511,4654.. → 4.507,47
        // Credits 7.622,47 < tax 8.325,82 → tax after credits             = 703,35
        // Zvw  4,85% × 23.289,02 = 1.129,51747                           → 1.129,52
        // Total 1.832,87; to pay 1.832,87; reserve 1.832,87 / 3 = 610,956.. → 610,96
        let e = estimate(
            &params_2026(),
            IncomeTaxInput {
                profit_cents: 3_000_000,
                hours_met: true,
                starter: true,
                remaining_months: 3,
                ..Default::default()
            },
        );
        assert_eq!(
            (
                e.zelfstandigenaftrek_cents,
                e.startersaftrek_cents,
                e.ondernemersaftrek_cents
            ),
            (120_000, 212_300, 332_300)
        );
        assert_eq!(e.mkb_vrijstelling_cents, 338_798);
        assert_eq!((e.taxable_profit_cents, e.box1_income_cents), (2_328_902, 2_328_902));
        assert_eq!(e.brackets[0].tax_cents, 832_582);
        assert_eq!((e.brackets[1].income_cents, e.brackets[2].income_cents), (0, 0));
        assert_eq!((e.rate_cap_cents, e.tax_cents), (0, 832_582));
        assert_eq!(
            (e.algemene_heffingskorting_cents, e.arbeidskorting_cents),
            (311_500, 450_747)
        );
        assert_eq!((e.credits_applied_cents, e.tax_after_credits_cents), (762_247, 70_335));
        assert_eq!(e.zvw_cents, 112_952);
        assert_eq!(
            (e.total_cents, e.to_pay_cents, e.monthly_reserve_cents),
            (183_287, 183_287, Some(61_096))
        );
    }

    #[test]
    fn credits_capped_at_tax() {
        // Profit 10.000,00, hours met, no starter, no salary, year over.
        // OA 1.200,00; mkb 12,7% × 8.800,00 = 1.117,60; taxable 7.682,40
        // Tax 35,75% × 7.682,40 = 2.746,458 → 2.746,46
        // AHK 3.115,00 + AK 8,324% × 7.682,40 = 639,4829.. → 639,48 → 3.754,48 > tax → credits 2.746,46, tax 0
        // Zvw 4,85% × 7.682,40 = 372,5964 → 372,60 = total; no reserve (remaining months 0)
        let e = estimate(
            &params_2026(),
            IncomeTaxInput {
                profit_cents: 1_000_000,
                hours_met: true,
                ..Default::default()
            },
        );
        assert_eq!(
            (e.mkb_vrijstelling_cents, e.taxable_profit_cents, e.tax_cents),
            (111_760, 768_240, 274_646)
        );
        assert_eq!(
            (e.algemene_heffingskorting_cents, e.arbeidskorting_cents),
            (311_500, 63_948)
        );
        assert_eq!((e.credits_applied_cents, e.tax_after_credits_cents), (274_646, 0));
        assert_eq!(
            (e.zvw_cents, e.to_pay_cents, e.monthly_reserve_cents),
            (37_260, 37_260, None)
        );
    }

    #[test]
    fn bracket_3_rate_cap_and_zvw_cap() {
        // Profit 100.000,00 + salary 30.000,00 (withheld 8.000,00), hours met, no starter, 12 months left.
        // OA 1.200,00; mkb 12,7% × 98.800,00 = 12.547,60; taxable profit 86.252,40
        // Box 1 = 86.252,40 + 30.000,00 = 116.252,40
        // Bracket 1  35,75% × 38.883,00 = 13.900,6725 → 13.900,67
        // Bracket 2  37,56% × 39.543,00 = 14.852,3508 → 14.852,35
        // Bracket 3  49,50% × 37.826,40 = 18.724,068  → 18.724,07   (sum 47.477,09)
        // Rate cap: deductions 13.747,60; income before them 130.000,00 is 51.574,00 above 78.426
        //   → all 13.747,60 × (49,50% − 37,56% = 11,94%) = 1.641,46344 → 1.641,46; tax 49.118,55
        // AHK: box 1 ≥ 78.426 → 0
        // AK: 5.685 − 6,51% × (116.252,40 − 45.592) = 5.685 − 4.599,99204 → 5.685,00 − 4.599,99 = 1.085,01
        // Tax after credits 49.118,55 − 1.085,01 = 48.033,54
        // Zvw base min(86.252,40; 79.409 − 30.000 = 49.409,00) = 49.409,00 × 4,85% = 2.396,3365 → 2.396,34
        // Total 50.429,88; to pay 50.429,88 − 8.000,00 = 42.429,88; reserve /12 = 3.535,8233 → 3.535,82
        let e = estimate(
            &params_2026(),
            IncomeTaxInput {
                profit_cents: 10_000_000,
                hours_met: true,
                starter: false,
                salary_cents: 3_000_000,
                withheld_cents: 800_000,
                remaining_months: 12,
            },
        );
        assert_eq!(
            (e.mkb_vrijstelling_cents, e.taxable_profit_cents, e.box1_income_cents),
            (1_254_760, 8_625_240, 11_625_240)
        );
        let b: Vec<_> = e.brackets.iter().map(|b| (b.income_cents, b.tax_cents)).collect();
        assert_eq!(
            b,
            vec![(3_888_300, 1_390_067), (3_954_300, 1_485_235), (3_782_640, 1_872_407)]
        );
        assert_eq!(
            (e.bracket_tax_cents, e.rate_cap_base_cents, e.rate_cap_cents),
            (4_747_709, 1_374_760, 164_146)
        );
        assert_eq!(e.tax_cents, 4_911_855);
        assert_eq!((e.algemene_heffingskorting_cents, e.arbeidskorting_cents), (0, 108_501));
        assert_eq!(e.tax_after_credits_cents, 4_803_354);
        assert_eq!((e.zvw_base_cents, e.zvw_cents), (4_940_900, 239_634));
        assert_eq!(
            (e.total_cents, e.to_pay_cents, e.monthly_reserve_cents),
            (5_042_988, 4_242_988, Some(353_582))
        );
    }

    #[test]
    fn zelfstandigenaftrek_capped_at_profit_without_starter_and_needs_hours() {
        let p = params_2026();
        let small = estimate(
            &p,
            IncomeTaxInput {
                profit_cents: 80_000,
                hours_met: true,
                ..Default::default()
            },
        );
        assert_eq!(
            (
                small.ondernemersaftrek_cents,
                small.mkb_vrijstelling_cents,
                small.taxable_profit_cents
            ),
            (80_000, 0, 0)
        );
        // Starter: not capped → loss of 3.323,00 − 800,00 = −2.523,00; mkb −320,42 (12,7% of the loss)
        let starter = estimate(
            &p,
            IncomeTaxInput {
                profit_cents: 80_000,
                hours_met: true,
                starter: true,
                ..Default::default()
            },
        );
        assert_eq!(
            (starter.mkb_vrijstelling_cents, starter.taxable_profit_cents),
            (-32_042, -220_258)
        );
        assert_eq!((starter.box1_income_cents, starter.total_cents), (0, 0));
        // No hours: no ondernemersaftrek at all, starter box ignored.
        let none = estimate(
            &p,
            IncomeTaxInput {
                profit_cents: 80_000,
                starter: true,
                ..Default::default()
            },
        );
        assert_eq!((none.ondernemersaftrek_cents, none.mkb_vrijstelling_cents), (0, 10_160));
    }
}
