/**
 * Money helpers for the UI. Amounts are integer cents; text is parsed and formatted with
 * string arithmetic so amounts never pass through floating point.
 */
export function formatEuro(cents: number): string {
  if (!Number.isSafeInteger(cents)) throw new Error(`invalid cents: ${cents}`);
  const negative = cents < 0;
  const abs = String(Math.abs(cents)).padStart(3, "0");
  const whole = abs.slice(0, -2).replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${negative ? "-" : ""}€ ${whole},${abs.slice(-2)}`;
}

/** Parse Dutch amount text ("1.210,50") into cents; null when empty or invalid. */
export function parseCents(text: string): number | null {
  const t = text.trim().replace(/[.\s]/g, "");
  if (!/^\d*(,\d{0,2})?$/.test(t) || t === "" || t === ",") return null;
  const [whole, frac = ""] = t.split(",");
  const cents = Number(whole || "0") * 100 + Number(frac.padEnd(2, "0"));
  return Number.isSafeInteger(cents) ? cents : null;
}

export const monthNames = [
  "januari", "februari", "maart", "april", "mei", "juni",
  "juli", "augustus", "september", "oktober", "november", "december",
];

export function formatDate(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${d}-${m}-${y}`;
}

export function todayIso(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** Cents -> editable Dutch text without currency sign: 121050 -> "1.210,50". */
export function centsToInput(cents: number): string {
  return formatEuro(cents).replace("€ ", "");
}

/** VAT in cents for a net amount and a rate in basis points, rounded half away from zero. */
export function vatFor(netCents: number, rateBp: number): number {
  const product = Math.abs(netCents) * rateBp;
  const q = Math.floor(product / 10000);
  const r = product - q * 10000;
  const v = 2 * r >= 10000 ? q + 1 : q;
  return netCents < 0 ? -v : v;
}

/** 1500 -> "1,5" */
export function formatQuantity(milli: number): string {
  const neg = milli < 0;
  const abs = Math.abs(milli);
  const whole = Math.floor(abs / 1000);
  const frac = String(abs % 1000).padStart(3, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${whole}${frac ? "," + frac : ""}`;
}

/** Parse "1,5" -> 1500 thousandths; null when invalid. */
export function parseQuantity(text: string): number | null {
  const t = text.trim().replace(/\./g, "");
  const m = /^(-?)(\d*)(?:,(\d{0,3}))?$/.exec(t);
  if (!m || (m[2] === "" && !m[3])) return null;
  const v = Number(m[2] || "0") * 1000 + Number((m[3] ?? "").padEnd(3, "0"));
  return m[1] ? -v : v;
}

export function addDays(iso: string, days: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(Date.UTC(y, m - 1, d + days));
  return date.toISOString().slice(0, 10);
}

/** Line total in cents for a quantity in thousandths, rounded half away from zero (matches the Rust side). */
export function lineNet(quantityMilli: number, unitPriceCents: number): number {
  const product = quantityMilli * unitPriceCents;
  const abs = Math.abs(product);
  const q = Math.floor(abs / 1000);
  const v = 2 * (abs - q * 1000) >= 1000 ? q + 1 : q;
  return product < 0 ? -v : v;
}

/** 90 -> "1:30" */
export function formatMinutes(minutes: number): string {
  const h = Math.floor(minutes / 60);
  return `${h}:${String(minutes % 60).padStart(2, "0")}`;
}
