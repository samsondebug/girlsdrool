/**
 * Query chips (ADR-0029). The grammar is parsed here into a structured filter; the core
 * compiles it to SQL and computes every total. Nothing here evaluates a filter against rows.
 *
 *   account:<name>      cat:<name or path>   tag:<name>   venture:<name>
 *   >100  >=100  <50  <=50  =12.34            |amount| compared in cents
 *   needs:review   is:unclassified   flag:<name>   status:pending|posted
 *   date:YYYY-MM-DD..YYYY-MM-DD   date:YYYY-MM   date:YYYY-MM-DD   since:DATE   until:DATE
 *   anything else is free text matched against payee and memo
 *
 * Values with spaces are quoted: cat:"Fixed › Rent".
 */
import { parseCentsInput } from "./money";

export interface LedgerFilter {
  account_ids: number[];
  category_ids: number[];
  tag_names: string[];
  venture_ids: number[];
  abs_gt_cents?: number;
  abs_ge_cents?: number;
  abs_lt_cents?: number;
  abs_le_cents?: number;
  abs_eq_cents?: number;
  needs_review: boolean;
  unclassified: boolean;
  flags_any: number;
  status?: "pending" | "posted";
  date_from?: string;
  date_to?: string;
  text?: string;
}

export const FLAG_BITS: Record<string, number> = {
  needs_review: 1,
  cash_withdrawal: 2,
  payment_app_unknown: 4,
  borrowing: 8,
  securities_sale: 16,
  fee: 32,
  interest: 64,
};

export interface Lookups {
  accounts: { id: number; name: string }[];
  categories: { id: number; name: string; path: string }[];
  ventures: { id: number; name: string }[];
}

export interface Chip {
  key: string;
  value: string;
  raw: string;
}

export interface ParsedQuery {
  chips: Chip[];
  text: string[];
}

/** Split on whitespace, honouring double quotes anywhere in a token. */
export function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quoted = false;
  for (const ch of input) {
    if (ch === '"') {
      quoted = !quoted;
      continue;
    }
    if (!quoted && /\s/.test(ch)) {
      if (current !== "") tokens.push(current);
      current = "";
      continue;
    }
    current += ch;
  }
  if (current !== "") tokens.push(current);
  return tokens;
}

const CHIP_KEYS = new Set([
  "account",
  "cat",
  "tag",
  "venture",
  "needs",
  "is",
  "flag",
  "status",
  "date",
  "since",
  "until",
]);

export function parseQuery(input: string): ParsedQuery {
  const chips: Chip[] = [];
  const text: string[] = [];
  for (const token of tokenize(input)) {
    const cmp = /^(>=|<=|>|<|=)(.+)$/.exec(token);
    if (cmp) {
      chips.push({ key: cmp[1] ?? "", value: cmp[2] ?? "", raw: token });
      continue;
    }
    const colon = token.indexOf(":");
    if (colon > 0) {
      const key = token.slice(0, colon).toLowerCase();
      const value = token.slice(colon + 1);
      if (CHIP_KEYS.has(key) && value !== "") {
        chips.push({ key, value, raw: token });
        continue;
      }
    }
    text.push(token);
  }
  return { chips, text };
}

export interface CompileResult {
  filter: LedgerFilter;
  errors: string[];
}

function emptyFilter(): LedgerFilter {
  return {
    account_ids: [],
    category_ids: [],
    tag_names: [],
    venture_ids: [],
    needs_review: false,
    unclassified: false,
    flags_any: 0,
  };
}

function findByName<T extends { id: number; name: string }>(
  items: T[],
  value: string,
): T | undefined {
  const needle = value.trim().toLowerCase();
  const asId = /^\d+$/.test(needle) ? Number(needle) : null;
  return items.find((i) => i.id === asId || i.name.toLowerCase() === needle);
}

function findCategory(categories: Lookups["categories"], value: string) {
  const needle = value.trim().toLowerCase();
  const asId = /^\d+$/.test(needle) ? Number(needle) : null;
  return (
    categories.find((c) => c.id === asId || c.path.toLowerCase() === needle) ??
    categories.find((c) => c.name.toLowerCase() === needle) ??
    categories.find((c) => c.path.toLowerCase().endsWith(`› ${needle}`))
  );
}

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const MONTH = /^\d{4}-\d{2}$/;

