-- 0007: filed VAT returns, time tracking and fixed assets.

-- A VAT return as filed: the figures are frozen, the VAT accounts are cleared to 1520 and the
-- months of the period are closed.
CREATE TABLE vat_return (
    id              TEXT PRIMARY KEY,
    period_start    TEXT NOT NULL CHECK (date(period_start) = period_start),
    period_end      TEXT NOT NULL CHECK (date(period_end) = period_end),
    figures_json    TEXT NOT NULL,
    payable_cents   INTEGER NOT NULL,
    entry_id        TEXT REFERENCES journal_entry(id),
    filed_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (period_start, period_end)
);

CREATE TRIGGER vat_return_immutable BEFORE UPDATE ON vat_return
BEGIN SELECT RAISE(ABORT, 'filed VAT returns are immutable'); END;
CREATE TRIGGER vat_return_no_delete BEFORE DELETE ON vat_return
BEGIN SELECT RAISE(ABORT, 'filed VAT returns cannot be deleted'); END;

-- Hours (urencriterium). A running timer has started_at set and minutes NULL.
CREATE TABLE time_entry (
    id              TEXT PRIMARY KEY,
    entry_date      TEXT NOT NULL CHECK (date(entry_date) = entry_date),
    started_at      TEXT,
    minutes         INTEGER CHECK (minutes IS NULL OR minutes > 0),
    activity        TEXT NOT NULL,
    project         TEXT,
    description     TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK (minutes IS NOT NULL OR started_at IS NOT NULL)
);
CREATE INDEX time_entry_date ON time_entry(entry_date);

-- Fixed assets with straight-line depreciation.
CREATE TABLE asset (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    asset_account         TEXT NOT NULL REFERENCES account(number) DEFAULT '0100',
    depreciation_account  TEXT NOT NULL REFERENCES account(number) DEFAULT '0110',
    expense_account       TEXT NOT NULL REFERENCES account(number) DEFAULT '4800',
    acquisition_date      TEXT NOT NULL CHECK (date(acquisition_date) = acquisition_date),
    cost_cents            INTEGER NOT NULL CHECK (cost_cents > 0),
    residual_cents        INTEGER NOT NULL DEFAULT 0 CHECK (residual_cents >= 0),
    months                INTEGER NOT NULL CHECK (months > 0),
    -- First month of depreciation (YYYY-MM-01).
    start_month           TEXT NOT NULL CHECK (date(start_month) = start_month),
    business_use_pct      INTEGER NOT NULL DEFAULT 100 CHECK (business_use_pct BETWEEN 0 AND 100),
    purchase_invoice_id   TEXT REFERENCES purchase_invoice(id),
    disposed_on           TEXT,
    notes                 TEXT,
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK (residual_cents < cost_cents)
);
