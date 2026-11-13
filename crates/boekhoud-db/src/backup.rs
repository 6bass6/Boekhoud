//! Backups of the database plus documents, with verification and retention, and the
//! export/import package for moving the administration to another PC.
//!
//! Backup layout in a target folder (the data dir's `backups/` or an extra folder):
//! `<prefix><stamp>.sqlite` (consistent copy via `VACUUM INTO`), `<prefix><stamp>.json` (verification
//! result) and one shared `documents/` mirror. Documents are content-addressed and immutable, so the
//! mirror only receives files it does not have yet; every backup database can be restored together with it.
//!
//! These functions take a `snapshot` closure that writes the database copy, so the caller decides how
//! long it holds the ledger (the app only locks it for the `VACUUM INTO`, not while copying documents).

use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::NaiveDateTime;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{audit, ChainStatus, DbError, Ledger, Result, MIGRATIONS};

pub const AUTO_PREFIX: &str = "auto-";
pub const MANUAL_PREFIX: &str = "handmatig-";
pub const DEFAULT_KEEP: usize = 14;
/// An automatic backup is made at app start when the newest good one is older than this.
pub const AUTO_INTERVAL_HOURS: i64 = 24;
/// Settings file in the data dir (there is no settings table; this avoids a migration).
pub const SETTINGS_FILE: &str = "backup-instellingen.json";
/// Subfolder created inside the owner's extra backup folder.
pub const EXTRA_SUBDIR: &str = "Boekhoud-back-ups";
const STAMP: &str = "%Y%m%d-%H%M%S";

fn default_keep() -> usize {
    DEFAULT_KEEP
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupSettings {
    /// Optional extra folder (USB drive, OneDrive folder) that also receives every backup.
    #[serde(default)]
    pub extra_dir: Option<String>,
    /// Number of automatic backups to keep per target folder.
    #[serde(default = "default_keep")]
    pub keep: usize,
}

impl Default for BackupSettings {
    fn default() -> Self {
        BackupSettings {
            extra_dir: None,
            keep: DEFAULT_KEEP,
        }
    }
}

impl BackupSettings {
    /// Missing or unreadable file → defaults (backups must never block the app).
    pub fn load(data_dir: &Path) -> Self {
        fs::read(data_dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| DbError::Io(e.to_string()))?;
        write_atomic(&data_dir.join(SETTINGS_FILE), &json)
    }

    /// The extra target folder, only if it is configured and reachable right now.
    pub fn extra_target(&self) -> Option<std::result::Result<PathBuf, String>> {
        let dir = self.extra_dir.as_deref().map(str::trim).filter(|d| !d.is_empty())?;
        let dir = Path::new(dir);
        // Do not create the base folder: an unplugged USB drive must not become a folder on C:.
        Some(if dir.is_dir() {
            Ok(dir.join(EXTRA_SUBDIR))
        } else {
            Err(format!("extra back-upmap {} is niet bereikbaar", dir.display()))
        })
    }
}

/// Outcome of checking a database copy and its documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    pub ok: bool,
    /// Posted entries whose hash chain was verified.
    pub entries: usize,
    /// Documents found with a matching checksum.
    pub documents: usize,
    /// Dutch descriptions of what is wrong (empty when ok).
    pub problems: Vec<String>,
}

