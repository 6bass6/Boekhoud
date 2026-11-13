//! Embedded, forward-only schema migrations.

use rusqlite::{params, Connection};

use crate::Result;

/// `(version, sql)` in ascending order. Never edit or remove a released entry.
pub const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001_core.sql")),
    (2, include_str!("../migrations/0002_seed_chart.sql")),
    (3, include_str!("../migrations/0003_tax_and_rates.sql")),
    (4, include_str!("../migrations/0004_invoicing_documents.sql")),
    (5, include_str!("../migrations/0005_platforms.sql")),
    (6, include_str!("../migrations/0006_bank.sql")),
    (7, include_str!("../migrations/0007_tax_hours_assets.sql")),
    (8, include_str!("../migrations/0008_tax_parameters.sql")),
];

fn ensure_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
         );",
    )?;
    Ok(())
}

fn applied(conn: &Connection) -> Result<Vec<u32>> {
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
    let v = stmt
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(v)
}

pub fn pending(conn: &Connection) -> Result<Vec<u32>> {
    ensure_table(conn)?;
    let done = applied(conn)?;
    Ok(MIGRATIONS
        .iter()
        .map(|(v, _)| *v)
        .filter(|v| !done.contains(v))
        .collect())
}

pub fn apply(conn: &Connection) -> Result<()> {
    ensure_table(conn)?;
    let done = applied(conn)?;
    for (version, sql) in MIGRATIONS {
        if done.contains(version) {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute("INSERT INTO schema_migrations (version) VALUES (?1)", params![version])?;
        tx.commit()?;
    }
    Ok(())
}
