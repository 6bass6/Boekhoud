//! VAT return per rubriek, drill-down, consistency checks, ICP listing and filing.
//!
//! Every figure is computed from posted journal lines tagged with a VAT code; the code's
//! rubriek and VAT accounts come from the `tax_code` table valid on the entry date.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Money, Source};

use crate::tax::iso;
use crate::{audit, post_in_tx, DbError, Ledger, Result};

pub const VAT_SETTLEMENT_ACCOUNT: &str = "1520";

/// Rubrieken shown on the return, in form order.
pub const RUBRICS: [(&str, &str); 11] = [
    ("1a", "Leveringen/diensten belast met hoog tarief"),
    ("1b", "Leveringen/diensten belast met laag tarief"),
    ("1c", "Leveringen/diensten belast met overige tarieven, behalve 0%"),
    ("1d", "Privégebruik"),
    ("1e", "Leveringen/diensten belast met 0% of niet bij u belast"),
    ("2a", "Leveringen/diensten waarbij de omzetbelasting naar u is verlegd"),
    ("3a", "Leveringen naar landen buiten de EU"),
    ("3b", "Leveringen naar of diensten in landen binnen de EU"),
    ("3c", "Installatie/afstandsverkopen binnen de EU"),
    ("4a", "Leveringen/diensten uit landen buiten de EU"),
    ("4b", "Leveringen/diensten uit landen binnen de EU"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RubricLine {
    pub rubric: String,
    pub label: String,
    pub base_cents: i64,
    pub vat_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatReturn {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub rubrics: Vec<RubricLine>,
    /// 5a: VAT due from rubrieken 1–4.
    pub due_cents: i64,
    /// 5b: input VAT.
    pub input_vat_cents: i64,
    /// 5c: 5a − 5b. Positive = to pay.
    pub balance_cents: i64,
    /// Same figures rounded to whole euros in your favour (turnover and VAT due down, input VAT up).
    pub rounded: RoundedReturn,
    pub warnings: Vec<String>,
    pub filed: Option<FiledInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundedReturn {
    pub rubrics: Vec<(String, i64, i64)>,
    pub due: i64,
    pub input_vat: i64,
    pub balance: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiledInfo {
    pub id: String,
    pub filed_at: String,
    pub entry_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatDetailLine {
    pub entry_id: String,
    pub entry_number: String,
    pub date: NaiveDate,
    pub description: String,
    pub account: String,
    pub tax_code: String,
    pub base_cents: i64,
    pub vat_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IcpLine {
    pub vat_number: Option<String>,
    pub name: String,
    pub country: Option<String>,
    pub services_cents: i64,
}

struct TaggedLine {
    entry_id: String,
    number: String,
    date: NaiveDate,
    description: String,
    account: String,
    is_vat: bool,
    amount: i64,
    code: String,
    rubric: Option<String>,
    kind: String,
    payable: Option<String>,
    deductible: Option<String>,
    source_kind: String,
    source_reference: Option<String>,
}

fn euros_down(cents: i64) -> i64 {
    cents.div_euclid(100)
}

fn euros_up(cents: i64) -> i64 {
    -(-cents).div_euclid(100)
}

/// Quarter containing `d`: (first day, last day).
pub fn quarter_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let q0 = (d.month0() / 3) * 3;
    let start = NaiveDate::from_ymd_opt(d.year(), q0 + 1, 1).expect("valid");
    let end = if q0 == 9 {
        NaiveDate::from_ymd_opt(d.year(), 12, 31)
    } else {
        NaiveDate::from_ymd_opt(d.year(), q0 + 4, 1).and_then(|n| n.pred_opt())
    }
    .expect("valid");
    (start, end)
}

impl Ledger {
    fn tagged_lines(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<TaggedLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.journal, e.fiscal_year, e.sequence, e.entry_date, e.description, l.account, a.is_vat,
                    l.amount_cents, l.tax_code,
                    (SELECT t.rubric FROM tax_code t WHERE t.code = l.tax_code AND t.valid_from <= e.entry_date
                       AND (t.valid_until IS NULL OR t.valid_until >= e.entry_date) ORDER BY t.valid_from DESC LIMIT 1),
                    (SELECT t.kind FROM tax_code t WHERE t.code = l.tax_code AND t.valid_from <= e.entry_date
                       AND (t.valid_until IS NULL OR t.valid_until >= e.entry_date) ORDER BY t.valid_from DESC LIMIT 1),
                    (SELECT t.payable_account FROM tax_code t WHERE t.code = l.tax_code AND t.valid_from <= e.entry_date
                       AND (t.valid_until IS NULL OR t.valid_until >= e.entry_date) ORDER BY t.valid_from DESC LIMIT 1),
                    (SELECT t.deductible_account FROM tax_code t WHERE t.code = l.tax_code AND t.valid_from <= e.entry_date
                       AND (t.valid_until IS NULL OR t.valid_until >= e.entry_date) ORDER BY t.valid_from DESC LIMIT 1),
                    e.source_kind, e.source_reference
             FROM journal_line l
             JOIN journal_entry e ON e.id = l.entry_id
             JOIN account a ON a.number = l.account
             WHERE e.status = 'posted' AND l.tax_code IS NOT NULL AND e.entry_date BETWEEN ?1 AND ?2
             ORDER BY e.entry_date, e.chain_index, l.line_no",
        )?;
        let rows = stmt.query_map(params![iso(start), iso(end)], |r| {
            let d: String = r.get(4)?;
            let journal: String = r.get(1)?;
            Ok(TaggedLine {
                entry_id: r.get(0)?,
                number: format!("{journal}-{}-{:04}", r.get::<_, i32>(2)?, r.get::<_, i64>(3)?),
                date: NaiveDate::parse_from_str(&d, "%Y-%m-%d").unwrap_or_default(),
                description: r.get(5)?,
                account: r.get(6)?,
                is_vat: r.get(7)?,
                amount: r.get(8)?,
                code: r.get(9)?,
                rubric: r.get(10)?,
                kind: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
                payable: r.get(12)?,
                deductible: r.get(13)?,
                source_kind: r.get(14)?,
                source_reference: r.get(15)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Compute the VAT return for a period.
    pub fn vat_return(&self, start: NaiveDate, end: NaiveDate) -> Result<VatReturn> {
        if end < start {
            return Err(DbError::Invalid("einddatum ligt voor de begindatum".into()));
        }
        let lines = self.tagged_lines(start, end)?;
        let mut base: BTreeMap<String, i64> = BTreeMap::new();
        let mut vat: BTreeMap<String, i64> = BTreeMap::new();
        let mut input_vat = 0i64;
        let mut unassigned: BTreeMap<String, i64> = BTreeMap::new();
        for l in &lines {
            let primary = l
                .rubric
                .as_deref()
                .and_then(|r| r.split('+').next())
                .filter(|r| *r != "5b")
                .map(str::to_string);
            if !l.is_vat {
                let signed = if l.kind == "sales" { -l.amount } else { l.amount };
                match &primary {
                    Some(r) => *base.entry(r.clone()).or_default() += signed,
                    None if l.kind == "sales" && l.code != "VKOR" => {
                        *unassigned.entry(l.code.clone()).or_default() += signed
                    }
                    None => {}
                }
            } else if l.payable.as_deref() == Some(l.account.as_str()) {
                if let Some(r) = &primary {
                    *vat.entry(r.clone()).or_default() += -l.amount;
                }
            } else if l.deductible.as_deref() == Some(l.account.as_str()) {
                input_vat += l.amount;
            }
        }
        let rubrics: Vec<RubricLine> = RUBRICS
            .iter()
            .map(|(r, label)| RubricLine {
                rubric: r.to_string(),
                label: label.to_string(),
                base_cents: base.get(*r).copied().unwrap_or(0),
                vat_cents: vat.get(*r).copied().unwrap_or(0),
            })
            .collect();
        let due: i64 = rubrics.iter().map(|r| r.vat_cents).sum();
        let mut warnings = Vec::new();
        for (code, amount) in unassigned {
            warnings.push(format!(
                "Omzet met btw-code {code} ({}) is niet aan een rubriek toegewezen; laat dit beoordelen.",
                Money::eur(amount)
            ));
        }

        // Every movement on a VAT account must carry a VAT code (except VAT settlements).
        let mut stmt = self.conn.prepare(
            "SELECT e.journal, e.fiscal_year, e.sequence, l.account, l.amount_cents
             FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id JOIN account a ON a.number = l.account
             WHERE e.status = 'posted' AND a.is_vat = 1 AND l.tax_code IS NULL AND e.source_kind <> 'vat_return'
               AND e.entry_date BETWEEN ?1 AND ?2",
        )?;
        let untagged = stmt.query_map(params![iso(start), iso(end)], |r| {
            Ok(format!(
                "{}-{}-{:04}",
                r.get::<_, String>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, i64>(2)?
            ))
        })?;
        for n in untagged {
            warnings.push(format!(
                "Boeking {} bevat btw zonder btw-code en telt niet mee in de aangifte.",
                n?
            ));
        }
        let open_bank: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM bank_transaction t WHERE t.status = 'open' AND t.booking_date BETWEEN ?1 AND ?2
               AND (t.entry_id IS NULL OR EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = t.entry_id))",
            params![iso(start), iso(end)],
            |r| r.get(0),
        )?;
        if open_bank > 0 {
            warnings.push(format!(
                "{open_bank} banktransacties in deze periode zijn nog niet verwerkt."
            ));
        }
        let missing_docs: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM purchase_invoice p WHERE p.document_id IS NULL AND p.invoice_date BETWEEN ?1 AND ?2
               AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)",
            params![iso(start), iso(end)],
            |r| r.get(0),
        )?;
        if missing_docs > 0 {
            warnings.push(format!(
                "{missing_docs} inkoopfacturen zonder bewijsstuk; zonder factuur is de btw niet aftrekbaar."
            ));
        }
        let drafts: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sales_invoice WHERE status = 'draft' AND invoice_date BETWEEN ?1 AND ?2",
            params![iso(start), iso(end)],
            |r| r.get(0),
        )?;
        if drafts > 0 {
            warnings.push(format!(
                "{drafts} conceptfacturen met een datum in deze periode zijn nog niet uitgereikt."
            ));
        }

        let rounded_rubrics: Vec<(String, i64, i64)> = rubrics
            .iter()
            .map(|r| (r.rubric.clone(), euros_down(r.base_cents), euros_down(r.vat_cents)))
            .collect();
        let r_due: i64 = rounded_rubrics.iter().map(|r| r.2).sum();
        let r_input = euros_up(input_vat);
        let filed = self
            .conn
            .query_row(
                "SELECT id, filed_at, entry_id FROM vat_return WHERE period_start = ?1 AND period_end = ?2",
                params![iso(start), iso(end)],
                |r| {
                    Ok(FiledInfo {
                        id: r.get(0)?,
                        filed_at: r.get(1)?,
                        entry_id: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(VatReturn {
            period_start: start,
            period_end: end,
            rubrics,
            due_cents: due,
            input_vat_cents: input_vat,
            balance_cents: due - input_vat,
            rounded: RoundedReturn {
                rubrics: rounded_rubrics,
                due: r_due,
                input_vat: r_input,
                balance: r_due - r_input,
            },
            warnings,
            filed,
        })
    }

    /// The journal lines behind one rubriek ("5b" for input VAT).
    pub fn vat_return_detail(&self, start: NaiveDate, end: NaiveDate, rubric: &str) -> Result<Vec<VatDetailLine>> {
        let lines = self.tagged_lines(start, end)?;
        let mut grouped: BTreeMap<(String, String), VatDetailLine> = BTreeMap::new();
        for l in lines {
            let parts: Vec<&str> = l.rubric.as_deref().map(|r| r.split('+').collect()).unwrap_or_default();
            let wanted = if rubric == "5b" {
                l.is_vat && l.deductible.as_deref() == Some(l.account.as_str())
            } else {
                parts.first() == Some(&rubric)
            };
            if !wanted {
                continue;
            }
            let e = grouped
                .entry((l.entry_id.clone(), l.code.clone()))
                .or_insert_with(|| VatDetailLine {
                    entry_id: l.entry_id.clone(),
                    entry_number: l.number.clone(),
                    date: l.date,
                    description: l.description.clone(),
                    account: String::new(),
                    tax_code: l.code.clone(),
                    base_cents: 0,
                    vat_cents: 0,
                });
            if !l.is_vat {
                e.base_cents += if l.kind == "sales" { -l.amount } else { l.amount };
                e.account = l.account.clone();
            } else if rubric == "5b" && l.deductible.as_deref() == Some(l.account.as_str()) {
                e.vat_cents += l.amount;
            } else if l.payable.as_deref() == Some(l.account.as_str()) {
                e.vat_cents += -l.amount;
            }
        }
        let mut out: Vec<VatDetailLine> = grouped.into_values().collect();
        out.sort_by(|a, b| (a.date, &a.entry_number).cmp(&(b.date, &b.entry_number)));
        Ok(out)
    }

    /// ICP listing (opgaaf intracommunautaire prestaties) for the period: services per customer VAT id.
    pub fn icp_listing(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<IcpLine>> {
        let lines = self.tagged_lines(start, end)?;
        let icp_codes: Vec<String> = {
            let mut stmt = self.conn.prepare("SELECT DISTINCT code FROM tax_code WHERE icp = 1")?;
            let codes = stmt
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<Vec<String>, _>>()?;
            codes
        };
        let mut out: BTreeMap<String, IcpLine> = BTreeMap::new();
        for l in lines.iter().filter(|l| !l.is_vat && icp_codes.contains(&l.code)) {
            let contact_id: Option<String> = match (l.source_kind.as_str(), &l.source_reference) {
                ("sales_invoice", Some(id)) => {
                    self.conn.query_row("SELECT contact_id FROM sales_invoice WHERE id = ?1", [id], |r| r.get(0)).optional()?
                }
                ("platform_report", Some(id)) => self
                    .conn
                    .query_row(
                        "SELECT c.contact_id FROM platform_report r JOIN platform_config c ON c.platform = r.platform WHERE r.id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten(),
                _ => None,
            };
            let (name, vat_number, country) = match &contact_id {
                Some(c) => {
                    let c = self.contact(c)?;
                    (c.name, c.vat_number, Some(c.country))
                }
                None => (format!("Onbekend ({})", l.number), None, None),
            };
            let key = vat_number.clone().unwrap_or_else(|| name.clone());
            let e = out.entry(key).or_insert(IcpLine {
                vat_number,
                name,
                country,
                services_cents: 0,
            });
            e.services_cents += -l.amount;
        }
        Ok(out.into_values().filter(|l| l.services_cents != 0).collect())
    }

    pub fn set_platform_contact(&mut self, platform: &str, contact_id: Option<&str>) -> Result<()> {
        if let Some(c) = contact_id {
            self.contact(c)?;
        }
        let n = self.conn.execute(
            "UPDATE platform_config SET contact_id = ?2 WHERE platform = ?1",
            params![platform, contact_id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(format!("platform {platform}")));
        }
        Ok(())
    }

    pub fn platform_contact(&self, platform: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row(
            "SELECT contact_id FROM platform_config WHERE platform = ?1",
            [platform],
            |r| r.get(0),
        )?)
    }

    /// Record a VAT return as filed: freeze the figures, clear the VAT accounts to 1520 and
    /// close the months of the period.
    pub fn file_vat_return(&mut self, start: NaiveDate, end: NaiveDate) -> Result<String> {
        let ret = self.vat_return(start, end)?;
        if ret.filed.is_some() {
            return Err(DbError::Invalid("deze aangifte is al vastgelegd".into()));
        }
        let mut stmt = self.conn.prepare(
            "SELECT l.account, SUM(l.amount_cents) FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
             JOIN account a ON a.number = l.account
             WHERE e.status = 'posted' AND a.is_vat = 1 AND e.source_kind <> 'vat_return' AND e.entry_date BETWEEN ?1 AND ?2
             GROUP BY l.account HAVING SUM(l.amount_cents) <> 0",
        )?;
        let balances: Vec<(String, i64)> = stmt
            .query_map(params![iso(start), iso(end)], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = uuid::Uuid::now_v7().to_string();
        let entry_id = if balances.is_empty() {
            None
        } else {
            let mut lines: Vec<EntryLine> = balances.iter().map(|(acc, sum)| EntryLine::credit(acc, *sum)).collect();
            let total: i64 = balances.iter().map(|(_, s)| s).sum();
            lines.push(EntryLine::debit(VAT_SETTLEMENT_ACCOUNT, total));
            let draft = EntryDraft {
                journal: JournalCode::MEM,
                date: end,
                description: format!(
                    "Btw-aangifte {} t/m {}",
                    start.format("%d-%m-%Y"),
                    end.format("%d-%m-%Y")
                ),
                lines: lines.into_iter().filter(|l| l.amount.cents != 0).collect(),
                source: Source {
                    kind: "vat_return".into(),
                    reference: Some(id.clone()),
                },
                idempotency_key: Some(format!("vat_return:{}:{}", iso(start), iso(end))),
            };
            Some(post_in_tx(&tx, &actor, &draft, None)?.id)
        };
        tx.execute(
            "INSERT INTO vat_return (id, period_start, period_end, figures_json, payable_cents, entry_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, iso(start), iso(end), serde_json::to_string(&ret).unwrap_or_default(), ret.rounded.balance * 100, entry_id],
        )?;
        let mut m = NaiveDate::from_ymd_opt(start.year(), start.month(), 1).expect("valid");
        while m <= end {
            tx.execute(
                "INSERT INTO period (year, month, status) VALUES (?1, ?2, 'closed')
                 ON CONFLICT(year, month) DO UPDATE SET status = 'closed', changed_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                params![m.year(), m.month()],
            )?;
            m = m.checked_add_months(chrono::Months::new(1)).expect("valid");
        }
        audit(
            &tx,
            &actor,
            "file",
            "vat_return",
            &id,
            None::<&()>,
            Some(&ret.rounded),
            entry_id.as_deref(),
        )?;
        tx.commit()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_and_rounding() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert_eq!(quarter_of(d(11, 11)), (d(10, 1), d(12, 31)));
        assert_eq!(quarter_of(d(2, 28)), (d(1, 1), d(3, 31)));
        assert_eq!(euros_down(21_099), 210);
        assert_eq!(euros_up(21_001), 211);
        assert_eq!(euros_down(-150), -2);
    }
}
