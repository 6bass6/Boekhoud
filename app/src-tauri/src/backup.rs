//! Backup, export and import commands. The logic lives in `boekhoud_db::backup`; this layer only
//! decides where files go and holds the ledger lock just for the database snapshot.

use std::path::{Path, PathBuf};

use boekhoud_db::backup::{self, BackupInfo, BackupSettings};
use boekhoud_db::Ledger;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{messages, AppState, CmdResult};

#[derive(Serialize)]
pub struct BackupStatus {
    /// Backups in the data dir's `backups/` folder, newest first.
    backups: Vec<BackupInfo>,
    last_good: Option<BackupInfo>,
    /// Whole days since the last good backup; `None` when there is none.
    days_since_good: Option<i64>,
    backup_dir: String,
    extra_dir: Option<String>,
    /// Outcome of the last backup to the extra folder, or why it was skipped.
    notice: Option<String>,
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

fn snapshot(state: &AppState, dest: &Path) -> boekhoud_db::Result<()> {
    let ledger = state
        .ledger
        .lock()
        .map_err(|_| boekhoud_db::DbError::Invalid("database is vergrendeld".into()))?;
    ledger.backup_to(dest)
}

fn summary(info: &BackupInfo) -> String {
    match &info.verification {
        Some(v) if v.ok => format!("{} gemaakt en gecontroleerd", info.file),
        Some(v) => format!("{} mislukt: {}", info.file, v.problems.join("; ")),
        None => format!("{} niet gecontroleerd", info.file),
    }
}

/// Copy to the extra folder (if configured); the result is kept as a notice for the UI.
fn backup_to_extra(state: &AppState, auto: bool) {
    let settings = BackupSettings::load(&state.data_dir);
    let docs = state.data_dir.join("documents");
    let notice = match settings.extra_target() {
        None => None,
        Some(Err(e)) => Some(format!("Extra map: {e}.")),
        Some(Ok(target)) => {
            let result = if auto {
                backup::auto_backup_if_due(&target, &docs, now(), settings.keep, |p| snapshot(state, p))
            } else {
                backup::create_backup(&target, &docs, backup::MANUAL_PREFIX, now(), |p| snapshot(state, p)).map(Some)
            };
            match result {
                Ok(Some(info)) => Some(format!("Extra map: {}.", summary(&info))),
                Ok(None) => return, // nothing due; keep the previous notice
                Err(e) => Some(format!("Extra map: {}", messages::db_nl(&e))),
            }
        }
    };
    if let Ok(mut n) = state.backup_notice.lock() {
        *n = notice;
    }
}

/// At app start: make an automatic backup when the last good one is older than 24 hours.
/// Runs in a background thread and never fails the app start; problems show up in Instellingen.
pub(crate) fn start_auto_backup(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let settings = BackupSettings::load(&state.data_dir);
        let local = state.data_dir.join("backups");
        if let Err(e) =
            backup::auto_backup_if_due(&local, &state.data_dir.join("documents"), now(), settings.keep, |p| {
                snapshot(&state, p)
            })
        {
            eprintln!("automatische back-up mislukt: {}", messages::db_nl(&e));
        }
        backup_to_extra(&state, true);
    });
}

fn status(state: &AppState) -> BackupStatus {
    let dir = state.data_dir.join("backups");
    let backups = backup::list_backups(&dir);
    let last_good = backup::last_good_backup(&backups).cloned();
    let days_since_good = last_good
        .as_ref()
        .and_then(|b| chrono::NaiveDateTime::parse_from_str(b.created.as_deref()?, "%Y-%m-%d %H:%M:%S").ok())
        .map(|t| (now() - t).num_days());
    BackupStatus {
        backups,
        last_good,
        days_since_good,
        backup_dir: dir.display().to_string(),
        extra_dir: BackupSettings::load(&state.data_dir).extra_dir,
        notice: state.backup_notice.lock().ok().and_then(|n| n.clone()),
    }
}

#[tauri::command]
pub fn backup_status(state: State<AppState>) -> BackupStatus {
    status(&state)
}

/// Manual backup: same code path as the automatic one, with the `handmatig-` prefix.
#[tauri::command]
pub fn backup_now(state: State<AppState>) -> CmdResult<BackupStatus> {
    let info = backup::create_backup(
        &state.data_dir.join("backups"),
        &state.data_dir.join("documents"),
        backup::MANUAL_PREFIX,
        now(),
        |p| snapshot(&state, p),
    )
    .map_err(|e| messages::db_nl(&e))?;
    backup_to_extra(&state, false);
    match &info.verification {
        Some(v) if !v.ok => Err(format!("Back-up mislukt: {}.", v.problems.join("; "))),
        _ => Ok(status(&state)),
    }
}

#[tauri::command]
pub fn save_backup_settings(state: State<AppState>, extra_dir: Option<String>) -> CmdResult<BackupStatus> {
    let extra_dir = crate::non_empty(extra_dir);
    if let Some(d) = &extra_dir {
        if !Path::new(d).is_absolute() {
            return Err("Geef een volledig pad op, bijvoorbeeld E:\\ of C:\\Users\\naam\\OneDrive\\Back-up.".into());
        }
    }
    let mut settings = BackupSettings::load(&state.data_dir);
    settings.extra_dir = extra_dir;
    settings.save(&state.data_dir).map_err(|e| messages::db_nl(&e))?;
    if let Ok(mut n) = state.backup_notice.lock() {
        *n = None;
    }
    Ok(status(&state))
}

/// Export to `dest` (a folder the owner typed) or to `<data dir>/exports`. Returns the export folder.
#[tauri::command]
pub fn export_administration(app: AppHandle, state: State<AppState>, dest: Option<String>) -> CmdResult<String> {
    let parent = match crate::non_empty(dest) {
        Some(d) if Path::new(&d).is_dir() => PathBuf::from(d),
        Some(d) => return Err(format!("De map {d} bestaat niet of is niet bereikbaar.")),
        None => state.data_dir.join("exports"),
    };
    let version = app.package_info().version.to_string();
    let out = backup::export_to(&parent, &state.data_dir.join("documents"), &version, now(), |p| {
        snapshot(&state, p)
    })
    .map_err(|e| messages::db_nl(&e))?;
    Ok(out.display().to_string())
}

/// Import an export folder into this (empty) data folder, then reopen the ledger so pending
/// migrations run with the usual pre-migration copy.
#[tauri::command]
pub fn import_administration(state: State<AppState>, path: String) -> CmdResult<String> {
    let src = PathBuf::from(path.trim());
    let mut guard = state
        .ledger
        .lock()
        .map_err(|_| "Database is vergrendeld.".to_string())?;
    // Refuse early, while the current database is still open.
    backup::ensure_empty_data_dir(&state.data_dir).map_err(|e| messages::db_nl(&e))?;
    backup::verify_export(&src).map_err(|e| messages::db_nl(&e))?;
    // Close the database file so it can be moved aside.
    *guard = Ledger::open_in_memory().map_err(|e| messages::db_nl(&e))?;
    let imported = backup::import_into(&src, &state.data_dir, now());
    *guard =
        crate::open_ledger(&state.data_dir).map_err(|e| format!("Database openen mislukt: {e}. Herstart de app."))?;
    let manifest = imported.map_err(|e| messages::db_nl(&e))?;
    guard
        .record_import(&src.display().to_string(), &manifest)
        .map_err(|e| messages::db_nl(&e))?;
    Ok(format!(
        "Administratie geïmporteerd ({} bestanden, gemaakt op {}).",
        manifest.files.len(),
        manifest.created_at
    ))
}
