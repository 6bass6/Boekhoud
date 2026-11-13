# Proposal: automatic update check and safe updates

**Status: pending_approval** (proposed 2026-11-13; nothing implemented yet)

Owner request: the app checks for new releases automatically and can update itself while the
database stays intact.

## Why the database is already safe from the installer

- The installer only replaces program files (`%LOCALAPPDATA%\Programs\Boekhoud` or similar). The
  administration lives in `%APPDATA%\nl.boekhoud.app\` (database, documents, backups) and is never
  touched by an install or uninstall-for-update.
- Schema changes after an update run as numbered migrations, and the app already copies the database
  to `backups/voor-migratie-*.sqlite` before running them.

## What is added on top (extra safety)

1. **Verified backup before installing**: the update button first makes a backup with the existing
   backup code (integrity check + hash chain + document checksums). If that backup fails, the update
   is not installed.
2. **Refuse a database that is newer than the app** (e.g. after reinstalling an older version): show a
   Dutch message instead of opening it. Today there is no such check.

## How updates are found and installed

- Official Tauri updater plugin (`tauri-plugin-updater` + `tauri-plugin-process` for the restart):
  new dependencies, maintained by the Tauri project.
- Releases are published as **GitHub Releases** of the (public) repository. The app reads
  `https://github.com/6bass6/Boekhoud/releases/latest/download/latest.json`. Only that file is fetched;
  no data from the administration is sent (GitHub sees the IP address, as with any download).
- Every update is **signed** with a Tauri signing key; the app refuses unsigned or wrongly signed files.
  The public key goes into `tauri.conf.json`; the private key + password become GitHub Actions secrets.
  The owner must keep a safe copy of the private key (password manager): if it is lost, installed
  copies can no longer be updated automatically (a manual reinstall still works).
- **Automation proposes, the owner decides**: the check runs at app start (and via a button in
  Instellingen); a found update shows version + release notes and an "Bijwerken" button. Nothing
  installs without a click. On Windows the app closes during install and restarts afterwards.
- **Release workflow**: pushing a tag `vX.Y.Z` (version bumped in `Cargo.toml`/`tauri.conf.json`)
  runs a GitHub Actions job that builds the signed NSIS installer and creates the GitHub Release with
  `latest.json`. Normal pushes to `main` keep producing the unsigned test artifact as now.
- Windows code signing (Authenticode, against SmartScreen warnings) is separate and not part of this.

## Tests

- Unit test: database with a newer schema is refused.
- Backup-before-update path reuses tested backup code; a failed backup blocks the update (test).
- End-to-end: release v0.2.0, install v0.1.x build on Windows, update via the button, check the
  administration is unchanged — this last step needs the owner on Windows.
