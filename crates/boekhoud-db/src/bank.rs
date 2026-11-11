//! Bank accounts, statement import (with duplicate detection), proposals and processing of
//! transactions into BNK entries, and reconciliation against statement balances.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Money, Source};

use crate::purchases::purchase_open_cents;
use crate::sales::{record_payment, sales_open_cents};
use crate::tax::{code_on, iso};
use crate::{audit, post_in_tx, DbError, Ledger, Result};

pub const EXCHANGE_DIFFERENCE_ACCOUNT: &str = "9000";
pub const PRIVATE_CONTRIBUTION: &str = "0510";
pub const PRIVATE_WITHDRAWAL: &str = "0520";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankAccount {
    pub id: String,
    pub iban: String,
    pub name: String,
    pub ledger_account: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewBankTransaction {
    pub booking_date: NaiveDate,
    pub value_date: Option<NaiveDate>,
    pub amount_cents: i64,
    pub currency: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub description: String,
    pub end_to_end_id: Option<String>,
    pub bank_reference: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankImportInput {
    pub format: String,
    pub opening: Option<(NaiveDate, i64)>,
    pub closing: Option<(NaiveDate, i64)>,
    pub transactions: Vec<NewBankTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankImportResult {
    pub import_id: String,
    pub new_transactions: usize,
    pub skipped_duplicates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankTransactionView {
    pub id: String,
    pub bank_account_id: String,
    pub booking_date: NaiveDate,
    pub amount_cents: i64,
    pub currency: String,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub description: String,
    /// open | processed | ignored
    pub state: String,
    pub entry_id: Option<String>,
    pub entry_number: Option<String>,
    pub ignore_reason: Option<String>,
}

/// How to process a bank transaction. Every variant except `Ignore` creates a BNK entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BankAction {
    SalesInvoice {
        invoice_id: String,
    },
    PurchaseInvoice {
        invoice_id: String,
    },
    /// Payout from a platform; `settle_cents` of the receivable is cleared (default: the
    /// transaction amount), any difference is an exchange difference.
    PlatformPayout {
        platform: String,
        settle_cents: Option<i64>,
    },
    /// Book directly against an account; with a VAT code the VAT is split out of the amount.
    Account {
        account: String,
        tax_code: Option<String>,
        description: Option<String>,
    },
    /// Private deposit (money in) or withdrawal (money out).
    Private,
    Ignore {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub action: BankAction,
    pub label: String,
    /// 0..=100
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reconciliation {
    pub bank_account_id: String,
    pub ledger_account: String,
    pub statement_date: Option<NaiveDate>,
    pub statement_balance_cents: Option<i64>,
    pub ledger_balance_at_statement_cents: Option<i64>,
    pub difference_cents: Option<i64>,
    pub open_transactions: i64,
    pub open_sum_cents: i64,
    pub ledger_balance_cents: i64,
}

fn date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| DbError::Corrupt(e.to_string()))
}

fn normalize_iban(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn base_key(t: &NewBankTransaction) -> String {
    match &t.bank_reference {
        Some(r) if !r.is_empty() => format!("ref:{r}"),
        _ => {
            let raw = format!(
                "{}|{}|{}|{}|{}",
                iso(t.booking_date),
                t.amount_cents,
                t.counterparty_iban.as_deref().unwrap_or(""),
                t.counterparty_name.as_deref().unwrap_or(""),
                t.description
            );
            format!("h:{}", &hex::encode(Sha256::digest(raw.as_bytes()))[..32])
        }
    }
}

const VIEW_SELECT: &str = "
    SELECT t.id, t.bank_account_id, t.booking_date, t.amount_cents, t.currency, t.counterparty_name,
           t.counterparty_iban, t.description, t.status, t.entry_id, t.ignore_reason,
           e.journal, e.fiscal_year, e.sequence,
           EXISTS(SELECT 1 FROM journal_entry r WHERE r.reverses_entry_id = t.entry_id)
    FROM bank_transaction t LEFT JOIN journal_entry e ON e.id = t.entry_id";

fn view_from_row(r: &Row) -> rusqlite::Result<(BankTransactionView, String)> {
    let status: String = r.get(8)?;
    let entry_id: Option<String> = r.get(9)?;
    let reversed: bool = r.get(14)?;
    let journal: Option<String> = r.get(11)?;
    let entry_number = match (&journal, r.get::<_, Option<i32>>(12)?, r.get::<_, Option<i64>>(13)?) {
        (Some(j), Some(y), Some(s)) if !reversed => Some(format!("{j}-{y}-{s:04}")),
        _ => None,
    };
    let state = if status == "ignored" {
        "ignored"
    } else if entry_id.is_some() && !reversed {
        "processed"
    } else {
        "open"
    };
    let d: String = r.get(2)?;
    Ok((
        BankTransactionView {
            id: r.get(0)?,
            bank_account_id: r.get(1)?,
            booking_date: NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid"),
            amount_cents: r.get(3)?,
            currency: r.get(4)?,
            counterparty_name: r.get(5)?,
            counterparty_iban: r.get(6)?,
            description: r.get(7)?,
            state: state.into(),
            entry_id: if reversed { None } else { entry_id },
            entry_number,
            ignore_reason: r.get(10)?,
        },
        d,
    ))
}

fn load_view(conn: &Connection, id: &str) -> Result<BankTransactionView> {
    let (mut v, d) = conn
        .query_row(&format!("{VIEW_SELECT} WHERE t.id = ?1"), [id], view_from_row)
        .optional()?
        .ok_or_else(|| DbError::NotFound(format!("banktransactie {id}")))?;
    v.booking_date = date(&d)?;
    Ok(v)
}

/// Contains `needle` as a whole word-ish token (case-insensitive)?
fn mentions(haystack: &str, needle: &str) -> bool {
    let n = needle.trim().to_lowercase();
    if n.len() < 3 {
        return false;
    }
    haystack.to_lowercase().contains(&n)
}

/// Split a VAT-inclusive amount into (net, vat) for a rate in basis points:
/// net = round(gross * 10000 / (10000 + bp)), half away from zero.
pub fn split_gross(gross_cents: i64, bp: i64) -> (i64, i64) {
    let num = gross_cents as i128 * 10_000;
    let den = (10_000 + bp) as i128;
    let (q, r) = (num / den, num % den);
    let net = if 2 * r.abs() >= den { q + num.signum() } else { q } as i64;
    (net, gross_cents - net)
}

impl Ledger {
    pub fn bank_accounts(&self) -> Result<Vec<BankAccount>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, iban, name, ledger_account FROM bank_account ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(BankAccount {
                id: r.get(0)?,
                iban: r.get(1)?,
                name: r.get(2)?,
                ledger_account: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Find a bank account by IBAN, or create it (linked to ledger account 1100 by default).
    pub fn ensure_bank_account(&mut self, iban: &str, name: Option<&str>) -> Result<BankAccount> {
        let iban = normalize_iban(iban);
        if iban.len() < 8 {
            return Err(DbError::Invalid("ongeldig IBAN".into()));
        }
        if let Some(a) = self.bank_accounts()?.into_iter().find(|a| a.iban == iban) {
            return Ok(a);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let name = name
            .map(str::to_string)
            .unwrap_or_else(|| format!("Bank {}", &iban[iban.len() - 4..]));
        self.conn.execute(
            "INSERT INTO bank_account (id, iban, name, ledger_account) VALUES (?1, ?2, ?3, '1100')",
            params![id, iban, name],
        )?;
        Ok(BankAccount {
            id,
            iban,
            name,
            ledger_account: "1100".into(),
        })
    }

    /// Import a statement. Transactions already present (same bank reference, or same date,
    /// amount, counterparty and description at the same position) are skipped.
    pub fn import_bank_statement(
        &mut self,
        account_id: &str,
        input: &BankImportInput,
        file: &[u8],
        filename: &str,
    ) -> Result<BankImportResult> {
        let sha = hex::encode(Sha256::digest(file));
        let dup: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_import WHERE file_sha256 = ?1)",
            [&sha],
            |r| r.get(0),
        )?;
        if dup {
            return Err(DbError::Invalid("dit afschrift is al geïmporteerd".into()));
        }
        if let Some(t) = input.transactions.iter().find(|t| t.currency != "EUR") {
            return Err(DbError::Invalid(format!(
                "transactie in {} — alleen EUR-rekeningen worden ondersteund",
                t.currency
            )));
        }
        let doc = self.add_document(file, filename, "bank_statement")?;
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let import_id = uuid::Uuid::now_v7().to_string();
        tx.execute(
            "INSERT INTO bank_import (id, bank_account_id, file_sha256, original_filename, format, document_id,
                 opening_date, opening_cents, closing_date, closing_cents, transactions_new, transactions_skipped)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 0)",
            params![
                import_id,
                account_id,
                sha,
                filename,
                input.format,
                doc.id,
                input.opening.map(|o| iso(o.0)),
                input.opening.map(|o| o.1),
                input.closing.map(|c| iso(c.0)),
                input.closing.map(|c| c.1)
            ],
        )?;
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let (mut new, mut skipped) = (0usize, 0usize);
        for t in &input.transactions {
            let base = base_key(t);
            let n = seen.entry(base.clone()).or_default();
            *n += 1;
            let key = if base.starts_with("ref:") {
                base
            } else {
                format!("{base}#{n}")
            };
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM bank_transaction WHERE bank_account_id = ?1 AND dedupe_key = ?2)",
                params![account_id, key],
                |r| r.get(0),
            )?;
            if exists {
                skipped += 1;
                continue;
            }
            tx.execute(
                "INSERT INTO bank_transaction (id, bank_account_id, import_id, booking_date, value_date, amount_cents, currency,
                     counterparty_name, counterparty_iban, description, end_to_end_id, bank_reference, dedupe_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    uuid::Uuid::now_v7().to_string(), account_id, import_id, iso(t.booking_date), t.value_date.map(iso),
                    t.amount_cents, t.currency, t.counterparty_name, t.counterparty_iban.as_deref().map(normalize_iban),
                    t.description, t.end_to_end_id, t.bank_reference, key
                ],
            )?;
            new += 1;
        }
        tx.execute(
            "UPDATE bank_import SET transactions_new = ?2, transactions_skipped = ?3 WHERE id = ?1",
            params![import_id, new as i64, skipped as i64],
        )?;
        audit(
            &tx,
            &actor,
            "import",
            "bank_import",
            &import_id,
            None::<&()>,
            Some(&(filename, new, skipped)),
            None,
        )?;
        tx.commit()?;
        Ok(BankImportResult {
            import_id,
            new_transactions: new,
            skipped_duplicates: skipped,
        })
    }

    /// Transactions of an account, newest first. `state`: open | processed | ignored | all.
    pub fn bank_transactions(
        &self,
        account_id: &str,
        state: &str,
        year: Option<i32>,
    ) -> Result<Vec<BankTransactionView>> {
        let mut stmt = self.conn.prepare(&format!(
            "{VIEW_SELECT} WHERE t.bank_account_id = ?1
               AND (?2 IS NULL OR CAST(strftime('%Y', t.booking_date) AS INTEGER) = ?2)
             ORDER BY t.booking_date DESC, t.created_at DESC"
        ))?;
        let rows = stmt.query_map(params![account_id, year], view_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            let (mut v, d) = row?;
            v.booking_date = date(&d)?;
            if state == "all" || v.state == state {
                out.push(v);
            }
        }
        Ok(out)
    }

    pub fn bank_transaction(&self, id: &str) -> Result<BankTransactionView> {
        load_view(&self.conn, id)
    }

    /// Proposals for processing a transaction, best first. Proposals never post anything.
    pub fn bank_suggestions(&self, id: &str) -> Result<Vec<Suggestion>> {
        let t = load_view(&self.conn, id)?;
        let text = format!("{} {}", t.counterparty_name.clone().unwrap_or_default(), t.description);
        let mut out: Vec<Suggestion> = Vec::new();

        if t.amount_cents > 0 {
            // Open sales invoices.
            for inv in self
                .sales_invoices(None)?
                .into_iter()
                .filter(|i| i.status == "issued" && !i.is_credit_note && i.open_cents > 0)
            {
                let number_hit = inv
                    .invoice_number
                    .as_deref()
                    .map(|n| mentions(&t.description, n))
                    .unwrap_or(false);
                let amount_hit = inv.open_cents == t.amount_cents;
                let name_hit = mentions(&text, &inv.contact_name);
                let confidence = match (number_hit, amount_hit, name_hit) {
                    (true, true, _) => 98,
                    (true, false, _) => 85,
                    (false, true, true) => 80,
                    (false, true, false) => 55,
                    (false, false, true) => 40,
                    _ => 0,
                };
                if confidence > 0 {
                    out.push(Suggestion {
                        action: BankAction::SalesInvoice {
                            invoice_id: inv.id.clone(),
                        },
                        label: format!(
                            "Ontvangst factuur {} {} (open {})",
                            inv.invoice_number.unwrap_or_default(),
                            inv.contact_name,
                            Money::eur(inv.open_cents)
                        ),
                        confidence,
                    });
                }
            }
            // Platform payouts.
            for (platform, words) in [("apple", ["apple"]), ("google", ["google"])] {
                if words.iter().any(|w| mentions(&text, w)) {
                    let cfg = self.platform_config(platform)?;
                    let open: i64 = self.conn.query_row(
                        "SELECT COALESCE(SUM(l.amount_cents), 0) FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
                         WHERE l.account = ?1 AND e.status = 'posted'",
                        [&cfg.receivable_account],
                        |r| r.get(0),
                    )?;
                    let settle = if open > 0 && (open - t.amount_cents).abs() * 20 <= open {
                        Some(open)
                    } else {
                        None
                    };
                    out.push(Suggestion {
                        action: BankAction::PlatformPayout {
                            platform: platform.into(),
                            settle_cents: settle,
                        },
                        label: format!("Uitbetaling {} (te ontvangen {})", cfg.name, Money::eur(open)),
                        confidence: if settle.is_some() { 92 } else { 70 },
                    });
                }
            }
        } else {
            // Open purchase invoices.
            for p in self.purchase_invoices(None)?.into_iter().filter(|p| p.state == "open") {
                let amount_hit = p.open_cents == -t.amount_cents;
                let ref_hit = p
                    .supplier_reference
                    .as_deref()
                    .map(|r| mentions(&t.description, r))
                    .unwrap_or(false);
                let name_hit = mentions(&text, &p.contact_name);
                let confidence = match (ref_hit, amount_hit, name_hit) {
                    (true, true, _) => 97,
                    (true, false, _) => 80,
                    (false, true, true) => 85,
                    (false, true, false) => 50,
                    _ => 0,
                };
                if confidence > 0 {
                    out.push(Suggestion {
                        action: BankAction::PurchaseInvoice {
                            invoice_id: p.id.clone(),
                        },
                        label: format!(
                            "Betaling inkoopfactuur {} {} ({})",
                            p.contact_name,
                            p.supplier_reference.unwrap_or_default(),
                            Money::eur(p.open_cents)
                        ),
                        confidence,
                    });
                }
            }
        }

        // Earlier transactions with the same counterparty, processed directly to an account.
        let previous: Option<(String, Option<String>, i64)> = self
            .conn
            .query_row(
                "SELECT action_account, action_tax_code, COUNT(*) FROM bank_transaction
                 WHERE action_kind = 'account' AND id <> ?1 AND entry_id IS NOT NULL
                   AND ((counterparty_iban IS NOT NULL AND counterparty_iban = ?2) OR (counterparty_name IS NOT NULL AND counterparty_name = ?3))
                   AND (amount_cents < 0) = (?4 < 0)
                 GROUP BY action_account, action_tax_code ORDER BY COUNT(*) DESC LIMIT 1",
                params![t.id, t.counterparty_iban, t.counterparty_name, t.amount_cents],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((account, tax_code, n)) = previous {
            let name: String = self
                .conn
                .query_row("SELECT name FROM account WHERE number = ?1", [&account], |r| r.get(0))
                .unwrap_or_default();
            out.push(Suggestion {
                action: BankAction::Account {
                    account: account.clone(),
                    tax_code: tax_code.clone(),
                    description: None,
                },
                label: format!(
                    "Zoals eerder ({n}×): {account} {name}{}",
                    tax_code.map(|c| format!(", btw {c}")).unwrap_or_default()
                ),
                confidence: (60 + 10 * n.min(3)) as u8,
            });
        }
        let lower = text.to_lowercase();
        if t.amount_cents < 0
            && (lower.contains("kosten") || lower.contains("rente") || lower.contains("fee"))
            && t.counterparty_iban.is_none()
        {
            out.push(Suggestion {
                action: BankAction::Account {
                    account: "4700".into(),
                    tax_code: None,
                    description: None,
                },
                label: "Bankkosten (4700)".into(),
                confidence: 60,
            });
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.confidence));
        Ok(out)
    }

    /// Process a transaction: creates the entry (or marks it ignored). All or nothing.
    pub fn process_bank_transaction(&mut self, id: &str, action: &BankAction) -> Result<BankTransactionView> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let t = load_view(&tx, id)?;
        if t.state != "open" {
            return Err(DbError::Invalid("deze transactie is al verwerkt".into()));
        }
        let bank_ledger: String = tx.query_row(
            "SELECT a.ledger_account FROM bank_account a WHERE a.id = ?1",
            [&t.bank_account_id],
            |r| r.get(0),
        )?;
        let attempt: i64 = tx.query_row(
            "SELECT COUNT(*) + 1 FROM journal_entry WHERE idempotency_key LIKE ?1",
            [format!("bank_transaction:{id}:%")],
            |r| r.get(0),
        )?;
        let key = format!("bank_transaction:{id}:{attempt}");
        let who = t.counterparty_name.clone().unwrap_or_else(|| "onbekend".into());
        let base_desc = if t.description.is_empty() {
            who.clone()
        } else {
            format!("{who} — {}", t.description)
        };
        let short = |s: &str| s.chars().take(200).collect::<String>();

        let (entry_id, kind, account, code): (Option<String>, &str, Option<String>, Option<String>) = match action {
            BankAction::Ignore { reason } => {
                if reason.trim().is_empty() {
                    return Err(DbError::Invalid("geef een reden om de transactie te negeren".into()));
                }
                tx.execute(
                    "UPDATE bank_transaction SET status = 'ignored', ignore_reason = ?2 WHERE id = ?1",
                    params![id, reason.trim()],
                )?;
                (None, "ignore", None, None)
            }
            BankAction::SalesInvoice { invoice_id } => {
                if t.amount_cents <= 0 {
                    return Err(DbError::Invalid("een ontvangst moet een bijschrijving zijn".into()));
                }
                let open = sales_open_cents(&tx, invoice_id)?;
                if open <= 0 {
                    return Err(DbError::Invalid("deze factuur staat niet meer open".into()));
                }
                let e = record_payment(
                    &tx,
                    &actor,
                    Some(invoice_id),
                    None,
                    t.booking_date,
                    t.amount_cents,
                    "bank",
                    &bank_ledger,
                    Some(id),
                    Some(key),
                    &short(&base_desc),
                )?;
                (Some(e), "sales_invoice", None, None)
            }
            BankAction::PurchaseInvoice { invoice_id } => {
                if t.amount_cents >= 0 {
                    return Err(DbError::Invalid("een betaling moet een afschrijving zijn".into()));
                }
                if purchase_open_cents(&tx, invoice_id)? <= 0 {
                    return Err(DbError::Invalid("deze inkoopfactuur staat niet meer open".into()));
                }
                let e = record_payment(
                    &tx,
                    &actor,
                    None,
                    Some(invoice_id),
                    t.booking_date,
                    -t.amount_cents,
                    "bank",
                    &bank_ledger,
                    Some(id),
                    Some(key),
                    &short(&base_desc),
                )?;
                (Some(e), "purchase_invoice", None, None)
            }
            BankAction::PlatformPayout { platform, settle_cents } => {
                if t.amount_cents <= 0 {
                    return Err(DbError::Invalid("een uitbetaling moet een bijschrijving zijn".into()));
                }
                let receivable: String = tx
                    .query_row(
                        "SELECT receivable_account FROM platform_config WHERE platform = ?1",
                        [platform],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| DbError::NotFound(format!("platform {platform}")))?;
                let settle = settle_cents.unwrap_or(t.amount_cents);
                let mut lines = vec![
                    EntryLine::debit(&bank_ledger, t.amount_cents),
                    EntryLine::credit(&receivable, settle),
                ];
                let diff = settle - t.amount_cents;
                if diff != 0 {
                    lines.push(EntryLine::debit(EXCHANGE_DIFFERENCE_ACCOUNT, diff));
                }
                let draft = EntryDraft {
                    journal: JournalCode::BNK,
                    date: t.booking_date,
                    description: short(&base_desc),
                    lines,
                    source: Source {
                        kind: "bank_transaction".into(),
                        reference: Some(id.into()),
                    },
                    idempotency_key: Some(key),
                };
                (
                    Some(post_in_tx(&tx, &actor, &draft, None)?.id),
                    "platform",
                    Some(receivable),
                    None,
                )
            }
            BankAction::Private => {
                let acc = if t.amount_cents > 0 {
                    PRIVATE_CONTRIBUTION
                } else {
                    PRIVATE_WITHDRAWAL
                };
                let draft = EntryDraft {
                    journal: JournalCode::BNK,
                    date: t.booking_date,
                    description: short(&format!("Privé: {base_desc}")),
                    lines: vec![
                        EntryLine::debit(&bank_ledger, t.amount_cents),
                        EntryLine::credit(acc, t.amount_cents),
                    ],
                    source: Source {
                        kind: "bank_transaction".into(),
                        reference: Some(id.into()),
                    },
                    idempotency_key: Some(key),
                };
                (
                    Some(post_in_tx(&tx, &actor, &draft, None)?.id),
                    "private",
                    Some(acc.into()),
                    None,
                )
            }
            BankAction::Account {
                account,
                tax_code,
                description,
            } => {
                let lines = account_lines(
                    &tx,
                    &bank_ledger,
                    account,
                    tax_code.as_deref(),
                    t.amount_cents,
                    t.booking_date,
                )?;
                let draft = EntryDraft {
                    journal: JournalCode::BNK,
                    date: t.booking_date,
                    description: short(
                        description
                            .as_deref()
                            .filter(|d| !d.trim().is_empty())
                            .unwrap_or(&base_desc),
                    ),
                    lines,
                    source: Source {
                        kind: "bank_transaction".into(),
                        reference: Some(id.into()),
                    },
                    idempotency_key: Some(key),
                };
                (
                    Some(post_in_tx(&tx, &actor, &draft, None)?.id),
                    "account",
                    Some(account.clone()),
                    tax_code.clone(),
                )
            }
        };
        if let Some(e) = &entry_id {
            tx.execute(
                "UPDATE bank_transaction SET entry_id = ?2, action_kind = ?3, action_account = ?4, action_tax_code = ?5 WHERE id = ?1",
                params![id, e, kind, account, code],
            )?;
        }
        audit(
            &tx,
            &actor,
            "process",
            "bank_transaction",
            id,
            None::<&()>,
            Some(action),
            entry_id.as_deref(),
        )?;
        let v = load_view(&tx, id)?;
        tx.commit()?;
        Ok(v)
    }

    /// Undo "ignore" (processed transactions are undone by reversing their entry).
    pub fn reopen_bank_transaction(&mut self, id: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE bank_transaction SET status = 'open', ignore_reason = NULL WHERE id = ?1 AND status = 'ignored'",
            [id],
        )?;
        if n == 0 {
            return Err(DbError::Invalid(
                "alleen genegeerde transacties kunnen worden heropend".into(),
            ));
        }
        Ok(())
    }

    /// Compare the ledger with the latest imported statement balance.
    pub fn bank_reconciliation(&self, account_id: &str) -> Result<Reconciliation> {
        let ledger_account: String = self
            .conn
            .query_row(
                "SELECT ledger_account FROM bank_account WHERE id = ?1",
                [account_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("bankrekening {account_id}")))?;
        let statement: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT closing_date, closing_cents FROM bank_import WHERE bank_account_id = ?1 AND closing_date IS NOT NULL
                 ORDER BY closing_date DESC, imported_at DESC LIMIT 1",
                [account_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let balance_until = |until: &str| -> Result<i64> {
            Ok(self.conn.query_row(
                "SELECT COALESCE(SUM(l.amount_cents), 0) FROM journal_line l JOIN journal_entry e ON e.id = l.entry_id
                 WHERE l.account = ?1 AND e.status = 'posted' AND e.entry_date <= ?2",
                params![ledger_account, until],
                |r| r.get(0),
            )?)
        };
        let ledger_now = balance_until("9999-12-31")?;
        let open: Vec<BankTransactionView> = self.bank_transactions(account_id, "open", None)?;
        let (sd, sb, lb, diff) = match statement {
            Some((d, b)) => {
                let at = balance_until(&d)?;
                // Open transactions up to the statement date explain part of the difference.
                (Some(date(&d)?), Some(b), Some(at), Some(b - at))
            }
            None => (None, None, None, None),
        };
        Ok(Reconciliation {
            bank_account_id: account_id.into(),
            ledger_account,
            statement_date: sd,
            statement_balance_cents: sb,
            ledger_balance_at_statement_cents: lb,
            difference_cents: diff,
            open_transactions: open.len() as i64,
            open_sum_cents: open.iter().map(|t| t.amount_cents).sum(),
            ledger_balance_cents: ledger_now,
        })
    }
}

