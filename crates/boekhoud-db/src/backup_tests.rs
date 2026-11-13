//! Tests for backups, restore, retention and export/import (fictional data only).

use std::fs;
use std::path::{Path, PathBuf};

use boekhoud_core::Source;
use chrono::NaiveDateTime;

use super::backup::*;
use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("boekhoud-{tag}-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn t(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
}

/// A data folder with one posted talk invoice and one document.
fn data_dir_with_ledger(root: &Path) -> (Ledger, PathBuf) {
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    let mut l = Ledger::open(&data.join("boekhoud.sqlite")).unwrap();
    l.post(&EntryDraft {
        journal: JournalCode::VRK,
        date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        description: "Lezing (fictief)".into(),
        lines: vec![
            EntryLine::debit("1300", 121_000),
            EntryLine::credit("8100", 100_000).with_tax_code("V21"),
            EntryLine::credit("1510", 21_000),
        ],
        source: Source::manual(),
        idempotency_key: None,
    })
    .unwrap();
    l.add_document(b"%PDF-1.4 fictieve bon", "bon.pdf", "receipt").unwrap();
    (l, data)
}

fn info(file: &str, ok: Option<bool>) -> BackupInfo {
    let stem = file.trim_end_matches(".sqlite");
    let stamp = &stem[stem.len() - 15..];
    BackupInfo {
        file: file.into(),
        kind: stem[..stem.len() - 16].into(),
        created: Some(
            NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S")
                .unwrap()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ),
        size_bytes: 1,
        verification: ok.map(|ok| Verification {
            ok,
            entries: 0,
            documents: 0,
            problems: vec![],
        }),
    }
}

#[test]
fn backup_is_verified_and_restores_with_equal_balances() {
    let tmp = TempDir::new("backup");
    let (l, data) = data_dir_with_ledger(&tmp.0);
    let target = data.join("backups");
    let b = create_backup(
        &target,
        &data.join("documents"),
        AUTO_PREFIX,
        t("2026-11-13 09:00"),
        |p| l.backup_to(p),
    )
    .unwrap();
    let v = b.verification.clone().unwrap();
    assert!(v.ok, "{:?}", v.problems);
    assert_eq!((v.entries, v.documents), (1, 1));
    assert_eq!(b.kind, "auto");
    assert_eq!(b.created.as_deref(), Some("2026-11-13 09:00:00"));

    // Manual restore as documented in docs/backup.md: database copy + documents mirror.
    let restore = tmp.0.join("restore");
    fs::create_dir_all(&restore).unwrap();
    fs::copy(target.join(&b.file), restore.join("boekhoud.sqlite")).unwrap();
    copy_tree(&target.join("documents"), &restore.join("documents"));
    let r = Ledger::open(&restore.join("boekhoud.sqlite")).unwrap();
    assert_eq!(r.trial_balance(None).unwrap(), l.trial_balance(None).unwrap());
    assert_eq!(r.verify_chain().unwrap(), ChainStatus::Intact(1));
    let doc: String = r.conn().query_row("SELECT id FROM document", [], |x| x.get(0)).unwrap();
    assert_eq!(r.document_bytes(&doc).unwrap().1, b"%PDF-1.4 fictieve bon");
}

