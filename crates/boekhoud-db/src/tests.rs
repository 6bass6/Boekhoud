use super::*;
use boekhoud_core::Source;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn entry(journal: JournalCode, date: NaiveDate, desc: &str, lines: Vec<EntryLine>) -> EntryDraft {
    EntryDraft {
        journal,
        date,
        description: desc.into(),
        lines,
        source: Source::manual(),
        idempotency_key: None,
    }
}

/// Scenario S3: talk invoiced to a Dutch client at 21%.
fn talk_invoice(date: NaiveDate) -> EntryDraft {
    entry(
        JournalCode::VRK,
        date,
        "Lezing (fictief)",
        vec![
            EntryLine::debit("1300", 121_000),
            EntryLine::credit("8100", 100_000).with_tax_code("V21"),
            EntryLine::credit("1510", 21_000),
        ],
    )
}

/// Scenario S1: Apple financial report, booked once per Apple period.
fn apple_report(key: &str) -> EntryDraft {
    EntryDraft {
        idempotency_key: Some(key.into()),
        source: Source {
            kind: "apple_report".into(),
            reference: Some(key.into()),
        },
        ..entry(
            JournalCode::PLT,
            d(2026, 10, 31),
            "App Store proceeds (fictief)",
            vec![
                EntryLine::debit("1310", 85_000),
                EntryLine::credit("8000", 85_000).with_tax_code("V0EU"),
            ],
        )
    }
}

#[test]
fn migrations_seed_chart_and_are_idempotent() {
    let ledger = Ledger::open_in_memory().unwrap();
    let accounts = ledger.accounts().unwrap();
    assert!(accounts
        .iter()
        .any(|a| a.number == "8100" && a.account_type == AccountType::Revenue));
    assert!(migrations::pending(ledger.conn()).unwrap().is_empty());
    migrations::apply(ledger.conn()).unwrap(); // second run is a no-op
}

#[test]
fn posting_numbers_gapless_per_journal_and_year() {
    let mut l = Ledger::open_in_memory().unwrap();
    let a = l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    let b = l.post(&talk_invoice(d(2026, 11, 2))).unwrap();
    let c = l.post(&apple_report("apple:2026-10:EUR")).unwrap();
    let next_year = l.post(&talk_invoice(d(2027, 1, 5))).unwrap();
    assert_eq!(a.number(), "VRK-2026-0001");
    assert_eq!(b.number(), "VRK-2026-0002");
    assert_eq!(c.number(), "PLT-2026-0001");
    assert_eq!(next_year.number(), "VRK-2027-0001");
}

#[test]
fn same_external_event_is_posted_once() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.post(&apple_report("apple:2026-10:EUR")).unwrap();
    let err = l.post(&apple_report("apple:2026-10:EUR")).unwrap_err();
    assert!(matches!(err, DbError::Duplicate(_)));
    assert_eq!(
        l.trial_balance(None)
            .unwrap()
            .iter()
            .find(|r| r.account == "8000")
            .unwrap()
            .credit_cents,
        85_000
    );
}

