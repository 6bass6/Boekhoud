//! Purchase invoices (received): registered and posted in one step, with any number of documents.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use boekhoud_core::invoice::{purchase_entry_lines, purchase_totals, PurchaseLine};
use boekhoud_core::{EntryDraft, JournalCode, Source};

use crate::contacts::contact_by_id;
use crate::documents::DocumentInfo;
use crate::sales::{payments_for, record_payment, PaymentView};
use crate::tax::{iso, vat_rules_on};
use crate::{audit, post_in_tx, reverse_in_tx, DbError, Ledger, Result};

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
    /// Attached documents (invoice, receipt, …), in display order.
    #[serde(default)]
    pub document_ids: Vec<String>,
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
    /// open | paid | paid_privately | reversed | corrected
    pub state: String,
    pub entry_id: String,
    /// The invoice this one corrects (replaces).
    pub corrects_id: Option<String>,
    /// The invoice that replaced this one.
    pub corrected_by_id: Option<String>,
}

/// Details that can change after registration because they are not part of the booked amounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseDetails {
    pub description: String,
    pub supplier_reference: Option<String>,
    pub due_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseInvoiceDetail {
    pub summary: PurchaseInvoiceSummary,
    pub lines: Vec<PurchaseLine>,
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub documents: Vec<DocumentInfo>,
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

/// Link documents to a purchase invoice after the ones already attached. Skips repeats in `ids`;
/// refuses unknown documents and documents that belong to another purchase invoice.
/// Returns the ids that were newly linked.
fn link_documents(conn: &Connection, invoice_id: &str, ids: &[String]) -> Result<Vec<String>> {
    let mut position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) FROM purchase_invoice_document WHERE purchase_invoice_id = ?1",
        [invoice_id],
        |r| r.get(0),
    )?;
    let mut added = Vec::new();
    for id in ids {
        if added.contains(id) {
            continue;
        }
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM document WHERE id = ?1)", [id], |r| {
            r.get(0)
        })?;
        if !exists {
            return Err(DbError::NotFound(format!("document {id}")));
        }
        let here: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM purchase_invoice_document WHERE document_id = ?1 AND purchase_invoice_id = ?2)",
            params![id, invoice_id],
            |r| r.get(0),
        )?;
        if here {
            continue;
        }
        // Linked to another invoice that still counts (not reversed or corrected)?
        let elsewhere: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM purchase_invoice_document l JOIN purchase_invoice p ON p.id = l.purchase_invoice_id
               WHERE l.document_id = ?1 AND l.purchase_invoice_id <> ?2
                 AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id))",
            params![id, invoice_id],
            |r| r.get(0),
        )?;
        if elsewhere {
            return Err(DbError::Invalid(
                "dit document hoort al bij een andere inkoopfactuur".into(),
            ));
        }
        position += 1;
        conn.execute(
            "INSERT INTO purchase_invoice_document (purchase_invoice_id, document_id, position) VALUES (?1, ?2, ?3)",
            params![invoice_id, id, position],
        )?;
        added.push(id.clone());
    }
    Ok(added)
}

