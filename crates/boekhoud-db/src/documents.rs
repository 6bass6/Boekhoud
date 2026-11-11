//! Content-addressed store for original documents (receipts, invoices, reports).
//!
//! Files live at `<root>/<aa>/<bb>/<sha256>` and are never modified. The `document` table holds
//! metadata and the retention date (7 years); the database refuses to delete documents earlier.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Utc};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{DbError, Ledger, Result};

pub const RETENTION_YEARS: i32 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentInfo {
    pub id: String,
    pub sha256: String,
    pub original_filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub document_type: String,
    pub uploaded_at: String,
    pub retention_until: String,
}

fn blob_path(root: &Path, sha: &str) -> PathBuf {
    root.join(&sha[0..2]).join(&sha[2..4]).join(sha)
}

/// Guess a MIME type from the file name; good enough for display purposes.
pub fn mime_from_filename(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit('.').next().unwrap_or("") {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "txt" | "tsv" => "text/plain",
        "zip" => "application/zip",
        "eml" => "message/rfc822",
        _ => "application/octet-stream",
    }
}

impl Ledger {
    fn document_root(&self) -> Result<&Path> {
        self.document_root
            .as_deref()
            .ok_or_else(|| DbError::Invalid("geen documentmap ingesteld".into()))
    }

    /// Store a file. Identical content is stored once; the existing record is returned.
    pub fn add_document(&mut self, bytes: &[u8], filename: &str, document_type: &str) -> Result<DocumentInfo> {
        if bytes.is_empty() {
            return Err(DbError::Invalid("leeg bestand".into()));
        }
        let sha = hex::encode(Sha256::digest(bytes));
        if let Some(existing) = self.document_by_sha(&sha)? {
            return Ok(existing);
        }
        let root = self.document_root()?.to_path_buf();
        let path = blob_path(&root, &sha);
        if !path.exists() {
            fs::create_dir_all(path.parent().expect("blob path has parent")).map_err(io)?;
            let tmp = path.with_extension("tmp");
            let mut f = fs::File::create(&tmp).map_err(io)?;
            f.write_all(bytes).map_err(io)?;
            f.sync_all().map_err(io)?;
            fs::rename(&tmp, &path).map_err(io)?;
        }
        let id = uuid::Uuid::now_v7().to_string();
        let retention = format!("{}-12-31", Utc::now().year() + RETENTION_YEARS);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO document (id, sha256, original_filename, mime_type, size_bytes, document_type, retention_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, sha, filename, mime_from_filename(filename), bytes.len() as i64, document_type, retention],
        )?;
        crate::audit(
            &tx,
            &self.actor,
            "add",
            "document",
            &id,
            None::<&()>,
            Some(&(filename, &sha)),
            None,
        )?;
        tx.commit()?;
        self.document(&id)
    }

    fn document_by_sha(&self, sha: &str) -> Result<Option<DocumentInfo>> {
        let id: Option<String> = self
            .conn
            .query_row("SELECT id FROM document WHERE sha256 = ?1", [sha], |r| r.get(0))
            .optional()?;
        id.map(|id| self.document(&id)).transpose()
    }

    pub fn document(&self, id: &str) -> Result<DocumentInfo> {
        self.conn
            .query_row(
                "SELECT id, sha256, original_filename, mime_type, size_bytes, document_type, uploaded_at, retention_until
                 FROM document WHERE id = ?1",
                [id],
                |r| {
                    Ok(DocumentInfo {
                        id: r.get(0)?,
                        sha256: r.get(1)?,
                        original_filename: r.get(2)?,
                        mime_type: r.get(3)?,
                        size_bytes: r.get(4)?,
                        document_type: r.get(5)?,
                        uploaded_at: r.get(6)?,
                        retention_until: r.get(7)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| DbError::NotFound(format!("document {id}")))
    }

    /// Read a document's bytes and verify them against the stored checksum.
    pub fn document_bytes(&self, id: &str) -> Result<(DocumentInfo, Vec<u8>)> {
        let info = self.document(id)?;
        let bytes = fs::read(blob_path(self.document_root()?, &info.sha256)).map_err(io)?;
        if hex::encode(Sha256::digest(&bytes)) != info.sha256 {
            return Err(DbError::Corrupt(format!(
                "document {} does not match its checksum",
                info.original_filename
            )));
        }
        Ok((info, bytes))
    }

    pub fn document_path(&self, id: &str) -> Result<PathBuf> {
        let info = self.document(id)?;
        Ok(blob_path(self.document_root()?, &info.sha256))
    }
}

fn io(e: std::io::Error) -> DbError {
    DbError::Io(e.to_string())
}
