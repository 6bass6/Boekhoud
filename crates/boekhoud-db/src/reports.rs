//! Per-year financial reports (P&L, balance sheet) and Dutch CSV export.
//!
//! There is no year-end closing yet, so revenue/expense accounts are never closed to equity.
//! These reports compensate by computation only: the P&L takes just the year's movements, and the
//! balance sheet shows the result of earlier years and of the current year as computed equity
//! lines. Nothing is posted.

use chrono::{Datelike, NaiveDate};
use rusqlite::params;
use serde::Serialize;

use boekhoud_core::AccountType;

use crate::{DbError, Ledger, Result, TrialBalanceRow};

/// One account on a report. `amount_cents` uses the natural sign of its section: revenue,
/// liabilities and equity are credit-positive; expenses and assets are debit-positive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportLine {
    pub account: String,
    pub name: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfitAndLoss {
    pub from: NaiveDate,
    pub until: NaiveDate,
    pub revenue: Vec<ReportLine>,
    pub expenses: Vec<ReportLine>,
    pub revenue_cents: i64,
    pub expenses_cents: i64,
    pub result_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BalanceSheet {
    pub until: NaiveDate,
    pub assets: Vec<ReportLine>,
    pub liabilities: Vec<ReportLine>,
    /// Equity accounts as booked (capital, private deposits/withdrawals).
    pub equity: Vec<ReportLine>,
    /// Computed, not posted: result of all fiscal years before `until`'s year.
    pub prior_years_result_cents: i64,
    /// Computed, not posted: result from 1 January of `until`'s year up to `until`.
    pub current_year_result_cents: i64,
    pub assets_cents: i64,
    pub liabilities_cents: i64,
    /// Equity accounts plus both computed result lines.
    pub equity_cents: i64,
}

impl Ledger {
    /// Sum (debit − credit) per account over posted lines with `from <= entry_date <= until`.
    fn sums_by_account(&self, from: &str, until: &str) -> Result<Vec<AccountSum>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.number, a.name, a.account_type, SUM(l.amount_cents)
             FROM account a
             JOIN journal_line l ON l.account = a.number
             JOIN journal_entry e ON e.id = l.entry_id
             WHERE e.status = 'posted' AND e.entry_date BETWEEN ?1 AND ?2
             GROUP BY a.number ORDER BY a.number",
        )?;
        let rows = stmt.query_map(params![from, until], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (number, name, t, sum) = row?;
            out.push((number, name, t.parse().map_err(DbError::Corrupt)?, sum));
        }
        Ok(out)
    }

    /// Winst-en-verliesrekening from 1 January of `until`'s year up to and including `until`.
    pub fn profit_and_loss(&self, until: NaiveDate) -> Result<ProfitAndLoss> {
        let from = NaiveDate::from_ymd_opt(until.year(), 1, 1).expect("1 January exists");
        let sums = self.sums_by_account(&ymd(from), &ymd(until))?;
        let revenue = section(&sums, AccountType::Revenue, -1);
        let expenses = section(&sums, AccountType::Expense, 1);
        let revenue_cents = total(&revenue);
        let expenses_cents = total(&expenses);
        Ok(ProfitAndLoss {
            from,
            until,
            revenue,
            expenses,
            revenue_cents,
            expenses_cents,
            result_cents: revenue_cents - expenses_cents,
        })
    }

    /// Balans at `until`: balance-sheet accounts cumulative over all years, plus computed result lines.
    pub fn balance_sheet(&self, until: NaiveDate) -> Result<BalanceSheet> {
        let sums = self.sums_by_account("0000-01-01", &ymd(until))?;
        let assets = section(&sums, AccountType::Asset, 1);
        let liabilities = section(&sums, AccountType::Liability, -1);
        let equity = section(&sums, AccountType::Equity, -1);
        // Result = credit-positive sum over revenue and expense accounts.
        let all_years_result: i64 = -sums
            .iter()
            .filter(|s| !s.2.is_balance_sheet())
            .map(|s| s.3)
            .sum::<i64>();
        let current_year_result = self.profit_and_loss(until)?.result_cents;
        let prior_years_result = all_years_result - current_year_result;
        let (assets_cents, liabilities_cents) = (total(&assets), total(&liabilities));
        Ok(BalanceSheet {
            until,
            equity_cents: total(&equity) + prior_years_result + current_year_result,
            assets,
            liabilities,
            equity,
            prior_years_result_cents: prior_years_result,
            current_year_result_cents: current_year_result,
            assets_cents,
            liabilities_cents,
        })
    }
}

