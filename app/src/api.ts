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

export interface ReportLine {
  account: string;
  name: string;
  amount_cents: number;
}

export interface ProfitAndLoss {
  from: string;
  until: string;
  revenue: ReportLine[];
  expenses: ReportLine[];
  revenue_cents: number;
  expenses_cents: number;
  result_cents: number;
}

export interface BalanceSheet {
  until: string;
  assets: ReportLine[];
  liabilities: ReportLine[];
  equity: ReportLine[];
  prior_years_result_cents: number;
  current_year_result_cents: number;
  assets_cents: number;
  liabilities_cents: number;
  equity_cents: number;
}

export type ReportKind = "winst-verlies" | "balans" | "proefbalans";

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
  iban: string | null;
  email: string | null;
  payment_term_days: number;
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

export interface Contact {
  id: string;
  name: string;
  contact_person: string | null;
  email: string | null;
  address: string | null;
  postal_code: string | null;
  city: string | null;
  country: string;
  vat_number: string | null;
  kvk_number: string | null;
  is_customer: boolean;
  is_supplier: boolean;
  is_platform: boolean;
  default_account: string | null;
  default_tax_code: string | null;
  archived: boolean;
}

export interface SalesLine {
  description: string;
  quantity_milli: number;
  unit_price_cents: number;
  tax_code: string;
  account: string;
}

export interface VatGroup { code: string; rate_bp: number | null; base_cents: number; vat_cents: number; reverse_charge: boolean }
export interface InvoiceTotals { subtotal_cents: number; vat_cents: number; total_cents: number; groups: VatGroup[] }
export interface PaymentView { id: string; date: string; amount_cents: number; method: string; entry_id: string; reversed: boolean }

export interface SalesInvoiceSummary {
  id: string;
  status: "draft" | "issued";
  invoice_number: string | null;
  invoice_date: string;
  due_date: string;
  contact_id: string;
  contact_name: string;
  is_credit_note: boolean;
  total_cents: number;
  open_cents: number;
  state: "draft" | "open" | "partially_paid" | "paid" | "overdue" | "credited" | "credit_note";
}

export interface SalesInvoiceDetail {
  summary: SalesInvoiceSummary;
  supply_date: string | null;
  reference: string | null;
  notes: string | null;
  credits_invoice_id: string | null;
  credits_invoice_number: string | null;
  lines: SalesLine[];
  totals: InvoiceTotals | null;
  vat_regime: VatRegime;
  entry_id: string | null;
  document_id: string | null;
  payments: PaymentView[];
}

export interface SalesLineForm { description: string; quantity: string; unit_price: string; tax_code: string; account: string }
export interface SalesInvoiceForm {
  contact_id: string;
  invoice_date: string;
  supply_date: string | null;
  due_date: string | null;
  reference: string | null;
  notes: string | null;
  lines: SalesLineForm[];
}

export interface PurchaseLine { description: string | null; account: string; net_cents: number; tax_code: string; vat_cents: number }
export interface PurchaseSummary {
  id: string;
  contact_id: string;
  contact_name: string;
  supplier_reference: string | null;
  invoice_date: string;
  due_date: string | null;
  description: string;
  total_cents: number;
  open_cents: number;
  has_document: boolean;
  state: "open" | "paid" | "paid_privately" | "reversed";
  entry_id: string;
}
export interface PurchaseDetail {
  summary: PurchaseSummary;
  lines: PurchaseLine[];
  subtotal_cents: number;
  vat_cents: number;
  document_id: string | null;
  payments: PaymentView[];
}
export interface PurchaseLineForm { description: string | null; account: string; net: string; tax_code: string; vat: string }
export interface PurchaseForm {
  contact_id: string;
  supplier_reference: string | null;
  invoice_date: string;
  due_date: string | null;
  description: string;
  paid_privately: boolean;
  document_id: string | null;
  lines: PurchaseLineForm[];
}

export interface DocumentInfo {
  id: string;
  sha256: string;
  original_filename: string;
  mime_type: string;
  size_bytes: number;
  document_type: string;
  uploaded_at: string;
  retention_until: string;
}

