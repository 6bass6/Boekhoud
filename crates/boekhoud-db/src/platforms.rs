//! App Store / Google Play reports: import (stored with the original file), then book the
//! euro proceeds as one PLT entry per report. Importing the same file twice is refused.

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Money, Source};

use crate::tax::iso;
use crate::{audit, post_in_tx, DbError, Ledger, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyAmount {
    pub currency: String,
    pub units: i64,
    /// Amount in `currency` from the report.
    pub amount_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductAmount {
    pub product: String,
    pub currency: String,
    pub units: i64,
    pub amount_cents: i64,
}

/// What a parser produced, in platform-neutral form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReportInput {
    pub platform: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub currencies: Vec<CurrencyAmount>,
    pub products: Vec<ProductAmount>,
    /// Extra breakdown for display (e.g. Google transaction types), stored as-is.
    pub breakdown: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformReportView {
    pub id: String,
    pub platform: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub original_filename: String,
    pub document_id: Option<String>,
    pub currencies: Vec<CurrencyAmount>,
    pub breakdown: serde_json::Value,
    pub proceeds_eur_cents: Option<i64>,
    pub entry_id: Option<String>,
    pub imported_at: String,
    /// Other reports of the same platform whose period overlaps (possible double import).
    pub overlapping: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformConfig {
    pub platform: String,
    pub name: String,
    pub receivable_account: String,
    pub revenue_account: String,
    pub tax_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EurAmount {
    pub currency: String,
    /// Euro proceeds for this currency as stated by the platform (Apple Payments page).
    pub eur_cents: i64,
}

/// `(platform, product, currency, units, amount_cents)`
pub type ProductRevenue = (String, String, String, i64, i64);

fn date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()))
}

impl Ledger {
    pub fn platform_config(&self, platform: &str) -> Result<PlatformConfig> {
        self.conn
            .query_row(
                "SELECT platform, name, receivable_account, revenue_account, tax_code FROM platform_config WHERE platform = ?1",
                [platform],
                |r| Ok(PlatformConfig { platform: r.get(0)?, name: r.get(1)?, receivable_account: r.get(2)?, revenue_account: r.get(3)?, tax_code: r.get(4)? }),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("platform {platform}")))
    }

    /// Store a parsed report together with its original file. Returns the report id.
    pub fn import_platform_report(
        &mut self,
        input: &PlatformReportInput,
        file: &[u8],
        filename: &str,
    ) -> Result<String> {
        self.platform_config(&input.platform)?;
        let sha = hex::encode(Sha256::digest(file));
        let exists: Option<String> = self
            .conn
            .query_row(
                "SELECT period_end FROM platform_report WHERE file_sha256 = ?1",
                [&sha],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(p) = exists {
            return Err(DbError::Invalid(format!(
                "dit rapport is al geïmporteerd (periode t/m {p})"
            )));
        }
        let doc = self.add_document(file, filename, "platform_report")?;
        let id = uuid::Uuid::now_v7().to_string();
        let summary = serde_json::json!({ "currencies": input.currencies, "breakdown": input.breakdown });
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO platform_report (id, platform, period_start, period_end, file_sha256, original_filename, document_id, summary_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, input.platform, iso(input.period_start), iso(input.period_end), sha, filename, doc.id, summary.to_string()],
        )?;
        for p in &input.products {
            tx.execute(
                "INSERT INTO platform_report_product (report_id, product, currency, units, amount_cents) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, p.product, p.currency, p.units, p.amount_cents],
            )?;
        }
        audit(
            &tx,
            &self.actor,
            "import",
            "platform_report",
            &id,
            None::<&()>,
            Some(&(filename, &sha)),
            None,
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn platform_reports(&self) -> Result<Vec<PlatformReportView>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM platform_report ORDER BY period_end DESC, platform")?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        ids.iter().map(|id| self.platform_report(id)).collect()
    }

    pub fn platform_report(&self, id: &str) -> Result<PlatformReportView> {
        let row = self
            .conn
            .query_row(
                "SELECT id, platform, period_start, period_end, original_filename, document_id, summary_json,
                        proceeds_eur_cents, entry_id, imported_at
                 FROM platform_report WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, Option<i64>>(7)?,
                        r.get::<_, Option<String>>(8)?,
                        r.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("rapport {id}")))?;
        let (id, platform, start, end, filename, document_id, summary, eur, entry_id, imported_at) = row;
        let summary: serde_json::Value = serde_json::from_str(&summary).map_err(|e| DbError::Corrupt(e.to_string()))?;
        let currencies: Vec<CurrencyAmount> =
            serde_json::from_value(summary["currencies"].clone()).map_err(|e| DbError::Corrupt(e.to_string()))?;
        let mut stmt = self.conn.prepare(
            "SELECT id FROM platform_report WHERE platform = ?1 AND id <> ?2 AND period_start <= ?4 AND period_end >= ?3",
        )?;
        let overlapping = stmt
            .query_map(params![platform, id, start, end], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(PlatformReportView {
            period_start: date(&start)?,
            period_end: date(&end)?,
            id,
            platform,
            original_filename: filename,
            document_id,
            currencies,
            breakdown: summary["breakdown"].clone(),
            proceeds_eur_cents: eur,
            entry_id,
            imported_at,
            overlapping,
        })
    }

    /// Remove an imported report that has not been booked yet (e.g. imported by mistake).
    pub fn delete_platform_report(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let booked: Option<Option<String>> = tx
            .query_row("SELECT entry_id FROM platform_report WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        match booked {
            None => return Err(DbError::NotFound(format!("rapport {id}"))),
            Some(Some(_)) => {
                return Err(DbError::Invalid(
                    "een geboekt rapport kan niet worden verwijderd".into(),
                ))
            }
            Some(None) => {}
        }
        tx.execute("DELETE FROM platform_report_product WHERE report_id = ?1", [id])?;
        tx.execute("DELETE FROM platform_report WHERE id = ?1", [id])?;
        audit(
            &tx,
            &self.actor,
            "delete",
            "platform_report",
            id,
            None::<&()>,
            None::<&()>,
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Book the euro proceeds of a report: debit the platform receivable, credit revenue per
    /// currency (reverse-charged service to the platform). `eur` must cover every currency.
    pub fn book_platform_report(&mut self, id: &str, eur: &[EurAmount]) -> Result<String> {
        let report = self.platform_report(id)?;
        if report.entry_id.is_some() {
            return Err(DbError::Invalid("dit rapport is al geboekt".into()));
        }
        let cfg = self.platform_config(&report.platform)?;
        let mut lines = Vec::new();
        let mut total = 0i64;
        for c in &report.currencies {
            let e = eur
                .iter()
                .find(|e| e.currency == c.currency)
                .ok_or_else(|| DbError::Invalid(format!("euro-opbrengst voor {} ontbreekt", c.currency)))?;
            if c.currency == "EUR" && e.eur_cents != c.amount_cents {
                return Err(DbError::Invalid(
                    "het EUR-bedrag moet gelijk zijn aan het rapport".into(),
                ));
            }
            if e.eur_cents == 0 {
                continue;
            }
            total += e.eur_cents;
            lines.push(EntryLine {
                account: cfg.revenue_account.clone(),
                amount: Money::eur(-e.eur_cents),
                tax_code: Some(cfg.tax_code.clone()),
                description: Some(format!(
                    "{} {} ({} st.)",
                    c.currency,
                    Money::new(
                        c.amount_cents,
                        boekhoud_core::Currency::new(&c.currency).map_err(|e| DbError::Invalid(e.to_string()))?
                    ),
                    c.units
                )),
            });
        }
        if total == 0 {
            return Err(DbError::Invalid("de opbrengst is € 0; er valt niets te boeken".into()));
        }
        lines.insert(
            0,
            EntryLine {
                account: cfg.receivable_account.clone(),
                amount: Money::eur(total),
                tax_code: None,
                description: None,
            },
        );
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let draft = EntryDraft {
            journal: JournalCode::PLT,
            date: report.period_end,
            description: format!(
                "{} opbrengst {} t/m {}",
                cfg.name,
                report.period_start.format("%d-%m-%Y"),
                report.period_end.format("%d-%m-%Y")
            ),
            lines,
            source: Source {
                kind: "platform_report".into(),
                reference: Some(id.into()),
            },
            idempotency_key: Some(format!("platform_report:{id}")),
        };
        let posted = post_in_tx(&tx, &actor, &draft, None)?;
        tx.execute(
            "UPDATE platform_report SET proceeds_eur_cents = ?2, entry_id = ?3 WHERE id = ?1",
            params![id, total, posted.id],
        )?;
        audit(
            &tx,
            &actor,
            "book",
            "platform_report",
            id,
            None::<&()>,
            Some(eur),
            Some(&posted.id),
        )?;
        tx.commit()?;
        Ok(posted.id)
    }

    /// Revenue per product (from booked and unbooked reports) for a year, in report currencies.
    pub fn platform_revenue_by_product(&self, year: i32) -> Result<Vec<ProductRevenue>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.platform, p.product, p.currency, SUM(p.units), SUM(p.amount_cents)
             FROM platform_report_product p JOIN platform_report r ON r.id = p.report_id
             WHERE CAST(strftime('%Y', r.period_end) AS INTEGER) = ?1
             GROUP BY r.platform, p.product, p.currency ORDER BY r.platform, p.product, p.currency",
        )?;
        let rows = stmt.query_map([year], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}
