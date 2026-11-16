use super::*;

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
    assert!(req.missing.iter().any(|m| m.contains("ondernemingsgegevens")));
    assert!(matches!(l.issue_sales_invoice(&inv), Err(DbError::Invalid(_))));
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
        .execute("UPDATE sales_invoice SET total_cents = 1 WHERE id = ?1", [&a])
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
    l.register_sales_payment(&a, d(2026, 11, 10), 21_000, "bank").unwrap();
    let s = &l.sales_invoices(Some(2026)).unwrap()[0];
    assert_eq!((s.open_cents, s.state.as_str()), (100_000, "partially_paid"));
    l.register_sales_payment(&a, d(2026, 11, 20), 100_000, "bank").unwrap();
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
    assert_eq!(tb.iter().find(|r| r.account == "1300").unwrap().balance_cents, 0);
}

#[test]
fn kor_invoice_needs_no_vat_id_and_charges_no_vat() {
    let mut l = Ledger::open_in_memory().unwrap();
    complete_company(&mut l);
    let mut co = l.company().unwrap().unwrap();
    co.vat_number = None;
    l.save_company(&co).unwrap();
    l.set_vat_regime(d(2026, 1, 1), VatRegime::Kor, "test").unwrap();
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
        document_ids: vec![],
    };
    let p = l.register_purchase_invoice(&input).unwrap();
    assert_eq!(
        (p.total_cents, p.open_cents, p.state.as_str()),
        (12_100, 12_100, "open")
    );
    assert!(matches!(l.register_purchase_invoice(&input), Err(DbError::Invalid(_))));
    l.register_purchase_payment(&p.id, d(2026, 11, 5), 12_100, "bank")
        .unwrap();
    assert_eq!(l.purchase_invoice(&p.id).unwrap().summary.state, "paid");
    assert!(l.conn().execute("DELETE FROM purchase_invoice", []).is_err());

    // Paid privately: credit Privé-stortingen, nothing open.
    let private = PurchaseInput {
        supplier_reference: Some("H-002".into()),
        paid_privately: true,
        ..input.clone()
    };
    let pp = l.register_purchase_invoice(&private).unwrap();
    assert_eq!((pp.open_cents, pp.state.as_str()), (0, "paid_privately"));
    let tb = l.trial_balance(None).unwrap();
    assert_eq!(tb.iter().find(|r| r.account == "0510").unwrap().balance_cents, -12_100);
    assert_eq!(tb.iter().find(|r| r.account == "1600").unwrap().balance_cents, 0);

    // Reversing the entry marks the invoice as reversed.
    l.reverse(&pp.entry_id, d(2026, 11, 6), "dubbel").unwrap();
    assert_eq!(l.purchase_invoice(&pp.id).unwrap().summary.state, "reversed");
}

