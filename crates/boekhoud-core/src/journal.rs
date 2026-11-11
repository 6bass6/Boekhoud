//! Journal entries and the rules every posted entry must satisfy.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::money::{Currency, Money};

/// Journal (dagboek) codes. Numbering is gapless per journal per fiscal year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JournalCode {
    /// Bank
    BNK,
    /// Verkoop (sales)
    VRK,
    /// Inkoop (purchases)
    INK,
    /// Memoriaal (general)
    MEM,
    /// Platformen (app stores, ad networks)
    PLT,
}

impl JournalCode {
    pub const ALL: [JournalCode; 5] = [
        JournalCode::BNK,
        JournalCode::VRK,
        JournalCode::INK,
        JournalCode::MEM,
        JournalCode::PLT,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            JournalCode::BNK => "BNK",
            JournalCode::VRK => "VRK",
            JournalCode::INK => "INK",
            JournalCode::MEM => "MEM",
            JournalCode::PLT => "PLT",
        }
    }

    pub fn parse(s: &str) -> Option<JournalCode> {
        JournalCode::ALL.into_iter().find(|j| j.as_str() == s)
    }
}

/// One line of an entry. Positive amount = debit, negative amount = credit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryLine {
    pub account: String,
    pub amount: Money,
    pub tax_code: Option<String>,
    pub description: Option<String>,
}

impl EntryLine {
    pub fn debit(account: &str, cents: i64) -> Self {
        EntryLine {
            account: account.into(),
            amount: Money::eur(cents),
            tax_code: None,
            description: None,
        }
    }

    pub fn credit(account: &str, cents: i64) -> Self {
        EntryLine {
            account: account.into(),
            amount: Money::eur(-cents),
            tax_code: None,
            description: None,
        }
    }

    pub fn with_tax_code(mut self, code: &str) -> Self {
        self.tax_code = Some(code.into());
        self
    }
}

/// Where an entry came from, for traceability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// e.g. `manual`, `sales_invoice`, `apple_report`, `bank_transaction`.
    pub kind: String,
    /// Identifier within that source, e.g. invoice id or report file checksum.
    pub reference: Option<String>,
}

impl Source {
    pub fn manual() -> Self {
        Source {
            kind: "manual".into(),
            reference: None,
        }
    }
}

/// An entry that has not been posted yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryDraft {
    pub journal: JournalCode,
    pub date: NaiveDate,
    pub description: String,
    pub lines: Vec<EntryLine>,
    pub source: Source,
    /// Unique key per external event (e.g. `apple:<vendor>:<period>:<currency>`).
    /// Posting the same key twice is refused.
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("entry needs at least two lines")]
    TooFewLines,
    #[error("line {0} has a zero amount")]
    ZeroLine(usize),
    #[error("line {0} is in {1}; the books are kept in EUR")]
    NotEur(usize, Currency),
    #[error("line {0} has no account")]
    MissingAccount(usize),
    #[error("entry does not balance: difference {0} cents")]
    Unbalanced(i64),
    #[error("entry has no description")]
    MissingDescription,
    #[error("amount overflow")]
    Overflow,
    #[error("unknown account: {0}")]
    UnknownAccount(String),
    #[error("inactive account: {0}")]
    InactiveAccount(String),
    #[error("unknown or expired VAT code on the entry date: {0}")]
    UnknownTaxCode(String),
    #[error("VAT code {code} cannot be used under the {regime} VAT regime")]
    TaxCodeNotAllowed { code: String, regime: String },
}

impl EntryDraft {
    /// Structural validation that does not need the database.
    /// Returns every problem found, not just the first.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if self.description.trim().is_empty() {
            errors.push(ValidationError::MissingDescription);
        }
        if self.lines.len() < 2 {
            errors.push(ValidationError::TooFewLines);
        }
        let mut sum: i64 = 0;
        let mut overflow = false;
        for (i, line) in self.lines.iter().enumerate() {
            let n = i + 1;
            if line.account.trim().is_empty() {
                errors.push(ValidationError::MissingAccount(n));
            }
            if line.amount.is_zero() {
                errors.push(ValidationError::ZeroLine(n));
            }
            if line.amount.currency != Currency::EUR {
                errors.push(ValidationError::NotEur(n, line.amount.currency));
            }
            match sum.checked_add(line.amount.cents) {
                Some(s) => sum = s,
                None => overflow = true,
            }
        }
        if overflow {
            errors.push(ValidationError::Overflow);
        } else if sum != 0 {
            errors.push(ValidationError::Unbalanced(sum));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Accounts referenced by this entry.
    pub fn accounts(&self) -> BTreeSet<&str> {
        self.lines.iter().map(|l| l.account.as_str()).collect()
    }

    /// Total of the debit side, in cents.
    pub fn total_debit(&self) -> i64 {
        self.lines
            .iter()
            .map(|l| l.amount.cents)
            .filter(|c| *c > 0)
            .sum()
    }
}