fn copy_tree(from: &Path, to: &Path) {
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            fs::create_dir_all(to).unwrap();
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn corrupted_backup_is_reported_and_keeps_older_good_backups() {
    let tmp = TempDir::new("corrupt");
    let (l, data) = data_dir_with_ledger(&tmp.0);
    let (target, docs) = (data.join("backups"), data.join("documents"));
    create_backup(&target, &docs, AUTO_PREFIX, t("2026-11-12 09:00"), |p| l.backup_to(p)).unwrap();

    // A copy that is not a database at all.
    let bad = create_backup(&target, &docs, AUTO_PREFIX, t("2026-11-13 09:00"), |p| {
        fs::write(p, vec![7u8; 8192]).map_err(|e| DbError::Io(e.to_string()))
    })
    .unwrap();
    assert!(!bad.verification.as_ref().unwrap().ok);
    // A snapshot that fails outright is recorded as failed too.
    let failed = create_backup(&target, &docs, AUTO_PREFIX, t("2026-11-13 10:00"), |_| {
        Err(DbError::Io("schijf vol (test)".into()))
    })
    .unwrap();
    assert!(failed.verification.unwrap().problems[0].contains("schijf vol"));

    // Retention with keep = 1 must not delete the only good backup.
    assert!(remove_old_backups(&target, 1).unwrap().is_empty());
    assert!(target.join("auto-20261112-090000.sqlite").exists());
    // The failed attempts do not count as a recent backup.
    assert!(auto_backup_due(&list_backups(&target), t("2026-11-13 11:00")));

    // A missing or altered document is detected.
    let good = target.join("auto-20261112-090000.sqlite");
    let blob = files_in(&target.join("documents"))[0].clone();
    fs::write(&blob, b"gewijzigd").unwrap();
    let v = verify_copy(&good, &target.join("documents"));
    assert!(!v.ok && v.problems[0].contains("controlesom"), "{v:?}");
    fs::remove_file(&blob).unwrap();
    assert!(verify_copy(&good, &target.join("documents")).problems[0].contains("ontbreekt"));
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(files_in(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn broken_hash_chain_in_copy_fails_verification() {
    let tmp = TempDir::new("chain");
    let (l, data) = data_dir_with_ledger(&tmp.0);
    let copy = tmp.0.join("copy.sqlite");
    l.backup_to(&copy).unwrap();
    let c = Connection::open(&copy).unwrap();
    // Tests may drop a trigger only to simulate tampering.
    let triggers: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = 'journal_line'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    for name in triggers {
        c.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
    c.execute(
        "UPDATE journal_line SET amount_cents = amount_cents + 1 WHERE line_no = 1",
        [],
    )
    .unwrap();
    drop(c);
    let v = verify_copy(&copy, &data.join("documents"));
    assert!(!v.ok && v.problems[0].contains("hashketen"), "{v:?}");
}

#[test]
fn retention_keeps_newest_good_and_never_touches_other_kinds() {
    let list = vec![
        info("auto-20261113-090000.sqlite", Some(false)),
        info("auto-20261112-090000.sqlite", Some(true)),
        info("auto-20261111-090000.sqlite", Some(true)),
        info("auto-20261110-090000.sqlite", None),
        info("auto-20261109-090000.sqlite", Some(true)),
        info("handmatig-20261101-090000.sqlite", Some(true)),
        info("voor-migratie-20261001-090000.sqlite", None),
    ];
    // The newest (failed) attempt is kept for diagnosis; unverified older ones go.
    assert_eq!(
        backups_to_remove(&list, 2),
        vec!["auto-20261110-090000.sqlite", "auto-20261109-090000.sqlite"]
    );
    assert_eq!(backups_to_remove(&list, 14), vec!["auto-20261110-090000.sqlite"]);
    // Only failed backups: nothing is removed.
    assert!(backups_to_remove(&list[..1], 1).is_empty());
}

#[test]
fn auto_backup_is_due_after_24_hours_of_good_backups() {
    let list = vec![
        info("auto-20261112-090000.sqlite", Some(true)),
        info("handmatig-20261113-080000.sqlite", Some(true)),
    ];
    assert!(!auto_backup_due(&list, t("2026-11-13 08:59")));
    assert!(auto_backup_due(&list, t("2026-11-13 09:00")));
    assert!(auto_backup_due(&[], t("2026-11-13 09:00")));
    assert_eq!(last_good_backup(&list).unwrap().kind, "handmatig");
}

#[test]
fn auto_backup_runs_once_per_day_and_applies_retention() {
    let tmp = TempDir::new("auto");
    let (l, data) = data_dir_with_ledger(&tmp.0);
    let (target, docs) = (data.join("backups"), data.join("documents"));
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("voor-migratie-20260101-090000.sqlite"), b"oud").unwrap();
    for day in 1..=4 {
        let now = t(&format!("2026-11-0{day} 09:00"));
        assert!(auto_backup_if_due(&target, &docs, now, 2, |p| l.backup_to(p))
            .unwrap()
            .is_some());
        assert!(auto_backup_if_due(&target, &docs, now, 2, |p| l.backup_to(p))
            .unwrap()
            .is_none());
    }
    let names: Vec<String> = list_backups(&target).into_iter().map(|b| b.file).collect();
    assert_eq!(
        names,
        vec![
            "auto-20261104-090000.sqlite",
            "auto-20261103-090000.sqlite",
            "voor-migratie-20260101-090000.sqlite"
        ]
    );
}

#[test]
fn extra_folder_and_settings() {
    let tmp = TempDir::new("settings");
    assert_eq!(BackupSettings::load(&tmp.0), BackupSettings::default());
    let s = BackupSettings {
        extra_dir: Some(tmp.0.join("usb").display().to_string()),
        keep: 14,
    };
    s.save(&tmp.0).unwrap();
    assert_eq!(BackupSettings::load(&tmp.0), s);
    // Unplugged drive: reported, and the folder is not created.
    assert!(s.extra_target().unwrap().is_err());
    assert!(!tmp.0.join("usb").exists());
    fs::create_dir_all(tmp.0.join("usb")).unwrap();
    assert_eq!(s.extra_target().unwrap().unwrap(), tmp.0.join("usb").join(EXTRA_SUBDIR));
    assert!(BackupSettings::default().extra_target().is_none());
}

fn export(tmp: &Path) -> (Ledger, PathBuf) {
    let (l, data) = data_dir_with_ledger(tmp);
    let out = export_to(
        &tmp.join("usb"),
        &data.join("documents"),
        "0.1.0",
        t("2026-11-13 09:00"),
        |p| l.backup_to(p),
    )
    .unwrap();
    assert_eq!(out.file_name().unwrap(), "boekhoud-export-20261113-090000");
    (l, out)
}

#[test]
fn export_import_round_trip_gives_identical_administration() {
    let tmp = TempDir::new("export");
    let (l, out) = export(&tmp.0);
    let m = verify_export(&out).unwrap();
    assert_eq!(m.files.len(), 2);
    assert_eq!(m.schema_version, MIGRATIONS.last().unwrap().0);

    // New PC: the app has already created an empty database.
    let new = tmp.0.join("nieuwe-pc");
    fs::create_dir_all(&new).unwrap();
    drop(Ledger::open(&new.join("boekhoud.sqlite")).unwrap());
    import_into(&out, &new, t("2026-11-14 10:00")).unwrap();
    assert!(new.join("backups/voor-import-20261114-100000.sqlite").exists());

    let mut r = Ledger::open(&new.join("boekhoud.sqlite")).unwrap();
    r.record_import(&out.display().to_string(), &m).unwrap();
    assert_eq!(r.trial_balance(None).unwrap(), l.trial_balance(None).unwrap());
    let doc: String = r.conn().query_row("SELECT id FROM document", [], |x| x.get(0)).unwrap();
    assert_eq!(r.document_bytes(&doc).unwrap().1, b"%PDF-1.4 fictieve bon");
    let audited: i64 = r
        .conn()
        .query_row("SELECT COUNT(*) FROM audit_log WHERE action = 'import'", [], |x| {
            x.get(0)
        })
        .unwrap();
    assert_eq!(audited, 1);

    // Importing a second time is refused: the folder is no longer empty.
    let err = import_into(&out, &new, t("2026-11-14 11:00")).unwrap_err();
    assert!(err.to_string().contains("bevat al een administratie"), "{err}");
}

#[test]
fn import_is_refused_into_a_folder_with_data() {
    let tmp = TempDir::new("nonempty");
    let (_l, out) = export(&tmp.0);
    let other = TempDir::new("nonempty-target");
    let (_o, data) = data_dir_with_ledger(&other.0);
    let before = fs::read(data.join("boekhoud.sqlite")).unwrap();
    let err = import_into(&out, &data, t("2026-11-14 10:00")).unwrap_err();
    assert!(err.to_string().contains("bevat al een administratie"), "{err}");
    assert_eq!(fs::read(data.join("boekhoud.sqlite")).unwrap(), before);
}

#[test]
fn import_is_refused_for_tampered_exports() {
    let tmp = TempDir::new("tamper");
    let (_l, out) = export(&tmp.0);
    let target = tmp.0.join("leeg");

    let doc = files_in(&out.join("documents"))[0].clone();
    let original = fs::read(&doc).unwrap();
    fs::write(&doc, b"vervalst").unwrap();
    let err = import_into(&out, &target, t("2026-11-14 10:00")).unwrap_err();
    assert!(err.to_string().contains("gewijzigd of beschadigd"), "{err}");
    fs::write(&doc, original).unwrap();

    let db = out.join("boekhoud.sqlite");
    let good_db = fs::read(&db).unwrap();
    let mut bytes = good_db.clone();
    let n = bytes.len();
    bytes[n - 10] ^= 0xFF;
    fs::write(&db, bytes).unwrap();
    assert!(import_into(&out, &target, t("2026-11-14 10:00")).is_err());
    assert!(!target.join("boekhoud.sqlite").exists());
    fs::write(&db, good_db).unwrap();

    // Paths that escape the export folder are rejected.
    let mut m: Manifest = serde_json::from_slice(&fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap();
    m.files.last_mut().unwrap().path = "../buiten.txt".into();
    fs::write(out.join(MANIFEST_FILE), serde_json::to_vec(&m).unwrap()).unwrap();
    let err = verify_export(&out).unwrap_err();
    assert!(err.to_string().contains("ongeldig pad"), "{err}");
}