#[test]
fn unbalanced_and_unknown_account_are_rejected() {
    let mut l = Ledger::open_in_memory().unwrap();
    let bad = entry(
        JournalCode::MEM,
        d(2026, 11, 1),
        "x",
        vec![EntryLine::debit("4900", 100), EntryLine::credit("1600", 99)],
    );
    assert!(matches!(l.post(&bad), Err(DbError::Validation(_))));
    let unknown = entry(
        JournalCode::MEM,
        d(2026, 11, 1),
        "x",
        vec![
            EntryLine::debit("7777", 100),
            EntryLine::credit("1600", 100),
        ],
    );
    match l.post(&unknown) {
        Err(DbError::Validation(e)) => {
            assert_eq!(e, vec![ValidationError::UnknownAccount("7777".into())])
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn database_refuses_to_alter_posted_data() {
    let mut l = Ledger::open_in_memory().unwrap();
    let p = l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    let c = l.conn();
    assert!(c
        .execute(
            "UPDATE journal_entry SET description='x' WHERE id=?1",
            [&p.id]
        )
        .is_err());
    assert!(c
        .execute("DELETE FROM journal_entry WHERE id=?1", [&p.id])
        .is_err());
    assert!(c
        .execute(
            "UPDATE journal_line SET amount_cents=1 WHERE entry_id=?1",
            [&p.id]
        )
        .is_err());
    assert!(c
        .execute("DELETE FROM journal_line WHERE entry_id=?1", [&p.id])
        .is_err());
    assert!(c
        .execute("INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES (?1,9,'4900',5)", [&p.id])
        .is_err());
    assert!(c.execute("DELETE FROM audit_log", []).is_err());
}

#[test]
fn database_refuses_to_post_unbalanced_even_bypassing_rust() {
    let l = Ledger::open_in_memory().unwrap();
    let c = l.conn();
    c.execute(
        "INSERT INTO journal_entry (id, journal, fiscal_year, entry_date, description, source_kind, status)
         VALUES ('x','MEM',2026,'2026-11-01','x','manual','draft')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES ('x',1,'4900',100)", []).unwrap();
    c.execute("INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES ('x',2,'1600',-99)", []).unwrap();
    let r = c.execute(
        "UPDATE journal_entry SET status='posted', sequence=1, chain_index=1, prev_hash='0', hash='0', posted_at='now' WHERE id='x'",
        [],
    );
    assert!(r.unwrap_err().to_string().contains("does not balance"));
}

#[test]
fn closed_period_blocks_posting_until_reopened() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.close_period(2026, 11).unwrap();
    assert!(matches!(
        l.post(&talk_invoice(d(2026, 11, 3))),
        Err(DbError::PeriodClosed(2026, 11))
    ));
    l.post(&talk_invoice(d(2026, 12, 1))).unwrap();
    assert!(l.reopen_period(2026, 11, "").is_err());
    l.reopen_period(2026, 11, "Vergeten factuur").unwrap();
    l.post(&talk_invoice(d(2026, 11, 3))).unwrap();
    let n: i64 = l
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='period'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn reversal_nets_to_zero_and_only_once() {
    let mut l = Ledger::open_in_memory().unwrap();
    let p = l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    let r = l
        .reverse(&p.id, d(2026, 11, 20), "Verkeerd bedrag")
        .unwrap();
    assert_eq!(r.number(), "VRK-2026-0002");
    assert!(l
        .trial_balance(None)
        .unwrap()
        .iter()
        .all(|row| row.balance_cents == 0));
    assert!(matches!(
        l.reverse(&p.id, d(2026, 11, 21), "nogmaals"),
        Err(DbError::AlreadyReversed(_))
    ));
}

#[test]
fn trial_balance_totals_match() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    l.post(&apple_report("apple:2026-10:EUR")).unwrap();
    // Scenario S1 payout with exchange difference
    l.post(&entry(
        JournalCode::BNK,
        d(2026, 12, 3),
        "Uitbetaling Apple (fictief)",
        vec![
            EntryLine::debit("1100", 84_820),
            EntryLine::debit("9000", 180),
            EntryLine::credit("1310", 85_000),
        ],
    ))
    .unwrap();
    let tb = l.trial_balance(None).unwrap();
    let debit: i64 = tb.iter().map(|r| r.debit_cents).sum();
    let credit: i64 = tb.iter().map(|r| r.credit_cents).sum();
    assert_eq!(debit, credit);
    assert_eq!(
        tb.iter()
            .find(|r| r.account == "1310")
            .unwrap()
            .balance_cents,
        0
    );
    // Cut-off date excludes the December payout.
    let nov = l.trial_balance(Some(d(2026, 11, 30))).unwrap();
    assert_eq!(
        nov.iter()
            .find(|r| r.account == "1310")
            .unwrap()
            .balance_cents,
        85_000
    );
}

#[test]
fn hash_chain_detects_tampering() {
    let mut l = Ledger::open_in_memory().unwrap();
    for i in 1..=3 {
        l.post(&talk_invoice(d(2026, 11, i))).unwrap();
    }
    assert_eq!(l.verify_chain().unwrap(), ChainStatus::Intact(3));

    // Simulate someone editing the file with an external tool: drop the guard and change an amount.
    let c = l.conn();
    c.execute_batch("DROP TRIGGER journal_line_no_update_when_posted;")
        .unwrap();
    c.execute(
        "UPDATE journal_line SET amount_cents = amount_cents + 1
         WHERE line_no = 1 AND entry_id = (SELECT id FROM journal_entry WHERE chain_index = 2)",
        [],
    )
    .unwrap();
    match l.verify_chain().unwrap() {
        ChainStatus::Broken { chain_index, .. } => assert_eq!(chain_index, 2),
        other => panic!("{other:?}"),
    }
}

#[test]
fn backup_produces_openable_copy() {
    let dir = std::env::temp_dir().join(format!("boekhoud-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut l = Ledger::open(&dir.join("main.sqlite")).unwrap();
    l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    let backup = dir.join("backup.sqlite");
    l.backup_to(&backup).unwrap();
    let restored = Ledger::open(&backup).unwrap();
    assert_eq!(restored.verify_chain().unwrap(), ChainStatus::Intact(1));
    std::fs::remove_dir_all(&dir).ok();
}
