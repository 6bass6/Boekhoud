//! Sales invoices and credit notes: draft → issued (gapless number + VRK entry), payments.

use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use boekhoud_core::invoice::{sales_entry_lines, sales_totals, InvoiceTotals, SalesLine};
use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Source};

use crate::company::Company;
use crate::contacts::{contact_by_id, Contact};
use crate::tax::{iso, regime_on, vat_rules_on, VatRegime};
use crate::{audit, post_in_tx, DbError, Ledger, Result};

pub const RECEIVABLE_ACCOUNT: &str = "1300";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesInvoiceInput {
    pub contact_id: String,
    pub invoice_date: NaiveDate,
    pub supply_date: Option<NaiveDate>,
    /// Defaults to invoice date + the company's payment term.
    pub due_date: Option<NaiveDate>,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<SalesLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalesInvoiceSummary {
    pub id: String,
    pub status: String,
    pub invoice_number: Option<String>,
    pub invoice_date: NaiveDate,
    pub due_date: NaiveDate,
    pub contact_id: String,
    pub contact_name: String,
    pub is_credit_note: bool,
    pub total_cents: i64,
    pub open_cents: i64,
    /// draft | open | partially_paid | paid | overdue | credited
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentView {
    pub id: String,
    pub date: NaiveDate,
    pub amount_cents: i64,
    pub method: String,
    pub entry_id: String,
    pub reversed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalesInvoiceDetail {
    pub summary: SalesInvoiceSummary,
    pub supply_date: Option<NaiveDate>,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub credits_invoice_id: Option<String>,
    pub credits_invoice_number: Option<String>,
    pub lines: Vec<SalesLine>,
    pub totals: Option<InvoiceTotals>,
    /// Snapshots as issued (None for drafts: use current company/contact).
    pub seller: Option<Company>,
    pub buyer: Option<Contact>,
    pub vat_regime: VatRegime,
    pub entry_id: Option<String>,
    pub document_id: Option<String>,
    pub payments: Vec<PaymentView>,
}

/// What is missing before an invoice may be issued (Dutch invoice requirements).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IssueRequirements {
    pub missing: Vec<String>,
}

type OptStrings8 = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn opt_date(s: Option<String>) -> Option<NaiveDate> {
    s.and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
}

fn date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()))
}

fn validate_lines(lines: &[SalesLine]) -> Result<()> {
    if lines.is_empty() {
        return Err(DbError::Invalid("de factuur heeft geen regels".into()));
    }
    for (i, l) in lines.iter().enumerate() {
        if l.description.trim().is_empty() {
            return Err(DbError::Invalid(format!(
                "regel {} heeft geen omschrijving",
                i + 1
            )));
        }
        if l.quantity_milli == 0 {
            return Err(DbError::Invalid(format!("regel {} heeft aantal 0", i + 1)));
        }
    }
    Ok(())
}

fn write_lines(tx: &Connection, id: &str, lines: &[SalesLine]) -> Result<()> {
    tx.execute("DELETE FROM sales_invoice_line WHERE invoice_id = ?1", [id])?;
    for (i, l) in lines.iter().enumerate() {
        tx.execute(
            "INSERT INTO sales_invoice_line (invoice_id, line_no, description, quantity_milli, unit_price_cents, tax_code, account)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, (i + 1) as i64, l.description.trim(), l.quantity_milli, l.unit_price_cents, l.tax_code, l.account],
        )?;
    }
    Ok(())
}

fn load_lines(conn: &Connection, id: &str) -> Result<Vec<SalesLine>> {
    let mut stmt = conn.prepare(
        "SELECT description, quantity_milli, unit_price_cents, tax_code, account
         FROM sales_invoice_line WHERE invoice_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(SalesLine {
            description: r.get(0)?,
            quantity_milli: r.get(1)?,
            unit_price_cents: r.get(2)?,
            tax_code: r.get(3)?,
            account: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn payment_term(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT payment_term_days FROM company WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(30))
}

/// Sum of non-reversed payments on an invoice.
fn paid_cents(conn: &Connection, column: &str, id: &str) -> Result<i64> {
    Ok(conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(p.amount_cents), 0) FROM payment p
             WHERE p.{column} = ?1
               AND NOT EXISTS (SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)"
        ),
        [id],
        |r| r.get(0),
    )?)
}