fn documents_of(conn: &Connection, invoice_id: &str) -> Result<Vec<DocumentInfo>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.sha256, d.original_filename, d.mime_type, d.size_bytes, d.document_type, d.uploaded_at,
                d.retention_until
         FROM purchase_invoice_document l JOIN document d ON d.id = l.document_id
         WHERE l.purchase_invoice_id = ?1 ORDER BY l.position",
    )?;
    let docs = stmt
        .query_map([invoice_id], |r| {
            Ok(DocumentInfo {
                id: r.get(0)?,
                sha256: r.get(1)?,
                original_filename: r.get(2)?,
                mime_type: r.get(3)?,
                size_bytes: r.get(4)?,
                document_type: r.get(5)?,
                uploaded_at: r.get(6)?,
                retention_until: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(docs)
}

fn summary(conn: &Connection, id: &str) -> Result<PurchaseInvoiceSummary> {
    let row = conn
        .query_row(
            "SELECT p.id, p.contact_id, c.name, p.supplier_reference, p.invoice_date, p.due_date, p.description,
                    p.total_cents,
                    EXISTS(SELECT 1 FROM purchase_invoice_document d WHERE d.purchase_invoice_id = p.id),
                    p.paid_privately, p.entry_id,
                    EXISTS(SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id),
                    p.corrects_invoice_id,
                    (SELECT n.id FROM purchase_invoice n WHERE n.corrects_invoice_id = p.id)
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
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, Option<String>>(13)?,
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
        corrects_id,
        corrected_by_id,
    ) = row;
    let open = purchase_open_cents(conn, &id)?;
    let state = if corrected_by_id.is_some() {
        "corrected"
    } else if reversed {
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
        corrects_id,
        corrected_by_id,
    })
}

fn check_duplicate_reference(
    conn: &Connection,
    contact_id: &str,
    reference: Option<&str>,
    except_id: Option<&str>,
) -> Result<()> {
    let Some(r) = reference else { return Ok(()) };
    let dup: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM purchase_invoice p WHERE contact_id = ?1 AND supplier_reference = ?2
           AND (?3 IS NULL OR id <> ?3)
           AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id))",
        params![contact_id, r, except_id],
        |row| row.get(0),
    )?;
    if dup {
        let name: String = conn.query_row("SELECT name FROM contact WHERE id = ?1", [contact_id], |row| row.get(0))?;
        return Err(DbError::Invalid(format!("factuur {r} van {name} is al geregistreerd")));
    }
    Ok(())
}

/// Register and post a purchase invoice inside the caller's transaction; returns its id.
fn register_in_tx(tx: &Connection, actor: &str, input: &PurchaseInput, corrects: Option<&str>) -> Result<String> {
    if input.description.trim().is_empty() {
        return Err(DbError::Invalid("omschrijving ontbreekt".into()));
    }
    let contact = contact_by_id(tx, &input.contact_id)?;
    let reference = input
        .supplier_reference
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());
    check_duplicate_reference(tx, &input.contact_id, reference, None)?;
    for (i, l) in input.lines.iter().enumerate() {
        let account: Option<(String, bool)> = tx
            .query_row(
                "SELECT name, purchase_allowed FROM account WHERE number = ?1",
                [&l.account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match account {
            Some((_, true)) => {}
            Some((name, false)) => {
                return Err(DbError::Invalid(format!(
                    "regel {}: {} {name} is geen kostenrekening; kies een kostenrekening (of een bedrijfsmiddel)",
                    i + 1,
                    l.account
                )))
            }
            None => return Err(DbError::Invalid(format!("regel {}: kies een kostenrekening", i + 1))),
        }
    }
    let rules = vat_rules_on(tx, input.invoice_date)?;
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
    let posted = post_in_tx(tx, actor, &draft, None)?;
    tx.execute(
        "INSERT INTO purchase_invoice (id, contact_id, supplier_reference, invoice_date, due_date, description,
             subtotal_cents, vat_cents, total_cents, paid_privately, entry_id, corrects_invoice_id)
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
            corrects
        ],
    )?;
    link_documents(tx, &id, &input.document_ids)?;
    for (i, l) in input.lines.iter().enumerate() {
        tx.execute(
            "INSERT INTO purchase_invoice_line (invoice_id, line_no, description, account, net_cents, tax_code, vat_cents)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, (i + 1) as i64, l.description, l.account, l.net_cents, l.tax_code, l.vat_cents],
        )?;
    }
    audit(
        tx,
        actor,
        "register",
        "purchase_invoice",
        &id,
        None::<&()>,
        Some(input),
        Some(&posted.id),
    )?;
    Ok(id)
}

