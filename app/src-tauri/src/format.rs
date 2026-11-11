//! Dutch formatting for documents (invoices). Integer arithmetic only.

use chrono::NaiveDate;

/// 121050 -> "€ 1.210,50"; -500 -> "-€ 5,00".
pub fn euro(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let whole = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!("{sign}€ {grouped},{:02}", abs % 100)
}

/// 1500 -> "1,5"; 1000 -> "1"; -2250 -> "-2,25".
pub fn quantity(milli: i64) -> String {
    let sign = if milli < 0 { "-" } else { "" };
    let abs = milli.unsigned_abs();
    let frac = abs % 1000;
    if frac == 0 {
        format!("{sign}{}", abs / 1000)
    } else {
        let f = format!("{frac:03}");
        format!("{sign}{},{}", abs / 1000, f.trim_end_matches('0'))
    }
}

/// Parse Dutch quantity text ("1,5", "2") into thousandths.
pub fn parse_quantity(s: &str) -> Option<i64> {
    let t = s.trim().replace('.', "").replace(',', ".");
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t),
    };
    let (i, f) = t.split_once('.').unwrap_or((&t, ""));
    if i.is_empty() && f.is_empty()
        || f.len() > 3
        || !i.chars().all(|c| c.is_ascii_digit())
        || !f.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let int: i64 = if i.is_empty() { 0 } else { i.parse().ok()? };
    let frac: i64 = format!("{f:0<3}").parse().ok()?;
    let v = int.checked_mul(1000)?.checked_add(frac)?;
    Some(if neg { -v } else { v })
}

pub fn date(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

/// 2100 -> "21%"; 950 -> "9,5%".
pub fn rate(bp: i64) -> String {
    if bp.rem_euclid(100) == 0 {
        format!("{}%", bp / 100)
    } else {
        format!("{},{}%", bp / 100, format!("{:02}", bp % 100).trim_end_matches('0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(euro(121_050), "€ 1.210,50");
        assert_eq!(euro(-500), "-€ 5,00");
        assert_eq!(euro(123_456_789), "€ 1.234.567,89");
        assert_eq!(quantity(1500), "1,5");
        assert_eq!(quantity(-2250), "-2,25");
        assert_eq!(parse_quantity("1,5"), Some(1500));
        assert_eq!(parse_quantity("120,5"), Some(120_500));
        assert_eq!(parse_quantity("1,2345"), None);
        assert_eq!(rate(2100), "21%");
        assert_eq!(rate(950), "9,5%");
    }
}
