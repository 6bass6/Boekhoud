# Boekhoud — instructions for Claude Code

Windows desktop bookkeeping app for a Dutch eenmanszaak (app-store income + paid talks).
Tauri 2 + Rust + React/TypeScript, SQLite, everything local on one PC.

**Start every session by reading `docs/handover.md`** (status, code map, known limitations, next tasks).
Before substantial changes also read `docs/architecture.md`, `docs/accounting-rules.md`, `docs/roadmap.md`.

## Commands

```sh
# Rust (from repo root)
cargo test --workspace                                    # all tests (app crate needs WebView2/WebKitGTK)
cargo test --workspace --exclude boekhoud-app             # core crates only
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all                                           # rustfmt.toml: max_width = 120

# Frontend + app (from app/)
pnpm install
pnpm build                                                # tsc --noEmit + vite build (type check)
pnpm tauri dev                                            # run the app (debug build → separate dev data folder)
pnpm tauri build                                          # Windows installer in target/release/bundle/ (unsigned)
# Release with auto-update: see docs/releasing.md (bump version, notes, tag vX.Y.Z)

# Fictional demo data (refuses to overwrite an existing file)
cargo run -p boekhoud-db --example demo_data -- <path>/boekhoud.sqlite
```

Data folders: release builds use `%APPDATA%\nl.boekhoud.app\`; **debug builds use the `dev` subfolder**;
`BOEKHOUD_DATA_DIR` overrides both. Each folder holds `boekhoud.sqlite`, `documents/`, `backups/`, `tmp/`.

**Never point a development build, a test, a script or `demo_data` at the owner's real data folder**
(`%APPDATA%\nl.boekhoud.app\` without `dev`). For demo data on Windows:
`$env:BOEKHOUD_DATA_DIR="$env:TEMP\boekhoud-demo"; cargo run -p boekhoud-db --example demo_data -- "$env:TEMP\boekhoud-demo\boekhoud.sqlite"; cd app; pnpm tauri dev`.

Windows prerequisites: rustup (stable, MSVC), Visual Studio Build Tools with "Desktop development with C++",
Node 22, pnpm 10 (`corepack enable`), WebView2 (present on Windows 10/11).

## Non-negotiable rules

1. **Money is integer cents** (`boekhoud_core::Money`, `i64`). Never `f32`/`f64` for amounts, rates applied
   to amounts, or quantities. Rates: basis points (`2100` = 21%) or micro-units (`parse_rate_micro`).
   Quantities: thousandths (`quantity_milli`). Rounding: half away from zero. In TypeScript only integer
   arithmetic (`parseCents`, `lineNet`, `vatFor` in `app/src/money.ts`); amounts go to Rust as Dutch text.
2. **Only `post_in_tx` (in `crates/boekhoud-db/src/lib.rs`) creates posted journal entries.** Every module
   that books something builds an `EntryDraft` and calls it inside its own transaction, with an
   **idempotency key** for anything triggered by an external event or a repeatable action.
3. **Posted data is never changed or deleted.** Corrections are reversals (`Ledger::reverse`). Do not
   weaken, drop or bypass the immutability triggers, and never edit rows of posted entries, issued invoices,
   registered purchases, payments, booked platform reports, imported bank facts, filed VAT returns,
   documents or the audit log. Tests may drop a trigger only to simulate tampering.
4. **Schema changes = a new numbered migration** `crates/boekhoud-db/migrations/000N_name.sql`, registered in
   `migrations.rs`. Never edit a migration that has been pushed to `main`. Migrations must work on a
   database that already contains data (use `ALTER TABLE … ADD COLUMN` with defaults, backfill with `UPDATE`).
5. **Fiscal rules are data, not code**: VAT codes (`tax_code`: rate, rubriek, ICP, reverse charge, allowed
   regime, VAT accounts), VAT regime per start date, platform booking rules (`platform_config`), future tax
   parameters — all versioned with `valid_from`. Never hard-code a rate, rubriek or account in UI components.
6. **Automation proposes, the owner decides.** Suggestions, imports and future OCR/AI never post on their
   own; the user confirms each booking.
7. **Every state change is audited** via `audit(tx, actor, action, entity_type, entity_id, old, new, entry_id)`
   in the same transaction.
8. **Public repository: fictional data only.** No real statements, receipts, platform reports, IBANs,
   names, VAT ids or keys in code, tests, fixtures, docs, commits or screenshots. Secrets belong in
   Windows Credential Manager (`keyring`), never in the database or repo.
9. **Language**: UI text and user-facing errors in Dutch; code, comments, identifiers, commit messages in
   English. `DbError::Invalid` carries a Dutch message (lower-case start; `messages.rs` capitalises it).
10. **Fiscal assumptions are documented.** Any new booking rule or tax interpretation goes into
    `docs/accounting-rules.md`, marked **[TOETSEN]** when it needs confirmation by a bookkeeper. Research
    current Dutch rules on belastingdienst.nl before implementing; cite the source in docs or migration comments.

## How to do common things

**Add a feature that books something**
1. Pure arithmetic → `boekhoud-core` (with unit tests).
2. Storage + booking → a module in `boekhoud-db/src/`, `impl Ledger { … }`, one
   `transaction_with_behavior(TransactionBehavior::Immediate)`, `post_in_tx`, `audit`, commit.
3. Integration tests in the matching file under `boekhoud-db/src/tests/` (helpers in `mod.rs`) or a new test module: happy path, idempotency
   (second attempt refused), immutability (raw SQL update fails), and the resulting trial balance.
4. Re-export public types in `boekhoud-db/src/lib.rs`.

**Expose it to the UI**
1. Command in the matching `app/src-tauri/src/*.rs` module: parse input (dates `%Y-%m-%d`, amounts
   with `Money::parse(text, ',', Currency::EUR)`), call the `Ledger` via `with_ledger`, return serialisable data.
2. Register it in `tauri::generate_handler![…]` in `app/src-tauri/src/lib.rs`.
3. Typed wrapper in `app/src/api.ts` (Tauri converts snake_case parameters to camelCase in `invoke`).
4. Screen in `app/src/views/`, navigation entry in `App.tsx`; reuse `useLoad`, `formatEuro`,
   `DocumentViewer`, existing CSS classes (`split`, `panel`, `box`, `tabs`, `tag`, `form-grid`).

**Add or change a VAT code or tax parameter**: new migration inserting a row with a new `valid_from`
(and set `valid_until` on the old row via `UPDATE` only if it has no posted lines depending on it being open-ended);
update `docs/accounting-rules.md` §4; add a test that the old code still validates for old dates.

**Add an importer**: pure parser in `boekhoud-import` (bytes → struct, tolerant header matching via
`text::find`, encoding via `text::decode`), tests with fictional samples written inline, conversion to the
`boekhoud-db` input type in the Tauri layer, original file archived with `add_document`.

## Definition of done (check before every commit)

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass
  (on a machine without WebView2/WebKitGTK use `--exclude boekhoud-app` and say so).
- `cd app && pnpm build` passes.
- New behaviour has tests; accounting changes include a regression test of the resulting balances.
- Docs updated: roadmap status, accounting rules, handover limitations/next tasks when they change.
- UI changes were looked at in `pnpm tauri dev` with demo data (describe what you checked).
- Commit messages: imperative summary line, body explaining what and why.

## Working with the owner

- The owner is not a bookkeeper; explain fiscal consequences in plain Dutch or English, briefly.
- Ask before: changing booking rules, anything irreversible on real data, adding cloud services or
  dependencies that send data off the PC, or committing to a fiscal interpretation.
- Work in small, reviewable commits; one bounded task at a time. The owner is the only developer:
  no pull requests — commit, push and merge to `main` directly once the definition of done passes,
  then check that CI on `main` is green.
- When the owner shares real files (bank exports, platform reports) to fix an importer: keep them out of
  the repository and out of commits, derive a minimal anonymised fixture, and delete temporary copies.