#[test]
fn purchase_invoice_holds_several_documents() {
    let (mut l, dir) = ledger_with_docs();
    let s = supplier(&mut l);
    let invoice = l
        .add_document(b"%PDF-1.4 factuur", "factuur.pdf", "purchase_invoice")
        .unwrap();
    let receipt = l.add_document(b"PNG kassabon", "bon.png", "purchase_invoice").unwrap();
    let input = PurchaseInput {
        contact_id: s.clone(),
        supplier_reference: Some("M-1".into()),
        invoice_date: d(2026, 11, 2),
        due_date: None,
        description: "Monitor".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4000".into(),
            net_cents: 20_000,
            tax_code: "I21".into(),
            vat_cents: 4_200,
        }],
        paid_privately: false,
        // A repeated id is linked once.
        document_ids: vec![invoice.id.clone(), receipt.id.clone(), invoice.id.clone()],
    };
    let p = l.register_purchase_invoice(&input).unwrap();
    assert!(p.has_document);
    let names = |l: &Ledger| -> Vec<String> {
        l.purchase_invoice(&p.id)
            .unwrap()
            .documents
            .into_iter()
            .map(|d| d.original_filename)
            .collect()
    };
    assert_eq!(names(&l), ["factuur.pdf", "bon.png"]);

    // More can be added later; they come after the existing ones and are audited.
    let note = l
        .add_document(b"%PDF-1.4 pakbon", "pakbon.pdf", "purchase_invoice")
        .unwrap();
    l.attach_purchase_documents(&p.id, &[note.id.clone(), receipt.id.clone()])
        .unwrap();
    assert_eq!(names(&l), ["factuur.pdf", "bon.png", "pakbon.pdf"]);
    let audited: i64 = l
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'attach_documents' AND entity_id = ?1",
            [&p.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 1);

    // A document cannot belong to two purchase invoices.
    let other = PurchaseInput {
        supplier_reference: Some("M-2".into()),
        document_ids: vec![receipt.id.clone()],
        ..input.clone()
    };
    assert!(matches!(l.register_purchase_invoice(&other), Err(DbError::Invalid(_))));
    assert!(matches!(
        l.attach_purchase_documents(&p.id, &["onbekend".into()]),
        Err(DbError::NotFound(_))
    ));

    // Links are permanent.
    assert!(l.conn().execute("DELETE FROM purchase_invoice_document", []).is_err());
    assert!(l
        .conn()
        .execute("UPDATE purchase_invoice_document SET position = 9", [])
        .is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn purchase_invoice_can_be_corrected_by_reversal_and_new_registration() {
    let (mut l, dir) = ledger_with_docs();
    let s = supplier(&mut l);
    let receipt = l.add_document(b"%PDF-1.4 bon", "bon.pdf", "purchase_invoice").unwrap();
    let input = PurchaseInput {
        contact_id: s.clone(),
        supplier_reference: Some("C-1".into()),
        invoice_date: d(2026, 11, 3),
        due_date: None,
        description: "Laptop".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4000".into(),
            net_cents: 100_000,
            tax_code: "I21".into(),
            vat_cents: 21_000,
        }],
        paid_privately: false,
        document_ids: vec![receipt.id.clone()],
    };
    let old = l.register_purchase_invoice(&input).unwrap();

    // Typo in the amount: 1.000 should have been 1.100. Same supplier reference and document.
    let fixed = PurchaseInput {
        lines: vec![PurchaseLine {
            net_cents: 110_000,
            vat_cents: 23_100,
            ..input.lines[0].clone()
        }],
        ..input.clone()
    };
    let new = l.correct_purchase_invoice(&old.id, &fixed).unwrap();
    assert_eq!((new.total_cents, new.state.as_str()), (133_100, "open"));
    assert_eq!(new.corrects_id.as_deref(), Some(old.id.as_str()));
    let old_now = l.purchase_invoice(&old.id).unwrap().summary;
    assert_eq!(old_now.state, "corrected");
    assert_eq!(old_now.corrected_by_id.as_deref(), Some(new.id.as_str()));
    assert_eq!(l.purchase_invoice(&new.id).unwrap().documents.len(), 1);

    // Only the corrected amounts remain in the books.
    let tb = l.trial_balance(None).unwrap();
    let bal = |a: &str| tb.iter().find(|r| r.account == a).map_or(0, |r| r.balance_cents);
    assert_eq!((bal("4000"), bal("1500"), bal("1600")), (110_000, 23_100, -133_100));

    // A corrected invoice cannot be corrected or edited again.
    assert!(matches!(
        l.correct_purchase_invoice(&old.id, &fixed),
        Err(DbError::Invalid(_))
    ));
    // The document still counts for the new invoice only.
    let other = PurchaseInput {
        supplier_reference: Some("C-2".into()),
        ..fixed.clone()
    };
    assert!(matches!(l.register_purchase_invoice(&other), Err(DbError::Invalid(_))));

    // Not after a payment.
    l.register_purchase_payment(&new.id, d(2026, 11, 10), 133_100, "bank")
        .unwrap();
    assert!(matches!(
        l.correct_purchase_invoice(&new.id, &input),
        Err(DbError::Invalid(_))
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn purchase_correction_is_refused_in_a_closed_period() {
    let mut l = Ledger::open_in_memory().unwrap();
    let s = supplier(&mut l);
    let input = PurchaseInput {
        contact_id: s,
        supplier_reference: None,
        invoice_date: d(2026, 10, 3),
        due_date: None,
        description: "Hosting oktober".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4000".into(),
            net_cents: 1_000,
            tax_code: "I21".into(),
            vat_cents: 210,
        }],
        paid_privately: false,
        document_ids: vec![],
    };
    let p = l.register_purchase_invoice(&input).unwrap();
    l.close_period(2026, 10).unwrap();
    let fixed = PurchaseInput {
        invoice_date: d(2026, 11, 3),
        ..input.clone()
    };
    assert!(matches!(
        l.correct_purchase_invoice(&p.id, &fixed),
        Err(DbError::Invalid(_))
    ));
    // Nothing was booked: the original is untouched.
    assert_eq!(l.purchase_invoice(&p.id).unwrap().summary.state, "open");
}

