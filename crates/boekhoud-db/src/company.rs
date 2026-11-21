//! Company (onderneming) details, used on invoices and reports.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{audit, DbError, Ledger, Result};

fn default_term() -> i64 {
    30
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub legal_name: String,
    pub trade_name: Option<String>,
    pub kvk_number: Option<String>,
    pub vat_number: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: String,
    #[serde(default)]
    pub iban: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default = "default_term")]
    pub payment_term_days: i64,
    /// Logo printed on invoices (an archived JPEG document). Set with `set_company_logo`;
    /// `save_company` leaves it untouched.
    #[serde(default)]
    pub logo_document_id: Option<String>,
}

impl Default for Company {
    fn default() -> Self {
        Company {
            legal_name: String::new(),
            trade_name: None,
            kvk_number: None,
            vat_number: None,
            address: None,
            postal_code: None,
            city: None,
            country: "NL".into(),
            iban: None,
            email: None,
            payment_term_days: 30,
            logo_document_id: None,
        }
    }
}

pub(crate) fn company_row(conn: &Connection) -> Result<Option<Company>> {
    Ok(conn
        .query_row(
            "SELECT legal_name, trade_name, kvk_number, vat_number, address, postal_code, city, country,
                    iban, email, payment_term_days, logo_document_id
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
                    iban: r.get(8)?,
                    email: r.get(9)?,
                    payment_term_days: r.get(10)?,
                    logo_document_id: r.get(11)?,
                })
            },
        )
        .optional()?)
}

impl Ledger {
    pub fn company(&self) -> Result<Option<Company>> {
        company_row(&self.conn)
    }

    pub fn save_company(&mut self, c: &Company) -> Result<()> {
        if c.legal_name.trim().is_empty() {
            return Err(DbError::Validation(vec![
                boekhoud_core::ValidationError::MissingDescription,
            ]));
        }
        if !(0..=365).contains(&c.payment_term_days) {
            return Err(DbError::Invalid(
                "betaaltermijn moet tussen 0 en 365 dagen liggen".into(),
            ));
        }
        let old = self.company()?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO company (id, legal_name, trade_name, kvk_number, vat_number, address, postal_code, city, country,
                                  iban, email, payment_term_days)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET legal_name=excluded.legal_name, trade_name=excluded.trade_name,
                 kvk_number=excluded.kvk_number, vat_number=excluded.vat_number, address=excluded.address,
                 postal_code=excluded.postal_code, city=excluded.city, country=excluded.country,
                 iban=excluded.iban, email=excluded.email, payment_term_days=excluded.payment_term_days,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![
                c.legal_name, c.trade_name, c.kvk_number, c.vat_number, c.address, c.postal_code, c.city, c.country,
                c.iban.as_ref().map(|v| v.replace(' ', "").to_uppercase()), c.email, c.payment_term_days
            ],
        )?;
        // The logo is not part of this form; record what is actually stored.
        let new = Company {
            logo_document_id: old.as_ref().and_then(|o| o.logo_document_id.clone()),
            ..c.clone()
        };
        audit(&tx, &self.actor, "save", "company", "1", old.as_ref(), Some(&new), None)?;
        tx.commit()?;
        Ok(())
    }

    /// Set (or with `None` remove) the logo printed on new invoices. The document must exist;
    /// checking that it is a usable image is up to the caller.
    pub fn set_company_logo(&mut self, document_id: Option<&str>) -> Result<()> {
        let old = self
            .company()?
            .ok_or_else(|| DbError::Invalid("sla eerst de ondernemingsgegevens op".into()))?;
        if let Some(id) = document_id {
            self.document(id)?;
        }
        let new = Company {
            logo_document_id: document_id.map(str::to_string),
            ..old.clone()
        };
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE company SET logo_document_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = 1",
            params![document_id],
        )?;
        audit(
            &tx,
            &self.actor,
            "set_logo",
            "company",
            "1",
            Some(&old),
            Some(&new),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }
}
