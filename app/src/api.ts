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