pub(crate) fn sales_open_cents(conn: &Connection, id: &str) -> Result<i64> {
    let (status, total, credits): (String, Option<i64>, Option<String>) = conn.query_row(
        "SELECT status, total_cents, credits_invoice_id FROM sales_invoice WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if status != "issued" || credits.is_some() {
        return Ok(0); // drafts are not receivable; credit notes settle against their original
    }
    let credited: i64 = conn.query_row(
        "SELECT COALESCE(SUM(total_cents), 0) FROM sales_invoice WHERE credits_invoice_id = ?1 AND status = 'issued'",
        [id],
        |r| r.get(0),
    )?;
    Ok(total.unwrap_or(0) + credited - paid_cents(conn, "sales_invoice_id", id)?)
}

fn summary(conn: &Connection, id: &str) -> Result<SalesInvoiceSummary> {
    let row = conn
        .query_row(
            "SELECT s.id, s.status, s.invoice_number, s.invoice_date, s.due_date, s.contact_id, c.name,
                    s.credits_invoice_id, s.total_cents
             FROM sales_invoice s JOIN contact c ON c.id = s.contact_id WHERE s.id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| DbError::NotFound(format!("factuur {id}")))?;
    let (id, status, number, inv_date, due, contact_id, contact_name, credits, total) = row;
    let due = date(&due)?;
    let total = match total {
        Some(t) => t,
        None => {
            let lines = load_lines(conn, &id)?;
            let rules = vat_rules_on(conn, date(&inv_date)?)?;
            sales_totals(&lines, &rules)
                .map(|t| t.total_cents)
                .unwrap_or(0)
        }
    };
    let open = sales_open_cents(conn, &id)?;
    let state = if status == "draft" {
        "draft"
    } else if credits.is_some() {
        "credit_note"
    } else if open == 0 {
        let credited: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sales_invoice WHERE credits_invoice_id = ?1 AND status='issued')",
            [&id],
            |r| r.get(0),
        )?;
        if credited && paid_cents(conn, "sales_invoice_id", &id)? == 0 {
            "credited"
        } else {
            "paid"
        }
    } else if open != total {
        "partially_paid"
    } else if due < Utc::now().date_naive() {
        "overdue"
    } else {
        "open"
    };
    Ok(SalesInvoiceSummary {
        id,
        status,
        invoice_number: number,
        invoice_date: date(&inv_date)?,
        due_date: due,
        contact_id,
        contact_name,
        is_credit_note: credits.is_some(),
        total_cents: total,
        open_cents: open,
        state: state.into(),
    })
}

pub(crate) fn payments_for(conn: &Connection, column: &str, id: &str) -> Result<Vec<PaymentView>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT p.id, p.payment_date, p.amount_cents, p.method, p.entry_id,
                EXISTS(SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = p.entry_id)
         FROM payment p WHERE p.{column} = ?1 ORDER BY p.payment_date"
    ))?;
    let rows = stmt.query_map([id], |r| {
        let d: String = r.get(1)?;
        Ok((
            r.get::<_, String>(0)?,
            d,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, d, amount_cents, method, entry_id, reversed) = row?;
        out.push(PaymentView {
            id,
            date: date(&d)?,
            amount_cents,
            method,
            entry_id,
            reversed,
        });
    }
    Ok(out)
}

/// Post a settlement entry (bank ↔ receivable/payable) and record the payment, inside `tx`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_payment(
    tx: &Connection,
    actor: &str,
    sales_invoice_id: Option<&str>,
    purchase_invoice_id: Option<&str>,
    payment_date: NaiveDate,
    amount_cents: i64,
    method: &str,
    counter_account: &str,
    bank_transaction_id: Option<&str>,
    description: &str,
) -> Result<String> {
    if amount_cents == 0 {
        return Err(DbError::Invalid("betaald bedrag is 0".into()));
    }
    let lines = if sales_invoice_id.is_some() {
        vec![
            EntryLine::debit(counter_account, amount_cents),
            EntryLine::credit(RECEIVABLE_ACCOUNT, amount_cents),
        ]
    } else {
        vec![
            EntryLine::debit(crate::purchases::PAYABLE_ACCOUNT, amount_cents),
            EntryLine::credit(counter_account, amount_cents),
        ]
    };
    let id = uuid::Uuid::now_v7().to_string();
    let draft = EntryDraft {
        journal: JournalCode::BNK,
        date: payment_date,
        description: description.into(),
        lines,
        source: Source {
            kind: "payment".into(),
            reference: Some(id.clone()),
        },
        idempotency_key: bank_transaction_id.map(|b| format!("bank_transaction:{b}")),
    };
    let posted = post_in_tx(tx, actor, &draft, None)?;
    tx.execute(
        "INSERT INTO payment (id, sales_invoice_id, purchase_invoice_id, payment_date, amount_cents, method, bank_transaction_id, entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, sales_invoice_id, purchase_invoice_id, iso(payment_date), amount_cents, method, bank_transaction_id, posted.id],
    )?;
    Ok(posted.id)
}

