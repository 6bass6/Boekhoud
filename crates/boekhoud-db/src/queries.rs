//! Read-side queries: entry lists, entry detail, ledger cards, periods.

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use boekhoud_core::JournalCode;

use crate::{DbError, Ledger, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntrySummary {
    pub id: String,
    pub number: String,
    pub journal: String,
    pub fiscal_year: i32,
    pub sequence: i64,
    pub date: NaiveDate,
    pub description: String,
    pub source_kind: String,
    pub source_reference: Option<String>,
    pub total_debit_cents: i64,
    pub reverses_entry_id: Option<String>,
    pub reversed_by_entry_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineView {
    pub line_no: i64,
    pub account: String,
    pub account_name: String,
    pub amount_cents: i64,
    pub tax_code: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryDetail {
    pub summary: EntrySummary,
    pub lines: Vec<LineView>,
    pub posted_at: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerCardRow {
    pub date: NaiveDate,
    pub entry_id: String,
    pub number: String,
    pub description: String,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub balance_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerCard {
    pub account: String,
    pub name: String,
    pub year: i32,
    /// Balance brought forward from earlier years (balance-sheet accounts only).
    pub opening_cents: i64,
    pub rows: Vec<LedgerCardRow>,
    pub closing_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeriodView {
    pub year: i32,
    pub month: u32,
    pub closed: bool,
}

const SUMMARY_SELECT: &str = "
    SELECT e.id, e.journal, e.fiscal_year, e.sequence, e.entry_date, e.description,
           e.source_kind, e.source_reference, e.reverses_entry_id,
           (SELECT r.id FROM journal_entry r WHERE r.reverses_entry_id = e.id),
           (SELECT COALESCE(SUM(amount_cents), 0) FROM journal_line l WHERE l.entry_id = e.id AND l.amount_cents > 0)
    FROM journal_entry e";

fn parse_date(i: usize, s: &str) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.into()))
}

fn number(journal: &str, year: i32, seq: i64) -> String {
    format!("{journal}-{year}-{seq:04}")
}

fn summary_from_row(r: &Row) -> rusqlite::Result<EntrySummary> {
    let journal: String = r.get(1)?;
    let year: i32 = r.get(2)?;
    let seq: i64 = r.get(3)?;
    let date: String = r.get(4)?;
    Ok(EntrySummary {
        id: r.get(0)?,
        number: number(&journal, year, seq),
        journal,
        fiscal_year: year,
        sequence: seq,
        date: parse_date(4, &date)?,
        description: r.get(5)?,
        source_kind: r.get(6)?,
        source_reference: r.get(7)?,
        reverses_entry_id: r.get(8)?,
        reversed_by_entry_id: r.get(9)?,
        total_debit_cents: r.get(10)?,
    })
}

impl Ledger {
    /// Posted entries, newest first, optionally filtered by fiscal year and journal.
    pub fn list_entries(
        &self,
        year: Option<i32>,
        journal: Option<JournalCode>,
        limit: u32,
    ) -> Result<Vec<EntrySummary>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SUMMARY_SELECT}
             WHERE e.status = 'posted' AND (?1 IS NULL OR e.fiscal_year = ?1) AND (?2 IS NULL OR e.journal = ?2)
             ORDER BY e.entry_date DESC, e.chain_index DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![year, journal.map(|j| j.as_str()), limit], summary_from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn entry_detail(&self, entry_id: &str) -> Result<EntryDetail> {
        let (summary, posted_at, hash) = self
            .conn
            .query_row(
                &format!(
                    "SELECT * FROM ({SUMMARY_SELECT} WHERE e.id = ?1 AND e.status = 'posted') s,
                            (SELECT posted_at, hash FROM journal_entry WHERE id = ?1)"
                ),
                [entry_id],
                |r| Ok((summary_from_row(r)?, r.get::<_, String>(11)?, r.get::<_, String>(12)?)),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(entry_id.to_string()))?;
        let mut stmt = self.conn.prepare(
            "SELECT l.line_no, l.account, a.name, l.amount_cents, l.tax_code, l.description
             FROM journal_line l JOIN account a ON a.number = l.account
             WHERE l.entry_id = ?1 ORDER BY l.line_no",
        )?;
        let lines = stmt
            .query_map([entry_id], |r| {
                Ok(LineView {
                    line_no: r.get(0)?,
                    account: r.get(1)?,
                    account_name: r.get(2)?,
                    amount_cents: r.get(3)?,
                    tax_code: r.get(4)?,
                    description: r.get(5)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(EntryDetail {
            summary,
            lines,
            posted_at,
            hash,
        })
    }

    /// Ledger card (grootboekkaart) for one account and fiscal year, with running balance.
    pub fn ledger_card(&self, account: &str, year: i32) -> Result<LedgerCard> {
        let (name, account_type): (String, String) = self
            .conn
            .query_row(
                "SELECT name, account_type FROM account WHERE number = ?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("account {account}")))?;
        let balance_sheet = matches!(account_type.as_str(), "asset" | "liability" | "equity");
        let opening: i64 = if balance_sheet {
            self.conn.query_row(
                "SELECT COALESCE(SUM(l.amount_cents), 0) FROM journal_line l
                 JOIN journal_entry e ON e.id = l.entry_id
                 WHERE l.account = ?1 AND e.status = 'posted' AND e.entry_date < ?2",
                params![account, format!("{year:04}-01-01")],
                |r| r.get(0),
            )?
        } else {
            0
        };
        let mut stmt = self.conn.prepare(
            "SELECT e.entry_date, e.id, e.journal, e.fiscal_year, e.sequence,
                    COALESCE(l.description, e.description), l.amount_cents
             FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
             WHERE l.account = ?1 AND e.status = 'posted'
               AND e.entry_date BETWEEN ?2 AND ?3
             ORDER BY e.entry_date, e.chain_index, l.line_no",
        )?;
        let mut balance = opening;
        let mut rows = Vec::new();
        let raw = stmt.query_map(
            params![account, format!("{year:04}-01-01"), format!("{year:04}-12-31")],
            |r| {
                let date: String = r.get(0)?;
                let journal: String = r.get(2)?;
                Ok((
                    parse_date(0, &date)?,
                    r.get::<_, String>(1)?,
                    number(&journal, r.get(3)?, r.get(4)?),
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )?;
        for row in raw {
            let (date, entry_id, number, description, amount) = row?;
            balance += amount;
            rows.push(LedgerCardRow {
                date,
                entry_id,
                number,
                description,
                debit_cents: amount.max(0),
                credit_cents: (-amount).max(0),
                balance_cents: balance,
            });
        }
        Ok(LedgerCard {
            account: account.to_string(),
            name,
            year,
            opening_cents: opening,
            rows,
            closing_cents: balance,
        })
    }

    /// Status of all twelve months of a year (months never touched are open).
    pub fn periods(&self, year: i32) -> Result<Vec<PeriodView>> {
        let mut stmt = self
            .conn
            .prepare("SELECT month FROM period WHERE year = ?1 AND status = 'closed'")?;
        let closed: Vec<u32> = stmt
            .query_map([year], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok((1..=12)
            .map(|m| PeriodView {
                year,
                month: m,
                closed: closed.contains(&m),
            })
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dashboard {
    pub year: i32,
    pub revenue_cents: i64,
    pub expenses_cents: i64,
    pub result_cents: i64,
    pub bank_cents: i64,
    pub receivables_open_cents: i64,
    pub receivables_overdue: i64,
    pub platform_receivable_cents: i64,
    pub payables_open_cents: i64,
    /// Positive = VAT to pay (all VAT accounts incl. settlement account).
    pub vat_position_cents: i64,
    pub bank_unprocessed: i64,
    pub purchases_without_document: i64,
    pub draft_invoices: i64,
    pub unbooked_platform_reports: i64,
    pub hours_minutes: i64,
    pub revenue_by_month: Vec<i64>,
    pub expenses_by_month: Vec<i64>,
}

impl Ledger {
    pub fn dashboard(&self, year: i32, today: NaiveDate) -> Result<Dashboard> {
        let sum_type = |t: &str| -> Result<i64> {
            Ok(self.conn.query_row(
                "SELECT COALESCE(SUM(l.amount_cents), 0) FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
                 JOIN account a ON a.number = l.account
                 WHERE e.status = 'posted' AND a.account_type = ?1 AND e.fiscal_year = ?2",
                params![t, year],
                |r| r.get(0),
            )?)
        };
        let by_month = |t: &str| -> Result<Vec<i64>> {
            let mut v = vec![0i64; 12];
            let mut stmt = self.conn.prepare(
                "SELECT CAST(strftime('%m', e.entry_date) AS INTEGER), SUM(l.amount_cents) FROM journal_line l
                 JOIN journal_entry e ON e.id = l.entry_id JOIN account a ON a.number = l.account
                 WHERE e.status = 'posted' AND a.account_type = ?1 AND e.fiscal_year = ?2 GROUP BY 1",
            )?;
            for row in stmt.query_map(params![t, year], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
                let (m, s) = row?;
                v[(m - 1) as usize] = if t == "revenue" { -s } else { s };
            }
            Ok(v)
        };
        let balance = |where_clause: &str| -> Result<i64> {
            Ok(self.conn.query_row(
                &format!(
                    "SELECT COALESCE(SUM(l.amount_cents), 0) FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
                     JOIN account a ON a.number = l.account WHERE e.status = 'posted' AND {where_clause}"
                ),
                [],
                |r| r.get(0),
            )?)
        };
        let count = |sql: &str| -> Result<i64> { Ok(self.conn.query_row(sql, [], |r| r.get(0))?) };
        let revenue = -sum_type("revenue")?;
        let expenses = sum_type("expense")?;
        let sales = self.sales_invoices(None)?;
        let open_sales: Vec<_> = sales
            .iter()
            .filter(|s| s.status == "issued" && !s.is_credit_note && s.open_cents != 0)
            .collect();
        let purchases = self.purchase_invoices(None)?;
        let today_s = today.format("%Y-%m-%d").to_string();
        Ok(Dashboard {
            year,
            revenue_cents: revenue,
            expenses_cents: expenses,
            result_cents: revenue - expenses,
            bank_cents: balance("l.account IN (SELECT ledger_account FROM bank_account UNION SELECT '1100')")?,
            receivables_open_cents: open_sales.iter().map(|s| s.open_cents).sum(),
            receivables_overdue: open_sales.iter().filter(|s| s.due_date.format("%Y-%m-%d").to_string() < today_s).count() as i64,
            platform_receivable_cents: balance("l.account IN (SELECT receivable_account FROM platform_config)")?,
            payables_open_cents: purchases.iter().map(|p| p.open_cents).sum(),
            vat_position_cents: -balance("(a.is_vat = 1 OR l.account = '1520')")?,
            bank_unprocessed: count(
                "SELECT COUNT(*) FROM bank_transaction t WHERE t.status = 'open' AND (t.entry_id IS NULL
                   OR EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = t.entry_id))",
            )?,
            purchases_without_document: purchases.iter().filter(|p| !p.has_document && p.state != "reversed").count() as i64,
            draft_invoices: sales.iter().filter(|s| s.status == "draft").count() as i64,
            unbooked_platform_reports: count("SELECT COUNT(*) FROM platform_report WHERE entry_id IS NULL")?,
            hours_minutes: self.conn.query_row(
                "SELECT COALESCE(SUM(minutes), 0) FROM time_entry WHERE CAST(strftime('%Y', entry_date) AS INTEGER) = ?1",
                [year],
                |r| r.get(0),
            )?,
            revenue_by_month: by_month("revenue")?,
            expenses_by_month: by_month("expense")?,
        })
    }
}