/// Lines for booking a bank amount directly to an account, splitting VAT when a code applies.
fn account_lines(
    conn: &Connection,
    bank_ledger: &str,
    account: &str,
    tax_code: Option<&str>,
    amount: i64,
    on: NaiveDate,
) -> Result<Vec<EntryLine>> {
    let mut lines = vec![EntryLine::debit(bank_ledger, amount)];
    let Some(code) = tax_code.filter(|c| !c.is_empty()) else {
        lines.push(EntryLine::credit(account, amount));
        return Ok(lines);
    };
    let tc = code_on(conn, code, on)?
        .ok_or_else(|| DbError::Invalid(format!("btw-code {code} is niet geldig op {}", iso(on))))?;
    let (payable, deductible): (Option<String>, Option<String>) = conn.query_row(
        "SELECT payable_account, deductible_account FROM tax_code WHERE code = ?1 AND valid_from = ?2",
        params![code, iso(tc.valid_from)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let bp = tc.rate_bp.unwrap_or(0);
    let tag = |l: EntryLine| l.with_tax_code(code);
    if tc.reverse_charge && amount < 0 {
        // Foreign service paid: full amount is the cost; self-assess VAT and deduct it.
        lines.push(tag(EntryLine::credit(account, amount)));
        let vat = Money::eur(-amount)
            .apply_basis_points(bp)
            .map_err(|e| DbError::Invalid(e.to_string()))?
            .cents;
        if let (Some(d), Some(p)) = (deductible, payable) {
            if vat != 0 {
                lines.push(tag(EntryLine::debit(&d, vat)));
                lines.push(tag(EntryLine::credit(&p, vat)));
            }
        }
        return Ok(lines);
    }
    let vat_account = if amount < 0 { deductible } else { payable };
    match vat_account {
        Some(va) if bp > 0 && !tc.reverse_charge => {
            let (net, vat) = split_gross(amount, bp);
            lines.push(tag(EntryLine::credit(account, net)));
            lines.push(tag(EntryLine::credit(&va, vat)));
        }
        _ => lines.push(tag(EntryLine::credit(account, amount))),
    }
    Ok(lines.into_iter().filter(|l| l.amount.cents != 0).collect())
}

#[cfg(test)]
mod split_tests {
    use super::split_gross;

    #[test]
    fn splits_vat_out_of_gross_amounts() {
        assert_eq!(split_gross(-12_100, 2100), (-10_000, -2_100));
        assert_eq!(split_gross(10_900, 900), (10_000, 900));
        assert_eq!(split_gross(-999, 2100), (-826, -173));
    }
}
