use super::*;

#[test]
fn platform_report_imports_once_and_books_euro_proceeds() {
    let (mut l, dir) = ledger_with_docs();
    let id = l
        .import_platform_report(&apple_input(), b"fictief rapport", "apple_okt.txt")
        .unwrap();
    assert!(matches!(
        l.import_platform_report(&apple_input(), b"fictief rapport", "kopie.txt"),
        Err(DbError::Invalid(_))
    ));

    // Every currency needs a euro amount; EUR must match the report.
    assert!(l
        .book_platform_report(
            &id,
            &[EurAmount {
                currency: "EUR".into(),
                eur_cents: 630
            }]
        )
        .is_err());
    assert!(l
        .book_platform_report(
            &id,
            &[
                EurAmount {
                    currency: "EUR".into(),
                    eur_cents: 600
                },
                EurAmount {
                    currency: "USD".into(),
                    eur_cents: 580
                }
            ]
        )
        .is_err());
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
    let r = l.platform_report(&id).unwrap();
    assert_eq!(r.proceeds_eur_cents, Some(1_210));
    assert!(l.book_platform_report(&id, &[]).is_err());
    assert!(l.delete_platform_report(&id).is_err());

    let tb = l.trial_balance(None).unwrap();
    assert_eq!(tb.iter().find(|r| r.account == "1310").unwrap().balance_cents, 1_210);
    assert_eq!(tb.iter().find(|r| r.account == "8000").unwrap().balance_cents, -1_210);
    let entries = l.list_entries(None, Some(JournalCode::PLT), 10).unwrap();
    assert_eq!(entries[0].date, d(2026, 10, 31));
    assert_eq!(l.platform_revenue_by_product(2026).unwrap().len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn overlapping_platform_reports_are_flagged_and_unbooked_can_be_deleted() {
    let (mut l, dir) = ledger_with_docs();
    let a = l.import_platform_report(&apple_input(), b"regio 1", "a.txt").unwrap();
    let b = l.import_platform_report(&apple_input(), b"regio 2", "b.txt").unwrap();
    assert_eq!(l.platform_report(&a).unwrap().overlapping, vec![b.clone()]);
    l.delete_platform_report(&b).unwrap();
    assert!(l.platform_report(&a).unwrap().overlapping.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
