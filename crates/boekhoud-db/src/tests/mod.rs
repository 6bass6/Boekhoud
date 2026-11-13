//! Integration tests per area (fictional data). Shared helpers live here.

mod bank;
mod invoicing;
mod ledger;
mod platforms;
mod tax;

pub(super) use super::*;
pub(super) use boekhoud_core::Source;

pub(super) fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

pub(super) fn entry(journal: JournalCode, date: NaiveDate, desc: &str, lines: Vec<EntryLine>) -> EntryDraft {
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
pub(super) fn talk_invoice(date: NaiveDate) -> EntryDraft {
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
pub(super) fn apple_report(key: &str) -> EntryDraft {
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

// ---------------------------------------------------------------------------
// Phase 1: tax codes, VAT regime, exchange rates, queries, company
// ---------------------------------------------------------------------------

pub(super) fn talk_with_code(date: NaiveDate, code: &str) -> EntryDraft {
    let mut e = talk_invoice(date);
    e.lines[1].tax_code = Some(code.into());
    e
}

// ---------------------------------------------------------------------------
// Phase 2: contacts, sales invoices, purchase invoices, documents
// ---------------------------------------------------------------------------

use boekhoud_core::invoice::{PurchaseLine, SalesLine};

pub(super) fn complete_company(l: &mut Ledger) {
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

pub(super) fn customer(l: &mut Ledger, vat: Option<&str>) -> String {
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

pub(super) fn talk_line(code: &str) -> SalesLine {
    SalesLine {
        description: "Lezing".into(),
        quantity_milli: 1000,
        unit_price_cents: 100_000,
        tax_code: code.into(),
        account: "8100".into(),
    }
}

pub(super) fn invoice_input(contact: &str, date: NaiveDate, code: &str) -> SalesInvoiceInput {
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

pub(super) fn supplier(l: &mut Ledger) -> String {
    l.save_contact(&Contact {
        name: "Hostingbedrijf".into(),
        country: "NL".into(),
        is_supplier: true,
        ..Default::default()
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// Phase 2c: platform reports
// ---------------------------------------------------------------------------

pub(super) fn ledger_with_docs() -> (Ledger, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("boekhoud-plt-{}", uuid::Uuid::now_v7()));
    let mut l = Ledger::open_in_memory().unwrap();
    l.set_document_root(&dir);
    (l, dir)
}

pub(super) fn apple_input() -> PlatformReportInput {
    PlatformReportInput {
        platform: "apple".into(),
        period_start: d(2026, 9, 28),
        period_end: d(2026, 10, 31),
        currencies: vec![
            CurrencyAmount {
                currency: "EUR".into(),
                units: 9,
                amount_cents: 630,
            },
            CurrencyAmount {
                currency: "USD".into(),
                units: 3,
                amount_cents: 630,
            },
        ],
        products: vec![ProductAmount {
            product: "Voorbeeld App".into(),
            currency: "EUR".into(),
            units: 9,
            amount_cents: 630,
        }],
        breakdown: serde_json::json!({}),
    }
}

// ---------------------------------------------------------------------------
// Phase 3: bank
// ---------------------------------------------------------------------------

pub(super) fn btx(day: u32, amount: i64, name: &str, desc: &str, reference: Option<&str>) -> NewBankTransaction {
    NewBankTransaction {
        booking_date: d(2026, 11, day),
        value_date: None,
        amount_cents: amount,
        currency: "EUR".into(),
        counterparty_name: Some(name.into()),
        counterparty_iban: Some(format!("NL00TEST{:010}", name.len())),
        description: desc.into(),
        end_to_end_id: None,
        bank_reference: reference.map(Into::into),
    }
}

pub(super) fn bank_setup() -> (Ledger, std::path::PathBuf, String) {
    let (mut l, dir) = ledger_with_docs();
    let acc = l
        .ensure_bank_account("NL00 BANK 0123 4567 89", Some("Zakelijk"))
        .unwrap();
    assert_eq!(l.ensure_bank_account("nl00bank0123456789", None).unwrap().id, acc.id);
    (l, dir, acc.id)
}

pub(super) fn import(
    l: &mut Ledger,
    acc: &str,
    txs: Vec<NewBankTransaction>,
    file: &[u8],
    closing: Option<(NaiveDate, i64)>,
) -> BankImportResult {
    l.import_bank_statement(
        acc,
        &BankImportInput {
            format: "test".into(),
            opening: None,
            closing,
            transactions: txs,
        },
        file,
        "afschrift.xml",
    )
    .unwrap()
}