impl Ledger {
    /// Register and post a purchase invoice (INK journal).
    pub fn register_purchase_invoice(&mut self, input: &PurchaseInput) -> Result<PurchaseInvoiceSummary> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = register_in_tx(&tx, &actor, input, None)?;
        let s = summary(&tx, &id)?;
        tx.commit()?;
        Ok(s)
    }

    /// Correct a registered purchase invoice: reverse its entry (on its own date, so the VAT
    /// correction lands in the same quarter) and register `input` as a new invoice that replaces it.
    /// Refused when payments are booked on it or a period involved is closed (e.g. VAT return filed).
    pub fn correct_purchase_invoice(&mut self, id: &str, input: &PurchaseInput) -> Result<PurchaseInvoiceSummary> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = summary(&tx, id)?;
        if matches!(old.state.as_str(), "reversed" | "corrected") {
            return Err(DbError::Invalid(
                "deze inkoopfactuur is al tegengeboekt of gecorrigeerd".into(),
            ));
        }
        let payments: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM payment p WHERE purchase_invoice_id = ?1
               AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id))",
            [id],
            |r| r.get(0),
        )?;
        if payments {
            return Err(DbError::Invalid(
                "er is al een betaling op deze inkoopfactuur geboekt; boek die eerst terug en corrigeer daarna".into(),
            ));
        }
        for date in [old.invoice_date, input.invoice_date] {
            let closed: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM period WHERE year = ?1 AND month = ?2 AND status = 'closed')",
                params![date.year(), date.month()],
                |r| r.get(0),
            )?;
            if closed {
                return Err(DbError::Invalid(format!(
                    "de periode {}-{:02} is afgesloten (bijvoorbeeld na de btw-aangifte); corrigeren kan daar niet meer",
                    date.year(),
                    date.month()
                )));
            }
        }
        reverse_in_tx(
            &tx,
            &actor,
            &old.entry_id,
            old.invoice_date,
            "inkoopfactuur gecorrigeerd",
        )?;
        let new_id = register_in_tx(&tx, &actor, input, Some(id))?;
        audit(
            &tx,
            &actor,
            "correct",
            "purchase_invoice",
            id,
            Some(&old),
            Some(&new_id),
            None,
        )?;
        let s = summary(&tx, &new_id)?;
        tx.commit()?;
        Ok(s)
    }

    /// Change description, supplier's invoice number and due date. The booked amounts, date, supplier
    /// and journal entry stay as they are.
    pub fn update_purchase_details(&mut self, id: &str, details: &PurchaseDetails) -> Result<PurchaseInvoiceSummary> {
        let description = details.description.trim();
        if description.is_empty() {
            return Err(DbError::Invalid("omschrijving ontbreekt".into()));
        }
        let reference = details
            .supplier_reference
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty());
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = summary(&tx, id)?;
        if matches!(old.state.as_str(), "reversed" | "corrected") {
            return Err(DbError::Invalid(
                "een tegengeboekte of gecorrigeerde inkoopfactuur kan niet meer worden gewijzigd".into(),
            ));
        }
        check_duplicate_reference(&tx, &old.contact_id, reference, Some(id))?;
        tx.execute(
            "UPDATE purchase_invoice SET description = ?2, supplier_reference = ?3, due_date = ?4 WHERE id = ?1",
            params![id, description, reference, details.due_date.map(iso)],
        )?;
        let before = PurchaseDetails {
            description: old.description,
            supplier_reference: old.supplier_reference,
            due_date: old.due_date,
        };
        audit(
            &tx,
            &actor,
            "update_details",
            "purchase_invoice",
            id,
            Some(&before),
            Some(details),
            None,
        )?;
        let s = summary(&tx, id)?;
        tx.commit()?;
        Ok(s)
    }

    /// Active accounts a purchase line may be booked on (costs and business assets).
    pub fn purchase_accounts(&self) -> Result<Vec<boekhoud_core::Account>> {
        Ok(self
            .accounts()?
            .into_iter()
            .filter(|a| a.active)
            .filter(|a| {
                self.conn
                    .query_row(
                        "SELECT purchase_allowed FROM account WHERE number = ?1",
                        [&a.number],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(false)
            })
            .collect())
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
        let (subtotal, vat): (i64, i64) = self.conn.query_row(
            "SELECT subtotal_cents, vat_cents FROM purchase_invoice WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
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
            documents: documents_of(&self.conn, id)?,
        })
    }

    /// Attach more documents to a registered purchase invoice (they are added after the existing ones).
    pub fn attach_purchase_documents(&mut self, id: &str, document_ids: &[String]) -> Result<()> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        summary(&tx, id)?;
        let added = link_documents(&tx, id, document_ids)?;
        if !added.is_empty() {
            audit(
                &tx,
                &actor,
                "attach_documents",
                "purchase_invoice",
                id,
                None::<&()>,
                Some(&added),
                None,
            )?;
        }
        tx.commit()?;
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
            None,
            desc.trim(),
        )?;
        tx.commit()?;
        Ok(entry)
    }
}
