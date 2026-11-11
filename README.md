# Boekhoud

Eigen boekhoudsoftware voor een Nederlandse eenmanszaak — een Windows-desktopapplicatie
(Tauri 2, Rust, React/TypeScript, SQLite). Alles draait lokaal op één pc.

- Architectuur: [`docs/architecture.md`](docs/architecture.md)
- Boekingsregels en rekeningschema (ter beoordeling door boekhouder): [`docs/accounting-rules.md`](docs/accounting-rules.md)
- Fasering: [`docs/roadmap.md`](docs/roadmap.md)

## Ontwikkelen

Vereisten: Rust (stable), Node.js 20+, pnpm. Op Windows: WebView2 (standaard aanwezig op Windows 10/11)
en de Visual Studio C++ Build Tools.

```sh
cargo test --workspace          # domein- en databasetests
cd app && pnpm install
pnpm tauri dev                  # app starten
pnpm tauri build                # Windows-installer bouwen
```

Demo-database met fictieve boekingen (bestaat het bestand al, dan wordt het niet overschreven):

```sh
cargo run -p boekhoud-db --example demo_data -- pad/naar/boekhoud.sqlite
```

De app bewaart zijn database in `%APPDATA%\nl.boekhoud.app\boekhoud.sqlite`, met back-ups in de
submap `backups` (automatisch vóór iedere database-update, en handmatig via Instellingen).

CI bouwt bij iedere push een Windows-installer als artifact.

> Deze repository is openbaar. Commit nooit echte financiële gegevens, bewijsstukken of sleutels.
