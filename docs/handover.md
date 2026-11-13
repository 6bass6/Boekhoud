# Handover — project status and how to continue

Written 12 November 2026 when development moved from Claude (web) to Claude Code.
Read this together with `CLAUDE.md` (working rules), `docs/architecture.md`, `docs/accounting-rules.md`
and `docs/roadmap.md`.

## 1. What this project is

Boekhoud is the owner's own bookkeeping application for a Dutch **eenmanszaak** (sole proprietorship),
a start-up whose income is mainly **App Store / Google Play** revenue, plus **paid talks** (lezingen)
invoiced directly. It is a **single-user Windows desktop app**, everything stored locally on one PC.

The original plan (*Technisch implementatieplan v1.0*, Dutch, web stack with Next.js/PostgreSQL/Redis)
was reviewed and re-architected for the desktop. Its functional goals and principles still apply:
accounting first, automation first, human in the loop, auditability, modularity, data ownership,
configurable tax rules, security by design.

### Owner's decisions (asked and answered — do not re-ask)

| Topic | Decision |
|---|---|
| Stack | Tauri 2 + Rust + React/TypeScript, SQLite |
| Devices | One Windows PC only; no sync, no server |
| AI / OCR | Preferably everything local; no cloud AI by default |
| Existing data | None — new start-up, no migration |
| Bank | Separate business account to be opened (bank not chosen yet → CAMT.053 + CSV) |
| Language | UI Dutch; code, comments, identifiers English |
| Priorities | App-store income is the main revenue; invoicing for talks is urgent |
| VAT regime | Undecided (KOR vs regular). App supports both with start dates; see accounting-rules §5 |
| Accountant | None engaged yet; `docs/accounting-rules.md` is written for a bookkeeper to review |

## 2. Current state (all on `main`, CI green)

Roadmap phases 0–3 are complete; phase 4 is partly done. **83 automated tests** (core, db, import, pdf, app).
CI (`.github/workflows/ci.yml`) runs fmt/clippy/tests for the core crates on Linux, and clippy + the
full test suite + the NSIS/MSI installer build on Windows. The installer is uploaded as the
`boekhoud-windows` artifact on every push to `main`.

### Features in the app (Dutch screen names)

| Screen | What it does |
|---|---|
| **Overzicht** | Dashboard: revenue, costs, result, bank, receivables, payables, VAT position, to-do list, hours progress, monthly chart. Tiles link to the screens behind them. |
| **Verkoopfacturen** | Draft → issue sales invoices for talks. Issue checks Dutch invoice requirements, assigns a gapless number per year, posts to VRK, freezes seller/buyer details, archives the PDF. Credit notes, manual payments, PDF preview. |
| **Inkoopfacturen** | Register purchase invoices with the receipt shown next to the form; VAT incl. reverse charge; "paid privately" option; duplicate detection; attach receipt later; payments. |
| **Platformen** | Import Apple financial report / Google Play earnings report; enter Apple euro proceeds per currency; book as one PLT entry; revenue per app. |
| **Bank** | Import CAMT.053 / CSV; skip duplicates; proposals; process into entries (invoice, platform payout, account with VAT split, private, ignore); reconciliation with the statement balance. |
| **Contacten** | Customers, suppliers, platforms (roles, VAT id, default account/VAT code). |
| **Boekingen / Nieuwe boeking** | Journal list with detail and reversal; manual entry form with live balance check. |
| **Btw-aangifte** | Quarterly VAT return per rubriek with drill-down, warnings, rounding, ICP listing, linking Apple/Google contacts for ICP, filing (clears VAT accounts to 1520, closes months). |
| **Uren** | Time entries and timer; urencriterium (1,225 h) progress and weekly pace. |
| **Activa** | Asset register; monthly straight-line depreciation posting. |
| **Winst & verlies / Balans** | Per-year P&L (1 January up to a chosen date) and balance sheet at a date, with prior-year and current-year results as computed equity lines; "Exporteren (CSV)" writes to `exports/` in the data folder and shows the path. |
| **Proef- en saldibalans / Grootboekkaart / Perioden / Rekeningschema / Instellingen** | Reports, period closing with audited reopening, chart of accounts, company details, VAT regime, manual backup, hash-chain integrity check. |