impl Verification {
    fn failed(problem: String) -> Self {
        Verification {
            ok: false,
            entries: 0,
            documents: 0,
            problems: vec![problem],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupInfo {
    /// File name of the database copy, e.g. `auto-20261113-090000.sqlite`.
    pub file: String,
    /// `auto`, `handmatig`, `voor-migratie`, `voor-import` or `overig`.
    pub kind: String,
    /// `YYYY-MM-DD HH:MM:SS` from the file name, if it has a stamp.
    pub created: Option<String>,
    pub size_bytes: u64,
    /// `None` for backups without a verification record (older manual and pre-migration copies).
    pub verification: Option<Verification>,
}

impl BackupInfo {
    fn created_at(&self) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(self.created.as_deref()?, "%Y-%m-%d %H:%M:%S").ok()
    }

    fn is_good(&self) -> bool {
        self.verification.as_ref().is_some_and(|v| v.ok)
    }
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

impl Ledger {
    /// Open a database read-only without applying migrations (for checking copies).
    pub fn open_read_only(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(DbError::Invalid(format!("bestand {} bestaat niet", path.display())));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Ledger {
            conn,
            actor: "owner".into(),
            document_root: path.parent().map(|p| p.join("documents")),
        })
    }

    /// Highest applied schema migration.
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| {
                r.get(0)
            })?)
    }

    /// Audit-log that this administration was imported from an export package.
    pub fn record_import(&mut self, source: &str, manifest: &Manifest) -> Result<()> {
        let actor = self.actor.clone();
        let tx = self.conn.unchecked_transaction()?;
        let summary = serde_json::json!({
            "source": source,
            "app_version": manifest.app_version,
            "schema_version": manifest.schema_version,
            "created_at": manifest.created_at,
            "files": manifest.files.len(),
        });
        audit(
            &tx,
            &actor,
            "import",
            "administration",
            source,
            None::<&()>,
            Some(&summary),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Check a database copy: SQLite integrity, the hash chain, and that every document it references is
/// present in `documents_dir` with a matching SHA-256.
pub fn verify_copy(db: &Path, documents_dir: &Path) -> Verification {
    match verify_inner(db, documents_dir) {
        Ok(v) => v,
        Err(e) => Verification::failed(format!("database niet te openen of te lezen: {e}")),
    }
}

fn verify_inner(db: &Path, documents_dir: &Path) -> Result<Verification> {
    let ledger = Ledger::open_read_only(db)?;
    let mut problems = Vec::new();
    let integrity: Vec<String> = ledger
        .conn
        .prepare("PRAGMA integrity_check")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    if integrity != ["ok"] {
        problems.push(format!("integriteitscontrole: {}", integrity.join("; ")));
        // A damaged file can make the next queries fail or lie; stop here.
        return Ok(Verification {
            ok: false,
            entries: 0,
            documents: 0,
            problems,
        });
    }
    let entries = match ledger.verify_chain()? {
        ChainStatus::Intact(n) => n,
        ChainStatus::Broken { chain_index, .. } => {
            problems.push(format!("hashketen verbroken bij boeking {chain_index}"));
            0
        }
    };
    let shas: Vec<String> = ledger
        .conn
        .prepare("SELECT sha256 FROM document ORDER BY sha256")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut documents = 0;
    for sha in &shas {
        match sha256_file(&crate::documents::blob_path(documents_dir, sha)) {
            Ok(h) if &h == sha => documents += 1,
            Ok(_) => problems.push(format!("document {sha} wijkt af van de controlesom")),
            Err(_) => problems.push(format!("document {sha} ontbreekt")),
        }
    }
    Ok(Verification {
        ok: problems.is_empty(),
        entries,
        documents,
        problems,
    })
}

// ---------------------------------------------------------------------------
// Backups
// ---------------------------------------------------------------------------

/// Write a backup into `target`: database copy (via `snapshot`), new documents into the shared
/// mirror, verification, and the verification record. A failure is recorded, not hidden: the
/// returned info then has `ok == false`. Errors are only returned when nothing could be written.
pub fn create_backup(
    target: &Path,
    documents_src: &Path,
    prefix: &str,
    now: NaiveDateTime,
    snapshot: impl FnOnce(&Path) -> Result<()>,
) -> Result<BackupInfo> {
    fs::create_dir_all(target).map_err(io)?;
    let name = format!("{prefix}{}.sqlite", now.format(STAMP));
    let db = target.join(&name);
    if db.exists() {
        return Err(DbError::Invalid(format!(
            "back-up {name} bestaat al; probeer het zo opnieuw"
        )));
    }
    let mirror = target.join("documents");
    let verification = match snapshot(&db).and_then(|_| copy_missing_files(documents_src, &mirror)) {
        Ok(_) => verify_copy(&db, &mirror),
        Err(e) => Verification::failed(format!("back-up schrijven mislukt: {e}")),
    };
    let json = serde_json::to_vec_pretty(&verification).map_err(|e| DbError::Io(e.to_string()))?;
    write_atomic(&db.with_extension("json"), &json)?;
    Ok(info_for(target, &name))
}

/// Make an automatic backup if needed, then apply retention. Returns `None` when no backup was due.
pub fn auto_backup_if_due(
    target: &Path,
    documents_src: &Path,
    now: NaiveDateTime,
    keep: usize,
    snapshot: impl FnOnce(&Path) -> Result<()>,
) -> Result<Option<BackupInfo>> {
    if !auto_backup_due(&list_backups(target), now) {
        return Ok(None);
    }
    let info = create_backup(target, documents_src, AUTO_PREFIX, now, snapshot)?;
    remove_old_backups(target, keep)?;
    Ok(Some(info))
}

/// All backups in `dir`, newest first.
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let Ok(read) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<BackupInfo> = read
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sqlite"))
        .map(|n| info_for(dir, &n))
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.file.cmp(&a.file)));
    out
}