export interface PlatformReport {
  id: string;
  platform: "apple" | "google";
  period_start: string;
  period_end: string;
  original_filename: string;
  document_id: string | null;
  currencies: { currency: string; units: number; amount_cents: number }[];
  breakdown: { rows?: number; types?: { transaction_type: string; rows: number; amount_cents: number }[] };
  proceeds_eur_cents: number | null;
  entry_id: string | null;
  imported_at: string;
  overlapping: string[];
}

export interface BankAccount { id: string; iban: string; name: string; ledger_account: string }
export interface BankTransaction {
  id: string;
  bank_account_id: string;
  booking_date: string;
  amount_cents: number;
  currency: string;
  counterparty_name: string | null;
  counterparty_iban: string | null;
  description: string;
  state: "open" | "processed" | "ignored";
  entry_id: string | null;
  entry_number: string | null;
  ignore_reason: string | null;
}
export type BankAction =
  | { kind: "sales_invoice"; invoice_id: string }
  | { kind: "purchase_invoice"; invoice_id: string }
  | { kind: "platform_payout"; platform: string; settle_cents: number | null }
  | { kind: "account"; account: string; tax_code: string | null; description: string | null }
  | { kind: "private" }
  | { kind: "ignore"; reason: string };
export interface Suggestion { action: BankAction; label: string; confidence: number }
export interface Reconciliation {
  statement_date: string | null;
  statement_balance_cents: number | null;
  ledger_balance_at_statement_cents: number | null;
  difference_cents: number | null;
  open_transactions: number;
  open_sum_cents: number;
  ledger_balance_cents: number;
}

export interface Dashboard {
  year: number;
  revenue_cents: number;
  expenses_cents: number;
  result_cents: number;
  bank_cents: number;
  receivables_open_cents: number;
  receivables_overdue: number;
  platform_receivable_cents: number;
  payables_open_cents: number;
  vat_position_cents: number;
  bank_unprocessed: number;
  purchases_without_document: number;
  draft_invoices: number;
  unbooked_platform_reports: number;
  hours_minutes: number;
  revenue_by_month: number[];
  expenses_by_month: number[];
}
export interface RubricLine { rubric: string; label: string; base_cents: number; vat_cents: number }
export interface VatReturn {
  period_start: string;
  period_end: string;
  rubrics: RubricLine[];
  due_cents: number;
  input_vat_cents: number;
  balance_cents: number;
  rounded: { rubrics: [string, number, number][]; due: number; input_vat: number; balance: number };
  warnings: string[];
  filed: { id: string; filed_at: string; entry_id: string | null } | null;
}
export interface VatDetailLine { entry_id: string; entry_number: string; date: string; description: string; account: string; tax_code: string; base_cents: number; vat_cents: number }
export interface IcpLine { vat_number: string | null; name: string; country: string | null; services_cents: number }
export interface TimeEntry { id: string; entry_date: string; started_at: string | null; minutes: number | null; activity: string; project: string | null; description: string | null }
export interface HoursSummary { year: number; total_minutes: number; by_month: number[]; by_activity: [string, number][]; criterion_hours: number; needed_per_week_minutes: number }
export interface IncomeTaxBracket { upto_cents: number | null; rate_micro: number; income_cents: number; tax_cents: number }
export interface IncomeTaxEstimate {
  profit_cents: number; zelfstandigenaftrek_cents: number; startersaftrek_cents: number; ondernemersaftrek_cents: number;
  mkb_vrijstelling_cents: number; taxable_profit_cents: number; salary_cents: number; box1_income_cents: number;
  brackets: IncomeTaxBracket[]; bracket_tax_cents: number; rate_cap_base_cents: number; rate_cap_cents: number; tax_cents: number;
  algemene_heffingskorting_cents: number; arbeidskorting_cents: number; credits_applied_cents: number; tax_after_credits_cents: number;
  zvw_base_cents: number; zvw_cents: number; total_cents: number; withheld_cents: number; to_pay_cents: number; monthly_reserve_cents: number | null;
}
export interface IncomeTaxParams {
  zelfstandigenaftrek_cents: number; startersaftrek_cents: number; mkb_rate_micro: number; deduction_rate_cap_micro: number;
  zvw_rate_micro: number; zvw_max_income_cents: number;
}
export interface IncomeTaxView {
  year: number; parameter_year: number; hours_minutes: number; criterion_hours: number; remaining_months: number;
  params: IncomeTaxParams; estimate: IncomeTaxEstimate; sources: string[];
}
export interface AssetView {
  id: string; name: string; acquisition_date: string; cost_cents: number; residual_cents: number; months: number;
  start_month: string; business_use_pct: number; monthly_cents: number; depreciated_cents: number; book_value_cents: number; months_posted: number; notes: string | null;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  accounts: () => invoke<Account[]>("accounts"),
  taxCodes: (date: string) => invoke<TaxCode[]>("tax_codes", { date }),
  postEntry: (entry: NewEntry) => invoke<PostedEntry>("post_entry", { entry }),
  listEntries: (year: number | null, journal: string | null) => invoke<EntrySummary[]>("list_entries", { year, journal }),
  entryDetail: (id: string) => invoke<EntryDetail>("entry_detail", { id }),
  reverseEntry: (id: string, date: string, reason: string) => invoke<PostedEntry>("reverse_entry", { id, date, reason }),
  trialBalance: (until: string | null) => invoke<TrialBalanceRow[]>("trial_balance", { until }),
  profitAndLoss: (until: string) => invoke<ProfitAndLoss>("profit_and_loss", { until }),
  balanceSheet: (until: string) => invoke<BalanceSheet>("balance_sheet", { until }),
  /** Writes a CSV to the data folder's exports/ and returns its path. */
  exportReportCsv: (report: ReportKind, until: string) => invoke<string>("export_report_csv", { report, until }),
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

