//! Text decoding and tolerant header handling shared by the CSV-like parsers.

use chrono::NaiveDate;

use crate::{ImportError, Result};

/// Decode bytes to text: UTF-8 (with or without BOM), UTF-16 LE/BE with BOM, else Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (text, _) = enc.decode_without_bom_handling(&bytes[bom_len..]);
        return text.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Normalise a header for matching: lowercase, alphanumerics only.
pub fn norm(h: &str) -> String {
    h.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Find the first header matching any of the candidate names.
pub fn find(headers: &[String], candidates: &[&str]) -> Option<usize> {
    let normed: Vec<String> = headers.iter().map(|h| norm(h)).collect();
    candidates
        .iter()
        .find_map(|c| normed.iter().position(|h| *h == norm(c)))
}

pub fn require(headers: &[String], candidates: &[&str]) -> Result<usize> {
    find(headers, candidates).ok_or_else(|| ImportError::MissingColumn(candidates[0].to_string()))
}

/// Parse an amount that may use either `.` or `,` as decimal separator (and the other as thousands).
/// Rules: with both separators present the last one is the decimal separator; a separator that
/// occurs more than once is a thousands separator; a single separator is the decimal separator.
/// More than two decimals are rounded half away from zero (platform reports sometimes carry more).
pub fn amount_cents(s: &str) -> Option<i64> {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t.strip_prefix('+').unwrap_or(&t).to_string()),
    };
    if t.is_empty() {
        return None;
    }
    let dots = t.matches('.').count();
    let commas = t.matches(',').count();
    let decimal = match (dots, commas) {
        (0, 0) => None,
        (_, 0) if dots > 1 => None,
        (0, _) if commas > 1 => None,
        (_, 0) => Some('.'),
        (0, _) => Some(','),
        _ => t.rfind(['.', ',']).map(|i| t.as_bytes()[i] as char),
    };
    let (int_part, frac_part) = match decimal {
        Some(sep) => {
            let i = t.rfind(sep)?;
            (t[..i].replace(['.', ','], ""), t[i + 1..].to_string())
        }
        None => (t.replace(['.', ','], ""), String::new()),
    };
    if (int_part.is_empty() && frac_part.is_empty())
        || !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let int: i64 = if int_part.is_empty() { 0 } else { int_part.parse().ok()? };
    let padded = format!("{frac_part:0<3}");
    let cents_frac: i64 = padded[..2].parse().ok()?;
    let round_up = padded.as_bytes()[2] >= b'5';
    let cents = int.checked_mul(100)?.checked_add(cents_frac)? + i64::from(round_up);
    Some(if neg { -cents } else { cents })
}

/// Parse common date formats: ISO, DD-MM-YYYY, DD/MM/YYYY, MM/DD/YYYY (when `us`), YYYYMMDD,
/// and Google's "Oct 1, 2026".
pub fn date(s: &str, us: bool) -> Option<NaiveDate> {
    let t = s.trim();
    let formats: &[&str] = if us {
        &["%Y-%m-%d", "%m/%d/%Y", "%Y%m%d", "%b %d, %Y", "%d-%m-%Y"]
    } else {
        &["%Y-%m-%d", "%d-%m-%Y", "%d/%m/%Y", "%Y%m%d", "%d.%m.%Y", "%b %d, %Y"]
    };
    formats.iter().find_map(|f| NaiveDate::parse_from_str(t, f).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_in_many_shapes() {
        assert_eq!(amount_cents("1.234,56"), Some(123_456));
        assert_eq!(amount_cents("1,234.56"), Some(123_456));
        assert_eq!(amount_cents("-0.15"), Some(-15));
        assert_eq!(amount_cents("12"), Some(1_200));
        assert_eq!(amount_cents("0.7"), Some(70));
        assert_eq!(amount_cents("2.345"), Some(235)); // three decimals: rounded
        assert_eq!(amount_cents("1.234.567"), Some(123_456_700)); // thousands only
        assert_eq!(amount_cents("+5,00"), Some(500));
        assert_eq!(amount_cents("abc"), None);
        assert_eq!(amount_cents(""), None);
    }

    #[test]
    fn decodes_utf16_and_cp1252() {
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("é,1".encode_utf16().flat_map(|u| u.to_le_bytes()))
            .collect();
        assert_eq!(decode(&utf16), "é,1");
        assert_eq!(decode(&[0x80, b'5']), "€5");
        assert_eq!(decode("\u{feff}abc".as_bytes()), "abc");
    }

    #[test]
    fn dates() {
        assert_eq!(date("10/31/2026", true), NaiveDate::from_ymd_opt(2026, 10, 31));
        assert_eq!(date("31-10-2026", false), NaiveDate::from_ymd_opt(2026, 10, 31));
        assert_eq!(date("20261031", false), NaiveDate::from_ymd_opt(2026, 10, 31));
        assert_eq!(date("Oct 1, 2026", true), NaiveDate::from_ymd_opt(2026, 10, 1));
    }
}