impl Ledger {
    pub fn create_sales_invoice(&mut self, input: &SalesInvoiceInput) -> Result<String> {
        validate_lines(&input.lines)?;
        let tx = self.conn.transaction()?;
        contact_by_id(&tx, &input.contact_id)?;
        let id = uuid::Uuid::now_v7().to_string();
        let due = match input.due_date {
            Some(d) => d,
            None => input.invoice_date + Duration::days(payment_term(&tx)?),
        };
        tx.execute(
            "INSERT INTO sales_invoice (id, contact_id, status, invoice_date, supply_date, due_date, reference, notes)
             VALUES (?1, ?2, 'draft', ?3, ?4, ?5, ?6, ?7)",
            params![id, input.contact_id, iso(input.invoice_date), input.supply_date.map(iso), iso(due), input.reference, input.notes],
        )?;
        write_lines(&tx, &id, &input.lines)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn update_sales_invoice(&mut self, id: &str, input: &SalesInvoiceInput) -> Result<()> {
        validate_lines(&input.lines)?;
        let tx = self.conn.transaction()?;
        let status: String = tx
            .query_row(
                "SELECT status FROM sales_invoice WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("factuur {id}")))?;
        if status != "draft" {
            return Err(DbError::Invalid(
                "een uitgereikte factuur kan niet worden gewijzigd; maak een creditnota".into(),
            ));
        }
        contact_by_id(&tx, &input.contact_id)?;
        let due = match input.due_date {
            Some(d) => d,
            None => input.invoice_date + Duration::days(payment_term(&tx)?),
        };
        tx.execute(
            "UPDATE sales_invoice SET contact_id=?2, invoice_date=?3, supply_date=?4, due_date=?5, reference=?6, notes=?7 WHERE id=?1",
            params![id, input.contact_id, iso(input.invoice_date), input.supply_date.map(iso), iso(due), input.reference, input.notes],
        )?;
        write_lines(&tx, id, &input.lines)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_draft_sales_invoice(&mut self, id: &str) -> Result<()> {
        let n = self.conn.execute(
            "DELETE FROM sales_invoice WHERE id = ?1 AND status = 'draft'",
            [id],
        )?;
        if n == 0 {
            return Err(DbError::Invalid(
                "alleen concepten kunnen worden verwijderd".into(),
            ));
        }
        Ok(())
    }

    /// Check the Dutch invoice requirements for a draft. Empty `missing` = ready to issue.
    pub fn sales_issue_requirements(&self, id: &str) -> Result<IssueRequirements> {
        requirements(&self.conn, id)
    }

    /// Issue a draft: assign the next invoice number of its year, post the VRK entry and freeze
    /// the seller/buyer details. All or nothing.
    pub fn issue_sales_invoice(&mut self, id: &str) -> Result<SalesInvoiceSummary> {
        let actor = self.actor.clone();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let req = requirements(&tx, id)?;
        if !req.missing.is_empty() {
            return Err(DbError::Invalid(format!(
                "factuur onvolledig: {}",
                req.missing.join("; ")
            )));
        }
        let (contact_id, inv_date, credits): (String, String, Option<String>) = tx.query_row(
            "SELECT contact_id, invoice_date, credits_invoice_id FROM sales_invoice WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let inv_date = date(&inv_date)?;
        let lines = load_lines(&tx, id)?;
        let rules = vat_rules_on(&tx, inv_date)?;
        let totals = sales_totals(&lines, &rules).map_err(|e| DbError::Invalid(e.to_string()))?;
        let entry_lines = sales_entry_lines(&lines, &rules, RECEIVABLE_ACCOUNT)
            .map_err(|e| DbError::Invalid(e.to_string()))?;
        let year = inv_date.year();
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(invoice_sequence), 0) + 1 FROM sales_invoice WHERE invoice_year = ?1 AND status = 'issued'",
            [year],
            |r| r.get(0),
        )?;
        let number = format!("{year}-{seq:04}");
        let contact = contact_by_id(&tx, &contact_id)?;
        let company = crate::company::company_row(&tx)?
            .ok_or_else(|| DbError::Invalid("ondernemingsgegevens ontbreken".into()))?;
        let kind = if credits.is_some() {
            "Creditnota"
        } else {
            "Factuur"
        };
        let draft = EntryDraft {
            journal: JournalCode::VRK,
            date: inv_date,
            description: format!("{kind} {number} {}", contact.name),
            lines: entry_lines,
            source: Source {
                kind: "sales_invoice".into(),
                reference: Some(id.into()),
            },
            idempotency_key: Some(format!("sales_invoice:{id}")),
        };
        let posted = post_in_tx(&tx, &actor, &draft, None)?;
        tx.execute(
            "UPDATE sales_invoice SET status='issued', invoice_year=?2, invoice_sequence=?3, invoice_number=?4,
                 seller_snapshot=?5, buyer_snapshot=?6, subtotal_cents=?7, vat_cents=?8, total_cents=?9,
                 entry_id=?10, issued_at=?11 WHERE id=?1",
            params![
                id, year, seq, number, json(&company), json(&contact),
                totals.subtotal_cents, totals.vat_cents, totals.total_cents, posted.id, Utc::now().to_rfc3339()
            ],
        )?;
        audit(
            &tx,
            &actor,
            "issue",
            "sales_invoice",
            id,
            None::<&()>,
            Some(&number),
            Some(&posted.id),
        )?;
        let s = summary(&tx, id)?;
        tx.commit()?;
        Ok(s)
    }

    /// Create a draft credit note for an issued invoice (all lines negated).
    pub fn create_credit_note(&mut self, invoice_id: &str, date: NaiveDate) -> Result<String> {
        let tx = self.conn.transaction()?;
        let (status, number, contact_id, credits): (String, Option<String>, String, Option<String>) = tx
            .query_row(
                "SELECT status, invoice_number, contact_id, credits_invoice_id FROM sales_invoice WHERE id = ?1",
                [invoice_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("factuur {invoice_id}")))?;
        if status != "issued" || credits.is_some() {
            return Err(DbError::Invalid(
                "alleen een uitgereikte factuur kan worden gecrediteerd".into(),
            ));
        }
        let lines: Vec<SalesLine> = load_lines(&tx, invoice_id)?
            .into_iter()
            .map(|l| SalesLine {
                quantity_milli: -l.quantity_milli,
                ..l
            })
            .collect();
        let id = uuid::Uuid::now_v7().to_string();
        let number = number.unwrap_or_default();
        tx.execute(
            "INSERT INTO sales_invoice (id, contact_id, status, invoice_date, due_date, credits_invoice_id, reference, notes)
             VALUES (?1, ?2, 'draft', ?3, ?3, ?4, ?5, ?6)",
            params![id, contact_id, iso(date), invoice_id, number, format!("Creditnota voor factuur {number}.")],
        )?;
        write_lines(&tx, &id, &lines)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn sales_invoices(&self, year: Option<i32>) -> Result<Vec<SalesInvoiceSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM sales_invoice WHERE (?1 IS NULL OR CAST(strftime('%Y', invoice_date) AS INTEGER) = ?1)
             ORDER BY status = 'draft' DESC, invoice_date DESC, invoice_sequence DESC",
        )?;
        let ids: Vec<String> = stmt
            .query_map([year], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        ids.iter().map(|id| summary(&self.conn, id)).collect()
    }

    pub fn sales_invoice(&self, id: &str) -> Result<SalesInvoiceDetail> {
        let s = summary(&self.conn, id)?;
        let (supply, reference, notes, credits, seller, buyer, entry_id, document_id): OptStrings8 = self.conn.query_row(
            "SELECT supply_date, reference, notes, credits_invoice_id, seller_snapshot, buyer_snapshot, entry_id, document_id
             FROM sales_invoice WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
        let lines = load_lines(&self.conn, id)?;
        let rules = vat_rules_on(&self.conn, s.invoice_date)?;
        let totals = sales_totals(&lines, &rules).ok();
        let credits_number = match &credits {
            Some(c) => self.conn.query_row(
                "SELECT invoice_number FROM sales_invoice WHERE id = ?1",
                [c],
                |r| r.get(0),
            )?,
            None => None,
        };
        let parse = |s: Option<String>| -> Result<Option<serde_json::Value>> {
            s.map(|v| serde_json::from_str(&v).map_err(|e| DbError::Corrupt(e.to_string())))
                .transpose()
        };
        let seller: Option<Company> = parse(seller)?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| DbError::Corrupt(e.to_string()))?;
        let buyer: Option<Contact> = parse(buyer)?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| DbError::Corrupt(e.to_string()))?;
        Ok(SalesInvoiceDetail {
            vat_regime: regime_on(&self.conn, s.invoice_date)?,
            payments: payments_for(&self.conn, "sales_invoice_id", id)?,
            summary: s,
            supply_date: opt_date(supply),
            reference,
            notes,
            credits_invoice_id: credits,
            credits_invoice_number: credits_number,
            lines,
            totals,
            seller,
            buyer,
            entry_id,
            document_id,
        })
    }

    /// Link the archived PDF of an issued invoice (only once).
    pub fn set_sales_invoice_document(&mut self, id: &str, document_id: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE sales_invoice SET document_id = ?2 WHERE id = ?1 AND document_id IS NULL AND status = 'issued'",
            params![id, document_id],
        )?;
        if n == 0 {
            return Err(DbError::Invalid(
                "PDF is al gekoppeld of factuur is geen uitgereikte factuur".into(),
            ));
        }
        Ok(())
    }

