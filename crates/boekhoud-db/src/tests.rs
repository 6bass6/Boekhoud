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

// ---------------------------------------------------------------------------
// Phase 1: tax codes, VAT regime, exchange rates, queries, company
// ---------------------------------------------------------------------------

fn talk_with_code(date: NaiveDate, code: &str) -> EntryDraft {
    let mut e = talk_invoice(date);
    e.lines[1].tax_code = Some(code.into());
    e
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
    assert_eq!(
        l.vat_regime_on(d(2026, 12, 31)).unwrap(),
        VatRegime::Regular
    );
    assert_eq!(l.vat_regime_on(d(2027, 3, 1)).unwrap(), VatRegime::Kor);

    // KOR: VKOR allowed, V21 refused, regime-neutral V0EU (Apple) still allowed.
    let mut k = kor_talk.clone();
    k.date = d(2027, 2, 1);
    l.post(&k).unwrap();
    match l.post(&talk_with_code(d(2027, 2, 1), "V21")) {
        Err(DbError::Validation(e)) => assert!(
            matches!(&e[0], ValidationError::TaxCodeNotAllowed { code, .. } if code == "V21")
        ),
        other => panic!("{other:?}"),
    }
    let mut apple = apple_report("apple:2027-01:EUR");
    apple.date = d(2027, 1, 31);
    l.post(&apple).unwrap();
}

#[test]
fn exchange_rate_falls_back_to_last_published_day() {
    let mut l = Ledger::open_in_memory().unwrap();
    l.add_exchange_rate(d(2026, 11, 6), "USD", 1_082_300, "ECB")
        .unwrap(); // Friday
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
    assert_eq!(
        orig.summary.reversed_by_entry_id.as_deref(),
        Some(r.id.as_str())
    );
    assert_eq!(orig.lines.len(), 3);
    assert_eq!(orig.lines[1].account_name, "Omzet lezingen");
    assert_eq!(orig.hash, p.hash);
    let rev = l.entry_detail(&r.id).unwrap();
    assert_eq!(
        rev.summary.reverses_entry_id.as_deref(),
        Some(p.id.as_str())
    );
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
        vec![
            EntryLine::debit("1100", 121_000),
            EntryLine::credit("1300", 121_000),
        ],
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
    assert!(matches!(
        l.ledger_card("7777", 2027),
        Err(DbError::NotFound(_))
    ));
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
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='company'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2);
}

// ---------------------------------------------------------------------------
// Phase 2: contacts, sales invoices, purchase invoices, documents
// ---------------------------------------------------------------------------

use boekhoud_core::invoice::{PurchaseLine, SalesLine};

fn complete_company(l: &mut Ledger) {
    l.save_company(&Company {
        legal_name: "Voorbeeld Apps".into(),
        kvk_number: Some("12345678".into()),
        vat_number: Some("NL000099998B57".into()),
        address: Some("Voorbeeldstraat 1".into()),
        postal_code: Some("1234 AB".into()),
        city: Some("Utrecht".into()),
        iban: Some("NL00 BANK 0123 4567 89".into()),
        ..Default::default()
    })
    .unwrap();
}

fn customer(l: &mut Ledger, vat: Option<&str>) -> String {
    l.save_contact(&Contact {
        name: "Stichting Voorbeeld".into(),
        address: Some("Laan 2".into()),
        postal_code: Some("5678 CD".into()),
        city: Some("Zwolle".into()),
        country: "NL".into(),
        vat_number: vat.map(Into::into),
        is_customer: true,
        ..Default::default()
    })
    .unwrap()
}

fn talk_line(code: &str) -> SalesLine {
    SalesLine {
        description: "Lezing".into(),
        quantity_milli: 1000,
        unit_price_cents: 100_000,
        tax_code: code.into(),
        account: "8100".into(),
    }
}

fn invoice_input(contact: &str, date: NaiveDate, code: &str) -> SalesInvoiceInput {
    SalesInvoiceInput {
        contact_id: contact.into(),
        invoice_date: date,
        supply_date: None,
        due_date: None,
        reference: None,
        notes: None,
        lines: vec![talk_line(code)],
    }
}

#[test]
fn contact_save_normalises_and_audits() {
    let mut l = Ledger::open_in_memory().unwrap();
    let id = customer(&mut l, Some("nl 0000 9999 8b57"));
    let c = l.contact(&id).unwrap();
    assert_eq!(c.vat_number.as_deref(), Some("NL000099998B57"));
    assert!(l
        .save_contact(&Contact {
            name: " ".into(),
            ..Default::default()
        })
        .is_err());
    assert_eq!(l.contacts(false).unwrap().len(), 1);
}

