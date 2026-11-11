//! Time tracking for the urencriterium (1,225 hours per calendar year for the entrepreneur's
//! deductions; see docs/accounting-rules.md). Time entries are not financial records and may be
//! edited, but every change is audited.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::tax::iso;
use crate::{audit, DbError, Ledger, Result};

pub const HOURS_CRITERION: i64 = 1225;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeEntry {
    #[serde(default)]
    pub id: String,
    pub entry_date: NaiveDate,
    /// Set while a timer runs (RFC 3339).
    pub started_at: Option<String>,
    pub minutes: Option<i64>,
    pub activity: String,
    pub project: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HoursSummary {
    pub year: i32,
    pub total_minutes: i64,
    pub by_month: Vec<i64>,
    pub by_activity: Vec<(String, i64)>,
    pub criterion_hours: i64,
    /// Hours per remaining week needed to reach the criterion (0 when reached).
    pub needed_per_week_minutes: i64,
}

fn from_row(r: &Row) -> rusqlite::Result<TimeEntry> {
    let d: String = r.get(1)?;
    Ok(TimeEntry {
        id: r.get(0)?,
        entry_date: NaiveDate::parse_from_str(&d, "%Y-%m-%d").unwrap_or_default(),
        started_at: r.get(2)?,
        minutes: r.get(3)?,
        activity: r.get(4)?,
        project: r.get(5)?,
        description: r.get(6)?,
    })
}

const COLS: &str = "id, entry_date, started_at, minutes, activity, project, description";

impl Ledger {
    pub fn time_entries(&self, year: i32) -> Result<Vec<TimeEntry>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM time_entry WHERE CAST(strftime('%Y', entry_date) AS INTEGER) = ?1
             ORDER BY entry_date DESC, created_at DESC"
        ))?;
        let rows = stmt.query_map([year], from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn save_time_entry(&mut self, e: &TimeEntry) -> Result<String> {
        if e.activity.trim().is_empty() {
            return Err(DbError::Invalid("kies een activiteit".into()));
        }
        if e.started_at.is_none() && e.minutes.unwrap_or(0) <= 0 {
            return Err(DbError::Invalid("vul een duur in".into()));
        }
        if e.minutes.unwrap_or(0) > 24 * 60 {
            return Err(DbError::Invalid("meer dan 24 uur op één dag".into()));
        }
        let tx = self.conn.transaction()?;
        let id = if e.id.is_empty() {
            uuid::Uuid::now_v7().to_string()
        } else {
            e.id.clone()
        };
        let old: Option<TimeEntry> = tx
            .query_row(&format!("SELECT {COLS} FROM time_entry WHERE id = ?1"), [&id], from_row)
            .optional()?;
        tx.execute(
            "INSERT INTO time_entry (id, entry_date, started_at, minutes, activity, project, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET entry_date=excluded.entry_date, started_at=excluded.started_at,
                 minutes=excluded.minutes, activity=excluded.activity, project=excluded.project, description=excluded.description",
            params![id, iso(e.entry_date), e.started_at, e.minutes, e.activity.trim(), e.project, e.description],
        )?;
        audit(&tx, &self.actor, "save", "time_entry", &id, old.as_ref(), Some(e), None)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn delete_time_entry(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let old: Option<TimeEntry> = tx
            .query_row(&format!("SELECT {COLS} FROM time_entry WHERE id = ?1"), [id], from_row)
            .optional()?;
        let old = old.ok_or_else(|| DbError::NotFound(format!("uren {id}")))?;
        tx.execute("DELETE FROM time_entry WHERE id = ?1", [id])?;
        audit(
            &tx,
            &self.actor,
            "delete",
            "time_entry",
            id,
            Some(&old),
            None::<&()>,
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Start a timer (only one can run at a time).
    pub fn start_timer(&mut self, activity: &str, project: Option<&str>, description: Option<&str>) -> Result<String> {
        if self.running_timer()?.is_some() {
            return Err(DbError::Invalid("er loopt al een timer".into()));
        }
        let now = Utc::now();
        self.save_time_entry(&TimeEntry {
            id: String::new(),
            entry_date: now.date_naive(),
            started_at: Some(now.to_rfc3339()),
            minutes: None,
            activity: activity.into(),
            project: project.map(Into::into),
            description: description.map(Into::into),
        })
    }

    pub fn running_timer(&self) -> Result<Option<TimeEntry>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM time_entry WHERE minutes IS NULL LIMIT 1"),
                [],
                from_row,
            )
            .optional()?)
    }

    /// Stop the running timer; durations are rounded up to whole minutes.
    pub fn stop_timer(&mut self, now: DateTime<Utc>) -> Result<TimeEntry> {
        let mut t = self
            .running_timer()?
            .ok_or_else(|| DbError::Invalid("er loopt geen timer".into()))?;
        let started = DateTime::parse_from_rfc3339(t.started_at.as_deref().unwrap_or_default())
            .map_err(|e| DbError::Corrupt(e.to_string()))?
            .with_timezone(&Utc);
        let secs = (now - started).num_seconds().max(1);
        t.minutes = Some((secs + 59) / 60);
        t.started_at = None;
        self.save_time_entry(&t)?;
        Ok(t)
    }

    pub fn hours_summary(&self, year: i32, today: NaiveDate) -> Result<HoursSummary> {
        let entries = self.time_entries(year)?;
        let mut by_month = vec![0i64; 12];
        let mut by_activity: std::collections::BTreeMap<String, i64> = Default::default();
        let mut total = 0;
        for e in entries.iter().filter(|e| e.minutes.is_some()) {
            let m = e.minutes.unwrap_or(0);
            by_month[e.entry_date.month0() as usize] += m;
            *by_activity.entry(e.activity.clone()).or_default() += m;
            total += m;
        }
        let remaining_minutes = (HOURS_CRITERION * 60 - total).max(0);
        let end = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid");
        let weeks_left = if today.year() > year {
            0
        } else if today.year() < year {
            52
        } else {
            ((end - today).num_days() / 7).max(1)
        };
        let per_week = if remaining_minutes == 0 || weeks_left == 0 {
            0
        } else {
            (remaining_minutes + weeks_left - 1) / weeks_left
        };
        Ok(HoursSummary {
            year,
            total_minutes: total,
            by_month,
            by_activity: by_activity.into_iter().collect(),
            criterion_hours: HOURS_CRITERION,
            needed_per_week_minutes: per_week,
        })
    }
}
