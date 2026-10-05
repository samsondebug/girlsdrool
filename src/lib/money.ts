/**
 * Display-only money formatting. The core returns integer cents; this file turns them into
 * strings and never sums, nets, or derives a figure. Integers up to 2^53 are exact in JS, which
 * covers any personal ledger (that is ninety trillion dollars).
 */

export type Cents = number;

function assertCents(cents: number): void {
  if (!Number.isSafeInteger(cents)) {
    throw new RangeError(`not an integer number of cents: ${String(cents)}`);
  }
}

function groupThousands(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

export interface FormatCentsOptions {
  /** "auto" shows "-" on negatives only; "always" also shows "+" on positives. */
  sign?: "auto" | "always";
  /** Include the currency symbol (default true). */
  symbol?: boolean;
}

/** `123456 → "$1,234.56"`, `-5 → "-$0.05"`. Exact; no rounding happens here. */
export function formatCents(cents: Cents, options: FormatCentsOptions = {}): string {
  assertCents(cents);
  const { sign = "auto", symbol = true } = options;
  const negative = cents < 0;
  const abs = Math.abs(cents);
  const dollars = Math.floor(abs / 100);
  const remainder = abs % 100;
  const body = `${groupThousands(String(dollars))}.${String(remainder).padStart(2, "0")}`;
  const prefix = negative ? "-" : sign === "always" && cents > 0 ? "+" : "";
  return `${prefix}${symbol ? "$" : ""}${body}`;
}

/**
 * Basis points to a percentage string. `1999 → "19.99%"`. When fewer than two fraction digits
 * are requested the value is rounded half away from zero, the only rounding rule Kept uses.
 */
export function formatBps(bps: number, fractionDigits: 0 | 1 | 2 = 2): string {
  assertCents(bps);
  const scale = 10 ** (2 - fractionDigits);
  const negative = bps < 0;
  const abs = Math.abs(bps);
  let q = Math.trunc(abs / scale);
  const r = abs - q * scale;
  if (2 * r >= scale) {
    q += 1;
  }
  const divisor = 10 ** fractionDigits;
  const whole = Math.trunc(q / divisor);
  const frac = q - whole * divisor;
  const fracText = fractionDigits === 0 ? "" : `.${String(frac).padStart(fractionDigits, "0")}`;
  return `${negative ? "-" : ""}${String(whole)}${fracText}%`;
}

/**
 * Parse what a person types into an amount field into integer cents, or `null` if it is not an
 * amount. Accepts `1,234.56`, `$12`, `-3.5`, `(40.00)`; rejects more than two decimals, letters,
 * and empty input. Integer arithmetic only.
 */
export function parseCentsInput(text: string): Cents | null {
  let s = text.trim();
  if (s === "") return null;
  let negative = false;
  if (s.startsWith("(") && s.endsWith(")")) {
    negative = true;
    s = s.slice(1, -1).trim();
  }
  if (s.endsWith("-")) {
    negative = !negative;
    s = s.slice(0, -1).trim();
  }
  s = s.replace(/[$,\s]/g, "");
  if (s.startsWith("-")) {
    negative = !negative;
    s = s.slice(1);
  } else if (s.startsWith("+")) {
    s = s.slice(1);
  }
  const match = /^(\d*)(?:\.(\d{0,2}))?$/.exec(s);
  if (!match) return null;
  const wholeText = match[1] ?? "";
  const fracText = match[2] ?? "";
  if (wholeText === "" && fracText === "") return null;
  const whole = wholeText === "" ? 0 : Number(wholeText);
  const frac = fracText === "" ? 0 : Number(fracText.padEnd(2, "0"));
  const magnitude = whole * 100 + frac;
  if (!Number.isSafeInteger(magnitude)) return null;
  return negative ? -magnitude : magnitude;
}
