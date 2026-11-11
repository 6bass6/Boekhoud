# Roadmap

Each phase is done only when: automated tests pass, acceptance criteria are met, docs are updated,
and any data migration has been verified on a backup. Priorities reflect the owner's situation:
app-store income is the main revenue and invoicing for talks is urgent.

| Phase | Scope | Acceptance |
|-------|-------|------------|
| **0. Foundation** ✅ | Workspace, Tauri shell, SQLite + migrations, CI with Windows build, docs | App starts on Windows, creates its database, CI green |
| **1. Accounting core** ✅ | Chart of accounts (seeded), journals, draft/post, gapless numbering, reversals, periods + locking, hash chain + verification, audit log, trial balance, exchange-rate table, basic tax codes | Every posted entry balances and is traceable; posted data cannot be altered (proven by tests) |
| **2. Talks invoicing + expenses + platform files** | Contacts, sales invoices with PDF (Dutch invoice requirements), credit notes, purchase invoices with document upload, manual Apple/Google financial-report import, simple VAT overview | Full administration possible by hand; an Apple report imported twice is booked once |
| **3. Bank** | CAMT.053 + CSV import, duplicate detection, matching to invoices/receivables, reconciliation, rules-based categorisation proposals | Bank balance in app equals bank statement |
| **4. Tax** | VAT return preparation per rubriek + ICP, KOR handling, IB-prognose (ondernemersaftrek, mkb-winstvrijstelling, Zvw, loondienst input), time tracking (urencriterium), assets + depreciation | Every figure on the VAT summary drills down to journal lines |
| **5. Automation** | UBL import, local OCR (Windows.Media.Ocr), extraction proposals with confidence scores, recurring-expense detection, missing-document warnings | Proposals never post without approval; every automation run is logged |
| **6. Platform APIs** | App Store Connect API, Google Play reports bucket, advertising platforms | Automated import reconciles with payouts |
| **7. Reports + hardening** | Dashboard, P&L, balance sheet, ledger cards, XAF export, CSV/XLSX/PDF, scheduled encrypted backups + restore test, code signing, updater, SQLCipher evaluation | Restore from backup proven; parallel run against spreadsheet for ≥ 1 quarter matches |

Before relying on the app: have `docs/accounting-rules.md` reviewed by a Dutch bookkeeper, and run it
in parallel with a spreadsheet for at least one VAT period.

## Status

- **Phase 0** done: workspace, Tauri shell, migrations, CI with Windows installer.
- **Phase 1** done: manual bookings in all journals, versioned VAT codes with KOR/regular regime check,
  exchange-rate table and exact conversion, entry list and detail, reversals, ledger cards with opening
  balance, trial balance, period closing/reopening, company details, manual backup, hash-chain check.
  Not yet: year-end closing entries (planned with the reports in phase 7; until then P&L accounts simply
  restart at zero per year on ledger cards) and an import screen for ECB rates (comes with phase 2 imports).
- **Next: phase 2** — sales invoices for talks (PDF), contacts, purchase invoices with documents,
  Apple/Google financial-report import.