fn info_for(dir: &Path, name: &str) -> BackupInfo {
    let stem = name.trim_end_matches(".sqlite");
    let kind = ["auto", "handmatig", "voor-migratie", "voor-import"]
        .into_iter()
        .find(|k| stem.starts_with(&format!("{k}-")))
        .unwrap_or("overig");
    let created = stem
        .get(stem.len().saturating_sub(15)..)
        .and_then(|s| NaiveDateTime::parse_from_str(s, STAMP).ok())
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string());
    let path = dir.join(name);
    BackupInfo {
        file: name.to_string(),
        kind: kind.to_string(),
        created,
        size_bytes: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        verification: fs::read(path.with_extension("json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok()),
    }
}

/// Newest verified-good backup of any kind (manual backups also count as protection).
pub fn last_good_backup(backups: &[BackupInfo]) -> Option<&BackupInfo> {
    backups.iter().filter(|b| b.is_good()).max_by_key(|b| b.created_at())
}

/// True when there is no good automatic backup younger than [`AUTO_INTERVAL_HOURS`].
pub fn auto_backup_due(backups: &[BackupInfo], now: NaiveDateTime) -> bool {
    !backups
        .iter()
        .filter(|b| b.kind == "auto" && b.is_good())
        .filter_map(BackupInfo::created_at)
        .any(|t| now - t < chrono::Duration::hours(AUTO_INTERVAL_HOURS))
}

/// Automatic backups to delete: good ones beyond the newest `keep`, and failed ones older than the
/// newest good one. Manual, pre-migration and other files are never selected; a failed backup never
/// causes a good one to be deleted.
pub fn backups_to_remove(backups: &[BackupInfo], keep: usize) -> Vec<String> {
    let mut autos: Vec<&BackupInfo> = backups
        .iter()
        .filter(|b| b.kind == "auto" && b.created.is_some())
        .collect();
    autos.sort_by(|a, b| b.created.cmp(&a.created));
    let newest_good = autos.iter().find(|b| b.is_good()).and_then(|b| b.created.clone());
    let mut good_seen = 0;
    let mut out = Vec::new();
    for b in autos {
        if b.is_good() {
            good_seen += 1;
            if good_seen > keep.max(1) {
                out.push(b.file.clone());
            }
        } else if newest_good.as_ref().is_some_and(|g| b.created.as_ref() < Some(g)) {
            out.push(b.file.clone());
        }
    }
    out
}

/// Apply [`backups_to_remove`] in `dir` (database copy and its verification record).
pub fn remove_old_backups(dir: &Path, keep: usize) -> Result<Vec<String>> {
    let remove = backups_to_remove(&list_backups(dir), keep);
    for name in &remove {
        let path = dir.join(name);
        fs::remove_file(&path).map_err(io)?;
        let _ = fs::remove_file(path.with_extension("json"));
    }
    Ok(remove)
}

// ---------------------------------------------------------------------------
// Export / import (moving to another PC)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Relative path with `/` separators, e.g. `documents/ab/cd/<sha>`.
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at: String,
    pub files: Vec<ManifestFile>,
}

pub const MANIFEST_FILE: &str = "manifest.json";
const DB_FILE: &str = "boekhoud.sqlite";