#[test]
fn issuing_requires_dutch_invoice_details() {
    let mut l = Ledger::open_in_memory().unwrap();
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "V21"))
        .unwrap();
    let req = l.sales_issue_requirements(&inv).unwrap();
    assert!(req
        .missing
        .iter()
        .any(|m| m.contains("ondernemingsgegevens")));
    assert!(matches!(
        l.issue_sales_invoice(&inv),
        Err(DbError::Invalid(_))
    ));
    complete_company(&mut l);
    assert!(l.sales_issue_requirements(&inv).unwrap().missing.is_empty());

    // Reverse charge needs the customer's VAT id.
    let rc = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "V0EU"))
        .unwrap();
    assert!(l
        .sales_issue_requirements(&rc)
        .unwrap()
        .missing
        .iter()
        .any(|m| m.contains("btw verlegd")));
}

#[test]
fn issue_assigns_gapless_numbers_and_posts() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let c = customer(&mut l, None);
    let a = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "V21"))
        .unwrap();
    let b = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 2), "V21"))
        .unwrap();
    let draft_to_delete = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 2), "V21"))
        .unwrap();
    l.delete_draft_sales_invoice(&draft_to_delete).unwrap();
    // Issue b first: numbers follow issue order, never gaps.
    assert_eq!(
        l.issue_sales_invoice(&b).unwrap().invoice_number.as_deref(),
        Some("2026-0001")
    );
    let sa = l.issue_sales_invoice(&a).unwrap();
    assert_eq!(sa.invoice_number.as_deref(), Some("2026-0002"));
    assert_eq!(sa.total_cents, 121_000);
    assert_eq!(sa.open_cents, 121_000);
    assert!(l.issue_sales_invoice(&a).is_err());
    assert!(l
        .update_sales_invoice(&a, &invoice_input(&c, d(2026, 11, 1), "V21"))
        .is_err());
    assert!(l.delete_draft_sales_invoice(&a).is_err());

    let tb = l.trial_balance(None).unwrap();
    let get = |acc: &str| {
        tb.iter()
            .find(|r| r.account == acc)
            .map(|r| r.balance_cents)
            .unwrap_or(0)
    };
    assert_eq!(get("1300"), 242_000);
    assert_eq!(get("8100"), -200_000);
    assert_eq!(get("1510"), -42_000);

    // The database itself refuses to alter an issued invoice.
    assert!(l
        .conn()
        .execute(
            "UPDATE sales_invoice SET total_cents = 1 WHERE id = ?1",
            [&a]
        )
        .is_err());
    assert!(l
        .conn()
        .execute("DELETE FROM sales_invoice_line WHERE invoice_id = ?1", [&a])
        .is_err());

    let detail = l.sales_invoice(&a).unwrap();
    assert_eq!(detail.buyer.unwrap().name, "Stichting Voorbeeld");
    assert_eq!(detail.seller.unwrap().legal_name, "Voorbeeld Apps");
}

#[test]
fn payments_and_credit_notes_settle_invoices() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let c = customer(&mut l, None);
    let a = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "V21"))
        .unwrap();
    l.issue_sales_invoice(&a).unwrap();
    l.register_sales_payment(&a, d(2026, 11, 10), 21_000, "bank")
        .unwrap();
    let s = &l.sales_invoices(Some(2026)).unwrap()[0];
    assert_eq!(
        (s.open_cents, s.state.as_str()),
        (100_000, "partially_paid")
    );
    l.register_sales_payment(&a, d(2026, 11, 20), 100_000, "bank")
        .unwrap();
    assert_eq!(l.sales_invoice(&a).unwrap().summary.state, "paid");

    // A credited invoice is settled by its credit note.
    let b = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 3), "V21"))
        .unwrap();
    l.issue_sales_invoice(&b).unwrap();
    let cn = l.create_credit_note(&b, d(2026, 11, 4)).unwrap();
    let cn_issued = l.issue_sales_invoice(&cn).unwrap();
    assert!(cn_issued.is_credit_note);
    assert_eq!(cn_issued.total_cents, -121_000);
    assert_eq!(cn_issued.invoice_number.as_deref(), Some("2026-0003"));
    let sb = l.sales_invoice(&b).unwrap().summary;
    assert_eq!((sb.open_cents, sb.state.as_str()), (0, "credited"));
    assert!(l.create_credit_note(&cn, d(2026, 11, 5)).is_err());

    let tb = l.trial_balance(None).unwrap();
    assert_eq!(
        tb.iter()
            .find(|r| r.account == "1300")
            .unwrap()
            .balance_cents,
        0
    );
}

