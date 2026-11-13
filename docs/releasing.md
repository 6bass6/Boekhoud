# Releasing a new version (and how updates reach the installed app)

Installed copies check `https://github.com/6bass6/Boekhoud/releases/latest/download/latest.json` at start
(and via Instellingen → Updates). If the version there is newer, the app shows it with the release notes
and a "Bijwerken" button. On click it first makes a verified backup (no backup → no update), then
downloads the signed installer, closes, installs and restarts. The data folder
(`%APPDATA%\nl.boekhoud.app\`) is never touched by the installer; schema changes run as migrations after
the restart, with the usual `voor-migratie-*` copy. A database written by a newer version is refused
(the app then shows only that message and the update check).

## Steps

1. Bump the version in **three** places: `Cargo.toml` (`[workspace.package] version`),
   `app/src-tauri/tauri.conf.json` (`version`) and `app/package.json` (`version`). Run `cargo check` so
   `Cargo.lock` follows.
2. Write the release notes in Dutch for the owner: `docs/releases/vX.Y.Z.md` (shown in the app).
3. Commit, push to `main`, wait for CI to be green.
4. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. The `Release` workflow (`.github/workflows/release.yml`) checks that the tag matches the version and
   that the notes exist, runs the tests, builds the NSIS installer with updater signature
   (`app/src-tauri/tauri.release.conf.json`) and publishes the GitHub Release with `latest.json`.

## Signing key

- Updates are signed with a Tauri updater key (minisign). The **public** key is in `tauri.conf.json`
  (`plugins.updater.pubkey`); the app refuses installers not signed with the matching private key.
- The **private** key and its password are GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The owner keeps an offline copy (password manager). Never commit it.
- If the private key is lost, installed copies can no longer update automatically: generate a new key,
  put the new public key in `tauri.conf.json`, release, and install that release once by hand.
- This is not Windows code signing (Authenticode); Windows SmartScreen may still warn on first install.

## Local builds

`pnpm tauri build` (without the release config) builds unsigned installers and needs no key; those
cannot be offered as updates. The CI artifact on every push to `main` is such a test build.
