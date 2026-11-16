-- 0010: correcting purchase invoices and editing details that are not booked.
--
-- 1. A correction reverses the original entry and registers a new invoice that points back to it.
-- 2. Description, supplier's invoice number and due date may change after registration (audited):
--    they are not part of the posted amounts. Everything that was booked stays immutable.
-- 3. A document may be linked again to the correcting invoice, so the link table loses its
--    UNIQUE(document_id); "one live invoice per document" is enforced in code (purchases.rs).

ALTER TABLE purchase_invoice ADD COLUMN corrects_invoice_id TEXT REFERENCES purchase_invoice(id);
CREATE UNIQUE INDEX purchase_invoice_corrects ON purchase_invoice(corrects_invoice_id)
    WHERE corrects_invoice_id IS NOT NULL;

DROP TRIGGER purchase_invoice_immutable;
CREATE TRIGGER purchase_invoice_immutable
BEFORE UPDATE ON purchase_invoice
WHEN NOT (NEW.id IS OLD.id AND NEW.contact_id IS OLD.contact_id AND NEW.invoice_date IS OLD.invoice_date
          AND NEW.subtotal_cents IS OLD.subtotal_cents AND NEW.vat_cents IS OLD.vat_cents
          AND NEW.total_cents IS OLD.total_cents AND NEW.paid_privately IS OLD.paid_privately
          AND NEW.entry_id IS OLD.entry_id AND NEW.created_at IS OLD.created_at
          AND NEW.corrects_invoice_id IS OLD.corrects_invoice_id
          AND (NEW.document_id IS OLD.document_id OR OLD.document_id IS NULL))
BEGIN
    SELECT RAISE(ABORT, 'purchase invoices are immutable once registered');
END;

DROP TRIGGER purchase_invoice_document_no_update;
DROP TRIGGER purchase_invoice_document_no_delete;
CREATE TABLE purchase_invoice_document_v2 (
    purchase_invoice_id TEXT NOT NULL REFERENCES purchase_invoice(id),
    document_id         TEXT NOT NULL REFERENCES document(id),
    position            INTEGER NOT NULL,
    added_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (purchase_invoice_id, document_id),
    UNIQUE (purchase_invoice_id, position)
);
INSERT INTO purchase_invoice_document_v2 (purchase_invoice_id, document_id, position, added_at)
SELECT purchase_invoice_id, document_id, position, added_at FROM purchase_invoice_document;
DROP TABLE purchase_invoice_document;
ALTER TABLE purchase_invoice_document_v2 RENAME TO purchase_invoice_document;
CREATE INDEX purchase_invoice_document_by_document ON purchase_invoice_document(document_id);

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
