import { invoke } from "@tauri-apps/api/core";

export type AccountType = "asset" | "liability" | "equity" | "revenue" | "expense";
export type VatRegime = "regular" | "kor";

export interface Account {
  number: string;
  name: string;
  account_type: AccountType;
  rgs_code: string | null;
  active: boolean;
}

export interface TaxCode {
  code: string;
  name: string;
  kind: "sales" | "purchase" | "none";
  rate_bp: number | null;
  rubric: string | null;
  regime: VatRegime | null;
  note: string | null;
}

export interface TrialBalanceRow {
  account: string;
  name: string;
  account_type: AccountType;
  debit_cents: number;
  credit_cents: number;
  balance_cents: number;
}

export interface PostedEntry {
  id: string;
  journal: string;
  fiscal_year: number;
  sequence: number;
  date: string;
}

export interface EntrySummary {
  id: string;
  number: string;
  journal: string;
  date: string;
  description: string;
  source_kind: string;
  total_debit_cents: number;
  reverses_entry_id: string | null;
  reversed_by_entry_id: string | null;
}

export interface LineView {
  line_no: number;
  account: string;
  account_name: string;
  amount_cents: number;
  tax_code: string | null;
  description: string | null;
}

export interface EntryDetail {
  summary: EntrySummary;
  lines: LineView[];
  posted_at: string;
  hash: string;
}

export interface LedgerCard {
  account: string;
  name: string;
  year: number;
  opening_cents: number;
  rows: { date: string; entry_id: string; number: string; description: string; debit_cents: number; credit_cents: number; balance_cents: number }[];
  closing_cents: number;
}

export interface PeriodView {
  year: number;
  month: number;
  closed: boolean;
}

export interface Company {
  legal_name: string;
  trade_name: string | null;
  kvk_number: string | null;
  vat_number: string | null;
  address: string | null;
  postal_code: string | null;
  city: string | null;
  country: string;
}

export interface NewLine {
  account: string;
  debit: string;
  credit: string;
  tax_code: string | null;
  description: string | null;
}

export interface NewEntry {
  journal: string;
  date: string;
  description: string;
  lines: NewLine[];
}

export type ChainStatus = { Intact: number } | { Broken: { chain_index: number; entry_id: string } };

export interface AppInfo {
  version: string;
  data_dir: string;
}

export const journals: { code: string; name: string }[] = [
  { code: "MEM", name: "Memoriaal" },
  { code: "BNK", name: "Bank" },
  { code: "VRK", name: "Verkoop" },
  { code: "INK", name: "Inkoop" },
  { code: "PLT", name: "Platformen" },
];

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  accounts: () => invoke<Account[]>("accounts"),
  taxCodes: (date: string) => invoke<TaxCode[]>("tax_codes", { date }),
  postEntry: (entry: NewEntry) => invoke<PostedEntry>("post_entry", { entry }),
  listEntries: (year: number | null, journal: string | null) => invoke<EntrySummary[]>("list_entries", { year, journal }),
  entryDetail: (id: string) => invoke<EntryDetail>("entry_detail", { id }),
  reverseEntry: (id: string, date: string, reason: string) => invoke<PostedEntry>("reverse_entry", { id, date, reason }),
  trialBalance: (until: string | null) => invoke<TrialBalanceRow[]>("trial_balance", { until }),
  ledgerCard: (account: string, year: number) => invoke<LedgerCard>("ledger_card", { account, year }),
  periods: (year: number) => invoke<PeriodView[]>("periods", { year }),
  closePeriod: (year: number, month: number) => invoke<void>("close_period", { year, month }),
  reopenPeriod: (year: number, month: number, reason: string) => invoke<void>("reopen_period", { year, month, reason }),
  verifyChain: () => invoke<ChainStatus>("verify_chain"),
  company: () => invoke<Company | null>("company"),
  saveCompany: (company: Company) => invoke<void>("save_company", { company }),
  vatRegime: (date: string) => invoke<VatRegime>("vat_regime", { date }),
  setVatRegime: (validFrom: string, regime: VatRegime, note: string) =>
    invoke<void>("set_vat_regime", { validFrom, regime, note }),
  backupNow: () => invoke<string>("backup_now"),
};

export function entryNumber(e: PostedEntry): string {
  return `${e.journal}-${e.fiscal_year}-${String(e.sequence).padStart(4, "0")}`;
}