/// Build the draft that fully reverses a posted entry (same lines, opposite sign).
pub fn reversal_of(
    original_lines: &[EntryLine],
    journal: JournalCode,
    date: NaiveDate,
    description: String,
    original_reference: &str,
) -> EntryDraft {
    EntryDraft {
        journal,
        date,
        description,
        lines: original_lines
            .iter()
            .map(|l| EntryLine {
                amount: -l.amount,
                ..l.clone()
            })
            .collect(),
        source: Source {
            kind: "reversal".into(),
            reference: Some(original_reference.into()),
        },
        idempotency_key: Some(format!("reversal:{original_reference}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    fn hosting_invoice() -> EntryDraft {
        // Scenario S6 from docs/accounting-rules.md
        EntryDraft {
            journal: JournalCode::INK,
            date: date(),
            description: "Hostingfactuur".into(),
            lines: vec![
                EntryLine::debit("4000", 10_000).with_tax_code("I21"),
                EntryLine::debit("1500", 2_100),
                EntryLine::credit("1600", 12_100),
            ],
            source: Source::manual(),
            idempotency_key: None,
        }
    }

    #[test]
    fn balanced_entry_is_valid() {
        assert_eq!(hosting_invoice().validate(), Ok(()));
        assert_eq!(hosting_invoice().total_debit(), 12_100);
    }

    #[test]
    fn unbalanced_entry_is_rejected() {
        let mut e = hosting_invoice();
        e.lines[2] = EntryLine::credit("1600", 12_000);
        assert_eq!(e.validate(), Err(vec![ValidationError::Unbalanced(100)]));
    }

    #[test]
    fn reports_all_problems() {
        let e = EntryDraft {
            journal: JournalCode::MEM,
            date: date(),
            description: " ".into(),
            lines: vec![EntryLine::debit("", 0)],
            source: Source::manual(),
            idempotency_key: None,
        };
        let errs = e.validate().unwrap_err();
        assert!(errs.contains(&ValidationError::MissingDescription));
        assert!(errs.contains(&ValidationError::TooFewLines));
        assert!(errs.contains(&ValidationError::MissingAccount(1)));
        assert!(errs.contains(&ValidationError::ZeroLine(1)));
    }

    #[test]
    fn foreign_currency_line_is_rejected() {
        let mut e = hosting_invoice();
        e.lines[0].amount = Money::new(10_000, Currency::USD);
        assert!(e
            .validate()
            .unwrap_err()
            .contains(&ValidationError::NotEur(1, Currency::USD)));
    }

    #[test]
    fn reversal_negates_and_balances() {
        let e = hosting_invoice();
        let r = reversal_of(
            &e.lines,
            e.journal,
            date(),
            "Correctie".into(),
            "INK-2026-0001",
        );
        assert_eq!(r.validate(), Ok(()));
        for (a, b) in e.lines.iter().zip(&r.lines) {
            assert_eq!(a.amount.cents, -b.amount.cents);
            assert_eq!(a.account, b.account);
        }
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Any set of non-zero debit lines balanced by one credit line validates,
            /// and changing any single line by a non-zero delta breaks the balance.
            #[test]
            fn balance_invariant(amounts in prop::collection::vec(1i64..10_000_000, 1..20),
                                 idx in any::<prop::sample::Index>(),
                                 delta in prop_oneof![-1_000_000i64..-1, 1i64..1_000_000]) {
                let total: i64 = amounts.iter().sum();
                let mut lines: Vec<EntryLine> =
                    amounts.iter().map(|a| EntryLine::debit("4900", *a)).collect();
                lines.push(EntryLine::credit("1600", total));
                let mut e = EntryDraft {
                    journal: JournalCode::MEM, date: date(), description: "x".into(),
                    lines, source: Source::manual(), idempotency_key: None,
                };
                prop_assert_eq!(e.validate(), Ok(()));

                let i = idx.index(e.lines.len());
                let new = e.lines[i].amount.cents + delta;
                prop_assume!(new != 0);
                e.lines[i].amount.cents = new;
                prop_assert!(e.validate().unwrap_err().contains(&ValidationError::Unbalanced(delta)));
            }
        }
    }
}
