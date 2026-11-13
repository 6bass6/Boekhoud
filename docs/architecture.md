# Architecture

Boekhoud is a single-user Windows desktop application for the bookkeeping of a Dutch
eenmanszaak (app-store income, talks, subscriptions, expenses, VAT, income-tax estimates).

This document replaces the web-oriented stack of the original *Technisch implementatieplan v1.0*
(Next.js / PostgreSQL / Redis / S3 / Docker). The functional goals and design principles of that
plan still apply; only the technical realisation changes.

## 1. Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Tauri 2** desktop app: Rust core + React/TypeScript UI | Small native `.exe`, Rust for correctness-critical logic, web UI for data-heavy forms and tables |
| D2 | **SQLite** (single file) as source of truth, via `rusqlite` (bundled) | No server, one portable file, transactional, trivially backed up |
| D3 | **Single user, single PC**, no login server | Owner's choice. Protection comes from Windows account + disk encryption + optional app-level encryption (phase 7) |
| D4 | **Everything local**, incl. OCR/extraction | Owner's choice. Windows built-in OCR (`Windows.Media.Ocr`) + UBL e-invoice parsing + rules. No cloud AI by default |
| D5 | **Money as integer cents (`i64`)**, never floats | Exact arithmetic; currency carried explicitly |
| D6 | **Rust workspace with crates as module boundaries** instead of 16 services with domain events | Same separation, far less ceremony for a single-user app |
| D7 | **Posted entries are immutable** (DB triggers) and **hash-chained** | Tamper-evidence for the 7-year bewaarplicht; corrections via reversal entries only |
| D8 | **Fiscal rules are data**, versioned with `valid_from`/`valid_until` | VAT codes, KOR status, tax parameters change yearly |
| D9 | UI language **Dutch**; code, comments, identifiers **English** | Owner's choice |
| D10 | **No automatic filing** with the Belastingdienst | Digipoort requires PKIoverheid; the app prepares figures, owner files via Mijn Belastingdienst Zakelijk |

## 2. Workspace layout

```
boekhoud/
├── crates/
│   ├── boekhoud-core/     # pure domain logic: Money, journal entries, validation, hashing, invoice arithmetic
│   ├── boekhoud-db/       # SQLite schema, migrations, posting, invoices, bank, platforms, VAT, hours, assets
│   ├── boekhoud-import/   # parsers: Apple/Google reports, CAMT.053, bank CSV (pure, no I/O)
│   └── boekhoud-pdf/      # invoice PDF with standard fonts
├── app/                   # Tauri 2 application
│   ├── src-tauri/         # Rust side: commands exposing core/db to the UI
│   └── src/               # React + TypeScript UI (Vite)
├── docs/
│   ├── architecture.md    # this file
│   ├── accounting-rules.md# chart of accounts + booking scenarios (for review by a bookkeeper)
│   └── roadmap.md         # phases and acceptance criteria
└── .github/workflows/     # CI: tests on Linux, Windows .exe build
```

Planned future crates (added when their phase starts, not before): `boekhoud-reports` (incl. XAF export).
The IB-prognose arithmetic lives in `boekhoud-core::income_tax` (small enough; no separate crate). Inside `boekhoud-db` each domain has its own module (`sales`, `purchases`,
`bank`, `platforms`, `vat`, `hours`, `assets`, `documents`); all postings go through `post_in_tx`.

Dependency rule: `core` depends on nothing internal. Every other crate may depend on `core`.
Only `boekhoud-db` talks to SQLite. The Tauri layer is thin: it validates input and calls crate functions.
**Only the ledger posting function in `boekhoud-db` can create posted journal entries.**

## 3. Data model (phase 0–1)

All tables carry `created_at`; mutable tables also `updated_at`. Primary keys are UUIDv7 text
(time-sortable) except where a natural sequence is required.

- `company` – legal name, trade name, KvK, btw-id, address, fiscal year start.
- `account` – number, name, type (asset/liability/equity/revenue/expense), parent, `rgs_code`, active.
- `journal` (dagboek) – code + name: `BNK` bank, `VRK` verkoop, `INK` inkoop, `MEM` memoriaal, `PLT` platformen.
- `journal_entry` – journal, fiscal year, **gapless sequence number per journal per year** (assigned at posting),
  entry date, description, `source_type`/`source_id`, `idempotency_key` (unique), status (`draft`/`posted`),
  `reverses_entry_id`, `prev_hash`, `hash`, `posted_at`.
- `journal_line` – entry, account, **signed amount in cents** (debit > 0, credit < 0), currency,
  original-currency amount + rate (optional), tax code, project, description.
