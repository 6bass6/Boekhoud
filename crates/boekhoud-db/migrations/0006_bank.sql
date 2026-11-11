-- 0006: bank accounts, imported statements and transactions.
-- A bank transaction is not a booking: it becomes one only when it is processed.

CREATE TABLE bank_account (
    id              TEXT PRIMARY KEY,
    iban            TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    ledger_account  TEXT NOT NULL REFERENCES account(number) DEFAULT '1100',
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE bank_import (
    id                  TEXT PRIMARY KEY,
    bank_account_id     TEXT NOT NULL REFERENCES bank_account(id),
    file_sha256         TEXT NOT NULL UNIQUE,
    original_filename   TEXT NOT NULL,
    format              TEXT NOT NULL,
    document_id         TEXT REFERENCES document(id),
    opening_date        TEXT,
    opening_cents       INTEGER,
    closing_date        TEXT,
    closing_cents       INTEGER,
    transactions_new    INTEGER NOT NULL,
    transactions_skipped INTEGER NOT NULL,
    imported_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE bank_transaction (
    id                  TEXT PRIMARY KEY,
    bank_account_id     TEXT NOT NULL REFERENCES bank_account(id),
    import_id           TEXT NOT NULL REFERENCES bank_import(id),
    booking_date        TEXT NOT NULL CHECK (date(booking_date) = booking_date),
    value_date          TEXT,
    amount_cents        INTEGER NOT NULL,
    currency            TEXT NOT NULL,
    counterparty_name   TEXT,
    counterparty_iban   TEXT,
    description         TEXT NOT NULL,
    end_to_end_id       TEXT,
    bank_reference      TEXT,
    dedupe_key          TEXT NOT NULL,
    -- open | ignored ; "processed" is derived from a non-reversed entry
    status              TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','ignored')),
    ignore_reason       TEXT,
    entry_id            TEXT REFERENCES journal_entry(id),
    -- How it was processed, so similar transactions can be proposed the same way next time.
    action_kind         TEXT,
    action_account      TEXT,
    action_tax_code     TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (bank_account_id, dedupe_key)
);

CREATE INDEX bank_transaction_status ON bank_transaction(bank_account_id, booking_date);

-- Imported facts are never changed; only processing fields may be set.
CREATE TRIGGER bank_transaction_facts_immutable
BEFORE UPDATE OF bank_account_id, import_id, booking_date, value_date, amount_cents, currency,
                 counterparty_name, counterparty_iban, description, end_to_end_id, bank_reference, dedupe_key
ON bank_transaction
BEGIN
    SELECT RAISE(ABORT, 'imported bank transactions are immutable');
END;

CREATE TRIGGER bank_transaction_no_delete
BEFORE DELETE ON bank_transaction
BEGIN
    SELECT RAISE(ABORT, 'bank transactions cannot be deleted');
END;