/// Write an export folder `boekhoud-export-<stamp>` inside `dest_parent` and verify it. The folder is
/// built under a `.onvolledig` name and only renamed when verification passed.
pub fn export_to(
    dest_parent: &Path,
    documents_src: &Path,
    app_version: &str,
    now: NaiveDateTime,
    snapshot: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let name = format!("boekhoud-export-{}", now.format(STAMP));
    let dest = dest_parent.join(&name);
    let work = dest_parent.join(format!("{name}.onvolledig"));
    if dest.exists() || work.exists() {
        return Err(DbError::Invalid(format!("{} bestaat al", dest.display())));
    }
    fs::create_dir_all(&work).map_err(io)?;
    let result = (|| {
        snapshot(&work.join(DB_FILE))?;
        copy_missing_files(documents_src, &work.join("documents"))?;
        let mut files = Vec::new();
        for rel in files_under(&work)? {
            let path = work.join(&rel);
            files.push(ManifestFile {
                path: rel.iter().map(|c| c.to_string_lossy()).collect::<Vec<_>>().join("/"),
                sha256: sha256_file(&path)?,
                size_bytes: fs::metadata(&path).map_err(io)?.len(),
            });
        }
        let manifest = Manifest {
            format: 1,
            app_version: app_version.to_string(),
            schema_version: Ledger::open_read_only(&work.join(DB_FILE))?.schema_version()?,
            created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            files,
        };
        let json = serde_json::to_vec_pretty(&manifest).map_err(|e| DbError::Io(e.to_string()))?;
        write_atomic(&work.join(MANIFEST_FILE), &json)?;
        verify_export(&work)?;
        fs::rename(&work, &dest).map_err(io)
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&work);
        return Err(e);
    }
    Ok(dest)
}

/// Check an export folder: manifest present, every listed file matches its checksum, the database
/// passes integrity and hash-chain checks, documents are complete, and the schema is not newer than
/// this app.
pub fn verify_export(dir: &Path) -> Result<Manifest> {
    let bytes = fs::read(dir.join(MANIFEST_FILE))
        .map_err(|_| DbError::Invalid(format!("geen {MANIFEST_FILE} gevonden in {}", dir.display())))?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| DbError::Invalid(format!("{MANIFEST_FILE} is onleesbaar: {e}")))?;
    if manifest.format != 1 {
        return Err(DbError::Invalid(format!("onbekend exportformaat {}", manifest.format)));
    }
    let newest = MIGRATIONS.last().map(|m| m.0).unwrap_or(0);
    if manifest.schema_version > newest {
        return Err(DbError::Invalid(format!(
            "de export komt uit een nieuwere versie van Boekhoud (schema {} > {newest}); werk eerst deze app bij",
            manifest.schema_version
        )));
    }
    if !manifest.files.iter().any(|f| f.path == DB_FILE) {
        return Err(DbError::Invalid(format!("de export bevat geen {DB_FILE}")));
    }
    for f in &manifest.files {
        let path = dir.join(safe_relative(&f.path)?);
        match sha256_file(&path) {
            Ok(h) if h == f.sha256 => {}
            Ok(_) => return Err(DbError::Invalid(format!("{} is gewijzigd of beschadigd", f.path))),
            Err(_) => return Err(DbError::Invalid(format!("{} ontbreekt in de export", f.path))),
        }
    }
    let db = dir.join(DB_FILE);
    if Ledger::open_read_only(&db)?.schema_version()? != manifest.schema_version {
        return Err(DbError::Invalid(
            "schemaversie in de database wijkt af van het manifest".into(),
        ));
    }
    let v = verify_copy(&db, &dir.join("documents"));
    if !v.ok {
        return Err(DbError::Invalid(format!(
            "controle van de export mislukt: {}",
            v.problems.join("; ")
        )));
    }
    Ok(manifest)
}

/// Refuse unless `data_dir` holds no administration yet: no database, or one without bookings,
/// invoices, documents or other user data, and an empty documents folder.
pub fn ensure_empty_data_dir(data_dir: &Path) -> Result<()> {
    let refuse = |what: &str| {
        Err(DbError::Invalid(format!(
            "de gegevensmap bevat al een administratie ({what}); importeren kan alleen in een lege gegevensmap"
        )))
    };
    if !files_under(&data_dir.join("documents"))?.is_empty() {
        return refuse("documenten");
    }
    let db = data_dir.join(DB_FILE);
    if !db.exists() {
        return Ok(());
    }
    let ledger = Ledger::open_read_only(&db)?;
    for (table, what) in [
        ("journal_entry", "boekingen"),
        ("sales_invoice", "verkoopfacturen"),
        ("purchase_invoice", "inkoopfacturen"),
        ("bank_transaction", "banktransacties"),
        ("platform_report", "platformrapporten"),
        ("time_entry", "uren"),
        ("asset", "activa"),
        ("document", "documenten"),
    ] {
        let n: i64 = ledger
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        if n > 0 {
            return refuse(what);
        }
    }
    Ok(())
}

