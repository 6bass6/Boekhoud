/**
 * Format integer cents as Dutch currency text, e.g. 121000 -> "€ 1.210,00".
 * Pure string arithmetic: amounts never pass through floating point.
 */
export function formatEuro(cents: number): string {
  if (!Number.isSafeInteger(cents)) throw new Error(`invalid cents: ${cents}`);
  const negative = cents < 0;
  const abs = String(Math.abs(cents)).padStart(3, "0");
  const whole = abs.slice(0, -2).replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${negative ? "-" : ""}€ ${whole},${abs.slice(-2)}`;
}
