# Working in this repository

Boekhoud: Windows desktop bookkeeping app (Tauri 2 + Rust + React/TS, SQLite) for a Dutch eenmanszaak.
Read `docs/architecture.md`, `docs/accounting-rules.md` and `docs/roadmap.md` before changing anything substantial.

## Rules

1. Work on one bounded module or task at a time; read the existing docs and code first.
2. Change accounting logic only after an impact analysis, and add regression tests.
3. Every schema change is a new numbered migration in `crates/boekhoud-db/migrations/`; never edit an applied migration.
4. Money is `boekhoud_core::Money` (integer cents). Never use `f32`/`f64` for amounts or rates applied to amounts.
5. Fiscal rules live in data (tax codes, parameters with `valid_from`), never in UI components.
6. Posted journal entries are created only through the ledger posting function in `boekhoud-db`.
7. AI/OCR/automation produce proposals only; they never post.
8. Run `cargo test --workspace` (and `cargo clippy --workspace -- -D warnings`) before calling a task done.
9. Never make destructive changes to financial data; corrections are reversal entries.
10. Document architecture decisions, booking rules and fiscal assumptions in `docs/`.
11. Use fictional data only. The repository is public: no real statements, receipts, reports or keys.
12. UI text in Dutch; code, comments and identifiers in English.
