//! Tauri layer: opens the ledger and exposes commands to the UI.
//! Keep this thin — business rules belong in `boekhoud-core` / `boekhoud-db`.
//! Amounts arrive from the UI as text and are parsed here with `Money::parse`, never as floats.

mod format;
mod invoicing;
mod messages;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use boekhoud_core::{Account, Currency, EntryDraft, EntryLine, JournalCode, Money, Source};
use boekhoud_db::{
    ChainStatus, Company, EntryDetail, EntrySummary, Ledger, LedgerCard, PeriodView, PostedEntry,
    TaxCode, TrialBalanceRow, VatRegime,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

pub(crate) struct AppState {
    ledger: Mutex<Ledger>,
    pub(crate) data_dir: PathBuf,
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    data_dir: String,
}

pub(crate) type CmdResult<T> = Result<T, String>;

/// Format cents for messages, e.g. -1234 -> "-€ 12,34".
pub(crate) fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}€ {},{:02}", abs / 100, abs % 100)
}

pub(crate) fn with_ledger<T>(
    state: &State<AppState>,
    f: impl FnOnce(&mut Ledger) -> boekhoud_db::Result<T>,
) -> CmdResult<T> {
    let mut ledger = state
        .ledger
        .lock()
        .map_err(|_| "Database is vergrendeld.".to_string())?;
    f(&mut ledger).map_err(|e| messages::db_nl(&e))
}

pub(crate) fn parse_date(s: &str) -> CmdResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("Ongeldige datum: {s}"))
}