/// Put a verified export into an empty `data_dir`. The ledger on `data_dir` must be closed; the
/// caller reopens it afterwards (which runs pending migrations) and calls [`Ledger::record_import`].
/// An existing empty database is kept as `backups/voor-import-<stamp>.sqlite`, never deleted.
pub fn import_into(export_dir: &Path, data_dir: &Path, now: NaiveDateTime) -> Result<Manifest> {
    ensure_empty_data_dir(data_dir)?;
    let manifest = verify_export(export_dir)?;
    for f in manifest.files.iter().filter(|f| f.path.starts_with("documents/")) {
        let rel = safe_relative(&f.path)?;
        copy_verified(&export_dir.join(&rel), &data_dir.join(&rel), &f.sha256)?;
    }
    let db = data_dir.join(DB_FILE);
    if db.exists() {
        let backups = data_dir.join("backups");
        fs::create_dir_all(&backups).map_err(io)?;
        let aside = backups.join(format!("voor-import-{}.sqlite", now.format(STAMP)));
        fs::rename(&db, &aside).map_err(io)?;
        for ext in ["-wal", "-shm"] {
            let side = data_dir.join(format!("{DB_FILE}{ext}"));
            if side.exists() {
                fs::rename(&side, backups.join(format!("{}{ext}", aside.display()))).map_err(io)?;
            }
        }
    }
    let db_sha = &manifest
        .files
        .iter()
        .find(|f| f.path == DB_FILE)
        .expect("checked")
        .sha256;
    copy_verified(&export_dir.join(DB_FILE), &db, db_sha)?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

fn io(e: std::io::Error) -> DbError {
    DbError::Io(e.to_string())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).map_err(io)?;
    Ok(hex::encode(hasher.finalize()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(io)?;
    fs::rename(&tmp, path).map_err(io)
}

/// Relative paths of all regular files under `root` (none if it does not exist); skips `.tmp` files.
fn files_under(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(root: &Path, rel: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for e in fs::read_dir(root.join(rel)).map_err(io)? {
            let e = e.map_err(io)?;
            let rel = rel.join(e.file_name());
            let t = e.file_type().map_err(io)?;
            if t.is_dir() {
                walk(root, &rel, out)?;
            } else if t.is_file() && rel.extension().is_none_or(|x| x != "tmp") {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if root.is_dir() {
        walk(root, Path::new(""), &mut out)?;
    }
    out.sort();
    Ok(out)
}

/// Copy files from `src` to `dest` that `dest` does not have yet (documents never change).
fn copy_missing_files(src: &Path, dest: &Path) -> Result<usize> {
    let mut copied = 0;
    for rel in files_under(src)? {
        let to = dest.join(&rel);
        if !to.exists() {
            fs::create_dir_all(to.parent().expect("file has parent")).map_err(io)?;
            let tmp = to.with_extension("tmp");
            fs::copy(src.join(&rel), &tmp).map_err(io)?;
            fs::rename(&tmp, &to).map_err(io)?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn copy_verified(from: &Path, to: &Path, sha: &str) -> Result<()> {
    fs::create_dir_all(to.parent().expect("file has parent")).map_err(io)?;
    let tmp = to.with_extension("tmp");
    fs::copy(from, &tmp).map_err(io)?;
    if sha256_file(&tmp)? != sha {
        let _ = fs::remove_file(&tmp);
        return Err(DbError::Invalid(format!(
            "kopie van {} wijkt af van de controlesom",
            from.display()
        )));
    }
    fs::rename(&tmp, to).map_err(io)
}

/// Manifest paths come from a file on disk: only plain relative paths are accepted.
fn safe_relative(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    if path.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(DbError::Invalid(format!("ongeldig pad in manifest: {path}")));
    }
    Ok(p)
}
