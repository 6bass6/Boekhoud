-- 0004: VAT posting accounts as data, contacts, documents, sales and purchase invoices, payments.

-- VAT accounts are marked so the VAT return can separate tax base from VAT.
ALTER TABLE account ADD COLUMN is_vat INTEGER NOT NULL DEFAULT 0 CHECK (is_vat IN (0,1));
UPDATE account SET is_vat = 1 WHERE number IN ('1500','1510','1511','1515');

-- Which ledger accounts a VAT code posts its VAT to.
ALTER TABLE tax_code ADD COLUMN payable_account TEXT REFERENCES account(number);
ALTER TABLE tax_code ADD COLUMN deductible_account TEXT REFERENCES account(number);
UPDATE tax_code SET payable_account = '1510' WHERE code = 'V21';
UPDATE tax_code SET payable_account = '1511' WHERE code = 'V9';
UPDATE tax_code SET deductible_account = '1500' WHERE code IN ('I21','I9');
UPDATE tax_code SET payable_account = '1515', deductible_account = '1500' WHERE code IN ('IEU','INEU');

ALTER TABLE company ADD COLUMN iban TEXT;
ALTER TABLE company ADD COLUMN email TEXT;
ALTER TABLE company ADD COLUMN payment_term_days INTEGER NOT NULL DEFAULT 30;

CREATE TABLE contact (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    contact_person   TEXT,
    email            TEXT,
    address          TEXT,
    postal_code      TEXT,
    city             TEXT,
    country          TEXT NOT NULL DEFAULT 'NL',
    vat_number       TEXT,
    kvk_number       TEXT,
    is_customer      INTEGER NOT NULL DEFAULT 0 CHECK (is_customer IN (0,1)),
    is_supplier      INTEGER NOT NULL DEFAULT 0 CHECK (is_supplier IN (0,1)),
    is_platform      INTEGER NOT NULL DEFAULT 0 CHECK (is_platform IN (0,1)),
    default_account  TEXT REFERENCES account(number),
    default_tax_code TEXT,
    archived         INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Original files (receipts, invoices, reports). The bytes live in a content-addressed store on disk.
CREATE TABLE document (
    id                 TEXT PRIMARY KEY,
    sha256             TEXT NOT NULL UNIQUE,
    original_filename  TEXT NOT NULL,
    mime_type          TEXT NOT NULL,
    size_bytes         INTEGER NOT NULL,
    document_type      TEXT NOT NULL,
    uploaded_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    retention_until    TEXT NOT NULL
);

CREATE TRIGGER document_no_update
BEFORE UPDATE OF sha256, original_filename, size_bytes, uploaded_at ON document
BEGIN
    SELECT RAISE(ABORT, 'documents are immutable');
END;

CREATE TRIGGER document_retention
BEFORE DELETE ON document
WHEN OLD.retention_until >= date('now')
BEGIN
    SELECT RAISE(ABORT, 'document is within its retention period');
END;

-- Sales invoices and credit notes.
CREATE TABLE sales_invoice (
    id                 TEXT PRIMARY KEY,
    contact_id         TEXT NOT NULL REFERENCES contact(id),
    status             TEXT NOT NULL CHECK (status IN ('draft','issued')),
    invoice_year       INTEGER,
    invoice_sequence   INTEGER,
    invoice_number     TEXT UNIQUE,
    invoice_date       TEXT NOT NULL CHECK (date(invoice_date) = invoice_date),
    supply_date        TEXT CHECK (supply_date IS NULL OR date(supply_date) = supply_date),
    due_date           TEXT NOT NULL CHECK (date(due_date) = due_date),
    credits_invoice_id TEXT REFERENCES sales_invoice(id),
    reference          TEXT,
    notes              TEXT,
    -- Snapshots taken at issue so the invoice stays the same if contact or company details change.
    seller_snapshot    TEXT,
    buyer_snapshot     TEXT,
    subtotal_cents     INTEGER,
    vat_cents          INTEGER,
    total_cents        INTEGER,
    entry_id           TEXT REFERENCES journal_entry(id),
    document_id        TEXT REFERENCES document(id),
    issued_at          TEXT,
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (invoice_year, invoice_sequence),
    CHECK (status = 'draft' OR (invoice_number IS NOT NULL AND entry_id IS NOT NULL AND total_cents IS NOT NULL))
);

CREATE TABLE sales_invoice_line (
    id               INTEGER PRIMARY KEY,
    invoice_id       TEXT NOT NULL REFERENCES sales_invoice(id) ON DELETE CASCADE,
    line_no          INTEGER NOT NULL,
    description      TEXT NOT NULL,
    -- Quantity in thousandths (1.5 hours = 1500).
    quantity_milli   INTEGER NOT NULL CHECK (quantity_milli <> 0),
    unit_price_cents INTEGER NOT NULL,
    tax_code         TEXT NOT NULL,
    account          TEXT NOT NULL REFERENCES account(number),
    UNIQUE (invoice_id, line_no)
);

-- Issued invoices are immutable (except linking the archived PDF once).
CREATE TRIGGER sales_invoice_issued_immutable
BEFORE UPDATE ON sales_invoice
WHEN OLD.status = 'issued'
     AND NOT (NEW.document_id IS NOT OLD.document_id AND OLD.document_id IS NULL
              AND NEW.status = OLD.status AND NEW.invoice_number IS OLD.invoice_number
              AND NEW.total_cents IS OLD.total_cents AND NEW.entry_id IS OLD.entry_id
              AND NEW.buyer_snapshot IS OLD.buyer_snapshot AND NEW.seller_snapshot IS OLD.seller_snapshot
              AND NEW.invoice_date IS OLD.invoice_date AND NEW.contact_id IS OLD.contact_id)
BEGIN
    SELECT RAISE(ABORT, 'issued invoices are immutable');
END;

CREATE TRIGGER sales_invoice_issued_no_delete
BEFORE DELETE ON sales_invoice
WHEN OLD.status = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'issued invoices cannot be deleted');
END;

