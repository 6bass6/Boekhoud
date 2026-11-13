//! SQLite storage for Boekhoud: migrations, the ledger posting function, periods,
//! trial balance and hash-chain verification.
//!
//! [`Ledger::post`] is the only way to create posted journal entries.

mod assets;
mod bank;
mod company;
mod contacts;
mod documents;
mod hours;
mod income_tax;
mod migrations;
mod platforms;
mod purchases;
mod queries;
mod reports;
mod sales;
mod tax;
mod vat;

use std::path::Path;

use boekhoud_core::hash::{chain_hash, CanonicalEntry, CanonicalLine, GENESIS_HASH};
use boekhoud_core::journal::reversal_of;
use boekhoud_core::{Account, AccountType, Currency, EntryDraft, EntryLine, JournalCode, Money, ValidationError};
use chrono::{Datelike, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use thiserror::Error;

pub use assets::{depreciation_for_month, AssetInput, AssetView};
pub use bank::{
    split_gross, BankAccount, BankAction, BankImportInput, BankImportResult, BankTransactionView, NewBankTransaction,
    Reconciliation, Suggestion,
};
pub use company::Company;
pub use contacts::Contact;
pub use documents::{mime_from_filename, DocumentInfo};
pub use hours::{HoursSummary, TimeEntry, HOURS_CRITERION};
pub use income_tax::IncomeTaxView;
pub use migrations::MIGRATIONS;
pub use platforms::{
    CurrencyAmount, EurAmount, PlatformConfig, PlatformReportInput, PlatformReportView, ProductAmount, ProductRevenue,
};
pub use purchases::{PurchaseInput, PurchaseInvoiceDetail, PurchaseInvoiceSummary};
pub use queries::{Dashboard, EntryDetail, EntrySummary, LedgerCard, LedgerCardRow, LineView, PeriodView};
pub use reports::{
    balance_sheet_csv, csv_amount, profit_and_loss_csv, trial_balance_csv, BalanceSheet, ProfitAndLoss, ReportLine,
};
pub use sales::{IssueRequirements, SalesInvoiceDetail, SalesInvoiceInput, SalesInvoiceSummary};
pub use tax::{TaxCode, VatRegime};
pub use vat::{quarter_of, FiledInfo, IcpLine, RubricLine, VatDetailLine, VatReturn};

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid entry: {0:?}")]
    Validation(Vec<ValidationError>),
    #[error("this event was already posted (idempotency key {0})")]
    Duplicate(String),
    #[error("period {0}-{1:02} is closed")]
    PeriodClosed(i32, u32),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("entry {0} was already reversed")]
    AlreadyReversed(String),
    #[error("corrupt data: {0}")]
    Corrupt(String),
    /// A user-facing (Dutch) explanation of why the request cannot be carried out.
    #[error("{0}")]
    Invalid(String),
    #[error("file error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// Summary of a posted entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostedEntry {
    pub id: String,
    pub journal: JournalCode,
    pub fiscal_year: i32,
    pub sequence: i64,
    pub date: NaiveDate,
    pub hash: String,
}

