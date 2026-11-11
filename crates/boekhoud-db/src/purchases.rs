//! Purchase invoices (received): registered and posted in one step, with optional document.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use boekhoud_core::invoice::{purchase_entry_lines, purchase_totals, PurchaseLine};
use boekhoud_core::{EntryDraft, JournalCode, Source};

use crate::contacts::contact_by_id;
use crate::sales::{payments_for, record_payment, PaymentView};
use crate::tax::{iso, vat_rules_on};
use crate::{audit, post_in_tx, DbError, Ledger, Result};

pub const PAYABLE_ACCOUNT: &str = "1600";
pub const PRIVATE_CONTRIBUTION_ACCOUNT: &str = "0510";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInput {
    pub contact_id: String,
    pub supplier_reference: Option<String>,
    pub invoice_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub description: String,
    pub lines: Vec<PurchaseLine>,
    #[serde(default)]
    pub paid_privately: bool,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseInvoiceSummary {
    pub id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub supplier_reference: Option<String>,
    pub invoice_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub description: String,
    pub total_cents: i64,
    pub open_cents: i64,
    pub has_document: bool,
    /// open | paid | paid_privately | reversed
    pub state: String,
    pub entry_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseInvoiceDetail {
    pub summary: PurchaseInvoiceSummary,
    pub lines: Vec<PurchaseLine>,
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub document_id: Option<String>,
    pub payments: Vec<PaymentView>,
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()))
}

pub(crate) fn purchase_open_cents(conn: &Connection, id: &str) -> Result<i64> {
    let (total, private, reversed): (i64, bool, bool) = conn.query_row(
        "SELECT total_cents, paid_privately,
                EXISTS(SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)
         FROM purchase_invoice p WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if private || reversed {
        return Ok(0);
    }
    let paid: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM payment p WHERE purchase_invoice_id = ?1
           AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)",
        [id],
        |r| r.get(0),
    )?;
    Ok(total - paid)
}

fn summary(conn: &Connection, id: &str) -> Result<PurchaseInvoiceSummary> {
    let row = conn
        .query_row(
            "SELECT p.id, p.contact_id, c.name, p.supplier_reference, p.invoice_date, p.due_date, p.description,
                    p.total_cents, p.document_id IS NOT NULL, p.paid_privately, p.entry_id,
                    EXISTS(SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)
             FROM purchase_invoice p JOIN contact c ON c.id = p.contact_id WHERE p.id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, bool>(8)?,
                    r.get::<_, bool>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, bool>(11)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| DbError::NotFound(format!("inkoopfactuur {id}")))?;
    let (
        id,
        contact_id,
        contact_name,
        supplier_reference,
        inv_date,
        due,
        description,
        total,
        has_doc,
        private,
        entry_id,
        reversed,
    ) = row;
    let open = purchase_open_cents(conn, &id)?;
    let state = if reversed {
        "reversed"
    } else if private {
        "paid_privately"
    } else if open == 0 {
        "paid"
    } else {
        "open"
    };
    Ok(PurchaseInvoiceSummary {
        id,
        contact_id,
        contact_name,
        supplier_reference,
        invoice_date: parse_date(&inv_date)?,
        due_date: due.map(|d| parse_date(&d)).transpose()?,
        description,
        total_cents: total,
        open_cents: open,
        has_document: has_doc,
        state: state.into(),
        entry_id,
    })
}