### Integrity guarantees (enforced in code **and** by SQLite triggers)

- Posted journal entries and their lines cannot be updated or deleted; corrections are reversals.
- Every posted entry balances, has ≥ 2 lines, is in an open period, and has valid VAT codes for its date
  and VAT regime.
- Gapless numbering per journal (BNK, VRK, INK, MEM, PLT) per fiscal year; gapless invoice numbers per year.
- Idempotency keys: the same external event (invoice issue, platform report, bank transaction, monthly
  depreciation, VAT filing) can never be posted twice.
- Tamper-evident SHA-256 hash chain over posted entries (`Ledger::verify_chain`).
- Issued invoices, registered purchases, payments, booked platform reports, imported bank facts, filed
  VAT returns and the audit log are immutable; documents cannot be deleted within 7-year retention and
  are checksum-verified on read.
- Automatic database copy before every schema migration.

## 3. Code map

```
crates/boekhoud-core     Pure domain logic, no I/O
  money.rs               Money (i64 cents + Currency), parsing, basis-point VAT, exchange conversion
  journal.rs             EntryDraft/EntryLine (signed cents), validation, reversal, JournalCode
  invoice.rs             Sales/purchase invoice arithmetic and the entry lines they produce (VatRule)
  hash.rs                Canonical serialisation + hash chain
  account.rs             Account types

crates/boekhoud-db       SQLite storage; Ledger is the single entry point
  lib.rs                 Ledger (open, migrations, post, reverse, periods, trial balance, verify_chain),
                         post_in_tx (THE posting function), audit() helper, DbError
  migrations/000N_*.sql  Forward-only schema migrations (0001 core … 0007 tax/hours/assets)
  tax.rs                 Tax codes, VAT regime, exchange rates, VatRule loading
  company.rs contacts.rs documents.rs
  sales.rs purchases.rs  Invoices, payments (record_payment), open amounts
  platforms.rs           Platform reports and booking
  bank.rs                Bank accounts, import, suggestions, processing, reconciliation
  vat.rs                 VAT return, detail, ICP, filing
  hours.rs assets.rs queries.rs (entries, ledger cards, periods, dashboard)
  reports.rs             Per-year P&L, balance sheet with computed result lines, Dutch CSV
  tests.rs               Integration tests per phase (fictional data)
  examples/demo_data.rs  Fictional demo database covering all modules

crates/boekhoud-import   Pure parsers: apple.rs, google.rs, bank.rs (CAMT.053 + CSV), text.rs helpers
crates/boekhoud-pdf      Invoice PDF with standard Helvetica (WinAnsi), wrapping, pagination

app/src-tauri/src        Thin Tauri layer: lib.rs (state, core commands, data dir), invoicing.rs,
                         platforms.rs, bank.rs, tax.rs, format.rs (Dutch formatting), messages.rs (Dutch errors)
app/src                  React UI: App.tsx (navigation), api.ts (typed invoke wrappers), money.ts,
                         hooks.ts (useLoad), views/*.tsx, components/DocumentViewer.tsx, styles.css
```

Data flow: UI form (amounts as Dutch text) → Tauri command parses with `Money::parse(…, ',')` →
`Ledger` method → (inside one transaction) validation → `post_in_tx` → audit log → commit.

## 4. Known limitations and technical debt (honest list)

1. **Importers are untested against real files.** Apple, Google, CAMT.053, ING and Rabobank parsers
   follow the documented formats and pass tests on fictional samples only. Expect adjustments when the
   owner provides real exports. Never commit real files; build anonymised fixtures from them.
2. **No year-end closing.** P&L accounts are not closed to equity. The trial balance up to a date is
   cumulative over all years, so from the second year it mixes years for revenue/expense accounts.
   Ledger cards already restart P&L accounts per year. *Mitigated in reports:* "Winst & verlies" and
   "Balans" (`crates/boekhoud-db/src/reports.rs`) split years by computation; the balance sheet shows the
   result of prior years and of the current year as computed equity lines. Closing entries and an
   opening balance are still missing, and the trial balance remains cumulative.