    /// Register a received payment for an issued invoice (bank account 1100 by default).
    pub fn register_sales_payment(
        &mut self,
        invoice_id: &str,
        date: NaiveDate,
        amount_cents: i64,
        method: &str,
    ) -> Result<String> {
        let actor = self.actor.clone();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let s = summary(&tx, invoice_id)?;
        if s.status != "issued" || s.is_credit_note {
            return Err(DbError::Invalid(
                "betalingen kunnen alleen op uitgereikte facturen worden geregistreerd".into(),
            ));
        }
        let desc = format!(
            "Ontvangst factuur {} {}",
            s.invoice_number.clone().unwrap_or_default(),
            s.contact_name
        );
        let entry = record_payment(
            &tx,
            &actor,
            Some(invoice_id),
            None,
            date,
            amount_cents,
            method,
            "1100",
            None,
            &desc,
        )?;
        tx.commit()?;
        Ok(entry)
    }
}

fn requirements(conn: &Connection, id: &str) -> Result<IssueRequirements> {
    let (status, contact_id, inv_date): (String, String, String) = conn
        .query_row(
            "SELECT status, contact_id, invoice_date FROM sales_invoice WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| DbError::NotFound(format!("factuur {id}")))?;
    let mut missing = Vec::new();
    if status != "draft" {
        missing.push("factuur is al uitgereikt".to_string());
        return Ok(IssueRequirements { missing });
    }
    let inv_date = date(&inv_date)?;
    let regime = regime_on(conn, inv_date)?;
    match crate::company::company_row(conn)? {
        None => missing.push("ondernemingsgegevens (Instellingen)".into()),
        Some(c) => {
            let need = |v: &Option<String>| v.as_deref().map(str::trim).unwrap_or("").is_empty();
            if need(&c.address) || need(&c.postal_code) || need(&c.city) {
                missing.push("adres van de onderneming".into());
            }
            if need(&c.kvk_number) {
                missing.push("KvK-nummer van de onderneming".into());
            }
            if regime == VatRegime::Regular && need(&c.vat_number) {
                missing.push("btw-id van de onderneming".into());
            }
        }
    }
    let contact = contact_by_id(conn, &contact_id)?;
    if contact.address.as_deref().unwrap_or("").trim().is_empty()
        || contact.city.as_deref().unwrap_or("").trim().is_empty()
    {
        missing.push(format!("adres van {}", contact.name));
    }
    let lines = load_lines(conn, id)?;
    let rules = vat_rules_on(conn, inv_date)?;
    match sales_totals(&lines, &rules) {
        Err(e) => missing.push(format!("regels: {e}")),
        Ok(t) => {
            if t.groups.iter().any(|g| g.reverse_charge)
                && contact.vat_number.as_deref().unwrap_or("").is_empty()
            {
                missing.push(format!(
                    "btw-id van {} (verplicht bij btw verlegd)",
                    contact.name
                ));
            }
        }
    }
    Ok(IssueRequirements { missing })
}
