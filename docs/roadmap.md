# Roadmap

Each phase is done only when: automated tests pass, acceptance criteria are met, docs are updated,
and any data migration has been verified on a backup. Priorities reflect the owner's situation:
app-store income is the main revenue and invoicing for talks is urgent.

| Phase | Scope | Acceptance |
|-------|-------|------------|
| **0. Foundation** ✅ | Workspace, Tauri shell, SQLite + migrations, CI with Windows build, docs | App starts on Windows, creates its database, CI green |
| **1. Accounting core** ✅ | Chart of accounts (seeded), journals, draft/post, gapless numbering, reversals, periods + locking, hash chain + verification, audit log, trial balance, exchange-rate table, basic tax codes | Every posted entry balances and is traceable; posted data cannot be altered (proven by tests) |
| **2. Talks invoicing + expenses + platform files** ✅ | Contacts, sales invoices with PDF (Dutch invoice requirements), credit notes, purchase invoices with document upload, manual Apple/Google financial-report import, simple VAT overview | Full administration possible by hand; an Apple report imported twice is booked once |
| **3. Bank** ✅ | CAMT.053 + CSV import, duplicate detection, matching to invoices/receivables, reconciliation, rules-based categorisation proposals | Bank balance in app equals bank statement |
| **4. Tax** (partly) | VAT return preparation per rubriek + ICP, KOR handling, IB-prognose (ondernemersaftrek, mkb-winstvrijstelling, Zvw, loondienst input), time tracking (urencriterium), assets + depreciation | Every figure on the VAT summary drills down to journal lines |
| **5. Automation** | UBL import, local OCR (Windows.Media.Ocr), extraction proposals with confidence scores, recurring-expense detection, missing-document warnings | Proposals never post without approval; every automation run is logged |
| **6. Platform APIs** | App Store Connect API, Google Play reports bucket, advertising platforms | Automated import reconciles with payouts |
| **7. Reports + hardening** | Dashboard, P&L, balance sheet, ledger cards, XAF export, CSV/XLSX/PDF, scheduled encrypted backups + restore test, code signing, updater, SQLCipher evaluation | Restore from backup proven; parallel run against spreadsheet for ≥ 1 quarter matches |

Before relying on the app: have `docs/accounting-rules.md` reviewed by a Dutch bookkeeper, and run it
in parallel with a spreadsheet for at least one VAT period.

## Status

- **Phase 0–1** done: foundation, accounting core usable from the app.
- **Phase 2** done: contacts; sales invoices for talks with PDF (Dutch invoice requirements checked
  before issuing, gapless numbering, frozen seller/buyer details, credit notes, payments); purchase
  invoices with the receipt shown next to the form; content-addressed document store with 7-year
  retention; Apple financial-report and Google Play earnings-report import with euro booking per report.
- **Phase 3** done: CAMT.053 and CSV (ING, Rabobank and similar) import with duplicate detection across
  overlapping exports; proposals (open invoices, platform payouts, earlier bookings of the same
  counterparty, bank costs); processing into entries; reversal reopens a transaction; reconciliation
  against the statement balance.
- **Phase 4** partly done: VAT return per rubriek with drill-down, consistency warnings and rounding,
  ICP listing, filing (VAT accounts cleared to 1520, months closed); time tracking with timer and
  urencriterium progress; fixed assets with monthly depreciation; dashboard.
  IB-prognose (income-tax estimate from the year result, hours and screen inputs for salary and withheld
  loonheffing; 2026 parameters as data with sources in `tax_parameter`; monthly reserve advice).
  **Not yet:** 2027 tax parameters (add when final); KOR-specific purchase codes;
  disposal of assets.
- **Phase 7** partly started: per-year **Winst & verlies** (up to a chosen date) and **Balans** with the
  results of prior years and the current year as computed (not posted) equity lines; CSV export
  (semicolon, comma decimal, UTF-8 with BOM) of both and of the trial balance, written to the data
  folder's `exports/`. **Not yet:** year-end closing/opening balance, cost per category, revenue per
  app, XLSX/PDF export, a save dialog.
- **Phase 7** also: automatic daily backup at app start (database + incremental documents
  mirror), verified after writing (integrity check, hash chain, document checksums), retention of 14
  automatic backups, optional extra folder (USB/OneDrive), restore test, export/import package for
  moving to another PC (see `docs/backup.md`). **Not yet:** backup encryption, configurable data folder.
- **Next:** year-end closing, then phase 5 (UBL import, local OCR, recurring-expense detection) and
  phase 7 items (XAF export, backup encryption, code signing).
