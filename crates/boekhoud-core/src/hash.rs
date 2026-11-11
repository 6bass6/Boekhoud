//! Tamper-evident hash chain over posted journal entries.
//!
//! `hash = SHA-256(prev_hash || canonical_bytes(entry))`, hex-encoded.
//! The canonical form is a line-based text format that is stable across versions:
//! changing it requires a new `CHAIN_VERSION` and a migration that records the switch.

use sha2::{Digest, Sha256};

/// Hash used as `prev_hash` for the very first posted entry.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub const CHAIN_VERSION: u32 = 1;

/// The fields of a posted entry that are covered by the hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalEntry<'a> {
    pub entry_id: &'a str,
    pub journal: &'a str,
    pub fiscal_year: i32,
    pub sequence: i64,
    /// ISO date `YYYY-MM-DD`.
    pub date: &'a str,
    pub description: &'a str,
    pub source_kind: &'a str,
    pub source_reference: Option<&'a str>,
    pub reverses_entry_id: Option<&'a str>,
    /// `(line_no, account, amount_cents, currency, tax_code, description)` in line order.
    pub lines: Vec<CanonicalLine<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalLine<'a> {
    pub line_no: i64,
    pub account: &'a str,
    pub amount_cents: i64,
    pub currency: &'a str,
    pub tax_code: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Escape so that field and record separators can never be forged by user text.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn opt(s: Option<&str>) -> String {
    match s {
        Some(v) => format!("={}", esc(v)),
        None => "-".to_string(),
    }
}

impl CanonicalEntry<'_> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!(
            "v{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            CHAIN_VERSION,
            esc(self.entry_id),
            esc(self.journal),
            self.fiscal_year,
            self.sequence,
            esc(self.date),
            esc(self.description),
            esc(self.source_kind),
            opt(self.source_reference),
            opt(self.reverses_entry_id),
        );
        for l in &self.lines {
            out.push_str(&format!(
                "L|{}|{}|{}|{}|{}|{}\n",
                l.line_no,
                esc(l.account),
                l.amount_cents,
                esc(l.currency),
                opt(l.tax_code),
                opt(l.description),
            ));
        }
        out.into_bytes()
    }
}

pub fn chain_hash(prev_hash: &str, entry: &CanonicalEntry<'_>) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(b"\n");
    h.update(entry.to_bytes());
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample<'a>(desc: &'a str) -> CanonicalEntry<'a> {
        CanonicalEntry {
            entry_id: "e1",
            journal: "MEM",
            fiscal_year: 2026,
            sequence: 1,
            date: "2026-11-11",
            description: desc,
            source_kind: "manual",
            source_reference: None,
            reverses_entry_id: None,
            lines: vec![
                CanonicalLine {
                    line_no: 1,
                    account: "4900",
                    amount_cents: 100,
                    currency: "EUR",
                    tax_code: None,
                    description: None,
                },
                CanonicalLine {
                    line_no: 2,
                    account: "1600",
                    amount_cents: -100,
                    currency: "EUR",
                    tax_code: None,
                    description: None,
                },
            ],
        }
    }

    #[test]
    fn hash_is_deterministic_and_sensitive() {
        let a = chain_hash(GENESIS_HASH, &sample("x"));
        assert_eq!(a, chain_hash(GENESIS_HASH, &sample("x")));
        assert_ne!(a, chain_hash(GENESIS_HASH, &sample("y")));
        assert_ne!(a, chain_hash(&a, &sample("x")));
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn separators_in_text_cannot_collide() {
        // Without escaping, these two descriptions could produce identical byte streams.
        let a = sample("a|manual");
        let mut b = sample("a");
        b.source_kind = "manual|manual";
        assert_ne!(a.to_bytes(), b.to_bytes());
    }
}
