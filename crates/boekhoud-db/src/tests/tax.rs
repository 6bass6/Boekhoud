use super::*;

#[test]
fn vat_return_rubrics_icp_and_filing() {
    let (mut l, dir) = ledger_with_docs();
    complete_company(&mut l);
    // Sales 21% (1a)
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 10, 5), "V21"))
        .unwrap();
    l.issue_sales_invoice(&inv).unwrap();
    // Apple proceeds (3b + ICP)
    let apple = l
        .save_contact(&Contact {
            name: "Apple Distribution International".into(),
            country: "IE".into(),
            vat_number: Some("IE0000000X".into()),
            is_platform: true,
            ..Default::default()
        })
        .unwrap();
    l.set_platform_contact("apple", Some(&apple)).unwrap();
    let mut input = apple_input();
    input.period_end = d(2026, 11, 1);
    let rep = l.import_platform_report(&input, b"r", "r.txt").unwrap();
    l.book_platform_report(
        &rep,
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
    // Purchases: NL 21% (5b) and EU service reverse charged (4b + 5b)
    let s = supplier(&mut l);
    l.register_purchase_invoice(&PurchaseInput {
        contact_id: s,
        supplier_reference: Some("P1".into()),
        invoice_date: d(2026, 11, 2),
        due_date: None,
        description: "Hosting en software".into(),
        lines: vec![
            PurchaseLine {
                description: None,
                account: "4000".into(),
                net_cents: 10_000,
                tax_code: "I21".into(),
                vat_cents: 2_100,
            },
            PurchaseLine {
                description: None,
                account: "4010".into(),
                net_cents: 5_000,
                tax_code: "IEU".into(),
                vat_cents: 1_050,
            },
        ],
        paid_privately: false,
        document_id: None,
    })
    .unwrap();
    // A manual booking with VAT but without a code triggers a warning.
    l.post(&entry(
        JournalCode::MEM,
        d(2026, 11, 3),
        "Handmatig",
        vec![
            EntryLine::debit("4300", 1_000),
            EntryLine::debit("1500", 210),
            EntryLine::credit("1600", 1_210),
        ],
    ))
    .unwrap();

    let (start, end) = quarter_of(d(2026, 11, 11));
    let r = l.vat_return(start, end).unwrap();
    let rub = |code: &str| r.rubrics.iter().find(|x| x.rubric == code).unwrap().clone();
    assert_eq!((rub("1a").base_cents, rub("1a").vat_cents), (100_000, 21_000));
    assert_eq!((rub("3b").base_cents, rub("3b").vat_cents), (1_210, 0));
    assert_eq!((rub("4b").base_cents, rub("4b").vat_cents), (5_000, 1_050));
    assert_eq!(r.due_cents, 22_050);
    assert_eq!(r.input_vat_cents, 2_100 + 1_050);
    assert_eq!(r.balance_cents, 22_050 - 3_150);
    assert_eq!((r.rounded.due, r.rounded.input_vat), (220, 32));
    assert!(r.warnings.iter().any(|w| w.contains("zonder btw-code")));
    assert!(r.warnings.iter().any(|w| w.contains("zonder bewijsstuk")));

    let detail = l.vat_return_detail(start, end, "1a").unwrap();
    assert_eq!(detail.len(), 1);
    assert_eq!((detail[0].base_cents, detail[0].vat_cents), (100_000, 21_000));
    let detail5b = l.vat_return_detail(start, end, "5b").unwrap();
    assert_eq!(detail5b.iter().map(|x| x.vat_cents).sum::<i64>(), 3_150);

    let icp = l.icp_listing(start, end).unwrap();
    assert_eq!(icp.len(), 1);
    assert_eq!(
        (icp[0].vat_number.as_deref(), icp[0].services_cents),
        (Some("IE0000000X"), 1_210)
    );

    // Filing clears the VAT accounts (period movements) to 1520 and closes the quarter.
    l.file_vat_return(start, end).unwrap();
    let tb = l.trial_balance(None).unwrap();
    let bal = |a: &str| tb.iter().find(|r| r.account == a).map(|r| r.balance_cents).unwrap_or(0);
    assert_eq!((bal("1500"), bal("1510"), bal("1515")), (0, 0, 0));
    assert_eq!(bal("1520"), -(21_000 + 1_050 - 2_100 - 1_050 - 210));
    assert!(l.periods(2026).unwrap()[10].closed);
    assert!(l.file_vat_return(start, end).is_err());
    assert!(l.vat_return(start, end).unwrap().filed.is_some());
    // The return itself is unchanged by the settlement entry.
    assert_eq!(l.vat_return(start, end).unwrap().due_cents, 22_050);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hours_and_timer() {
    let mut l = Ledger::open_in_memory().unwrap();
    let id = l
        .save_time_entry(&TimeEntry {
            id: String::new(),
            entry_date: d(2026, 11, 2),
            started_at: None,
            minutes: Some(90),
            activity: "Ontwikkeling".into(),
            project: None,
            description: None,
        })
        .unwrap();
    assert!(l
        .save_time_entry(&TimeEntry {
            id: String::new(),
            entry_date: d(2026, 11, 2),
            started_at: None,
            minutes: Some(0),
            activity: "x".into(),
            project: None,
            description: None
        })
        .is_err());
    l.start_timer("Administratie", None, None).unwrap();
    assert!(l.start_timer("x", None, None).is_err());
    let started = l.running_timer().unwrap().unwrap().started_at.unwrap();
    let start = chrono::DateTime::parse_from_rfc3339(&started)
        .unwrap()
        .with_timezone(&chrono::Utc);
    let t = l.stop_timer(start + chrono::Duration::seconds(30 * 60 + 5)).unwrap();
    assert_eq!(t.minutes, Some(31));
    let year = chrono::Utc::now().year();
    let s = l.hours_summary(year, chrono::Utc::now().date_naive()).unwrap();
    assert!(s.total_minutes >= 31);
    let s26 = l.hours_summary(2026, d(2026, 11, 11)).unwrap();
    assert!(s26.by_month[10] >= 90);
    assert!(s26.needed_per_week_minutes > 0);
    l.delete_time_entry(&id).unwrap();
}

#[test]
fn assets_depreciate_monthly_once() {
    let mut l = Ledger::open_in_memory().unwrap();
    let id = l
        .add_asset(&AssetInput {
            name: "Laptop".into(),
            acquisition_date: d(2026, 1, 15),
            cost_cents: 200_000,
            residual_cents: 0,
            months: 60,
            start_month: None,
            business_use_pct: 100,
            purchase_invoice_id: None,
            notes: None,
        })
        .unwrap();
    assert_eq!(l.post_depreciation(d(2026, 3, 10)).unwrap(), 3);
    assert_eq!(l.post_depreciation(d(2026, 3, 31)).unwrap(), 0);
    let a = l.assets().unwrap().into_iter().find(|a| a.id == id).unwrap();
    assert_eq!(
        (a.months_posted, a.depreciated_cents, a.book_value_cents),
        (3, 9_999, 190_001)
    );
    let tb = l.trial_balance(None).unwrap();
    assert_eq!(tb.iter().find(|r| r.account == "4800").unwrap().balance_cents, 9_999);
    assert!(l
        .add_asset(&AssetInput {
            name: "x".into(),
            acquisition_date: d(2026, 1, 1),
            cost_cents: 100,
            residual_cents: 100,
            months: 1,
            start_month: None,
            business_use_pct: 100,
            purchase_invoice_id: None,
            notes: None
        })
        .is_err());
}

#[test]
fn dashboard_summarises_the_year() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 3, 5), "V21"))
        .unwrap();
    l.issue_sales_invoice(&inv).unwrap();
    l.create_sales_invoice(&invoice_input(&c, d(2026, 4, 5), "V21"))
        .unwrap();
    let db = l.dashboard(2026, d(2026, 11, 11)).unwrap();
    assert_eq!((db.revenue_cents, db.result_cents), (100_000, 100_000));
    assert_eq!(
        (db.receivables_open_cents, db.receivables_overdue, db.draft_invoices),
        (121_000, 1, 1)
    );
    assert_eq!(db.vat_position_cents, 21_000);
    assert_eq!(db.revenue_by_month[2], 100_000);
}

