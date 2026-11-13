use super::*;

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
        vec![EntryLine::debit("7777", 100), EntryLine::credit("1600", 100)],
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
        .execute("UPDATE journal_entry SET description='x' WHERE id=?1", [&p.id])
        .is_err());
    assert!(c.execute("DELETE FROM journal_entry WHERE id=?1", [&p.id]).is_err());
    assert!(c
        .execute("UPDATE journal_line SET amount_cents=1 WHERE entry_id=?1", [&p.id])
        .is_err());
    assert!(c
        .execute("DELETE FROM journal_line WHERE entry_id=?1", [&p.id])
        .is_err());
    assert!(c
        .execute(
            "INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES (?1,9,'4900',5)",
            [&p.id]
        )
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
    c.execute(
        "INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES ('x',1,'4900',100)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO journal_line (entry_id,line_no,account,amount_cents) VALUES ('x',2,'1600',-99)",
        [],
    )
    .unwrap();
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
        .query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='period'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn reversal_nets_to_zero_and_only_once() {
    let mut l = Ledger::open_in_memory().unwrap();
    let p = l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    let r = l.reverse(&p.id, d(2026, 11, 20), "Verkeerd bedrag").unwrap();
    assert_eq!(r.number(), "VRK-2026-0002");
    assert!(l.trial_balance(None).unwrap().iter().all(|row| row.balance_cents == 0));
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
    assert_eq!(tb.iter().find(|r| r.account == "1310").unwrap().balance_cents, 0);
    // Cut-off date excludes the December payout.
    let nov = l.trial_balance(Some(d(2026, 11, 30))).unwrap();
    assert_eq!(nov.iter().find(|r| r.account == "1310").unwrap().balance_cents, 85_000);
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

#[test]
fn database_from_newer_app_version_is_refused() {
    let dir = std::env::temp_dir().join(format!("boekhoud-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.sqlite");
    {
        let l = Ledger::open(&path).unwrap();
        l.conn
            .execute("INSERT INTO schema_migrations (version) VALUES (9999)", [])
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    match Ledger::open(&path) {
        Err(DbError::Invalid(m)) => assert!(m.contains("nieuwere versie"), "{m}"),
        other => panic!("expected refusal, got {:?}", other.map(|_| ())),
    }
    assert!(matches!(Ledger::pending_migrations(&path), Err(DbError::Invalid(_))));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "refused database must stay untouched"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_tax_code_is_rejected() {
    let mut l = Ledger::open_in_memory().unwrap();
    match l.post(&talk_with_code(d(2026, 11, 1), "X99")) {
        Err(DbError::Validation(e)) => {
            assert_eq!(e, vec![ValidationError::UnknownTaxCode("X99".into())])
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn expired_tax_code_is_rejected_after_valid_until() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.conn()
        .execute(
            "INSERT INTO tax_code (code, valid_from, valid_until, name, kind, rate_bp)
             VALUES ('OLD', '2020-01-01', '2026-06-30', 'Vervallen code', 'sales', 600)",
            [],
        )
        .unwrap();
    let mut e = talk_with_code(d(2026, 6, 30), "OLD");
    l.post(&e).unwrap();
    e.date = d(2026, 7, 1);
    assert!(matches!(l.post(&e), Err(DbError::Validation(_))));
}

#[test]
fn kor_regime_controls_which_codes_are_allowed() {
    let mut l = Ledger::open_in_memory().unwrap();
    assert_eq!(l.vat_regime_on(d(2026, 1, 1)).unwrap(), VatRegime::Regular);
    // Regular: VKOR not allowed.
    let kor_talk = entry(
        JournalCode::VRK,
        d(2026, 11, 1),
        "Lezing KOR",
        vec![
            EntryLine::debit("1300", 100_000),
            EntryLine::credit("8100", 100_000).with_tax_code("VKOR"),
        ],
    );
    assert!(matches!(l.post(&kor_talk), Err(DbError::Validation(_))));

    l.set_vat_regime(d(2027, 1, 1), VatRegime::Kor, "Aangemeld voor KOR")
        .unwrap();
    assert_eq!(l.vat_regime_on(d(2026, 12, 31)).unwrap(), VatRegime::Regular);
    assert_eq!(l.vat_regime_on(d(2027, 3, 1)).unwrap(), VatRegime::Kor);

    // KOR: VKOR allowed, V21 refused, regime-neutral V0EU (Apple) still allowed.
    let mut k = kor_talk.clone();
    k.date = d(2027, 2, 1);
    l.post(&k).unwrap();
    match l.post(&talk_with_code(d(2027, 2, 1), "V21")) {
        Err(DbError::Validation(e)) => {
            assert!(matches!(&e[0], ValidationError::TaxCodeNotAllowed { code, .. } if code == "V21"))
        }
        other => panic!("{other:?}"),
    }
    let mut apple = apple_report("apple:2027-01:EUR");
    apple.date = d(2027, 1, 31);
    l.post(&apple).unwrap();
}

#[test]
fn exchange_rate_falls_back_to_last_published_day() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.add_exchange_rate(d(2026, 11, 6), "USD", 1_082_300, "ECB").unwrap(); // Friday
    let (date, rate) = l.exchange_rate_on(d(2026, 11, 8), "USD").unwrap().unwrap(); // Sunday
    assert_eq!(date, d(2026, 11, 6));
    assert_eq!(
        Money::new(10_823, Currency::USD).to_eur(rate).unwrap(),
        Money::eur(10_000)
    );
    assert!(l.exchange_rate_on(d(2026, 11, 5), "USD").unwrap().is_none());
}

#[test]
fn entry_list_and_detail_show_reversal_links() {
    let mut l = Ledger::open_in_memory().unwrap();
    let p = l.post(&talk_invoice(d(2026, 11, 1))).unwrap();
    l.post(&apple_report("apple:2026-10:EUR")).unwrap();
    let r = l.reverse(&p.id, d(2026, 11, 2), "fout").unwrap();

    let all = l.list_entries(Some(2026), None, 100).unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].id, r.id); // newest first
    let vrk = l.list_entries(None, Some(JournalCode::VRK), 100).unwrap();
    assert_eq!(vrk.len(), 2);

    let orig = l.entry_detail(&p.id).unwrap();
    assert_eq!(orig.summary.number, "VRK-2026-0001");
    assert_eq!(orig.summary.total_debit_cents, 121_000);
    assert_eq!(orig.summary.reversed_by_entry_id.as_deref(), Some(r.id.as_str()));
    assert_eq!(orig.lines.len(), 3);
    assert_eq!(orig.lines[1].account_name, "Omzet lezingen");
    assert_eq!(orig.hash, p.hash);
    let rev = l.entry_detail(&r.id).unwrap();
    assert_eq!(rev.summary.reverses_entry_id.as_deref(), Some(p.id.as_str()));
    assert!(matches!(l.entry_detail("nope"), Err(DbError::NotFound(_))));
}

#[test]
fn ledger_card_carries_balance_sheet_accounts_forward() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.post(&talk_invoice(d(2026, 12, 15))).unwrap();
    l.post(&entry(
        JournalCode::BNK,
        d(2027, 1, 10),
        "Betaling lezing",
        vec![EntryLine::debit("1100", 121_000), EntryLine::credit("1300", 121_000)],
    ))
    .unwrap();
    l.post(&talk_invoice(d(2027, 2, 1))).unwrap();

    let deb = l.ledger_card("1300", 2027).unwrap();
    assert_eq!(deb.opening_cents, 121_000);
    assert_eq!(deb.rows.len(), 2);
    assert_eq!(deb.rows[0].credit_cents, 121_000);
    assert_eq!(deb.rows[0].balance_cents, 0);
    assert_eq!(deb.closing_cents, 121_000);

    // P&L accounts start at zero each year.
    let rev = l.ledger_card("8100", 2027).unwrap();
    assert_eq!(rev.opening_cents, 0);
    assert_eq!(rev.closing_cents, -100_000);
    assert!(matches!(l.ledger_card("7777", 2027), Err(DbError::NotFound(_))));
}

#[test]
fn periods_report_closed_months() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.close_period(2026, 3).unwrap();
    let p = l.periods(2026).unwrap();
    assert_eq!(p.len(), 12);
    assert!(p[2].closed && !p[3].closed);
}

#[test]
fn company_is_saved_and_audited() {
    let mut l = Ledger::open_in_memory().unwrap();
    assert!(l.company().unwrap().is_none());
    let c = Company {
        legal_name: "Voorbeeld Apps".into(),
        country: "NL".into(),
        ..Default::default()
    };
    l.save_company(&c).unwrap();
    let mut c2 = c.clone();
    c2.kvk_number = Some("12345678".into());
    l.save_company(&c2).unwrap();
    assert_eq!(l.company().unwrap(), Some(c2));
    assert!(l.save_company(&Company::default()).is_err());
    let n: i64 = l
        .conn()
        .query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='company'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 2);
}