type AccountSum = (String, String, AccountType, i64);

/// Report lines of one account type, with `sign` turning debit − credit into the section's natural sign.
fn section(sums: &[AccountSum], t: AccountType, sign: i64) -> Vec<ReportLine> {
    sums.iter()
        .filter(|s| s.2 == t && s.3 != 0)
        .map(|s| ReportLine {
            account: s.0.clone(),
            name: s.1.clone(),
            amount_cents: sign * s.3,
        })
        .collect()
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn total(lines: &[ReportLine]) -> i64 {
    lines.iter().map(|l| l.amount_cents).sum()
}

// ---- CSV (semicolon-separated, comma decimal, UTF-8 with BOM so Excel opens it correctly) ----

/// 123456 -> "1234,56"; -5 -> "-0,05". No thousands separator so Excel reads it as a number.
pub fn csv_amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{},{:02}", abs / 100, abs % 100)
}

/// Account number as Excel text: "0100" would otherwise be read as the number 100.
fn csv_account(a: &str) -> String {
    if a.starts_with('0') {
        format!("=\"{a}\"")
    } else {
        a.to_string()
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv(rows: &[Vec<String>]) -> String {
    let mut out = String::from("\u{feff}");
    for row in rows {
        let fields: Vec<String> = row.iter().map(|f| csv_field(f)).collect();
        out.push_str(&fields.join(";"));
        out.push_str("\r\n");
    }
    out
}

fn nl_date(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| c.to_string()).collect()
}

fn section_rows(rows: &mut Vec<Vec<String>>, title: &str, lines: &[ReportLine], total_label: &str, sum: i64) {
    rows.push(row(&[title]));
    for l in lines {
        rows.push(vec![
            csv_account(&l.account),
            l.name.clone(),
            csv_amount(l.amount_cents),
        ]);
    }
    rows.push(vec![String::new(), total_label.into(), csv_amount(sum)]);
    rows.push(vec![]);
}

pub fn profit_and_loss_csv(p: &ProfitAndLoss) -> String {
    let mut rows = vec![
        vec![
            "Winst-en-verliesrekening".into(),
            format!("{} t/m {}", nl_date(p.from), nl_date(p.until)),
        ],
        vec![],
        row(&["Rekening", "Naam", "Bedrag"]),
    ];
    section_rows(
        &mut rows,
        "Opbrengsten",
        &p.revenue,
        "Totaal opbrengsten",
        p.revenue_cents,
    );
    section_rows(&mut rows, "Kosten", &p.expenses, "Totaal kosten", p.expenses_cents);
    rows.push(vec![String::new(), "Resultaat".into(), csv_amount(p.result_cents)]);
    csv(&rows)
}

pub fn balance_sheet_csv(b: &BalanceSheet) -> String {
    let mut rows = vec![
        vec!["Balans per".into(), nl_date(b.until)],
        vec![],
        row(&["Rekening", "Naam", "Bedrag"]),
    ];
    section_rows(&mut rows, "Activa", &b.assets, "Totaal activa", b.assets_cents);
    section_rows(
        &mut rows,
        "Vreemd vermogen",
        &b.liabilities,
        "Totaal vreemd vermogen",
        b.liabilities_cents,
    );
    let mut equity = b.equity.clone();
    equity.push(ReportLine {
        account: String::new(),
        name: "Resultaat voorgaande boekjaren (berekend)".into(),
        amount_cents: b.prior_years_result_cents,
    });
    equity.push(ReportLine {
        account: String::new(),
        name: "Resultaat lopend boekjaar (berekend)".into(),
        amount_cents: b.current_year_result_cents,
    });
    section_rows(
        &mut rows,
        "Eigen vermogen",
        &equity,
        "Totaal eigen vermogen",
        b.equity_cents,
    );
    rows.push(vec![
        String::new(),
        "Totaal passiva".into(),
        csv_amount(b.liabilities_cents + b.equity_cents),
    ]);
    csv(&rows)
}

pub fn trial_balance_csv(until: NaiveDate, tb: &[TrialBalanceRow]) -> String {
    let mut rows = vec![
        vec!["Proef- en saldibalans t/m".into(), nl_date(until)],
        vec![],
        row(&["Rekening", "Naam", "Debet", "Credit", "Saldo"]),
    ];
    for r in tb {
        rows.push(vec![
            csv_account(&r.account),
            r.name.clone(),
            csv_amount(r.debit_cents),
            csv_amount(r.credit_cents),
            csv_amount(r.balance_cents),
        ]);
    }
    let (debit, credit) = tb
        .iter()
        .fold((0, 0), |a, r| (a.0 + r.debit_cents, a.1 + r.credit_cents));
    rows.push(vec![
        String::new(),
        "Totaal".into(),
        csv_amount(debit),
        csv_amount(credit),
    ]);
    csv(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boekhoud_core::{EntryLine, JournalCode, Source};

    use crate::EntryDraft;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn post(l: &mut Ledger, journal: JournalCode, date: NaiveDate, lines: Vec<EntryLine>) {
        l.post(&EntryDraft {
            journal,
            date,
            description: "Fictief".into(),
            lines,
            source: Source::manual(),
            idempotency_key: None,
        })
        .unwrap();
    }

    /// Fictional business spanning 2025 and 2026.
    fn two_years() -> Ledger {
        let mut l = Ledger::open_in_memory().unwrap();
        use EntryLine as E;
        use JournalCode::*;
        // 2025: capital, a talk, hosting, a laptop with depreciation.
        post(
            &mut l,
            MEM,
            d(2025, 1, 2),
            vec![E::debit("1100", 500_000), E::credit("0510", 500_000)],
        );
        post(
            &mut l,
            VRK,
            d(2025, 3, 1),
            vec![
                E::debit("1300", 121_000),
                E::credit("8100", 100_000),
                E::credit("1510", 21_000),
            ],
        );
        post(
            &mut l,
            INK,
            d(2025, 4, 1),
            vec![
                E::debit("4000", 20_000),
                E::debit("1500", 4_200),
                E::credit("1600", 24_200),
            ],
        );
        post(
            &mut l,
            INK,
            d(2025, 5, 1),
            vec![E::debit("0100", 150_000), E::credit("1100", 150_000)],
        );
        post(
            &mut l,
            MEM,
            d(2025, 12, 31),
            vec![E::debit("4800", 30_000), E::credit("0110", 30_000)],
        );
        // 2026: Apple revenue, payment of the talk, a private withdrawal, bank costs.
        post(
            &mut l,
            PLT,
            d(2026, 2, 1),
            vec![E::debit("1310", 85_000), E::credit("8000", 85_000)],
        );
        post(
            &mut l,
            BNK,
            d(2026, 2, 10),
            vec![E::debit("1100", 121_000), E::credit("1300", 121_000)],
        );
        post(
            &mut l,
            BNK,
            d(2026, 3, 1),
            vec![E::debit("0520", 40_000), E::credit("1100", 40_000)],
        );
        post(
            &mut l,
            BNK,
            d(2026, 6, 30),
            vec![E::debit("4700", 1_234), E::credit("1100", 1_234)],
        );
        post(
            &mut l,
            MEM,
            d(2026, 12, 31),
            vec![E::debit("4800", 30_000), E::credit("0110", 30_000)],
        );
        l
    }

    #[test]
    fn profit_and_loss_takes_only_the_years_movements() {
        let l = two_years();
        let p25 = l.profit_and_loss(d(2025, 12, 31)).unwrap();
        assert_eq!(
            (p25.revenue_cents, p25.expenses_cents, p25.result_cents),
            (100_000, 50_000, 50_000)
        );
        let p26 = l.profit_and_loss(d(2026, 12, 31)).unwrap();
        assert_eq!(
            (p26.revenue_cents, p26.expenses_cents, p26.result_cents),
            (85_000, 31_234, 53_766)
        );
        assert_eq!(
            p26.revenue.iter().map(|r| r.account.as_str()).collect::<Vec<_>>(),
            ["8000"]
        );
        // Up to a date inside the year.
        let mid = l.profit_and_loss(d(2026, 3, 31)).unwrap();
        assert_eq!(mid.result_cents, 85_000);
        // Must agree with the dashboard for each year.
        for (year, p) in [(2025, &p25), (2026, &p26)] {
            assert_eq!(l.dashboard(year, d(2026, 12, 31)).unwrap().result_cents, p.result_cents);
        }
    }

    #[test]
    fn balance_sheet_balances_with_computed_result_lines() {
        let l = two_years();
        for until in [d(2025, 12, 31), d(2026, 6, 30), d(2026, 12, 31)] {
            let b = l.balance_sheet(until).unwrap();
            assert_eq!(b.assets_cents, b.liabilities_cents + b.equity_cents, "{until}");
        }
        let b = l.balance_sheet(d(2026, 12, 31)).unwrap();
        assert_eq!(b.prior_years_result_cents, 50_000);
        assert_eq!(b.current_year_result_cents, 53_766);
        // 0510 +5000,00 and 0520 −400,00, plus both results.
        assert_eq!(b.equity_cents, 500_000 - 40_000 + 50_000 + 53_766);
        let dep = b.assets.iter().find(|a| a.account == "0110").unwrap();
        assert_eq!(dep.amount_cents, -60_000);
        // First year: nothing before it.
        assert_eq!(l.balance_sheet(d(2025, 12, 31)).unwrap().prior_years_result_cents, 0);
    }

    #[test]
    fn csv_is_dutch_excel_friendly() {
        assert_eq!(csv_amount(123_456), "1234,56");
        assert_eq!(csv_amount(-5), "-0,05");
        assert_eq!(csv_field("a;b \"c\""), "\"a;b \"\"c\"\"\"");
        // Leading zero kept as Excel text; the quote inside makes csv_field quote the whole formula.
        assert_eq!(csv_field(&csv_account("0100")), "\"=\"\"0100\"\"\"");
        assert_eq!(csv_account("8000"), "8000");
        let l = two_years();
        let text = profit_and_loss_csv(&l.profit_and_loss(d(2026, 12, 31)).unwrap());
        assert!(text.starts_with("\u{feff}Winst-en-verliesrekening;01-01-2026 t/m 31-12-2026\r\n"));
        assert!(text.contains("8000;Omzet App Store;850,00\r\n"));
        assert!(text.ends_with(";Resultaat;537,66\r\n"));
        let text = balance_sheet_csv(&l.balance_sheet(d(2026, 12, 31)).unwrap());
        assert!(text.contains(";Resultaat voorgaande boekjaren (berekend);500,00\r\n"));
        let tb = l.trial_balance(Some(d(2026, 12, 31))).unwrap();
        assert!(trial_balance_csv(d(2026, 12, 31), &tb).contains("1100;Bank zakelijk;"));
    }
}
