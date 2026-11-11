//! Create a database filled with fictional demo bookings (scenarios from docs/accounting-rules.md).
//!
//! Usage: cargo run -p boekhoud-db --example demo_data -- <path/to/boekhoud.sqlite>
//! Refuses to touch an existing file.

use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Source};
use boekhoud_db::{Company, Ledger};
use chrono::NaiveDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: demo_data <path>")?;
    let path = std::path::Path::new(&path);
    if path.exists() {
        return Err(format!("{} already exists; refusing to overwrite", path.display()).into());
    }
    let mut l = Ledger::open(path)?;
    l.save_company(&Company {
        legal_name: "Voorbeeld Apps".into(),
        city: Some("Utrecht".into()),
        country: "NL".into(),
        ..Default::default()
    })?;
    let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    let e = |journal, date, desc: &str, lines, key: Option<&str>| EntryDraft {
        journal,
        date,
        description: desc.into(),
        lines,
        source: Source::manual(),
        idempotency_key: key.map(Into::into),
    };
    use EntryLine as L;
    let entries = vec![
        e(
            JournalCode::MEM,
            d(1, 2),
            "Inbreng eigen geld",
            vec![L::debit("1100", 500_000), L::credit("0510", 500_000)],
            None,
        ),
        e(
            JournalCode::INK,
            d(1, 15),
            "Laptop (fictief)",
            vec![
                L::debit("0100", 200_000),
                L::debit("1500", 42_000).with_tax_code("I21"),
                L::credit("1600", 242_000),
            ],
            None,
        ),
        e(
            JournalCode::BNK,
            d(1, 16),
            "Betaling laptop",
            vec![L::debit("1600", 242_000), L::credit("1100", 242_000)],
            None,
        ),
        e(
            JournalCode::INK,
            d(2, 1),
            "Hosting februari",
            vec![
                L::debit("4000", 10_000).with_tax_code("I21"),
                L::debit("1500", 2_100),
                L::credit("1600", 12_100),
            ],
            None,
        ),
        e(
            JournalCode::VRK,
            d(3, 10),
            "Lezing Stichting Voorbeeld",
            vec![
                L::debit("1300", 121_000),
                L::credit("8100", 100_000).with_tax_code("V21"),
                L::credit("1510", 21_000),
            ],
            None,
        ),
        e(
            JournalCode::PLT,
            d(3, 28),
            "App Store proceeds periode 6",
            vec![
                L::debit("1310", 85_000),
                L::credit("8000", 85_000).with_tax_code("V0EU"),
            ],
            Some("demo:apple:p6"),
        ),
        e(
            JournalCode::BNK,
            d(4, 2),
            "Ontvangst lezing",
            vec![L::debit("1100", 121_000), L::credit("1300", 121_000)],
            None,
        ),
        e(
            JournalCode::BNK,
            d(5, 6),
            "Uitbetaling Apple",
            vec![
                L::debit("1100", 84_820),
                L::debit("9000", 180),
                L::credit("1310", 85_000),
            ],
            None,
        ),
    ];
    for entry in &entries {
        let p = l.post(entry)?;
        println!("{} {}", p.number(), entry.description);
    }
    l.close_period(2026, 1)?;
    Ok(())
}
