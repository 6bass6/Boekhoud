//! Command for the IB-prognose (income-tax estimate).

use boekhoud_core::{Currency, Money};
use boekhoud_db::IncomeTaxView;
use tauri::State;

use crate::{with_ledger, AppState, CmdResult};

/// Dutch amount text, empty = 0; negative amounts are refused.
fn parse_amount(text: &str, label: &str) -> CmdResult<i64> {
    if text.trim().is_empty() {
        return Ok(0);
    }
    match Money::parse(text, ',', Currency::EUR) {
        Ok(m) if m.cents >= 0 => Ok(m.cents),
        _ => Err(format!("Ongeldig bedrag bij {label}: {text}")),
    }
}

#[tauri::command]
pub fn income_tax_estimate(
    state: State<AppState>,
    year: i32,
    salary: String,
    withheld: String,
    starter: bool,
) -> CmdResult<IncomeTaxView> {
    let salary = parse_amount(&salary, "loon")?;
    let withheld = parse_amount(&withheld, "ingehouden loonheffing")?;
    let today = chrono::Local::now().date_naive();
    with_ledger(&state, |l| {
        l.income_tax_estimate(year, today, salary, withheld, starter)
    })
}

#[cfg(test)]
mod tests {
    use super::parse_amount;

    #[test]
    fn parses_dutch_amounts() {
        assert_eq!(parse_amount("", "loon"), Ok(0));
        assert_eq!(parse_amount("30.000,50", "loon"), Ok(3_000_050));
        assert!(parse_amount("-5", "loon").is_err());
        assert!(parse_amount("abc", "loon").is_err());
    }
}
