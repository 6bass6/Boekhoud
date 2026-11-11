//! Commands for the VAT return, ICP, hours, assets and the dashboard.

use boekhoud_core::{Currency, Money};
use boekhoud_db::{AssetInput, AssetView, Dashboard, HoursSummary, IcpLine, TimeEntry, VatDetailLine, VatReturn};
use chrono::Utc;
use serde::Deserialize;
use tauri::State;

use crate::{parse_date, with_ledger, AppState, CmdResult};

#[tauri::command]
pub fn dashboard(state: State<AppState>, year: i32) -> CmdResult<Dashboard> {
    let today = chrono::Local::now().date_naive();
    with_ledger(&state, |l| l.dashboard(year, today))
}

#[tauri::command]
pub fn vat_return(state: State<AppState>, start: String, end: String) -> CmdResult<VatReturn> {
    let (s, e) = (parse_date(&start)?, parse_date(&end)?);
    with_ledger(&state, |l| l.vat_return(s, e))
}

#[tauri::command]
pub fn vat_return_detail(
    state: State<AppState>,
    start: String,
    end: String,
    rubric: String,
) -> CmdResult<Vec<VatDetailLine>> {
    let (s, e) = (parse_date(&start)?, parse_date(&end)?);
    with_ledger(&state, |l| l.vat_return_detail(s, e, &rubric))
}

#[tauri::command]
pub fn icp_listing(state: State<AppState>, start: String, end: String) -> CmdResult<Vec<IcpLine>> {
    let (s, e) = (parse_date(&start)?, parse_date(&end)?);
    with_ledger(&state, |l| l.icp_listing(s, e))
}

#[tauri::command]
pub fn file_vat_return(state: State<AppState>, start: String, end: String) -> CmdResult<String> {
    let (s, e) = (parse_date(&start)?, parse_date(&end)?);
    with_ledger(&state, |l| l.file_vat_return(s, e))
}

#[tauri::command]
pub fn platform_contact(state: State<AppState>, platform: String) -> CmdResult<Option<String>> {
    with_ledger(&state, |l| l.platform_contact(&platform))
}

#[tauri::command]
pub fn set_platform_contact(state: State<AppState>, platform: String, contact_id: Option<String>) -> CmdResult<()> {
    with_ledger(&state, |l| {
        l.set_platform_contact(&platform, contact_id.as_deref().filter(|c| !c.is_empty()))
    })
}

#[tauri::command]
pub fn time_entries(state: State<AppState>, year: i32) -> CmdResult<Vec<TimeEntry>> {
    with_ledger(&state, |l| l.time_entries(year))
}

#[derive(Deserialize)]
pub struct TimeForm {
    id: Option<String>,
    entry_date: String,
    hours: String,
    activity: String,
    project: Option<String>,
    description: Option<String>,
}

/// "1,5" or "1:30" -> 90 minutes.
fn parse_duration(s: &str) -> Option<i64> {
    let t = s.trim();
    if let Some((h, m)) = t.split_once(':') {
        let (h, m): (i64, i64) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
        return (m < 60).then_some(h * 60 + m);
    }
    crate::format::parse_quantity(t).map(|milli| (milli * 60 + 500) / 1000)
}

#[tauri::command]
pub fn save_time_entry(state: State<AppState>, entry: TimeForm) -> CmdResult<String> {
    let minutes = parse_duration(&entry.hours)
        .filter(|m| *m > 0)
        .ok_or("Vul de duur in als uren (1,5) of uu:mm (1:30).")?;
    let e = TimeEntry {
        id: entry.id.unwrap_or_default(),
        entry_date: parse_date(&entry.entry_date)?,
        started_at: None,
        minutes: Some(minutes),
        activity: entry.activity,
        project: entry.project.filter(|p| !p.trim().is_empty()),
        description: entry.description.filter(|p| !p.trim().is_empty()),
    };
    with_ledger(&state, |l| l.save_time_entry(&e))
}

#[tauri::command]
pub fn delete_time_entry(state: State<AppState>, id: String) -> CmdResult<()> {
    with_ledger(&state, |l| l.delete_time_entry(&id))
}

#[tauri::command]
pub fn start_timer(state: State<AppState>, activity: String, description: Option<String>) -> CmdResult<String> {
    with_ledger(&state, |l| {
        l.start_timer(&activity, None, description.as_deref().filter(|d| !d.is_empty()))
    })
}

#[tauri::command]
pub fn stop_timer(state: State<AppState>) -> CmdResult<TimeEntry> {
    with_ledger(&state, |l| l.stop_timer(Utc::now()))
}

#[tauri::command]
pub fn running_timer(state: State<AppState>) -> CmdResult<Option<TimeEntry>> {
    with_ledger(&state, |l| l.running_timer())
}

#[tauri::command]
pub fn hours_summary(state: State<AppState>, year: i32) -> CmdResult<HoursSummary> {
    let today = chrono::Local::now().date_naive();
    with_ledger(&state, |l| l.hours_summary(year, today))
}

#[derive(Deserialize)]
pub struct AssetForm {
    name: String,
    acquisition_date: String,
    cost: String,
    residual: String,
    years: String,
    business_use_pct: i64,
    notes: Option<String>,
}

#[tauri::command]
pub fn add_asset(state: State<AppState>, asset: AssetForm) -> CmdResult<String> {
    let money = |s: &str, what: &str| -> CmdResult<i64> {
        if s.trim().is_empty() {
            return Ok(0);
        }
        Money::parse(s.trim(), ',', Currency::EUR)
            .map(|m| m.cents)
            .map_err(|_| format!("Ongeldig bedrag bij {what}."))
    };
    let years_milli = crate::format::parse_quantity(&asset.years)
        .filter(|y| *y > 0)
        .ok_or("Vul de afschrijvingstermijn in jaren in.")?;
    let input = AssetInput {
        name: asset.name,
        acquisition_date: parse_date(&asset.acquisition_date)?,
        cost_cents: money(&asset.cost, "aanschafprijs")?,
        residual_cents: money(&asset.residual, "restwaarde")?,
        months: (years_milli * 12 + 500) / 1000,
        start_month: None,
        business_use_pct: asset.business_use_pct.clamp(0, 100),
        purchase_invoice_id: None,
        notes: asset.notes.filter(|n| !n.trim().is_empty()),
    };
    with_ledger(&state, |l| l.add_asset(&input))
}

#[tauri::command]
pub fn assets(state: State<AppState>) -> CmdResult<Vec<AssetView>> {
    with_ledger(&state, |l| l.assets())
}

#[tauri::command]
pub fn post_depreciation(state: State<AppState>, until: String) -> CmdResult<usize> {
    let until = parse_date(&until)?;
    with_ledger(&state, |l| l.post_depreciation(until))
}

#[cfg(test)]
mod tests {
    use super::parse_duration;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("1,5"), Some(90));
        assert_eq!(parse_duration("1:30"), Some(90));
        assert_eq!(parse_duration("0:75"), None);
        assert_eq!(parse_duration("2"), Some(120));
        assert_eq!(parse_duration("x"), None);
    }
}
