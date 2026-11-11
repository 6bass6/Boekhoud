//! Customers, suppliers and platforms.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::{DbError, Ledger, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Empty for a new contact.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: String,
    pub vat_number: Option<String>,
    pub kvk_number: Option<String>,
    pub is_customer: bool,
    pub is_supplier: bool,
    pub is_platform: bool,
    pub default_account: Option<String>,
    pub default_tax_code: Option<String>,
    pub archived: bool,
}

const COLS: &str = "id, name, contact_person, email, address, postal_code, city, country, vat_number, kvk_number,
                    is_customer, is_supplier, is_platform, default_account, default_tax_code, archived";

fn from_row(r: &Row) -> rusqlite::Result<Contact> {
    Ok(Contact {
        id: r.get(0)?,
        name: r.get(1)?,
        contact_person: r.get(2)?,
        email: r.get(3)?,
        address: r.get(4)?,
        postal_code: r.get(5)?,
        city: r.get(6)?,
        country: r.get(7)?,
        vat_number: r.get(8)?,
        kvk_number: r.get(9)?,
        is_customer: r.get(10)?,
        is_supplier: r.get(11)?,
        is_platform: r.get(12)?,
        default_account: r.get(13)?,
        default_tax_code: r.get(14)?,
        archived: r.get(15)?,
    })
}

pub(crate) fn contact_by_id(conn: &rusqlite::Connection, id: &str) -> Result<Contact> {
    conn.query_row(&format!("SELECT {COLS} FROM contact WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| DbError::NotFound(format!("contact {id}")))
}

impl Ledger {
    pub fn contacts(&self, include_archived: bool) -> Result<Vec<Contact>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM contact WHERE (?1 OR archived = 0) ORDER BY name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([include_archived], from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn contact(&self, id: &str) -> Result<Contact> {
        contact_by_id(&self.conn, id)
    }

    /// Create (empty id) or update a contact. Returns the id.
    pub fn save_contact(&mut self, c: &Contact) -> Result<String> {
        if c.name.trim().is_empty() {
            return Err(DbError::Invalid("naam ontbreekt".into()));
        }
        let tx = self.conn.transaction()?;
        let (id, old) = if c.id.is_empty() {
            (uuid::Uuid::now_v7().to_string(), None)
        } else {
            (c.id.clone(), Some(contact_by_id(&tx, &c.id)?))
        };
        tx.execute(
            "INSERT INTO contact (id, name, contact_person, email, address, postal_code, city, country, vat_number,
                 kvk_number, is_customer, is_supplier, is_platform, default_account, default_tax_code, archived)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, contact_person=excluded.contact_person,
                 email=excluded.email, address=excluded.address, postal_code=excluded.postal_code, city=excluded.city,
                 country=excluded.country, vat_number=excluded.vat_number, kvk_number=excluded.kvk_number,
                 is_customer=excluded.is_customer, is_supplier=excluded.is_supplier, is_platform=excluded.is_platform,
                 default_account=excluded.default_account, default_tax_code=excluded.default_tax_code,
                 archived=excluded.archived, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![
                id,
                c.name.trim(),
                c.contact_person,
                c.email,
                c.address,
                c.postal_code,
                c.city,
                if c.country.trim().is_empty() {
                    "NL".to_string()
                } else {
                    c.country.trim().to_uppercase()
                },
                c.vat_number.as_ref().map(|v| v.replace(' ', "").to_uppercase()),
                c.kvk_number,
                c.is_customer,
                c.is_supplier,
                c.is_platform,
                c.default_account,
                c.default_tax_code,
                c.archived
            ],
        )?;
        let new = contact_by_id(&tx, &id)?;
        crate::audit(&tx, &self.actor, "save", "contact", &id, old.as_ref(), Some(&new), None)?;
        tx.commit()?;
        Ok(id)
    }
}