#[test]
fn income_tax_estimate_uses_ledger_result_hours_and_latest_parameters() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let c = customer(&mut l, None);
    let inv = l
        .create_sales_invoice(&invoice_input(&c, d(2026, 3, 5), "V21"))
        .unwrap();
    l.issue_sales_invoice(&inv).unwrap();
    let today = d(2026, 11, 13);

    // Profit 1.000,00, hours not met: no ondernemersaftrek; mkb 12,7% = 127,00; taxable 873,00.
    let v = l.income_tax_estimate(2026, today, 0, 0, true).unwrap();
    assert_eq!((v.parameter_year, v.remaining_months), (2026, 2));
    assert_eq!(v.params.brackets.len(), 3);
    assert_eq!(v.params.arbeidskorting.len(), 5);
    assert_eq!(v.params.algemene_heffingskorting[1].rate_micro, -63_980);
    let e = &v.estimate;
    assert_eq!(
        (e.profit_cents, e.ondernemersaftrek_cents, e.mkb_vrijstelling_cents),
        (100_000, 0, 12_700)
    );
    // Tax 35,75% × 873,00 = 312,10 is cancelled by the credits; Zvw 4,85% × 873,00 = 42,34.
    assert_eq!((e.tax_after_credits_cents, e.zvw_cents), (0, 4_234));
    // Withheld 50,00 → refund of 7,66; no reserve.
    let v = l.income_tax_estimate(2026, today, 0, 5_000, false).unwrap();
    assert_eq!(
        (v.estimate.to_pay_cents, v.estimate.monthly_reserve_cents),
        (-766, None)
    );
    assert!(v
        .sources
        .iter()
        .all(|s| s.starts_with("https://www.belastingdienst.nl/")));

    // Urencriterium met: zelfstandigenaftrek capped at the profit (not a starter) → taxable 0.
    // Raw insert: one entry of 1,225 h is refused by save_time_entry (max 24 h per day).
    l.conn
        .execute(
            "INSERT INTO time_entry (id, entry_date, minutes, activity) VALUES ('t1', '2026-06-30', ?1, 'Ontwikkeling')",
            [HOURS_CRITERION * 60],
        )
        .unwrap();
    let e = l.income_tax_estimate(2026, today, 0, 0, false).unwrap().estimate;
    assert_eq!(
        (e.zelfstandigenaftrek_cents, e.taxable_profit_cents, e.total_cents),
        (100_000, 0, 0)
    );

    // A year without parameters falls back to the latest year that has them.
    let v = l.income_tax_estimate(2027, today, 0, 0, false).unwrap();
    assert_eq!((v.year, v.parameter_year, v.remaining_months), (2027, 2026, 12));
}
