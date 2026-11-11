//! Exact money arithmetic in integer minor units (cents).

use std::fmt;
use std::ops::Neg;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// ISO 4217 currency code, stored as three ASCII uppercase letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency([u8; 3]);

impl Currency {
    pub const EUR: Currency = Currency(*b"EUR");
    pub const USD: Currency = Currency(*b"USD");

    pub fn new(code: &str) -> Result<Self, MoneyError> {
        let b = code.as_bytes();
        if b.len() == 3 && b.iter().all(u8::is_ascii_uppercase) {
            Ok(Currency([b[0], b[1], b[2]]))
        } else {
            Err(MoneyError::InvalidCurrency(code.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        // Invariant: always three ASCII uppercase letters.
        std::str::from_utf8(&self.0).expect("currency is ASCII")
    }
}

impl TryFrom<String> for Currency {
    type Error = MoneyError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Currency::new(&s)
    }
}

impl From<Currency> for String {
    fn from(c: Currency) -> Self {
        c.as_str().to_string()
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("invalid currency code: {0}")]
    InvalidCurrency(String),
    #[error("currency mismatch: {0} vs {1}")]
    CurrencyMismatch(Currency, Currency),
    #[error("amount overflow")]
    Overflow,
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
}

/// An amount of money: integer minor units (cents) plus currency.
/// All supported currencies are treated as having two decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub cents: i64,
    pub currency: Currency,
}

impl Money {
    pub const fn new(cents: i64, currency: Currency) -> Self {
        Money { cents, currency }
    }

    pub const fn eur(cents: i64) -> Self {
        Money::new(cents, Currency::EUR)
    }

    pub const fn zero(currency: Currency) -> Self {
        Money::new(0, currency)
    }

    pub fn is_zero(&self) -> bool {
        self.cents == 0
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        self.same_currency(other)?;
        let cents = self
            .cents
            .checked_add(other.cents)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(cents, self.currency))
    }

    pub fn checked_sub(self, other: Money) -> Result<Money, MoneyError> {
        self.checked_add(-other)
    }

    /// Multiply by a rate expressed in basis points (1/100 of a percent), e.g. 21% = 2100.
    /// Rounds half away from zero to whole cents.
    pub fn apply_basis_points(self, bp: i64) -> Result<Money, MoneyError> {
        let product = (self.cents as i128) * (bp as i128);
        let cents = div_round_half_away(product, 10_000);
        let cents = i64::try_from(cents).map_err(|_| MoneyError::Overflow)?;
        Ok(Money::new(cents, self.currency))
    }

    /// Parse a decimal amount such as `1210.5`, `-12,34` or `1.210,00`.
    ///
    /// `decimal_sep` is the decimal separator used by the source (`.` or `,`);
    /// the other of the two is accepted as thousands separator and ignored.
    /// More than two decimals is rejected rather than silently rounded.
    pub fn parse(s: &str, decimal_sep: char, currency: Currency) -> Result<Money, MoneyError> {
        let err = || MoneyError::InvalidAmount(s.to_string());
        let thousands = match decimal_sep {
            '.' => ',',
            ',' => '.',
            _ => return Err(err()),
        };
        let t = s.trim();
        let (neg, t) = match t.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let cleaned: String = t.chars().filter(|c| *c != thousands && *c != ' ').collect();
        let (int_part, frac_part) = match cleaned.split_once(decimal_sep) {
            Some((i, f)) => (i, f),
            None => (cleaned.as_str(), ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(err());
        }
        if !int_part.chars().all(|c| c.is_ascii_digit())
            || !frac_part.chars().all(|c| c.is_ascii_digit())
            || frac_part.len() > 2
        {
            return Err(err());
        }
        let int_val: i64 = if int_part.is_empty() {
            0
        } else {
            int_part.parse().map_err(|_| err())?
        };
        let frac_val: i64 = match frac_part.len() {
            0 => 0,
            1 => frac_part.parse::<i64>().map_err(|_| err())? * 10,
            _ => frac_part.parse().map_err(|_| err())?,
        };
        let cents = int_val
            .checked_mul(100)
            .and_then(|v| v.checked_add(frac_val))
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(if neg { -cents } else { cents }, currency))
    }

    fn same_currency(&self, other: Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch(self.currency, other.currency))
        }
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money::new(-self.cents, self.currency)
    }
}

/// Canonical, locale-independent representation: `-1234.56 EUR`.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.cents < 0 { "-" } else { "" };
        let abs = self.cents.unsigned_abs();
        write!(f, "{sign}{}.{:02} {}", abs / 100, abs % 100, self.currency)
    }
}

/// Integer division rounding half away from zero.
pub(crate) fn div_round_half_away(n: i128, d: i128) -> i128 {
    debug_assert!(d > 0);
    let q = n / d;
    let r = n % d;
    if 2 * r.abs() >= d {
        q + n.signum()
    } else {
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dutch_and_english_formats() {
        let p = |s, sep| Money::parse(s, sep, Currency::EUR).unwrap().cents;
        assert_eq!(p("1.210,00", ','), 121_000);
        assert_eq!(p("1,210.00", '.'), 121_000);
        assert_eq!(p("-12,3", ','), -1_230);
        assert_eq!(p("0.05", '.'), 5);
        assert_eq!(p("42", '.'), 4_200);
        assert_eq!(p(",5", ','), 50);
    }

    #[test]
    fn rejects_bad_amounts() {
        for s in ["", "abc", "12,345", "1e3", "-", "1,2,3"] {
            assert!(Money::parse(s, ',', Currency::EUR).is_err(), "{s}");
        }
        assert!(Money::parse("1.005", '.', Currency::EUR).is_err());
    }

    #[test]
    fn vat_rounding_half_away_from_zero() {
        assert_eq!(
            Money::eur(10_000).apply_basis_points(2100).unwrap().cents,
            2_100
        );
        // 0.50 cent rounds up: 2.38 * 21% = 0.4998 -> 0.50
        assert_eq!(Money::eur(238).apply_basis_points(2100).unwrap().cents, 50);
        // 0.05 * 9% = 0.0045 -> 0.00 ; 0.06 * 9% = 0.0054 -> 0.01
        assert_eq!(Money::eur(5).apply_basis_points(900).unwrap().cents, 0);
        assert_eq!(Money::eur(6).apply_basis_points(900).unwrap().cents, 1);
        assert_eq!(
            Money::eur(-238).apply_basis_points(2100).unwrap().cents,
            -50
        );
    }

    #[test]
    fn currency_mismatch_is_error() {
        let r = Money::eur(1).checked_add(Money::new(1, Currency::USD));
        assert!(matches!(r, Err(MoneyError::CurrencyMismatch(_, _))));
    }

    #[test]
    fn display_is_canonical() {
        assert_eq!(Money::eur(-123_456).to_string(), "-1234.56 EUR");
        assert_eq!(Money::eur(5).to_string(), "0.05 EUR");
    }
}