3. **All bank accounts post to ledger account 1100.** A second bank account (e.g. savings) needs its
   own ledger account (`bank_account.ledger_account` exists but is always set to 1100).
4. **No income-tax estimate (IB-prognose).** Not started; needs yearly parameters as data with sources.
5. **VAT details to confirm** (marked **[TOETSEN]** in accounting-rules): rubriek for services to non-EU
   businesses (`V0NEU` has no rubriek and shows a warning), net vs gross for platform revenue, KOR
   purchase codes (reverse charge under KOR), private use of assets (`business_use_pct` is stored, not used).
6. **`set_vat_regime` overwrites a row with the same start date** (audited). Consider making regime
   history append-only.
7. **Exchange rates** can be stored and used, but there is no ECB import screen.
8. **No asset disposal**, no link from asset to its purchase invoice in the UI.
9. **No code signing, auto-updater, scheduled/encrypted backups or SQLCipher** yet (phase 7).
10. **`crates/boekhoud-db/src/tests.rs` is large** (~1,300 lines); splitting per module would help.
11. ~~UI has no React error boundary~~ — done: `components/ErrorBoundary.tsx` wraps each screen; a render error shows a Dutch message with "Probeer opnieuw" and the navigation keeps working.
12. **UI was only checked on Linux** (WebKitGTK under Xvfb). Real Windows/WebView2 testing by the owner
    is still needed (PDF iframe preview, file pickers, opening documents with `explorer`).

## 5. Recommended next tasks (in order)

1. **IB-prognose** (roadmap phase 4). Add a migration with a `tax_parameter` table (year, key, value as
   integer cents/basis points, source URL, valid_from). Research the 2026 and 2027 values from
   belastingdienst.nl: box 1 brackets and rates, zelfstandigenaftrek, startersaftrek, mkb-winstvrijstelling,
   Zvw-bijdrage rate and maximum, algemene heffingskorting and arbeidskorting (including how they phase
   out with income), and how the urencriterium affects eligibility. Inputs: fiscal profit from the ledger,
   estimated salary income (loondienst), hours from `hours_summary`. Output: estimate plus a monthly reserve
   advice. Show clearly: accounting profit ≠ fiscal profit ≠ taxable income ≠ final assessment. Tests with
   fixed parameters.
2. **Year-end closing** + opening balance, and a per-year trial balance / P&L / balance sheet.
3. **Reports and exports** — partly done: per-year P&L and balance sheet with CSV export (also the trial
   balance). Still open: cost per category, revenue per app in EUR, XLSX/PDF, a save dialog (CSV now goes
   to `exports/` in the data folder because the app has no dialog plugin).
4. **XAF export** (XML Auditfile Financieel 3.2) for accountants and the Belastingdienst.
5. **Validate importers with real exports** once the owner has them (see limitation 1).
6. **Backups**: scheduled daily backup of database + documents to a chosen folder, optional encryption,
   automated restore test; configurable data folder.
7. **Phase 5 automation**: UBL/Peppol invoice import, local OCR via `Windows.Media.Ocr`, recurring-expense
   detection, missing-receipt reminders. All of it produces proposals only.
8. **Phase 7 hardening**: code signing, Tauri updater, SQLCipher evaluation, splitting tests.

## 6. Owner's context worth knowing

- Talks: normally 21% VAT; the VAT exemption for talks only applies when given by or for public bodies,
  associations or foundations, aimed at science/general development, for a cost-covering fee.
- KOR: € 20,000 limit counts only turnover taxed in the Netherlands; app-store services to the Irish
  Apple/Google entities are taxed abroad and appear not to count. Trade-off: no VAT on talks vs no
  input-VAT deduction. To be confirmed by a bookkeeper.
- The repository is **public**. Only fictional data in code, tests, fixtures and screenshots.