  contacts: (includeArchived = false) => invoke<Contact[]>("contacts", { includeArchived }),
  saveContact: (contact: Contact) => invoke<string>("save_contact", { contact }),

  salesInvoices: (year: number | null) => invoke<SalesInvoiceSummary[]>("sales_invoices", { year }),
  salesInvoice: (id: string) => invoke<SalesInvoiceDetail>("sales_invoice", { id }),
  saveSalesInvoice: (id: string | null, invoice: SalesInvoiceForm) => invoke<string>("save_sales_invoice", { id, invoice }),
  deleteSalesInvoice: (id: string) => invoke<void>("delete_sales_invoice", { id }),
  salesIssueRequirements: (id: string) => invoke<{ missing: string[] }>("sales_issue_requirements", { id }),
  issueSalesInvoice: (id: string) => invoke<SalesInvoiceSummary>("issue_sales_invoice", { id }),
  createCreditNote: (id: string, date: string) => invoke<string>("create_credit_note", { id, date }),
  registerSalesPayment: (id: string, date: string, amountText: string) => invoke<string>("register_sales_payment", { id, date, amountText }),
  openSalesInvoicePdf: (id: string) => invoke<void>("open_sales_invoice_pdf", { id }),

  registerPurchase: (invoice: PurchaseForm) => invoke<PurchaseSummary>("register_purchase_invoice", { invoice }),
  purchaseInvoices: (year: number | null) => invoke<PurchaseSummary[]>("purchase_invoices", { year }),
  purchaseInvoice: (id: string) => invoke<PurchaseDetail>("purchase_invoice", { id }),
  attachPurchaseDocument: (id: string, documentId: string) => invoke<void>("attach_purchase_document", { id, documentId }),
  registerPurchasePayment: (id: string, date: string, amountText: string) => invoke<string>("register_purchase_payment", { id, date, amountText }),

  uploadDocument: (filename: string, dataBase64: string, documentType: string) =>
    invoke<DocumentInfo>("upload_document", { filename, dataBase64, documentType }),
  documentContent: (id: string) => invoke<{ info: DocumentInfo; data_base64: string }>("document_content", { id }),
  openDocument: (id: string) => invoke<void>("open_document", { id }),