function monthRange(month: string): [string, string] {
  const [y, m] = month.split("-").map(Number);
  const year = y ?? 0;
  const mon = m ?? 1;
  const last = new Date(Date.UTC(year, mon, 0)).getUTCDate();
  return [`${month}-01`, `${month}-${String(last).padStart(2, "0")}`];
}

/** Resolve names to ids and build the filter the core executes. Unknown names are errors. */
export function compileQuery(input: string, lookups: Lookups): CompileResult {
  const { chips, text } = parseQuery(input);
  const filter = emptyFilter();
  const errors: string[] = [];
  for (const chip of chips) {
    switch (chip.key) {
      case "account": {
        const a = findByName(lookups.accounts, chip.value);
        if (a) filter.account_ids.push(a.id);
        else errors.push(`unknown account "${chip.value}"`);
        break;
      }
      case "cat": {
        const c = findCategory(lookups.categories, chip.value);
        if (c) filter.category_ids.push(c.id);
        else errors.push(`unknown category "${chip.value}"`);
        break;
      }
      case "tag":
        filter.tag_names.push(chip.value);
        break;
      case "venture": {
        const v = findByName(lookups.ventures, chip.value);
        if (v) filter.venture_ids.push(v.id);
        else errors.push(`unknown venture "${chip.value}"`);
        break;
      }
      case "needs":
        if (chip.value.toLowerCase() === "review") filter.needs_review = true;
        else errors.push(`needs:${chip.value} is not a filter (try needs:review)`);
        break;
      case "is":
        if (chip.value.toLowerCase() === "unclassified") filter.unclassified = true;
        else errors.push(`is:${chip.value} is not a filter (try is:unclassified)`);
        break;
      case "flag": {
        const bit = FLAG_BITS[chip.value.toLowerCase()];
        if (bit === undefined) errors.push(`unknown flag "${chip.value}"`);
        else filter.flags_any |= bit;
        break;
      }
      case "status": {
        const s = chip.value.toLowerCase();
        if (s === "pending" || s === "posted") filter.status = s;
        else errors.push(`status:${chip.value} is not a status`);
        break;
      }
      case "date": {
        const [from, to, ...rest] = chip.value.split("..");
        if (
          from !== undefined &&
          to !== undefined &&
          rest.length === 0 &&
          DATE.test(from) &&
          DATE.test(to)
        ) {
          filter.date_from = from;
          filter.date_to = to;
        } else if (MONTH.test(chip.value)) {
          [filter.date_from, filter.date_to] = monthRange(chip.value);
        } else if (DATE.test(chip.value)) {
          filter.date_from = chip.value;
          filter.date_to = chip.value;
        } else {
          errors.push(`date:${chip.value} must be YYYY-MM-DD, YYYY-MM or a ..range`);
        }
        break;
      }
      case "since":
        if (DATE.test(chip.value)) filter.date_from = chip.value;
        else errors.push(`since:${chip.value} must be YYYY-MM-DD`);
        break;
      case "until":
        if (DATE.test(chip.value)) filter.date_to = chip.value;
        else errors.push(`until:${chip.value} must be YYYY-MM-DD`);
        break;
      case ">":
      case ">=":
      case "<":
      case "<=":
      case "=": {
        const cents = parseCentsInput(chip.value);
        if (cents === null) {
          errors.push(`${chip.raw} is not an amount`);
          break;
        }
        const abs = Math.abs(cents);
        if (chip.key === ">") filter.abs_gt_cents = abs;
        else if (chip.key === ">=") filter.abs_ge_cents = abs;
        else if (chip.key === "<") filter.abs_lt_cents = abs;
        else if (chip.key === "<=") filter.abs_le_cents = abs;
        else filter.abs_eq_cents = abs;
        break;
      }
      default:
        errors.push(`unknown chip ${chip.raw}`);
    }
  }
  if (text.length > 0) filter.text = text.join(" ");
  return { filter, errors };
}
