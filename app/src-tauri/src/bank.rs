//! Commands for bank accounts, statement import and processing transactions.

use base64::Engine;
use boekhoud_db::{
    BankAccount, BankAction, BankImportInput, BankImportResult, BankTransactionView, NewBankTransaction,
    Reconciliation, Suggestion,
};
use boekhoud_import::bank;
use serde::Serialize;
use tauri::State;

use crate::{with_ledger, AppState, CmdResult};

#[derive(Serialize)]
pub struct ImportOutcome {
    account: BankAccount,
    format: String,
    result: BankImportResult,
}

pub fn to_input(st: &bank::BankStatement) -> CmdResult<BankImportInput> {
    let mut txs = Vec::new();
    for (i, t) in st.transactions.iter().enumerate() {
        let booking = t
            .booking_date
            .or(t.value_date)
            .ok_or_else(|| format!("Transactie {} heeft geen datum.", i + 1))?;
        txs.push(NewBankTransaction {
            booking_date: booking,
            value_date: t.value_date,
            amount_cents: t.amount_cents,
            currency: if t.currency.is_empty() {
                "EUR".into()
            } else {
                t.currency.clone()
            },
            counterparty_name: t.counterparty_name.clone(),
            counterparty_iban: t.counterparty_iban.clone(),
            description: t.description.clone(),
            end_to_end_id: t.end_to_end_id.clone(),
            bank_reference: t.bank_reference.clone(),
        });
    }
    Ok(BankImportInput {
        format: st.format.clone(),
        opening: st.opening_balance.as_ref().map(|b| (b.date, b.amount_cents)),
        closing: st.closing_balance.as_ref().map(|b| (b.date, b.amount_cents)),
        transactions: txs,
    })
}

#[tauri::command]
pub fn import_bank_file(
    state: State<AppState>,
    filename: String,
    data_base64: String,
    bank_account_id: Option<String>,
) -> CmdResult<ImportOutcome> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| "Bestand kon niet worden gelezen.".to_string())?;
    let st = bank::parse(&bytes).map_err(|e| format!("Afschrift niet herkend: {e}"))?;
    let input = to_input(&st)?;
    with_ledger(&state, |l| {
        let account = match (&st.account_iban, bank_account_id.filter(|i| !i.is_empty())) {
            (Some(iban), _) => l.ensure_bank_account(iban, None)?,
            (None, Some(id)) => l
                .bank_accounts()?
                .into_iter()
                .find(|a| a.id == id)
                .ok_or_else(|| boekhoud_db::DbError::NotFound("bankrekening".into()))?,
            (None, None) => {
                return Err(boekhoud_db::DbError::Invalid(
                    "het bestand noemt geen rekeningnummer; kies eerst een bankrekening".into(),
                ))
            }
        };
        let result = l.import_bank_statement(&account.id, &input, &bytes, &filename)?;
        Ok(ImportOutcome {
            account,
            format: st.format.clone(),
            result,
        })
    })
}

#[tauri::command]
pub fn bank_accounts(state: State<AppState>) -> CmdResult<Vec<BankAccount>> {
    with_ledger(&state, |l| l.bank_accounts())
}

#[tauri::command]
pub fn add_bank_account(state: State<AppState>, iban: String, name: String) -> CmdResult<BankAccount> {
    let name = Some(name.trim()).filter(|n| !n.is_empty());
    with_ledger(&state, |l| l.ensure_bank_account(&iban, name))
}

#[tauri::command]
pub fn bank_transactions(
    state: State<AppState>,
    account_id: String,
    filter: String,
    year: Option<i32>,
) -> CmdResult<Vec<BankTransactionView>> {
    with_ledger(&state, |l| l.bank_transactions(&account_id, &filter, year))
}

#[tauri::command]
pub fn bank_suggestions(state: State<AppState>, id: String) -> CmdResult<Vec<Suggestion>> {
    with_ledger(&state, |l| l.bank_suggestions(&id))
}

#[tauri::command]
pub fn process_bank_transaction(
    state: State<AppState>,
    id: String,
    action: BankAction,
) -> CmdResult<BankTransactionView> {
    with_ledger(&state, |l| l.process_bank_transaction(&id, &action))
}

#[tauri::command]
pub fn reopen_bank_transaction(state: State<AppState>, id: String) -> CmdResult<()> {
    with_ledger(&state, |l| l.reopen_bank_transaction(&id))
}

#[tauri::command]
pub fn bank_reconciliation(state: State<AppState>, account_id: String) -> CmdResult<Reconciliation> {
    with_ledger(&state, |l| l.bank_reconciliation(&account_id))
}