  importPlatformFile: (filename: string, dataBase64: string) => invoke<PlatformReport>("import_platform_file", { filename, dataBase64 }),
  platformReports: () => invoke<PlatformReport[]>("platform_reports"),
  bookPlatformReport: (id: string, amounts: { currency: string; eur: string }[]) => invoke<string>("book_platform_report", { id, amounts }),
  deletePlatformReport: (id: string) => invoke<void>("delete_platform_report", { id }),
  importBankFile: (filename: string, dataBase64: string, bankAccountId: string | null) =>
    invoke<{ account: BankAccount; format: string; result: { new_transactions: number; skipped_duplicates: number } }>("import_bank_file", { filename, dataBase64, bankAccountId }),
  bankAccounts: () => invoke<BankAccount[]>("bank_accounts"),
  addBankAccount: (iban: string, name: string) => invoke<BankAccount>("add_bank_account", { iban, name }),
  bankTransactions: (accountId: string, filter: string, year: number | null) => invoke<BankTransaction[]>("bank_transactions", { accountId, filter, year }),
  bankSuggestions: (id: string) => invoke<Suggestion[]>("bank_suggestions", { id }),
  processBankTransaction: (id: string, action: BankAction) => invoke<BankTransaction>("process_bank_transaction", { id, action }),
  reopenBankTransaction: (id: string) => invoke<void>("reopen_bank_transaction", { id }),
  bankReconciliation: (accountId: string) => invoke<Reconciliation>("bank_reconciliation", { accountId }),
  dashboard: (year: number) => invoke<Dashboard>("dashboard", { year }),
  vatReturn: (start: string, end: string) => invoke<VatReturn>("vat_return", { start, end }),
  vatReturnDetail: (start: string, end: string, rubric: string) => invoke<VatDetailLine[]>("vat_return_detail", { start, end, rubric }),
  icpListing: (start: string, end: string) => invoke<IcpLine[]>("icp_listing", { start, end }),
  fileVatReturn: (start: string, end: string) => invoke<string>("file_vat_return", { start, end }),
  platformContact: (platform: string) => invoke<string | null>("platform_contact", { platform }),
  setPlatformContact: (platform: string, contactId: string | null) => invoke<void>("set_platform_contact", { platform, contactId }),
  timeEntries: (year: number) => invoke<TimeEntry[]>("time_entries", { year }),
  saveTimeEntry: (entry: { id: string | null; entry_date: string; hours: string; activity: string; project: string | null; description: string | null }) =>
    invoke<string>("save_time_entry", { entry }),
  deleteTimeEntry: (id: string) => invoke<void>("delete_time_entry", { id }),
  startTimer: (activity: string, description: string | null) => invoke<string>("start_timer", { activity, description }),
  stopTimer: () => invoke<TimeEntry>("stop_timer"),
  runningTimer: () => invoke<TimeEntry | null>("running_timer"),
  hoursSummary: (year: number) => invoke<HoursSummary>("hours_summary", { year }),
  incomeTaxEstimate: (year: number, salary: string, withheld: string, starter: boolean) =>
    invoke<IncomeTaxView>("income_tax_estimate", { year, salary, withheld, starter }),
  addAsset: (asset: { name: string; acquisition_date: string; cost: string; residual: string; years: string; business_use_pct: number; notes: string | null }) =>
    invoke<string>("add_asset", { asset }),
  assets: () => invoke<AssetView[]>("assets"),
  postDepreciation: (until: string) => invoke<number>("post_depreciation", { until }),
  platformRevenueByProduct: (year: number) => invoke<[string, string, string, number, number][]>("platform_revenue_by_product", { year }),
};

/** Read a File picked in an <input type="file"> as base64 (without data: prefix). */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",", 2)[1] ?? "");
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export function entryNumber(e: PostedEntry): string {
  return `${e.journal}-${e.fiscal_year}-${String(e.sequence).padStart(4, "0")}`;
}
