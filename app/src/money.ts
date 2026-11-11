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