impl PostedEntry {
    /// Human-readable entry number, e.g. `VRK-2026-0007`.
    pub fn number(&self) -> String {
        format!("{}-{}-{:04}", self.journal.as_str(), self.fiscal_year, self.sequence)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrialBalanceRow {
    pub account: String,
    pub name: String,
    pub account_type: AccountType,
    pub debit_cents: i64,
    pub credit_cents: i64,
    /// debit − credit
    pub balance_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChainStatus {
    /// Number of posted entries verified.
    Intact(usize),
    /// The first entry (by chain index) whose stored hash does not match.
    Broken { chain_index: i64, entry_id: String },
}

pub struct Ledger {
    conn: Connection,
    actor: String,
    document_root: Option<std::path::PathBuf>,
}

impl Ledger {
    /// Open (or create) a database file and apply pending migrations.
    ///
    /// Callers that care about their data should call [`Ledger::pending_migrations`] and
    /// make a backup first; see `docs/architecture.md` §6.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let mut ledger = Self::init(conn)?;
        ledger.document_root = path.parent().map(|p| p.join("documents"));
        Ok(ledger)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        migrations::apply(&conn)?;
        Ok(Ledger {
            conn,
            actor: "owner".into(),
            document_root: None,
        })
    }

    /// Directory for the content-addressed document store (defaults to `<db dir>/documents`).
    pub fn set_document_root(&mut self, dir: &Path) {
        self.document_root = Some(dir.to_path_buf());
    }

    /// Migrations that would be applied when opening the file at `path`.
    pub fn pending_migrations(path: &Path) -> Result<Vec<u32>> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let conn = Connection::open(path)?;
        migrations::pending(&conn)
    }

    /// Consistent online backup of the database to `dest`.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        self.conn.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self
            .conn
            .prepare("SELECT number, name, account_type, rgs_code, active FROM account ORDER BY number")?;
        let rows = stmt.query_map([], |r| {
            let t: String = r.get(2)?;
            Ok(Account {
                number: r.get(0)?,
                name: r.get(1)?,
                account_type: t.parse().map_err(|e: String| {
                    rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
                })?,
                rgs_code: r.get(3)?,
                active: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Validate and post an entry: assigns the next gapless number in its journal and year,
    /// extends the hash chain and writes an audit log record — all in one transaction.
    pub fn post(&mut self, draft: &EntryDraft) -> Result<PostedEntry> {
        self.post_inner(draft, None)
    }

    fn post_inner(&mut self, draft: &EntryDraft, reverses: Option<&str>) -> Result<PostedEntry> {
        let actor = self.actor.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let posted = post_in_tx(&tx, &actor, draft, reverses)?;
        tx.commit()?;
        Ok(posted)
    }

    /// Fully reverse a posted entry with a new entry dated `date`.
    pub fn reverse(&mut self, entry_id: &str, date: NaiveDate, reason: &str) -> Result<PostedEntry> {
        let (journal, year, sequence, status): (String, i32, Option<i64>, String) = self
            .conn
            .query_row(
                "SELECT journal, fiscal_year, sequence, status FROM journal_entry WHERE id = ?1",
                [entry_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(entry_id.to_string()))?;
        if status != "posted" {
            return Err(DbError::NotFound(format!("posted entry {entry_id}")));
        }
        let already: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entry WHERE reverses_entry_id = ?1)",
            [entry_id],
            |r| r.get(0),
        )?;
        if already {
            return Err(DbError::AlreadyReversed(entry_id.to_string()));
        }
        let journal =
            JournalCode::parse(&journal).ok_or_else(|| DbError::Corrupt(format!("unknown journal {journal}")))?;
        let number = format!("{}-{}-{:04}", journal.as_str(), year, sequence.unwrap_or_default());
        let lines = self.entry_lines(entry_id)?;
        let draft = reversal_of(
            &lines,
            journal,
            date,
            format!("Tegenboeking {number}: {reason}"),
            entry_id,
        );
        self.post_inner(&draft, Some(entry_id))
    }

    pub(crate) fn entry_lines(&self, entry_id: &str) -> Result<Vec<EntryLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT account, amount_cents, currency, tax_code, description
             FROM journal_line WHERE entry_id = ?1 ORDER BY line_no",
        )?;
        let rows = stmt.query_map([entry_id], |r| {
            let cur: String = r.get(2)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, cur, r.get(3)?, r.get(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (account, cents, cur, tax_code, description) = row?;
            let currency = Currency::new(&cur).map_err(|e| DbError::Corrupt(e.to_string()))?;
            out.push(EntryLine {
                account,
                amount: Money::new(cents, currency),
                tax_code,
                description,
            });
        }
        Ok(out)
    }

    /// Close a month: no further postings dated in it.
    pub fn close_period(&mut self, year: i32, month: u32) -> Result<()> {
        self.set_period(year, month, "closed", None)
    }

    /// Reopen a closed month. Requires a reason, which is recorded in the audit log.
    pub fn reopen_period(&mut self, year: i32, month: u32, reason: &str) -> Result<()> {
        if reason.trim().is_empty() {
            return Err(DbError::Validation(vec![ValidationError::MissingDescription]));
        }
        self.set_period(year, month, "open", Some(reason))
    }

    fn set_period(&mut self, year: i32, month: u32, status: &str, reason: Option<&str>) -> Result<()> {
        if !(1..=12).contains(&month) {
            return Err(DbError::NotFound(format!("period {year}-{month}")));
        }
        let tx = self.conn.transaction()?;
        let old: Option<String> = tx
            .query_row(
                "SELECT status FROM period WHERE year=?1 AND month=?2",
                params![year, month],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute(
            "INSERT INTO period (year, month, status) VALUES (?1, ?2, ?3)
             ON CONFLICT(year, month) DO UPDATE SET status = excluded.status,
                 changed_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![year, month, status],
        )?;
        let new_value = match reason {
            Some(r) => format!("{status}: {r}"),
            None => status.to_string(),
        };
        tx.execute(
            "INSERT INTO audit_log (actor, action, entity_type, entity_id, old_value, new_value)
             VALUES (?1, ?2, 'period', ?3, ?4, ?5)",
            params![
                self.actor,
                if status == "closed" {
                    "close_period"
                } else {
                    "reopen_period"
                },
                format!("{year}-{month:02}"),
                old.unwrap_or_else(|| "open".into()),
                new_value
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Trial balance (proef- en saldibalans) over all posted entries up to and including `until`.
    pub fn trial_balance(&self, until: Option<NaiveDate>) -> Result<Vec<TrialBalanceRow>> {
        let until = until
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "9999-12-31".into());
        let mut stmt = self.conn.prepare(
            "SELECT a.number, a.name, a.account_type,
                    COALESCE(SUM(CASE WHEN l.amount_cents > 0 THEN l.amount_cents END), 0),
                    COALESCE(SUM(CASE WHEN l.amount_cents < 0 THEN -l.amount_cents END), 0)
             FROM account a
             JOIN journal_line l ON l.account = a.number
             JOIN journal_entry e ON e.id = l.entry_id
             WHERE e.status = 'posted' AND e.entry_date <= ?1
             GROUP BY a.number ORDER BY a.number",
        )?;
        let rows = stmt.query_map([until], |r| {
            let t: String = r.get(2)?;
            let debit: i64 = r.get(3)?;
            let credit: i64 = r.get(4)?;
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, t, debit, credit))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (account, name, t, debit, credit) = row?;
            out.push(TrialBalanceRow {
                account,
                name,
                account_type: t.parse().map_err(DbError::Corrupt)?,
                debit_cents: debit,
                credit_cents: credit,
                balance_cents: debit - credit,
            });
        }
        Ok(out)
    }

    /// Recompute every hash in chain order and compare with what is stored.
    pub fn verify_chain(&self) -> Result<ChainStatus> {
        let mut stmt = self.conn.prepare(
            "SELECT id, journal, fiscal_year, sequence, entry_date, description, source_kind,
                    source_reference, reverses_entry_id, chain_index, prev_hash, hash
             FROM journal_entry WHERE status = 'posted' ORDER BY chain_index",
        )?;
        type Row = (
            String,
            String,
            i32,
            i64,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            i64,
            String,
            String,
        );
        let entries: Vec<Row> = stmt
            .query_map([], |r| {
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
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut expected_prev = GENESIS_HASH.to_string();
        for (expected_index, (id, journal, year, seq, date, desc, kind, src_ref, reverses, idx, prev, stored)) in
            (1i64..).zip(entries.iter())
        {
            let lines = self.entry_lines(id)?;
            let canonical = CanonicalEntry {
                entry_id: id,
                journal,
                fiscal_year: *year,
                sequence: *seq,
                date,
                description: desc,
                source_kind: kind,
                source_reference: src_ref.as_deref(),
                reverses_entry_id: reverses.as_deref(),
                lines: lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| CanonicalLine {
                        line_no: (i + 1) as i64,
                        account: &l.account,
                        amount_cents: l.amount.cents,
                        currency: l.amount.currency.as_str(),
                        tax_code: l.tax_code.as_deref(),
                        description: l.description.as_deref(),
                    })
                    .collect(),
            };
            let recomputed = chain_hash(&expected_prev, &canonical);
            if *idx != expected_index || *prev != expected_prev || recomputed != *stored {
                return Ok(ChainStatus::Broken {
                    chain_index: *idx,
                    entry_id: id.clone(),
                });
            }
            expected_prev = stored.clone();
        }
        Ok(ChainStatus::Intact(entries.len()))
    }

    /// Raw connection access for tests in this crate.
    #[cfg(test)]
    fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// Append an audit-log record inside the caller's transaction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn audit<O: Serialize + ?Sized, N: Serialize + ?Sized>(
    tx: &Connection,
    actor: &str,
    action: &str,
    entity_type: &str,
    entity_id: &str,
    old: Option<&O>,
    new: Option<&N>,
    entry_id: Option<&str>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO audit_log (actor, action, entity_type, entity_id, old_value, new_value, entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            actor,
            action,
            entity_type,
            entity_id,
            old.map(|o| serde_json::to_string(o).unwrap_or_default()),
            new.map(|n| serde_json::to_string(n).unwrap_or_default()),
            entry_id
        ],
    )?;
    Ok(())
}

/// Post an entry inside an existing transaction; the caller commits.
/// This is the single place where journal entries become posted.
pub(crate) fn post_in_tx(
    tx: &Connection,
    actor: &str,
    draft: &EntryDraft,
    reverses: Option<&str>,
) -> Result<PostedEntry> {
    draft.validate().map_err(DbError::Validation)?;

    // Accounts must exist and be active.
    let mut errors = Vec::new();
    for acc in draft.accounts() {
        let active: Option<bool> = tx
            .query_row("SELECT active FROM account WHERE number = ?1", [acc], |r| r.get(0))
            .optional()?;
        match active {
            None => errors.push(ValidationError::UnknownAccount(acc.to_string())),
            Some(false) => errors.push(ValidationError::InactiveAccount(acc.to_string())),
            Some(true) => {}
        }
    }
    let regime = tax::regime_on(tx, draft.date)?;
    let codes: std::collections::BTreeSet<&str> = draft.lines.iter().filter_map(|l| l.tax_code.as_deref()).collect();
    for code in codes {
        match tax::code_on(tx, code, draft.date)? {
            None => errors.push(ValidationError::UnknownTaxCode(code.to_string())),
            Some(tc) => {
                if let Some(required) = tc.regime {
                    if required != regime {
                        errors.push(ValidationError::TaxCodeNotAllowed {
                            code: code.to_string(),
                            regime: regime.as_str().to_string(),
                        });
                    }
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(DbError::Validation(errors));
    }

    if let Some(key) = &draft.idempotency_key {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entry WHERE idempotency_key = ?1)",
            [key],
            |r| r.get(0),
        )?;
        if exists {
            return Err(DbError::Duplicate(key.clone()));
        }
    }

    let (year, month) = (draft.date.year(), draft.date.month());
    let closed: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM period WHERE year=?1 AND month=?2 AND status='closed')",
        params![year, month],
        |r| r.get(0),
    )?;
    if closed {
        return Err(DbError::PeriodClosed(year, month));
    }

    let id = uuid::Uuid::now_v7().to_string();
    let date_str = draft.date.format("%Y-%m-%d").to_string();
    tx.execute(
        "INSERT INTO journal_entry (id, journal, fiscal_year, entry_date, description,
             source_kind, source_reference, idempotency_key, status, reverses_entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'draft', ?9)",
        params![
            id,
            draft.journal.as_str(),
            year,
            date_str,
            draft.description,
            draft.source.kind,
            draft.source.reference,
            draft.idempotency_key,
            reverses,
        ],
    )?;
    for (i, line) in draft.lines.iter().enumerate() {
        tx.execute(
            "INSERT INTO journal_line (entry_id, line_no, account, amount_cents, currency, tax_code, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                (i + 1) as i64,
                line.account,
                line.amount.cents,
                line.amount.currency.as_str(),
                line.tax_code,
                line.description,
            ],
        )?;
    }

    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM journal_entry
         WHERE journal = ?1 AND fiscal_year = ?2 AND status = 'posted'",
        params![draft.journal.as_str(), year],
        |r| r.get(0),
    )?;
    let (chain_index, prev_hash): (i64, String) = tx
        .query_row(
            "SELECT chain_index + 1, hash FROM journal_entry
             WHERE status = 'posted' ORDER BY chain_index DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((1, GENESIS_HASH.to_string()));

    let canonical = CanonicalEntry {
        entry_id: &id,
        journal: draft.journal.as_str(),
        fiscal_year: year,
        sequence,
        date: &date_str,
        description: &draft.description,
        source_kind: &draft.source.kind,
        source_reference: draft.source.reference.as_deref(),
        reverses_entry_id: reverses,
        lines: draft
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| CanonicalLine {
                line_no: (i + 1) as i64,
                account: &l.account,
                amount_cents: l.amount.cents,
                currency: l.amount.currency.as_str(),
                tax_code: l.tax_code.as_deref(),
                description: l.description.as_deref(),
            })
            .collect(),
    };
    let hash = chain_hash(&prev_hash, &canonical);

    tx.execute(
        "UPDATE journal_entry SET status='posted', sequence=?2, chain_index=?3, prev_hash=?4,
             hash=?5, posted_at=?6 WHERE id=?1",
        params![id, sequence, chain_index, prev_hash, hash, Utc::now().to_rfc3339()],
    )?;

    let posted = PostedEntry {
        id: id.clone(),
        journal: draft.journal,
        fiscal_year: year,
        sequence,
        date: draft.date,
        hash,
    };
    tx.execute(
        "INSERT INTO audit_log (actor, action, entity_type, entity_id, new_value, entry_id)
         VALUES (?1, 'post', 'journal_entry', ?2, ?3, ?2)",
        params![
            actor,
            id,
            serde_json::to_string(draft).map_err(|e| DbError::Corrupt(e.to_string()))?
        ],
    )?;
    Ok(posted)
}

#[cfg(test)]
mod tests;
