-- 0005: app-store reports and how each platform is booked.

-- Booking rules per platform are data (docs/accounting-rules.md S1/S2).
CREATE TABLE platform_config (
    platform            TEXT PRIMARY KEY CHECK (platform IN ('apple','google')),
    name                TEXT NOT NULL,
    receivable_account  TEXT NOT NULL REFERENCES account(number),
    revenue_account     TEXT NOT NULL REFERENCES account(number),
    tax_code            TEXT NOT NULL,
    contact_id          TEXT REFERENCES contact(id)
);

INSERT INTO platform_config (platform, name, receivable_account, revenue_account, tax_code) VALUES
    ('apple',  'Apple App Store',   '1310', '8000', 'V0EU'),
    ('google', 'Google Play',       '1320', '8010', 'V0EU');

CREATE TABLE platform_report (
    id                  TEXT PRIMARY KEY,
    platform            TEXT NOT NULL REFERENCES platform_config(platform),
    period_start        TEXT NOT NULL CHECK (date(period_start) = period_start),
    period_end          TEXT NOT NULL CHECK (date(period_end) = period_end),
    file_sha256         TEXT NOT NULL UNIQUE,
    original_filename   TEXT NOT NULL,
    document_id         TEXT REFERENCES document(id),
    -- Parsed totals (per currency / per transaction type) as JSON, for display and audit.
    summary_json        TEXT NOT NULL,
    proceeds_eur_cents  INTEGER,
    entry_id            TEXT REFERENCES journal_entry(id),
    imported_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Per product and currency, for revenue-per-app reporting.
CREATE TABLE platform_report_product (
    id              INTEGER PRIMARY KEY,
    report_id       TEXT NOT NULL REFERENCES platform_report(id),
    product         TEXT NOT NULL,
    currency        TEXT NOT NULL,
    units           INTEGER NOT NULL,
    amount_cents    INTEGER NOT NULL
);

CREATE TRIGGER platform_report_booked_immutable
BEFORE UPDATE ON platform_report
WHEN OLD.entry_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'booked platform reports are immutable');
END;

CREATE TRIGGER platform_report_no_delete_booked
BEFORE DELETE ON platform_report
WHEN OLD.entry_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'booked platform reports cannot be deleted');
END;
