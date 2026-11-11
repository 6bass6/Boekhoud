-- 0001: accounting core schema.
-- Never edit this file after it has been released; add a new migration instead.

CREATE TABLE company (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    legal_name       TEXT NOT NULL,
    trade_name       TEXT,
    kvk_number       TEXT,
    vat_number       TEXT,
    address          TEXT,
    postal_code      TEXT,
    city             TEXT,
    country          TEXT NOT NULL DEFAULT 'NL',
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE account (
    number           TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    account_type     TEXT NOT NULL CHECK (account_type IN ('asset','liability','equity','revenue','expense')),
    parent_number    TEXT REFERENCES account(number),
    rgs_code         TEXT,
    active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE journal (
    code             TEXT PRIMARY KEY,
    name             TEXT NOT NULL
);

CREATE TABLE period (
    year             INTEGER NOT NULL,
    month            INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
    status           TEXT NOT NULL CHECK (status IN ('open','closed')),
    changed_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (year, month)
);

CREATE TABLE journal_entry (
    id                 TEXT PRIMARY KEY,
    journal            TEXT NOT NULL REFERENCES journal(code),
    fiscal_year        INTEGER NOT NULL,
    sequence           INTEGER,
    entry_date         TEXT NOT NULL CHECK (date(entry_date) = entry_date),
    description        TEXT NOT NULL,
    source_kind        TEXT NOT NULL,
    source_reference   TEXT,
    idempotency_key    TEXT UNIQUE,
    status             TEXT NOT NULL CHECK (status IN ('draft','posted')),
    reverses_entry_id  TEXT REFERENCES journal_entry(id),
    chain_index        INTEGER UNIQUE,
    prev_hash          TEXT,
    hash               TEXT,
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    posted_at          TEXT,
    UNIQUE (journal, fiscal_year, sequence),
    CHECK (status = 'draft' OR (sequence IS NOT NULL AND chain_index IS NOT NULL
                                AND prev_hash IS NOT NULL AND hash IS NOT NULL AND posted_at IS NOT NULL))
);

CREATE TABLE journal_line (
    id               INTEGER PRIMARY KEY,
    entry_id         TEXT NOT NULL REFERENCES journal_entry(id),
    line_no          INTEGER NOT NULL,
    account          TEXT NOT NULL REFERENCES account(number),
    amount_cents     INTEGER NOT NULL CHECK (amount_cents <> 0),
    currency         TEXT NOT NULL DEFAULT 'EUR' CHECK (currency = 'EUR'),
    tax_code         TEXT,
    description      TEXT,
    UNIQUE (entry_id, line_no)
);

CREATE INDEX journal_line_account ON journal_line(account);
CREATE INDEX journal_entry_date ON journal_entry(entry_date);

CREATE TABLE audit_log (
    id               INTEGER PRIMARY KEY,
    at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    actor            TEXT NOT NULL,
    action           TEXT NOT NULL,
    entity_type      TEXT NOT NULL,
    entity_id        TEXT NOT NULL,
    old_value        TEXT,
    new_value        TEXT,
    entry_id         TEXT REFERENCES journal_entry(id)
);

-- ---------------------------------------------------------------------------
-- Integrity triggers: the database refuses changes to posted data even if
-- application code has a bug.
-- ---------------------------------------------------------------------------

-- Posted entries can never be changed or deleted.
CREATE TRIGGER journal_entry_posted_no_update
BEFORE UPDATE ON journal_entry
WHEN OLD.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'posted journal entries are immutable');
END;

CREATE TRIGGER journal_entry_posted_no_delete
BEFORE DELETE ON journal_entry
WHEN OLD.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'posted journal entries cannot be deleted');
END;

-- An entry can only be inserted as draft; posting happens via UPDATE so the checks below run.
CREATE TRIGGER journal_entry_insert_as_draft
BEFORE INSERT ON journal_entry
WHEN NEW.status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'journal entries must be inserted as draft');
END;

-- Posting requires >= 2 lines that sum to zero, and an open period.
CREATE TRIGGER journal_entry_post_checks
BEFORE UPDATE OF status ON journal_entry
WHEN NEW.status = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'entry needs at least two lines')
    WHERE (SELECT COUNT(*) FROM journal_line WHERE entry_id = NEW.id) < 2;
    SELECT RAISE(ABORT, 'entry does not balance')
    WHERE (SELECT SUM(amount_cents) FROM journal_line WHERE entry_id = NEW.id) <> 0;
    SELECT RAISE(ABORT, 'period is closed')
    WHERE EXISTS (SELECT 1 FROM period
                  WHERE year = CAST(strftime('%Y', NEW.entry_date) AS INTEGER)
                    AND month = CAST(strftime('%m', NEW.entry_date) AS INTEGER)
                    AND status = 'closed');
END;

-- Lines of a posted entry can never be inserted, changed or deleted.
CREATE TRIGGER journal_line_no_insert_when_posted
BEFORE INSERT ON journal_line
WHEN (SELECT status FROM journal_entry WHERE id = NEW.entry_id) = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'cannot add lines to a posted entry');
END;

CREATE TRIGGER journal_line_no_update_when_posted
BEFORE UPDATE ON journal_line
WHEN (SELECT status FROM journal_entry WHERE id = OLD.entry_id) = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'lines of posted entries are immutable');
END;

CREATE TRIGGER journal_line_no_delete_when_posted
BEFORE DELETE ON journal_line
WHEN (SELECT status FROM journal_entry WHERE id = OLD.entry_id) = 'posted'
BEGIN
    SELECT RAISE(ABORT, 'lines of posted entries cannot be deleted');
END;

-- The audit log is append-only.
CREATE TRIGGER audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit log is append-only');
END;

CREATE TRIGGER audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit log is append-only');
END;