CREATE TRIGGER sales_invoice_line_locked_insert
BEFORE INSERT ON sales_invoice_line
WHEN (SELECT status FROM sales_invoice WHERE id = NEW.invoice_id) = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'issued invoices are immutable');
END;

CREATE TRIGGER sales_invoice_line_locked_update
BEFORE UPDATE ON sales_invoice_line
WHEN (SELECT status FROM sales_invoice WHERE id = OLD.invoice_id) = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'issued invoices are immutable');
END;

CREATE TRIGGER sales_invoice_line_locked_delete
BEFORE DELETE ON sales_invoice_line
WHEN (SELECT status FROM sales_invoice WHERE id = OLD.invoice_id) = 'issued'
BEGIN
    SELECT RAISE(ABORT, 'issued invoices are immutable');
END;

-- Purchase invoices (received). Posted on registration.
CREATE TABLE purchase_invoice (
    id                 TEXT PRIMARY KEY,
    contact_id         TEXT NOT NULL REFERENCES contact(id),
    supplier_reference TEXT,
    invoice_date       TEXT NOT NULL CHECK (date(invoice_date) = invoice_date),
    due_date           TEXT CHECK (due_date IS NULL OR date(due_date) = due_date),
    description        TEXT NOT NULL,
    subtotal_cents     INTEGER NOT NULL,
    vat_cents          INTEGER NOT NULL,
    total_cents        INTEGER NOT NULL,
    -- Paid directly from a private account: credit Privé-stortingen instead of Crediteuren.
    paid_privately     INTEGER NOT NULL DEFAULT 0 CHECK (paid_privately IN (0,1)),
    entry_id           TEXT NOT NULL REFERENCES journal_entry(id),
    document_id        TEXT REFERENCES document(id),
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TRIGGER purchase_invoice_immutable
BEFORE UPDATE ON purchase_invoice
WHEN NOT (OLD.document_id IS NULL AND NEW.document_id IS NOT NULL
          AND NEW.entry_id IS OLD.entry_id AND NEW.total_cents IS OLD.total_cents
          AND NEW.contact_id IS OLD.contact_id AND NEW.invoice_date IS OLD.invoice_date)
BEGIN
    SELECT RAISE(ABORT, 'purchase invoices are immutable once registered');
END;

CREATE TRIGGER purchase_invoice_no_delete
BEFORE DELETE ON purchase_invoice
BEGIN
    SELECT RAISE(ABORT, 'purchase invoices cannot be deleted; reverse the entry instead');
END;

CREATE TABLE purchase_invoice_line (
    id               INTEGER PRIMARY KEY,
    invoice_id       TEXT NOT NULL REFERENCES purchase_invoice(id),
    line_no          INTEGER NOT NULL,
    description      TEXT,
    account          TEXT NOT NULL REFERENCES account(number),
    net_cents        INTEGER NOT NULL,
    tax_code         TEXT NOT NULL,
    vat_cents        INTEGER NOT NULL,
    UNIQUE (invoice_id, line_no)
);

-- Settlements of invoices (sales: received; purchase: paid). Each has its own journal entry.
CREATE TABLE payment (
    id                  TEXT PRIMARY KEY,
    sales_invoice_id    TEXT REFERENCES sales_invoice(id),
    purchase_invoice_id TEXT REFERENCES purchase_invoice(id),
    payment_date        TEXT NOT NULL CHECK (date(payment_date) = payment_date),
    amount_cents        INTEGER NOT NULL CHECK (amount_cents <> 0),
    method              TEXT NOT NULL,
    bank_transaction_id TEXT,
    entry_id            TEXT NOT NULL REFERENCES journal_entry(id),
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK ((sales_invoice_id IS NULL) <> (purchase_invoice_id IS NULL))
);

CREATE TRIGGER payment_immutable BEFORE UPDATE ON payment
BEGIN SELECT RAISE(ABORT, 'payments are immutable'); END;
CREATE TRIGGER payment_no_delete BEFORE DELETE ON payment
BEGIN SELECT RAISE(ABORT, 'payments cannot be deleted'); END;
