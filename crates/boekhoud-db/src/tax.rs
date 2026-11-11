//! VAT codes, VAT regime and exchange rates. All of these are versioned data.

use std::str::FromStr;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::{DbError, Ledger, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VatRegime {
    /// Regular VAT: charge VAT, deduct input VAT, file returns.
    Regular,
    /// Kleineondernemersregeling: no VAT charged, no input VAT deducted.
    Kor,
}

impl VatRegime {
    pub fn as_str(&self) -> &'static str {
        match self {
            VatRegime::Regular => "regular",
            VatRegime::Kor => "kor",
        }
    }
}

impl FromStr for VatRegime {
    type Err = DbError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "regular" => Ok(VatRegime::Regular),
            "kor" => Ok(VatRegime::Kor),
            other => Err(DbError::Corrupt(format!("unknown VAT regime {other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaxCode {
    pub code: String,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
    pub name: String,
    pub kind: String,
    pub rate_bp: Option<i64>,
    pub rubric: Option<String>,
    pub icp: bool,
    pub reverse_charge: bool,
    pub regime: Option<VatRegime>,
    pub note: Option<String>,
}

const TAX_CODE_COLS: &str =
    "code, valid_from, valid_until, name, kind, rate_bp, rubric, icp, reverse_charge, regime, note";

fn date_col(row: &Row, i: usize) -> rusqlite::Result<NaiveDate> {
    let s: String = row.get(i)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, e.into())
    })
}

fn tax_code_from_row(row: &Row) -> rusqlite::Result<TaxCode> {
    let until: Option<String> = row.get(2)?;
    let regime: Option<String> = row.get(9)?;
    Ok(TaxCode {
        code: row.get(0)?,
        valid_from: date_col(row, 1)?,
        valid_until: until
            .map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d"))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
            })?,
        name: row.get(3)?,
        kind: row.get(4)?,
        rate_bp: row.get(5)?,
        rubric: row.get(6)?,
        icp: row.get(7)?,
        reverse_charge: row.get(8)?,
        regime: regime
            .map(|r| r.parse::<VatRegime>())
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, e.into())
            })?,
        note: row.get(10)?,
    })
}

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// The version of `code` that is valid on `date`, if any.
pub(crate) fn code_on(conn: &Connection, code: &str, date: NaiveDate) -> Result<Option<TaxCode>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {TAX_CODE_COLS} FROM tax_code
                 WHERE code = ?1 AND valid_from <= ?2 AND (valid_until IS NULL OR valid_until >= ?2)
                 ORDER BY valid_from DESC LIMIT 1"
            ),
            params![code, iso(date)],
            tax_code_from_row,
        )
        .optional()?)
}

/// The VAT regime in force on `date`. Defaults to regular when nothing is configured.
pub(crate) fn regime_on(conn: &Connection, date: NaiveDate) -> Result<VatRegime> {
    let r: Option<String> = conn
        .query_row(
            "SELECT regime FROM vat_regime WHERE valid_from <= ?1 ORDER BY valid_from DESC LIMIT 1",
            [iso(date)],
            |r| r.get(0),
        )
        .optional()?;
    r.map(|s| s.parse())
        .transpose()
        .map(|r| r.unwrap_or(VatRegime::Regular))
}

impl Ledger {
    /// VAT codes valid on `date`.
    pub fn tax_codes(&self, date: NaiveDate) -> Result<Vec<TaxCode>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TAX_CODE_COLS} FROM tax_code t
             WHERE valid_from <= ?1 AND (valid_until IS NULL OR valid_until >= ?1)
             ORDER BY kind DESC, code"
        ))?;
        let rows = stmt.query_map([iso(date)], tax_code_from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn tax_code_on(&self, code: &str, date: NaiveDate) -> Result<Option<TaxCode>> {
        code_on(&self.conn, code, date)
    }

    pub fn vat_regime_on(&self, date: NaiveDate) -> Result<VatRegime> {
        regime_on(&self.conn, date)
    }

    /// Record a VAT regime starting on `valid_from` (e.g. joining or leaving the KOR).
    pub fn set_vat_regime(
        &mut self,
        valid_from: NaiveDate,
        regime: VatRegime,
        note: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let old = regime_on(&tx, valid_from)?;
        tx.execute(
            "INSERT INTO vat_regime (valid_from, regime, note) VALUES (?1, ?2, ?3)
             ON CONFLICT(valid_from) DO UPDATE SET regime = excluded.regime, note = excluded.note",
            params![iso(valid_from), regime.as_str(), note],
        )?;
        tx.execute(
            "INSERT INTO audit_log (actor, action, entity_type, entity_id, old_value, new_value)
             VALUES (?1, 'set_vat_regime', 'vat_regime', ?2, ?3, ?4)",
            params![
                self.actor,
                iso(valid_from),
                old.as_str(),
                format!("{}: {note}", regime.as_str())
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Store an exchange rate (units of `currency` per EUR, scaled by 1,000,000).
    pub fn add_exchange_rate(
        &mut self,
        date: NaiveDate,
        currency: &str,
        units_per_eur_micro: i64,
        source: &str,
    ) -> Result<()> {
        boekhoud_core::Currency::new(currency).map_err(|e| DbError::Corrupt(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO exchange_rate (rate_date, currency, units_per_eur_micro, source) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(rate_date, currency, source) DO UPDATE SET units_per_eur_micro = excluded.units_per_eur_micro",
            params![iso(date), currency, units_per_eur_micro, source],
        )?;
        Ok(())
    }

    /// Most recent rate on or before `date` (ECB publishes no rates in weekends and holidays).
    pub fn exchange_rate_on(
        &self,
        date: NaiveDate,
        currency: &str,
    ) -> Result<Option<(NaiveDate, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT rate_date, units_per_eur_micro FROM exchange_rate
                 WHERE currency = ?1 AND rate_date <= ?2 ORDER BY rate_date DESC LIMIT 1",
                params![currency, iso(date)],
                |r| Ok((date_col(r, 0)?, r.get(1)?)),
            )
            .optional()?)
    }
}