#[test]
fn kor_invoice_needs_no_vat_id_and_charges_no_vat() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let mut co = l.company().unwrap().unwrap();
    co.vat_number = None;
    l.save_company(&co).unwrap();
    l.set_vat_regime(d(2026, 1, 1), VatRegime::Kor, "test")
        .unwrap();
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "VKOR"))
        .unwrap();
    let s = l.issue_sales_invoice(&inv).unwrap();
    assert_eq!(s.total_cents, 100_000);
    // V21 under KOR is refused at posting.
    let bad = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 2), "V21"))
        .unwrap();
    assert!(l.issue_sales_invoice(&bad).is_err());
}

fn supplier(l: &mut Ledger) -> String {
    l.save_contact(&Contact {
        name: "Hostingbedrijf".into(),
        country: "NL".into(),
        is_supplier: true,
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn purchase_invoice_posts_and_detects_duplicates() {
    let mut l = Ledger::open_in_memory().unwrap();
    let s = supplier(&mut l);
    let input = PurchaseInput {
        contact_id: s.clone(),
        supplier_reference: Some("H-001".into()),
        invoice_date: d(2026, 11, 1),
        due_date: None,
        description: "Hosting november".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4000".into(),
            net_cents: 10_000,
            tax_code: "I21".into(),
            vat_cents: 2_100,
        }],
        paid_privately: false,
        document_id: None,
    };
    let p = l.register_purchase_invoice(&input).unwrap();
    assert_eq!(
        (p.total_cents, p.open_cents, p.state.as_str()),
        (12_100, 12_100, "open")
    );
    assert!(matches!(
        l.register_purchase_invoice(&input),
        Err(DbError::Invalid(_))
    ));
    l.register_purchase_payment(&p.id, d(2026, 11, 5), 12_100, "bank")
        .unwrap();
    assert_eq!(l.purchase_invoice(&p.id).unwrap().summary.state, "paid");
    assert!(l
        .conn()
        .execute("DELETE FROM purchase_invoice", [])
        .is_err());

    // Paid privately: credit Privé-stortingen, nothing open.
    let private = PurchaseInput {
        supplier_reference: Some("H-002".into()),
        paid_privately: true,
        ..input.clone()
    };
    let pp = l.register_purchase_invoice(&private).unwrap();
    assert_eq!((pp.open_cents, pp.state.as_str()), (0, "paid_privately"));
    let tb = l.trial_balance(None).unwrap();
    assert_eq!(
        tb.iter()
            .find(|r| r.account == "0510")
            .unwrap()
            .balance_cents,
        -12_100
    );
    assert_eq!(
        tb.iter()
            .find(|r| r.account == "1600")
            .unwrap()
            .balance_cents,
        0
    );

    // Reversing the entry marks the invoice as reversed.
    l.reverse(&pp.entry_id, d(2026, 11, 6), "dubbel").unwrap();
    assert_eq!(
        l.purchase_invoice(&pp.id).unwrap().summary.state,
        "reversed"
    );
}

#[test]
fn documents_are_deduplicated_verified_and_retained() {
    let dir = std::env::temp_dir().join(format!("boekhoud-docs-{}", uuid::Uuid::now_v7()));
    let mut l = Ledger::open_in_memory().unwrap();
    l.set_document_root(&dir);
    let a = l
        .add_document(b"%PDF-1.4 fictief", "bon.pdf", "receipt")
        .unwrap();
    let b = l
        .add_document(b"%PDF-1.4 fictief", "kopie.pdf", "receipt")
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(a.mime_type, "application/pdf");
    let (_, bytes) = l.document_bytes(&a.id).unwrap();
    assert_eq!(bytes, b"%PDF-1.4 fictief");
    assert!(l
        .conn()
        .execute("DELETE FROM document WHERE id = ?1", [&a.id])
        .is_err());

    // Tampering with the file on disk is detected.
    std::fs::write(l.document_path(&a.id).unwrap(), b"changed").unwrap();
    assert!(matches!(l.document_bytes(&a.id), Err(DbError::Corrupt(_))));
    std::fs::remove_dir_all(&dir).ok();
}
