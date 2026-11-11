//! Company (onderneming) details, used on invoices and reports.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{DbError, Ledger, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub legal_name: String,
    pub trade_name: Option<String>,
    pub kvk_number: Option<String>,
    pub vat_number: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: String,
}

impl Ledger {
    pub fn company(&self) -> Result<Option<Company>> {
        Ok(self
            .conn
            .query_row(
                "SELECT legal_name, trade_name, kvk_number, vat_number, address, postal_code, city, country
                 FROM company WHERE id = 1",
                [],
                |r| {
                    Ok(Company {
                        legal_name: r.get(0)?,
                        trade_name: r.get(1)?,
                        kvk_number: r.get(2)?,
                        vat_number: r.get(3)?,
                        address: r.get(4)?,
                        postal_code: r.get(5)?,
                        city: r.get(6)?,
                        country: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_company(&mut self, c: &Company) -> Result<()> {
        if c.legal_name.trim().is_empty() {
            return Err(DbError::Validation(vec![
                boekhoud_core::ValidationError::MissingDescription,
            ]));
        }
        let old = self.company()?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO company (id, legal_name, trade_name, kvk_number, vat_number, address, postal_code, city, country)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET legal_name=excluded.legal_name, trade_name=excluded.trade_name,
                 kvk_number=excluded.kvk_number, vat_number=excluded.vat_number, address=excluded.address,
                 postal_code=excluded.postal_code, city=excluded.city, country=excluded.country,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![c.legal_name, c.trade_name, c.kvk_number, c.vat_number, c.address, c.postal_code, c.city, c.country],
        )?;
        tx.execute(
            "INSERT INTO audit_log (actor, action, entity_type, entity_id, old_value, new_value)
             VALUES (?1, 'save', 'company', '1', ?2, ?3)",
            params![
                self.actor,
                old.as_ref().and_then(|o| serde_json::to_string(o).ok()),
                serde_json::to_string(c).ok()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}
