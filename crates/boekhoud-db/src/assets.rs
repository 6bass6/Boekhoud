//! Fixed assets with straight-line monthly depreciation, posted as MEM entries.
//! Each month's depreciation has its own idempotency key, so running it again never doubles.

use chrono::{Datelike, Months, NaiveDate};
use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};

use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Source};

use crate::tax::iso;
use crate::{audit, post_in_tx, DbError, Ledger, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetInput {
    pub name: String,
    pub acquisition_date: NaiveDate,
    pub cost_cents: i64,
    pub residual_cents: i64,
    pub months: i64,
    /// Defaults to the month of acquisition.
    pub start_month: Option<NaiveDate>,
    pub business_use_pct: i64,
    pub purchase_invoice_id: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetView {
    pub id: String,
    pub name: String,
    pub acquisition_date: NaiveDate,
    pub cost_cents: i64,
    pub residual_cents: i64,
    pub months: i64,
    pub start_month: NaiveDate,
    pub business_use_pct: i64,
    pub monthly_cents: i64,
    pub depreciated_cents: i64,
    pub book_value_cents: i64,
    pub months_posted: i64,
    pub notes: Option<String>,
}

/// Depreciation for month number `n` (1-based) of a schedule; the last month absorbs rounding.
pub fn depreciation_for_month(cost: i64, residual: i64, months: i64, n: i64) -> i64 {
    let base = cost - residual;
    if n < 1 || n > months {
        return 0;
    }
    let monthly = base / months;
    if n == months {
        base - monthly * (months - 1)
    } else {
        monthly
    }
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid")
}

/// id, name, acquisition date, cost, residual, months, start month, business %, notes
type AssetRow = (String, String, String, i64, i64, i64, String, i64, Option<String>);
/// id, name, cost, residual, months, start month, disposed on
type ScheduleRow = (String, String, i64, i64, i64, String, Option<String>);

fn from_row(r: &Row) -> rusqlite::Result<AssetRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
    ))
}

impl Ledger {
    pub fn add_asset(&mut self, a: &AssetInput) -> Result<String> {
        if a.name.trim().is_empty()
            || a.cost_cents <= 0
            || a.months <= 0
            || a.residual_cents < 0
            || a.residual_cents >= a.cost_cents
        {
            return Err(DbError::Invalid(
                "controleer naam, aanschafprijs, restwaarde en looptijd".into(),
            ));
        }
        let id = uuid::Uuid::now_v7().to_string();
        let start = first_of_month(a.start_month.unwrap_or(a.acquisition_date));
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO asset (id, name, acquisition_date, cost_cents, residual_cents, months, start_month, business_use_pct, purchase_invoice_id, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, a.name.trim(), iso(a.acquisition_date), a.cost_cents, a.residual_cents, a.months, iso(start), a.business_use_pct, a.purchase_invoice_id, a.notes],
        )?;
        audit(&tx, &self.actor, "add", "asset", &id, None::<&()>, Some(a), None)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn assets(&self) -> Result<Vec<AssetView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, acquisition_date, cost_cents, residual_cents, months, start_month, business_use_pct, notes
             FROM asset ORDER BY acquisition_date DESC",
        )?;
        let rows: Vec<_> = stmt.query_map([], from_row)?.collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for (id, name, acq, cost, residual, months, start, pct, notes) in rows {
            let (posted, depreciated): (i64, i64) = self.conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(l.amount_cents), 0) FROM journal_entry e JOIN journal_line l ON l.entry_id = e.id
                 WHERE e.idempotency_key LIKE ?1 AND l.amount_cents > 0
                   AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = e.id)",
                [format!("depreciation:{id}:%")],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()));
            out.push(AssetView {
                acquisition_date: parse(&acq)?,
                start_month: parse(&start)?,
                monthly_cents: depreciation_for_month(cost, residual, months, 1),
                depreciated_cents: depreciated,
                book_value_cents: cost - depreciated,
                months_posted: posted,
                id,
                name,
                cost_cents: cost,
                residual_cents: residual,
                months,
                business_use_pct: pct,
                notes,
            });
        }
        Ok(out)
    }

    /// Post all depreciation due up to and including the month of `until`. Returns the number
    /// of entries created. Months in closed periods are skipped and reported as an error.
    pub fn post_depreciation(&mut self, until: NaiveDate) -> Result<usize> {
        let assets: Vec<ScheduleRow> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, name, cost_cents, residual_cents, months, start_month, disposed_on FROM asset")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut created = 0;
        for (id, name, cost, residual, months, start, disposed) in assets {
            let start = NaiveDate::parse_from_str(&start, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()))?;
            let (dep_acc, exp_acc): (String, String) = tx.query_row(
                "SELECT depreciation_account, expense_account FROM asset WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            for n in 1..=months {
                let month = start.checked_add_months(Months::new((n - 1) as u32)).expect("valid");
                if month > first_of_month(until) {
                    break;
                }
                if disposed.as_deref().map(|d| iso(month).as_str() > d).unwrap_or(false) {
                    break;
                }
                let key = format!("depreciation:{id}:{}", month.format("%Y-%m"));
                let exists: Option<String> = tx
                    .query_row("SELECT id FROM journal_entry WHERE idempotency_key = ?1", [&key], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if exists.is_some() {
                    continue;
                }
                let amount = depreciation_for_month(cost, residual, months, n);
                if amount == 0 {
                    continue;
                }
                let last_day = month
                    .checked_add_months(Months::new(1))
                    .and_then(|m| m.pred_opt())
                    .expect("valid");
                let draft = EntryDraft {
                    journal: JournalCode::MEM,
                    date: last_day,
                    description: format!("Afschrijving {name} {}/{months}", n),
                    lines: vec![EntryLine::debit(&exp_acc, amount), EntryLine::credit(&dep_acc, amount)],
                    source: Source {
                        kind: "depreciation".into(),
                        reference: Some(id.clone()),
                    },
                    idempotency_key: Some(key),
                };
                post_in_tx(&tx, &actor, &draft, None)?;
                created += 1;
            }
        }
        tx.commit()?;
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::depreciation_for_month;

    #[test]
    fn schedule_sums_to_base() {
        let total: i64 = (1..=60).map(|n| depreciation_for_month(200_000, 0, 60, n)).sum();
        assert_eq!(total, 200_000);
        assert_eq!(depreciation_for_month(200_000, 0, 60, 1), 3_333);
        assert_eq!(depreciation_for_month(200_000, 0, 60, 60), 3_353);
        assert_eq!(depreciation_for_month(200_000, 0, 60, 61), 0);
    }
}