- `period` – year/month, status (`open`/`closed`), closed_at.
- `audit_log` – who/what/when, old and new values (JSON), affected entry.
- `tax_code` – versioned VAT codes (`valid_from`/`valid_until`): rate in basis points, VAT-return rubriek,
  ICP and reverse-charge flags, and the VAT regime under which the code may be used.
- `vat_regime` – `regular` or `kor` per start date. Posting refuses codes that do not fit the regime
  in force on the entry date (e.g. `V21` under the KOR, `VKOR` under the regular regime).
- `exchange_rate` – units of foreign currency per EUR, scaled by 10^6 (integer), per date and source;
  lookups fall back to the most recent earlier date.
- `schema_migrations` – applied migration versions.

Later migrations add (see `crates/boekhoud-db/migrations/`): contacts, documents, sales and purchase
invoices, payments (0004); platform configuration and reports (0005); bank accounts, imports and
transactions (0006); filed VAT returns, time entries and assets (0007). Issued invoices, registered
purchases, payments, booked platform reports, imported bank facts, filed VAT returns and documents are
protected by triggers in the same way as posted entries.

### Invariants enforced in code *and* in the database

1. A posted entry balances: Σ amounts = 0 (i.e. Σ debit = Σ credit), per currency of the books (EUR).
2. A posted entry has ≥ 2 lines and no zero-amount lines.
3. Posted entries and their lines cannot be updated or deleted (SQLite triggers abort).
4. Entries cannot be posted into a closed period.
4a. Every VAT code on a line exists and is valid on the entry date, and fits the VAT regime of that date.
5. `idempotency_key` is unique: the same external event can never be posted twice.
6. Each posted entry stores `hash = SHA-256(prev_hash ‖ canonical serialisation)`; a verification
   routine walks the chain and reports the first break.
7. Every report is reproducible from `journal_line` alone.

## 4. Money and currencies

- Books are kept in EUR. `Money` = `i64` cents + ISO currency code; arithmetic across currencies is a type error.
- Foreign-currency lines store the original amount and the rate used; the EUR amount is what is posted.
- Rate source: ECB reference rates (imported, stored in `exchange_rate` with date and source).
- Apple/Google already convert to EUR in their payout reports; differences between the booked
  receivable and the actual bank receipt go to *Koersverschillen*.
- Rounding: banker's rounding is **not** used; VAT is rounded half-away-from-zero per invoice line total,
  matching common Dutch invoicing practice (to be confirmed, see accounting-rules.md).

## 5. Documents

Originals are stored unmodified in a local content-addressed store
(`<data dir>/documents/ab/cd/<sha256>`), referenced from the `document` table with original filename,
MIME type, size, checksum and retention date (≥ 7 years). Deleting is impossible while the retention
date is in the future; any change creates a new document and an audit log entry.

## 6. Security, backup, continuity

- Data directory under `%APPDATA%\nl.boekhoud.app` (to be made configurable, e.g. to a BitLocker-protected drive).
- Secrets (App Store Connect key, Google service account) in **Windows Credential Manager** via the `keyring` crate, never in the database or repo.
- Backups: one-click and scheduled (daily on first start of the day) to a user-chosen folder
  (e.g. OneDrive or USB): consistent SQLite online backup + document store, packed and optionally encrypted.
  Automatic backup **before every schema migration**. Restore is tested in CI with a fixture.
- App-level database encryption (SQLCipher) is evaluated in phase 7; until then rely on BitLocker.
- The repository is public: **never commit real financial data, documents or keys.** Tests use fictional data only.

## 7. Integrations (later phases)

| Source | Method |
|--------|--------|
| Bank | CAMT.053 XML import (all Dutch banks offer it), CSV fallback. PSD2 deferred: aggregators need a server-side redirect, which conflicts with D3/D4 |
| Apple | Phase 2: manual import of *Financial Reports* files. Phase 6: App Store Connect API (Finance + Sales reports, JWT with API key) |
| Google Play | Phase 2: manual import of earnings reports. Phase 6: reports bucket on Google Cloud Storage via service account |
| Purchase invoices | UBL/Peppol XML first (exact data), then PDF/image via local OCR + rules |
| VAT return | Prepared figures per rubriek + ICP-opgaaf, copied manually into Mijn Belastingdienst Zakelijk |
| Accountant | XAF (XML Auditfile Financieel) export, plus CSV/XLSX/PDF reports |

## 8. Distribution

- Windows installer (NSIS/MSI) built by GitHub Actions.
- Code signing before real use (otherwise SmartScreen warnings).
- Tauri updater for new versions; migrations run only after an automatic backup.
