import { invoke } from "@tauri-apps/api/core";

export type AccountType = "asset" | "liability" | "equity" | "revenue" | "expense";

export interface Account {
  number: string;
  name: string;
  account_type: AccountType;
  rgs_code: string | null;
  active: boolean;
}

export interface TrialBalanceRow {
  account: string;
  name: string;
  account_type: AccountType;
  debit_cents: number;
  credit_cents: number;
  balance_cents: number;
}

export type ChainStatus = { Intact: number } | { Broken: { chain_index: number; entry_id: string } };

export interface AppInfo {
  version: string;
  data_dir: string;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  accounts: () => invoke<Account[]>("accounts"),
  trialBalance: () => invoke<TrialBalanceRow[]>("trial_balance"),
  verifyChain: () => invoke<ChainStatus>("verify_chain"),
};