fn parse_journal(s: &str) -> CmdResult<JournalCode> {
    JournalCode::parse(s).ok_or_else(|| format!("Onbekend dagboek: {s}"))
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

#[derive(Deserialize)]
struct NewLine {
    account: String,
    /// Dutch-formatted amount text, e.g. "1.210,00". Exactly one of debit/credit is filled.
    debit: String,
    credit: String,
    tax_code: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct NewEntry {
    journal: String,
    date: String,
    description: String,
    lines: Vec<NewLine>,
}

fn build_draft(input: NewEntry) -> CmdResult<EntryDraft> {
    let mut lines = Vec::new();
    for (i, l) in input.lines.into_iter().enumerate() {
        let n = i + 1;
        let (d, c) = (l.debit.trim(), l.credit.trim());
        if d.is_empty() && c.is_empty() && l.account.trim().is_empty() {
            continue; // fully empty row in the form
        }
        if d.starts_with('-') || c.starts_with('-') {
            return Err(format!(
                "Regel {n}: gebruik geen negatieve bedragen; kies debet of credit."
            ));
        }
        let amount = match (d.is_empty(), c.is_empty()) {
            (false, true) => Money::parse(d, ',', Currency::EUR),
            (true, false) => Money::parse(c, ',', Currency::EUR).map(|m| -m),
            (true, true) => return Err(format!("Regel {n}: vul een debet- of creditbedrag in.")),
            (false, false) => {
                return Err(format!("Regel {n}: vul óf debet óf credit in, niet beide."))
            }
        }
        .map_err(|_| format!("Regel {n}: ongeldig bedrag."))?;
        lines.push(EntryLine {
            account: l.account.trim().to_string(),
            amount,
            tax_code: non_empty(l.tax_code),
            description: non_empty(l.description),
        });
    }
    Ok(EntryDraft {
        journal: parse_journal(&input.journal)?,
        date: parse_date(&input.date)?,
        description: input.description.trim().to_string(),
        lines,
        source: Source::manual(),
        idempotency_key: None,
    })
}

#[tauri::command]
fn app_info(app: tauri::AppHandle, state: State<AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
    }
}

#[tauri::command]
fn accounts(state: State<AppState>) -> CmdResult<Vec<Account>> {
    with_ledger(&state, |l| l.accounts())
}

#[tauri::command]
fn tax_codes(state: State<AppState>, date: String) -> CmdResult<Vec<TaxCode>> {
    let date = parse_date(&date)?;
    with_ledger(&state, |l| l.tax_codes(date))
}

#[tauri::command]
fn post_entry(state: State<AppState>, entry: NewEntry) -> CmdResult<PostedEntry> {
    let draft = build_draft(entry)?;
    with_ledger(&state, |l| l.post(&draft))
}

#[tauri::command]
fn list_entries(
    state: State<AppState>,
    year: Option<i32>,
    journal: Option<String>,
) -> CmdResult<Vec<EntrySummary>> {
    let journal = journal
        .filter(|j| !j.is_empty())
        .map(|j| parse_journal(&j))
        .transpose()?;
    with_ledger(&state, |l| l.list_entries(year, journal, 1000))
}

#[tauri::command]
fn entry_detail(state: State<AppState>, id: String) -> CmdResult<EntryDetail> {
    with_ledger(&state, |l| l.entry_detail(&id))
}

#[tauri::command]
fn reverse_entry(
    state: State<AppState>,
    id: String,
    date: String,
    reason: String,
) -> CmdResult<PostedEntry> {
    let date = parse_date(&date)?;
    if reason.trim().is_empty() {
        return Err("Geef een reden voor de tegenboeking.".into());
    }
    with_ledger(&state, |l| l.reverse(&id, date, reason.trim()))
}

#[tauri::command]
fn trial_balance(state: State<AppState>, until: Option<String>) -> CmdResult<Vec<TrialBalanceRow>> {
    let until = until
        .filter(|s| !s.is_empty())
        .map(|s| parse_date(&s))
        .transpose()?;
    with_ledger(&state, |l| l.trial_balance(until))
}

#[tauri::command]
fn ledger_card(state: State<AppState>, account: String, year: i32) -> CmdResult<LedgerCard> {
    with_ledger(&state, |l| l.ledger_card(&account, year))
}

#[tauri::command]
fn periods(state: State<AppState>, year: i32) -> CmdResult<Vec<PeriodView>> {
    with_ledger(&state, |l| l.periods(year))
}

#[tauri::command]
fn close_period(state: State<AppState>, year: i32, month: u32) -> CmdResult<()> {
    with_ledger(&state, |l| l.close_period(year, month))
}

#[tauri::command]
fn reopen_period(state: State<AppState>, year: i32, month: u32, reason: String) -> CmdResult<()> {
    if reason.trim().is_empty() {
        return Err("Geef een reden om de periode te heropenen.".into());
    }
    with_ledger(&state, |l| l.reopen_period(year, month, reason.trim()))
}

#[tauri::command]
fn verify_chain(state: State<AppState>) -> CmdResult<ChainStatus> {
    with_ledger(&state, |l| l.verify_chain())
}

#[tauri::command]
fn company(state: State<AppState>) -> CmdResult<Option<Company>> {
    with_ledger(&state, |l| l.company())
}

#[tauri::command]
fn save_company(state: State<AppState>, company: Company) -> CmdResult<()> {
    if company.legal_name.trim().is_empty() {
        return Err("Vul de statutaire naam in.".into());
    }
    let company = Company {
        legal_name: company.legal_name.trim().to_string(),
        trade_name: non_empty(company.trade_name),
        kvk_number: non_empty(company.kvk_number),
        vat_number: non_empty(company.vat_number),
        address: non_empty(company.address),
        postal_code: non_empty(company.postal_code),
        city: non_empty(company.city),
        country: if company.country.trim().is_empty() {
            "NL".into()
        } else {
            company.country.trim().to_uppercase()
        },
        iban: non_empty(company.iban),
        email: non_empty(company.email),
        payment_term_days: company.payment_term_days,
    };
    with_ledger(&state, |l| l.save_company(&company))
}

#[tauri::command]
fn vat_regime(state: State<AppState>, date: String) -> CmdResult<VatRegime> {
    let date = parse_date(&date)?;
    with_ledger(&state, |l| l.vat_regime_on(date))
}

#[tauri::command]
fn set_vat_regime(
    state: State<AppState>,
    valid_from: String,
    regime: String,
    note: String,
) -> CmdResult<()> {
    let date = parse_date(&valid_from)?;
    let regime = match regime.as_str() {
        "regular" => VatRegime::Regular,
        "kor" => VatRegime::Kor,
        _ => return Err("Onbekend btw-regime.".into()),
    };
    with_ledger(&state, |l| l.set_vat_regime(date, regime, note.trim()))
}

#[tauri::command]
fn backup_now(state: State<AppState>) -> CmdResult<String> {
    let dir = state.data_dir.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Back-upmap aanmaken mislukt: {e}"))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("handmatig-{stamp}.sqlite"));
    with_ledger(&state, |l| l.backup_to(&dest))?;
    Ok(dest.display().to_string())
}

/// Open the database, making a timestamped backup first if migrations are pending.
fn open_ledger(data_dir: &Path) -> Result<Ledger, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let db_path = data_dir.join("boekhoud.sqlite");
    if !Ledger::pending_migrations(&db_path)?.is_empty() {
        let backups = data_dir.join("backups");
        std::fs::create_dir_all(&backups)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        std::fs::copy(
            &db_path,
            backups.join(format!("voor-migratie-{stamp}.sqlite")),
        )?;
    }
    Ok(Ledger::open(&db_path)?)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let ledger = open_ledger(&data_dir)?;
            app.manage(AppState {
                ledger: Mutex::new(ledger),
                data_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            accounts,
            tax_codes,
            post_entry,
            list_entries,
            entry_detail,
            reverse_entry,
            trial_balance,
            ledger_card,
            periods,
            close_period,
            reopen_period,
            verify_chain,
            company,
            save_company,
            vat_regime,
            set_vat_regime,
            backup_now,
            invoicing::contacts,
            invoicing::save_contact,
            invoicing::sales_invoices,
            invoicing::sales_invoice,
            invoicing::save_sales_invoice,
            invoicing::delete_sales_invoice,
            invoicing::sales_issue_requirements,
            invoicing::issue_sales_invoice,
            invoicing::create_credit_note,
            invoicing::register_sales_payment,
            invoicing::open_sales_invoice_pdf,
            invoicing::register_purchase_invoice,
            invoicing::purchase_invoices,
            invoicing::purchase_invoice,
            invoicing::attach_purchase_document,
            invoicing::register_purchase_payment,
            invoicing::upload_document,
            invoicing::document_content,
            invoicing::open_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running Boekhoud");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(account: &str, debit: &str, credit: &str) -> NewLine {
        NewLine {
            account: account.into(),
            debit: debit.into(),
            credit: credit.into(),
            tax_code: None,
            description: None,
        }
    }

    fn input(lines: Vec<NewLine>) -> NewEntry {
        NewEntry {
            journal: "MEM".into(),
            date: "2026-11-11".into(),
            description: "Test".into(),
            lines,
        }
    }

    #[test]
    fn form_input_becomes_signed_cents() {
        let d = build_draft(input(vec![
            line("4900", "1.210,50", ""),
            line("1600", "", "1.210,50"),
            line("", "", ""),
        ]))
        .unwrap();
        assert_eq!(d.lines.len(), 2);
        assert_eq!(d.lines[0].amount.cents, 121_050);
        assert_eq!(d.lines[1].amount.cents, -121_050);
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn form_rejects_ambiguous_lines() {
        assert!(build_draft(input(vec![line("4900", "1", "1")])).is_err());
        assert!(build_draft(input(vec![line("4900", "", "")])).is_err());
        assert!(build_draft(input(vec![line("4900", "abc", "")])).is_err());
        assert!(build_draft(input(vec![line("4900", "-5", "")])).is_err());
        assert!(build_draft(input(vec![line("4900", "", "-5")])).is_err());
    }

    #[test]
    fn dutch_messages_cover_errors() {
        let msg = messages::db_nl(&boekhoud_db::DbError::Validation(vec![
            boekhoud_core::ValidationError::Unbalanced(-150),
        ]));
        assert_eq!(msg, "De boeking is niet in balans (verschil -€ 1,50).");
    }
}
