//! IB-prognose: loads the yearly parameters from `tax_parameter` and feeds the ledger's year result
//! and hours into the pure calculation in `boekhoud_core::income_tax`.

use std::collections::HashMap;

use boekhoud_core::income_tax::{estimate, Bracket, CreditSegment, IncomeTaxEstimate, IncomeTaxInput, IncomeTaxParams};
use chrono::{Datelike, NaiveDate};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::{DbError, Ledger, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeTaxView {
    pub year: i32,
    /// Year whose parameters were used; differs from `year` when that year has none yet.
    pub parameter_year: i32,
    pub hours_minutes: i64,
    pub criterion_hours: i64,
    pub remaining_months: i64,
    pub params: IncomeTaxParams,
    pub estimate: IncomeTaxEstimate,
    /// Distinct source URLs of the parameters used.
    pub sources: Vec<String>,
}

impl Ledger {
    /// Parameters for `year`, or for the latest year that has parameters. Returns the year used.
    pub fn income_tax_params(&self, year: i32) -> Result<(i32, IncomeTaxParams, Vec<String>)> {
        let used: i32 = self
            .conn
            .query_row(
                "SELECT COALESCE((SELECT year FROM tax_parameter WHERE year = ?1 LIMIT 1), MAX(year)) FROM tax_parameter",
                [year],
                |r| r.get(0),
            )
            .optional()?
            .flatten()
            .ok_or_else(|| DbError::Invalid("er zijn geen belastingparameters ingevoerd".into()))?;
        let mut stmt = self
            .conn
            .prepare("SELECT key, value, source_url FROM tax_parameter WHERE year = ?1")?;
        let mut values = HashMap::new();
        let mut sources: Vec<String> = Vec::new();
        for row in stmt.query_map(params![used], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get(2)?))
        })? {
            let (k, v, s) = row?;
            values.insert(k, v);
            if !sources.contains(&s) {
                sources.push(s);
            }
        }
        sources.sort();
        let get = |k: &str| {
            values
                .get(k)
                .copied()
                .ok_or_else(|| DbError::Invalid(format!("belastingparameter {k} ontbreekt voor {used}")))
        };
        // Indexed segments 1, 2, … until `<prefix>_N_rate` is missing.
        let segments = |prefix: &str| -> Vec<(Option<i64>, i64, i64)> {
            (1..)
                .map_while(|n| {
                    let rate = values.get(&format!("{prefix}_{n}_rate"))?;
                    let upto = values.get(&format!("{prefix}_{n}_upto")).copied();
                    let base = values.get(&format!("{prefix}_{n}_base")).copied().unwrap_or(0);
                    Some((upto, base, *rate))
                })
                .collect()
        };
        let credits = |prefix: &str| {
            segments(prefix)
                .into_iter()
                .map(|(upto_cents, base_cents, rate_micro)| CreditSegment {
                    upto_cents,
                    base_cents,
                    rate_micro,
                })
                .collect()
        };
        let p = IncomeTaxParams {
            brackets: segments("box1_bracket")
                .into_iter()
                .map(|(upto_cents, _, rate_micro)| Bracket { upto_cents, rate_micro })
                .collect(),
            zelfstandigenaftrek_cents: get("zelfstandigenaftrek")?,
            startersaftrek_cents: get("startersaftrek")?,
            mkb_rate_micro: get("mkb_winstvrijstelling_rate")?,
            deduction_rate_cap_micro: get("aftrek_max_rate")?,
            algemene_heffingskorting: credits("ahk"),
            arbeidskorting: credits("arbeidskorting"),
            zvw_rate_micro: get("zvw_rate")?,
            zvw_max_income_cents: get("zvw_max_income")?,
        };
        if p.brackets.is_empty() {
            return Err(DbError::Invalid(format!("schijven box 1 ontbreken voor {used}")));
        }
        Ok((used, p, sources))
    }

    /// Income-tax estimate for `year`. Salary, withheld loonheffing and the starter flag are screen
    /// inputs and are not stored.
    pub fn income_tax_estimate(
        &self,
        year: i32,
        today: NaiveDate,
        salary_cents: i64,
        withheld_cents: i64,
        starter: bool,
    ) -> Result<IncomeTaxView> {
        let (parameter_year, params, sources) = self.income_tax_params(year)?;
        let profit = self.dashboard(year, today)?.result_cents;
        let hours = self.hours_summary(year, today)?;
        let remaining_months = match today.year().cmp(&year) {
            std::cmp::Ordering::Less => 12,
            std::cmp::Ordering::Equal => 13 - today.month() as i64,
            std::cmp::Ordering::Greater => 0,
        };
        let input = IncomeTaxInput {
            profit_cents: profit,
            hours_met: hours.total_minutes >= hours.criterion_hours * 60,
            starter,
            salary_cents,
            withheld_cents,
            remaining_months,
        };
        Ok(IncomeTaxView {
            year,
            parameter_year,
            hours_minutes: hours.total_minutes,
            criterion_hours: hours.criterion_hours,
            remaining_months,
            estimate: estimate(&params, input),
            params,
            sources,
        })
    }
}
