//! Tauri layer: opens the ledger and exposes read-only commands to the UI.
//! Keep this thin — business rules belong in `boekhoud-core` / `boekhoud-db`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use boekhoud_core::Account;
use boekhoud_db::{ChainStatus, Ledger, TrialBalanceRow};
use serde::Serialize;
use tauri::{Manager, State};

struct AppState {
    ledger: Mutex<Ledger>,
    data_dir: PathBuf,
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    data_dir: String,
}

type CmdResult<T> = Result<T, String>;

fn with_ledger<T>(
    state: &State<AppState>,
    f: impl FnOnce(&Ledger) -> boekhoud_db::Result<T>,
) -> CmdResult<T> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;
    f(&ledger).map_err(|e| e.to_string())
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
fn trial_balance(state: State<AppState>) -> CmdResult<Vec<TrialBalanceRow>> {
    with_ledger(&state, |l| l.trial_balance(None))
}

#[tauri::command]
fn verify_chain(state: State<AppState>) -> CmdResult<ChainStatus> {
    with_ledger(&state, |l| l.verify_chain())
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
            trial_balance,
            verify_chain
        ])
        .run(tauri::generate_context!())
        .expect("error while running Boekhoud");
}
