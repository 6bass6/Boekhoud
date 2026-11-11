//! Create a database filled with fictional demo data covering every module.
//!
//! Usage: cargo run -p boekhoud-db --example demo_data -- <path/to/boekhoud.sqlite>
//! Refuses to touch an existing file. All names, numbers and IBANs are fictional.

use boekhoud_core::invoice::{PurchaseLine, SalesLine};
use boekhoud_core::{EntryDraft, EntryLine, JournalCode, Source};
use boekhoud_db::{
    AssetInput, BankAction, BankImportInput, Company, Contact, CurrencyAmount, EurAmount, Ledger, NewBankTransaction,
    PlatformReportInput, ProductAmount, PurchaseInput, SalesInvoiceInput, TimeEntry,
};
use chrono::NaiveDate;

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: demo_data <path>")?;
    let path = std::path::Path::new(&path);
    if path.exists() {
        return Err(format!("{} already exists; refusing to overwrite", path.display()).into());
    }
    let mut l = Ledger::open(path)?;
    l.save_company(&Company {
        legal_name: "Voorbeeld Apps".into(),
        kvk_number: Some("12345678".into()),
        vat_number: Some("NL000099998B57".into()),
        address: Some("Voorbeeldstraat 1".into()),
        postal_code: Some("1234 AB".into()),
        city: Some("Utrecht".into()),
        iban: Some("NL00BANK0123456789".into()),
        email: Some("info@voorbeeld.invalid".into()),
        ..Default::default()
    })?;

    let contact =
        |name: &str, city: &str, country: &str, vat: Option<&str>, customer: bool, supplier: bool, platform: bool| {
            Contact {
                name: name.into(),
                address: Some("Voorbeeldlaan 10".into()),
                postal_code: Some("1000 AA".into()),
                city: Some(city.into()),
                country: country.into(),
                vat_number: vat.map(Into::into),
                is_customer: customer,
                is_supplier: supplier,
                is_platform: platform,
                ..Default::default()
            }
        };
    let stichting = l.save_contact(&contact(
        "Stichting Voorbeeld",
        "Zwolle",
        "NL",
        None,
        true,
        false,
        false,
    ))?;
    let school = l.save_contact(&contact("Hogeschool Fictief", "Arnhem", "NL", None, true, false, false))?;
    let hosting = l.save_contact(&Contact {
        default_account: Some("4000".into()),
        default_tax_code: Some("I21".into()),
        ..contact("Hosting & Co", "Amsterdam", "NL", None, false, true, false)
    })?;
    let software = l.save_contact(&Contact {
        default_account: Some("4010".into()),
        default_tax_code: Some("IEU".into()),
        ..contact("Software Ltd", "Dublin", "IE", Some("IE0000000A"), false, true, false)
    })?;
    let apple = l.save_contact(&contact(
        "Apple (fictief contact)",
        "Cork",
        "IE",
        Some("IE0000000X"),
        false,
        false,
        true,
    ))?;
    l.set_platform_contact("apple", Some(&apple))?;

    // Opening: own money in.
    l.post(&EntryDraft {
        journal: JournalCode::MEM,
        date: d(1, 2),
        description: "Inbreng eigen geld".into(),
        lines: vec![EntryLine::debit("1100", 500_000), EntryLine::credit("0510", 500_000)],
        source: Source::manual(),
        idempotency_key: None,
    })?;

    // Sales invoices for talks.
    let talk = |desc: &str, price: i64| SalesLine {
        description: desc.into(),
        quantity_milli: 1000,
        unit_price_cents: price,
        tax_code: "V21".into(),
        account: "8100".into(),
    };
    for (c, date, desc, price) in [
        (&stichting, d(3, 10), "Lezing ‘Apps bouwen’, 9 maart", 100_000),
        (&school, d(9, 20), "Gastcollege mobiele apps, 18 september", 75_000),
        (&stichting, d(10, 28), "Workshop app-ontwerp, 27 oktober", 120_000),
    ] {
        let id = l.create_sales_invoice(&SalesInvoiceInput {
            contact_id: c.clone(),
            invoice_date: date,
            supply_date: None,
            due_date: None,
            reference: None,
            notes: None,
            lines: vec![talk(desc, price)],
        })?;
        l.issue_sales_invoice(&id)?;
    }
    l.create_sales_invoice(&SalesInvoiceInput {
        contact_id: school.clone(),
        invoice_date: d(11, 10),
        supply_date: None,
        due_date: None,
        reference: Some("PO-2026-17".into()),
        notes: None,
        lines: vec![talk("Lezing 'Van idee tot App Store'", 90_000)],
    })?;

    // Purchases.
    for (m, reference) in [(9, "H-0901"), (10, "H-1001"), (11, "H-1101")] {
        l.register_purchase_invoice(&PurchaseInput {
            contact_id: hosting.clone(),
            supplier_reference: Some(reference.into()),
            invoice_date: d(m, 1),
            due_date: None,
            description: "Hosting".into(),
            lines: vec![PurchaseLine {
                description: None,
                account: "4000".into(),
                net_cents: 2_500,
                tax_code: "I21".into(),
                vat_cents: 525,
            }],
            paid_privately: false,
            document_id: None,
        })?;
    }
    l.register_purchase_invoice(&PurchaseInput {
        contact_id: software.clone(),
        supplier_reference: Some("SL-778".into()),
        invoice_date: d(10, 15),
        due_date: None,
        description: "Ontwerpsoftware jaarlicentie".into(),
        lines: vec![PurchaseLine {
            description: None,
            account: "4010".into(),
            net_cents: 24_000,
            tax_code: "IEU".into(),
            vat_cents: 5_040,
        }],
        paid_privately: false,
        document_id: None,
    })?;

    // Apple report for an October period.
    let rep = l.import_platform_report(
        &PlatformReportInput {
            platform: "apple".into(),
            period_start: d(9, 28),
            period_end: d(11, 1),
            currencies: vec![
                CurrencyAmount {
                    currency: "EUR".into(),
                    units: 412,
                    amount_cents: 61_840,
                },
                CurrencyAmount {
                    currency: "USD".into(),
                    units: 230,
                    amount_cents: 38_150,
                },
            ],
            products: vec![
                ProductAmount {
                    product: "Voorbeeld App".into(),
                    currency: "EUR".into(),
                    units: 412,
                    amount_cents: 61_840,
                },
                ProductAmount {
                    product: "Voorbeeld App".into(),
                    currency: "USD".into(),
                    units: 230,
                    amount_cents: 38_150,
                },
            ],
            breakdown: serde_json::json!({ "rows": 2 }),
        },
        b"fictief apple rapport oktober",
        "financial_report_oktober.txt",
    )?;
    l.book_platform_report(
        &rep,
        &[
            EurAmount {
                currency: "EUR".into(),
                eur_cents: 61_840,
            },
            EurAmount {
                currency: "USD".into(),
                eur_cents: 35_090,
            },
        ],
    )?;

    // Bank statement.
    let acc = l.ensure_bank_account("NL00BANK0123456789", Some("Zakelijke rekening"))?;
    let tx = |m: u32, day: u32, amount: i64, name: &str, desc: &str| NewBankTransaction {
        booking_date: d(m, day),
        value_date: None,
        amount_cents: amount,
        currency: "EUR".into(),
        counterparty_name: Some(name.into()),
        counterparty_iban: Some(format!("NL00DEMO{:010}", name.len())),
        description: desc.into(),
        end_to_end_id: None,
        bank_reference: None,
    };
    l.import_bank_statement(
        &acc.id,
        &BankImportInput {
            format: "demo".into(),
            opening: None,
            closing: None,
            transactions: vec![
                tx(3, 30, 121_000, "Stichting Voorbeeld", "Factuur 2026-0001"),
                tx(9, 3, -3_025, "Hosting & Co", "H-0901"),
                tx(10, 2, -3_025, "Hosting & Co", "H-1001"),
                tx(10, 6, 90_750, "Hogeschool Fictief", "Factuur 2026-0002"),
                tx(10, 16, -24_000, "Software Ltd", "SL-778"),
                tx(11, 3, -3_025, "Hosting & Co", "H-1101"),
                tx(11, 4, -1_250, "Bank", "Kosten zakelijke rekening"),
                tx(11, 12, 96_710, "Apple Distribution International", "APPLE PAYMENT 1234"),
                tx(11, 13, -8_500, "Koffiebar Centrum", "Lunch met klant"),
            ],
        },
        b"fictief bankafschrift",
        "afschrift_2026.xml",
    )?;
    // Process all but the last two using the top suggestion.
    let open = l.bank_transactions(&acc.id, "open", None)?;
    for t in open
        .iter()
        .filter(|t| !t.description.contains("Lunch") && !t.description.contains("Kosten"))
    {
        if let Some(s) = l.bank_suggestions(&t.id)?.into_iter().next() {
            l.process_bank_transaction(&t.id, &s.action)?;
        }
    }
    if let Some(t) = open.iter().find(|t| t.description.contains("Kosten")) {
        l.process_bank_transaction(
            &t.id,
            &BankAction::Account {
                account: "4700".into(),
                tax_code: None,
                description: None,
            },
        )?;
    }

    // Hours and an asset.
    for (m, day, minutes, activity) in [
        (9, 1, 300, "Ontwikkeling"),
        (9, 18, 240, "Lezingen"),
        (10, 3, 420, "Ontwikkeling"),
        (10, 27, 360, "Lezingen"),
        (11, 5, 120, "Administratie"),
        (11, 9, 480, "Ontwikkeling"),
    ] {
        l.save_time_entry(&TimeEntry {
            id: String::new(),
            entry_date: d(m, day),
            started_at: None,
            minutes: Some(minutes),
            activity: activity.into(),
            project: Some("Voorbeeld App".into()),
            description: None,
        })?;
    }
    l.add_asset(&AssetInput {
        name: "Laptop (fictief)".into(),
        acquisition_date: d(1, 15),
        cost_cents: 200_000,
        residual_cents: 0,
        months: 60,
        start_month: None,
        business_use_pct: 100,
        purchase_invoice_id: None,
        notes: None,
    })?;
    l.post(&EntryDraft {
        journal: JournalCode::INK,
        date: d(1, 15),
        description: "Laptop (fictief), privé voorgeschoten".into(),
        lines: vec![
            EntryLine::debit("0100", 200_000),
            EntryLine::debit("1500", 42_000).with_tax_code("I21"),
            EntryLine::credit("0510", 242_000),
        ],
        source: Source::manual(),
        idempotency_key: None,
    })?;
    let n = l.post_depreciation(d(10, 31))?;
    println!("Demo database created at {} ({n} depreciation entries)", path.display());
    Ok(())
}
