-- 0012: optional company logo for invoices. The image is an archived document (content-addressed,
-- immutable); issued invoices keep the logo id in their frozen seller snapshot.
ALTER TABLE company ADD COLUMN logo_document_id TEXT REFERENCES document(id);
