-- 0009: a purchase invoice can have several documents (e.g. invoice + receipt + delivery note).
-- Links are append-only: a document stays attached once linked, like the documents themselves.
-- purchase_invoice.document_id is kept for old databases but no longer written; this table is leading.

CREATE TABLE purchase_invoice_document (
    purchase_invoice_id TEXT NOT NULL REFERENCES purchase_invoice(id),
    -- A document belongs to at most one purchase invoice.
    document_id         TEXT NOT NULL UNIQUE REFERENCES document(id),
    position            INTEGER NOT NULL,
    added_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (purchase_invoice_id, document_id),
    UNIQUE (purchase_invoice_id, position)
);

INSERT INTO purchase_invoice_document (purchase_invoice_id, document_id, position, added_at)
SELECT id, document_id, 1, created_at FROM purchase_invoice WHERE document_id IS NOT NULL;

CREATE TRIGGER purchase_invoice_document_no_update
BEFORE UPDATE ON purchase_invoice_document
BEGIN
    SELECT RAISE(ABORT, 'purchase invoice documents cannot be changed');
END;

CREATE TRIGGER purchase_invoice_document_no_delete
BEFORE DELETE ON purchase_invoice_document
BEGIN
    SELECT RAISE(ABORT, 'purchase invoice documents cannot be removed');
END;
