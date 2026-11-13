use super::*;

#[test]
fn bank_import_skips_duplicates_across_files() {
    let (mut l, dir, acc) = bank_setup();
    let a = btx(1, -500, "Koffie", "pinbetaling", None);
    let r1 = import(
        &mut l,
        &acc,
        vec![a.clone(), a.clone(), btx(2, 1000, "X", "y", Some("REF1"))],
        b"file1",
        None,
    );
    assert_eq!((r1.new_transactions, r1.skipped_duplicates), (3, 0));
    // Overlapping second export: two identical coffees and the same referenced payment again, plus one new.
    let r2 = import(
        &mut l,
        &acc,
        vec![
            a.clone(),
            a,
            btx(2, 1000, "X", "y", Some("REF1")),
            btx(3, -100, "Z", "z", None),
        ],
        b"file2",
        None,
    );
    assert_eq!((r2.new_transactions, r2.skipped_duplicates), (1, 3));
    assert!(l
        .import_bank_statement(&acc, &BankImportInput::default(), b"file2", "x")
        .is_err());
    assert_eq!(l.bank_transactions(&acc, "open", None).unwrap().len(), 4);
    assert!(l
        .conn()
        .execute("UPDATE bank_transaction SET amount_cents = 1", [])
        .is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bank_receipt_settles_sales_invoice_via_suggestion() {
    let (mut l, dir, acc) = bank_setup();
    complete_company(&mut l);
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 11, 1), "V21"))
        .unwrap();
    l.issue_sales_invoice(&inv).unwrap();
    import(
        &mut l,
        &acc,
        vec![btx(10, 121_000, "Stichting Voorbeeld", "Factuur 2026-0001", Some("R1"))],
        b"f",
        None,
    );
    let t = &l.bank_transactions(&acc, "open", None).unwrap()[0];
    let s = l.bank_suggestions(&t.id).unwrap();
    assert_eq!(s[0].confidence, 98);
    assert_eq!(
        s[0].action,
        BankAction::SalesInvoice {
            invoice_id: inv.clone()
        }
    );
    let v = l.process_bank_transaction(&t.id, &s[0].action).unwrap();
    assert_eq!(v.state, "processed");
    assert!(v.entry_number.as_deref().unwrap().starts_with("BNK-2026-"));
    assert_eq!(l.sales_invoice(&inv).unwrap().summary.state, "paid");
    assert!(l.process_bank_transaction(&t.id, &s[0].action).is_err());

    // Reversing the bank entry reopens both the transaction and the invoice; it can be processed again.
    l.reverse(v.entry_id.as_deref().unwrap(), d(2026, 11, 11), "verkeerd")
        .unwrap();
    assert_eq!(l.bank_transaction(&t.id).unwrap().state, "open");
    assert_eq!(l.sales_invoice(&inv).unwrap().summary.open_cents, 121_000);
    l.process_bank_transaction(
        &t.id,
        &BankAction::SalesInvoice {
            invoice_id: inv.clone(),
        },
    )
    .unwrap();
    assert_eq!(l.sales_invoice(&inv).unwrap().summary.state, "paid");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bank_platform_payout_books_exchange_difference() {
    let (mut l, dir, acc) = bank_setup();
    let id = l.import_platform_report(&apple_input(), b"r", "r.txt").unwrap();
    l.book_platform_report(
        &id,
        &[
            EurAmount {
                currency: "EUR".into(),
                eur_cents: 630,
            },
            EurAmount {
                currency: "USD".into(),
                eur_cents: 580,
            },
        ],
    )
    .unwrap();
    import(
        &mut l,
        &acc,
        vec![btx(
            15,
            1_205,
            "Apple Distribution International",
            "APPLE PAYMENT",
            None,
        )],
        b"f",
        None,
    );
    let t = &l.bank_transactions(&acc, "open", None).unwrap()[0];
    let s = l.bank_suggestions(&t.id).unwrap();
    assert_eq!(
        s[0].action,
        BankAction::PlatformPayout {
            platform: "apple".into(),
            settle_cents: Some(1_210)
        }
    );
    l.process_bank_transaction(&t.id, &s[0].action).unwrap();
    let tb = l.trial_balance(None).unwrap();
    let bal = |a: &str| tb.iter().find(|r| r.account == a).map(|r| r.balance_cents).unwrap_or(0);
    assert_eq!((bal("1310"), bal("9000"), bal("1100")), (0, 5, 1_205));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bank_direct_bookings_split_vat_and_learn() {
    let (mut l, dir, acc) = bank_setup();
    import(
        &mut l,
        &acc,
        vec![
            btx(1, -12_100, "Hostingbedrijf", "november", None),
            btx(2, -10_000, "Software Ltd", "licentie", None),
            btx(3, 50_000, "Eigenaar", "storting", None),
            btx(4, -12_100, "Hostingbedrijf", "december", None),
        ],
        b"f",
        None,
    );
    let txs = l.bank_transactions(&acc, "open", None).unwrap();
    let find = |desc: &str| txs.iter().find(|t| t.description == desc).unwrap().id.clone();
    l.process_bank_transaction(
        &find("november"),
        &BankAction::Account {
            account: "4000".into(),
            tax_code: Some("I21".into()),
            description: None,
        },
    )
    .unwrap();
    l.process_bank_transaction(
        &find("licentie"),
        &BankAction::Account {
            account: "4010".into(),
            tax_code: Some("IEU".into()),
            description: None,
        },
    )
    .unwrap();
    l.process_bank_transaction(&find("storting"), &BankAction::Private)
        .unwrap();
    let tb = l.trial_balance(None).unwrap();
    let bal = |a: &str| tb.iter().find(|r| r.account == a).map(|r| r.balance_cents).unwrap_or(0);
    assert_eq!(bal("4000"), 10_000);
    assert_eq!(bal("1500"), 2_100 + 2_100);
    assert_eq!(bal("1515"), -2_100);
    assert_eq!(bal("4010"), 10_000);
    assert_eq!(bal("0510"), -50_000);

    // The second hosting payment is proposed the same way.
    let s = l.bank_suggestions(&find("december")).unwrap();
    assert_eq!(
        s[0].action,
        BankAction::Account {
            account: "4000".into(),
            tax_code: Some("I21".into()),
            description: None
        }
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bank_ignore_and_reconciliation() {
    let (mut l, dir, acc) = bank_setup();
    import(
        &mut l,
        &acc,
        vec![
            btx(1, 100_000, "Eigenaar", "inbreng", None),
            btx(2, -2_00, "Bank", "kosten", None),
        ],
        b"f",
        Some((d(2026, 11, 30), 99_800)),
    );
    let txs = l.bank_transactions(&acc, "open", None).unwrap();
    let r = l.bank_reconciliation(&acc).unwrap();
    assert_eq!(
        (r.statement_balance_cents, r.difference_cents, r.open_transactions),
        (Some(99_800), Some(99_800), 2)
    );
    let inbreng = txs.iter().find(|t| t.amount_cents > 0).unwrap();
    let kosten = txs.iter().find(|t| t.amount_cents < 0).unwrap();
    assert!(l
        .process_bank_transaction(&kosten.id, &BankAction::Ignore { reason: " ".into() })
        .is_err());
    l.process_bank_transaction(&kosten.id, &BankAction::Ignore { reason: "test".into() })
        .unwrap();
    assert_eq!(l.bank_transaction(&kosten.id).unwrap().state, "ignored");
    l.reopen_bank_transaction(&kosten.id).unwrap();
    l.process_bank_transaction(
        &kosten.id,
        &BankAction::Account {
            account: "4700".into(),
            tax_code: None,
            description: None,
        },
    )
    .unwrap();
    l.process_bank_transaction(&inbreng.id, &BankAction::Private).unwrap();
    let r = l.bank_reconciliation(&acc).unwrap();
    assert_eq!(
        (r.difference_cents, r.open_transactions, r.ledger_balance_cents),
        (Some(0), 0, 99_800)
    );
    std::fs::remove_dir_all(&dir).ok();
}