#[test]
fn purchase_details_can_be_edited_without_touching_the_booking() {
    let mut l = Ledger::open_in_memory().unwrap();
    let s = supplier(&mut l);
    let base = PurchaseInput {
        contact_id: s,
        supplier_reference: Some("E-1".into()),
        invoice_date: d(2026, 11, 4),
        due_date: None,
        description: "Hostng".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4000".into(),
            net_cents: 5_000,
            tax_code: "I21".into(),
            vat_cents: 1_050,
        }],
        paid_privately: false,
        document_ids: vec![],
    };
    let p = l.register_purchase_invoice(&base).unwrap();
    let second = l
        .register_purchase_invoice(&PurchaseInput {
            supplier_reference: Some("E-2".into()),
            ..base.clone()
        })
        .unwrap();
    let details = PurchaseDetails {
        description: "Hosting".into(),
        supplier_reference: Some("E-1a".into()),
        due_date: Some(d(2026, 12, 4)),
    };
    let s1 = l.update_purchase_details(&p.id, &details).unwrap();
    assert_eq!(
        (s1.description.as_str(), s1.supplier_reference.as_deref(), s1.due_date),
        ("Hosting", Some("E-1a"), Some(d(2026, 12, 4)))
    );
    assert_eq!(
        (s1.total_cents, s1.entry_id.as_str()),
        (p.total_cents, p.entry_id.as_str())
    );
    // Another invoice's number from the same supplier is refused; an empty description too.
    let clash = PurchaseDetails {
        supplier_reference: Some("E-1a".into()),
        ..details.clone()
    };
    assert!(matches!(
        l.update_purchase_details(&second.id, &clash),
        Err(DbError::Invalid(_))
    ));
    let empty = PurchaseDetails {
        description: " ".into(),
        ..details.clone()
    };
    assert!(matches!(
        l.update_purchase_details(&p.id, &empty),
        Err(DbError::Invalid(_))
    ));
    // Booked fields stay locked at database level.
    assert!(l
        .conn()
        .execute("UPDATE purchase_invoice SET total_cents = 1 WHERE id = ?1", [&p.id])
        .is_err());
    let audited: i64 = l
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'update_details' AND entity_id = ?1",
            [&p.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 1);
}

#[test]
fn migration_keeps_existing_purchase_documents() {
    use crate::migrations::MIGRATIONS;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    for (_, sql) in &MIGRATIONS[..8] {
        conn.execute_batch(sql).unwrap();
    }
    // Foreign keys are switched off above, so minimal rows suffice.
    conn.execute_batch(
        "INSERT INTO document (id, sha256, original_filename, mime_type, size_bytes, document_type, retention_until)
             VALUES ('doc1', 'abc', 'bon.pdf', 'application/pdf', 3, 'purchase_invoice', '2033-12-31');
         INSERT INTO purchase_invoice (id, contact_id, invoice_date, description, subtotal_cents, vat_cents,
             total_cents, entry_id, document_id)
             VALUES ('p1', 'c1', '2026-10-01', 'met bon', 100, 21, 121, 'e1', 'doc1'),
                    ('p2', 'c1', '2026-10-02', 'zonder bon', 100, 21, 121, 'e2', NULL);",
    )
    .unwrap();
    conn.execute_batch(MIGRATIONS[8].1).unwrap();
    let links: Vec<(String, String, i64)> = conn
        .prepare("SELECT purchase_invoice_id, document_id, position FROM purchase_invoice_document")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(links, [("p1".to_string(), "doc1".to_string(), 1)]);
}

#[test]
fn documents_are_deduplicated_verified_and_retained() {
    let dir = std::env::temp_dir().join(format!("boekhoud-docs-{}", uuid::Uuid::now_v7()));
    let mut l = Ledger::open_in_memory().unwrap();
    l.set_document_root(&dir);
    let a = l.add_document(b"%PDF-1.4 fictief", "bon.pdf", "receipt").unwrap();
    let b = l.add_document(b"%PDF-1.4 fictief", "kopie.pdf", "receipt").unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(a.mime_type, "application/pdf");
    let (_, bytes) = l.document_bytes(&a.id).unwrap();
    assert_eq!(bytes, b"%PDF-1.4 fictief");
    assert!(l.conn().execute("DELETE FROM document WHERE id = ?1", [&a.id]).is_err());

    // Tampering with the file on disk is detected.
    std::fs::write(l.document_path(&a.id).unwrap(), b"changed").unwrap();
    assert!(matches!(l.document_bytes(&a.id), Err(DbError::Corrupt(_))));
    std::fs::remove_dir_all(&dir).ok();
}