impl Ledger {
    /// Register and post a purchase invoice (INK journal).
    pub fn register_purchase_invoice(&mut self, input: &PurchaseInput) -> Result<PurchaseInvoiceSummary> {
        if input.description.trim().is_empty() {
            return Err(DbError::Invalid("omschrijving ontbreekt".into()));
        }
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let contact = contact_by_id(&tx, &input.contact_id)?;
        let reference = input
            .supplier_reference
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty());
        if let Some(r) = reference {
            let dup: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM purchase_invoice WHERE contact_id = ?1 AND supplier_reference = ?2)",
                params![input.contact_id, r],
                |row| row.get(0),
            )?;
            if dup {
                return Err(DbError::Invalid(format!(
                    "factuur {r} van {} is al geregistreerd",
                    contact.name
                )));
            }
        }
        if let Some(doc) = &input.document_id {
            let used: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM purchase_invoice WHERE document_id = ?1)",
                [doc],
                |row| row.get(0),
            )?;
            if used {
                return Err(DbError::Invalid(
                    "dit document hoort al bij een andere inkoopfactuur".into(),
                ));
            }
        }
        let rules = vat_rules_on(&tx, input.invoice_date)?;
        let (net, vat, total) = purchase_totals(&input.lines, &rules).map_err(|e| DbError::Invalid(e.to_string()))?;
        let credit_account = if input.paid_privately {
            PRIVATE_CONTRIBUTION_ACCOUNT
        } else {
            PAYABLE_ACCOUNT
        };
        let lines =
            purchase_entry_lines(&input.lines, &rules, credit_account).map_err(|e| DbError::Invalid(e.to_string()))?;
        let id = uuid::Uuid::now_v7().to_string();
        let draft = EntryDraft {
            journal: JournalCode::INK,
            date: input.invoice_date,
            description: format!("{} — {}", contact.name, input.description.trim()),
            lines,
            source: Source {
                kind: "purchase_invoice".into(),
                reference: Some(id.clone()),
            },
            idempotency_key: Some(format!("purchase_invoice:{id}")),
        };
        let posted = post_in_tx(&tx, &actor, &draft, None)?;
        tx.execute(
            "INSERT INTO purchase_invoice (id, contact_id, supplier_reference, invoice_date, due_date, description,
                 subtotal_cents, vat_cents, total_cents, paid_privately, entry_id, document_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                input.contact_id,
                reference,
                iso(input.invoice_date),
                input.due_date.map(iso),
                input.description.trim(),
                net,
                vat,
                total,
                input.paid_privately,
                posted.id,
                input.document_id
            ],
        )?;
        for (i, l) in input.lines.iter().enumerate() {
            tx.execute(
                "INSERT INTO purchase_invoice_line (invoice_id, line_no, description, account, net_cents, tax_code, vat_cents)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, (i + 1) as i64, l.description, l.account, l.net_cents, l.tax_code, l.vat_cents],
            )?;
        }
        audit(
            &tx,
            &actor,
            "register",
            "purchase_invoice",
            &id,
            None::<&()>,
            Some(input),
            Some(&posted.id),
        )?;
        let s = summary(&tx, &id)?;
        tx.commit()?;
        Ok(s)
    }

    pub fn purchase_invoices(&self, year: Option<i32>) -> Result<Vec<PurchaseInvoiceSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM purchase_invoice WHERE (?1 IS NULL OR CAST(strftime('%Y', invoice_date) AS INTEGER) = ?1)
             ORDER BY invoice_date DESC, created_at DESC",
        )?;
        let ids: Vec<String> = stmt
            .query_map([year], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        ids.iter().map(|id| summary(&self.conn, id)).collect()
    }

    pub fn purchase_invoice(&self, id: &str) -> Result<PurchaseInvoiceDetail> {
        let s = summary(&self.conn, id)?;
        let (subtotal, vat, document_id): (i64, i64, Option<String>) = self.conn.query_row(
            "SELECT subtotal_cents, vat_cents, document_id FROM purchase_invoice WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT description, account, net_cents, tax_code, vat_cents FROM purchase_invoice_line
             WHERE invoice_id = ?1 ORDER BY line_no",
        )?;
        let lines = stmt
            .query_map([id], |r| {
                Ok(PurchaseLine {
                    description: r.get(0)?,
                    account: r.get(1)?,
                    net_cents: r.get(2)?,
                    tax_code: r.get(3)?,
                    vat_cents: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(PurchaseInvoiceDetail {
            payments: payments_for(&self.conn, "purchase_invoice_id", id)?,
            summary: s,
            lines,
            subtotal_cents: subtotal,
            vat_cents: vat,
            document_id,
        })
    }

    /// Attach the original document afterwards (only if none is attached yet).
    pub fn attach_purchase_document(&mut self, id: &str, document_id: &str) -> Result<()> {
        self.document(document_id)?;
        let n = self.conn.execute(
            "UPDATE purchase_invoice SET document_id = ?2 WHERE id = ?1 AND document_id IS NULL",
            params![id, document_id],
        )?;
        if n == 0 {
            return Err(DbError::Invalid("er is al een document gekoppeld".into()));
        }
        Ok(())
    }

    pub fn register_purchase_payment(
        &mut self,
        invoice_id: &str,
        date: NaiveDate,
        amount_cents: i64,
        method: &str,
    ) -> Result<String> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let s = summary(&tx, invoice_id)?;
        if s.state != "open" {
            return Err(DbError::Invalid("deze inkoopfactuur staat niet open".into()));
        }
        let desc = format!(
            "Betaling {} {}",
            s.contact_name,
            s.supplier_reference.clone().unwrap_or_default()
        );
        let entry = record_payment(
            &tx,
            &actor,
            None,
            Some(invoice_id),
            date,
            amount_cents,
            method,
            "1100",
            None,
            desc.trim(),
        )?;
        tx.commit()?;
        Ok(entry)
    }
}
