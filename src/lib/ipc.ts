/**
 * Typed wrappers over Tauri `invoke`. Shapes mirror the Rust `cmd` module and the repo structs
 * it returns. The webview never computes a money figure the core also computes.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { LedgerFilter } from "./query-chips";

export type AppErrorKind =
  | "Locked"
  | "WrongPassphrase"
  | "Db"
  | "Migration"
  | "Io"
  | "Parse"
  | "Unsupported"
  | "Validation"
  | "Conflict"
  | "NotFound"
  | "PolicyBlocked"
  | "Overflow"
  | "Internal";

export interface AppErrorShape {
  kind: AppErrorKind;
  message: string;
  detail: Record<string, unknown> | null;
}

export class AppError extends Error {
  readonly kind: AppErrorKind;
  readonly detail: Record<string, unknown> | null;

  constructor(shape: AppErrorShape) {
    super(shape.message);
    this.name = "AppError";
    this.kind = shape.kind;
    this.detail = shape.detail;
  }

  /** The field a `Validation` error points at, if any. */
  get field(): string | null {
    const f = this.detail?.["field"];
    return typeof f === "string" ? f : null;
  }
}

function isErrorShape(value: unknown): value is AppErrorShape {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v["kind"] === "string" && typeof v["message"] === "string";
}

/** Anything thrown by `invoke` becomes an `AppError`; unknown shapes are `Internal`. */
export function toAppError(error: unknown): AppError {
  if (error instanceof AppError) return error;
  if (isErrorShape(error)) {
    return new AppError({
      kind: error.kind,
      message: error.message,
      detail: error.detail ?? null,
    });
  }
  const message = error instanceof Error ? error.message : String(error);
  return new AppError({ kind: "Internal", message, detail: null });
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw toAppError(error);
  }
}

// ---- app --------------------------------------------------------------------------------------

export type AppStateKind = "needs_data_dir" | "needs_database" | "locked" | "unlocked";

export interface AppStatus {
  state: AppStateKind;
  data_dir: string | null;
  remembered: boolean;
  version: string;
}

export type Theme = "dark" | "light";

export interface Settings {
  zone: string;
  timing_buffer_cents: number;
  recon_stale_after_days: number;
  dedup_similarity_bps: number;
  theme: Theme;
  backup_keep_daily: number;
}

export type SettingKey = keyof Settings;

/** One setting with a value of the right type for its key. */
export type SettingUpdate = { [K in SettingKey]: { key: K; value: Settings[K] } }[SettingKey];

// ---- accounts and categories --------------------------------------------------------------------

export type AccountKind =
  "checking" | "savings" | "credit" | "brokerage" | "loan" | "payment_app" | "cash" | "venture";

export const ACCOUNT_KINDS: AccountKind[] = [
  "checking",
  "savings",
  "credit",
  "brokerage",
  "loan",
  "payment_app",
  "cash",
  "venture",
];

export interface Account {
  id: number;
  name: string;
  institution: string;
  kind: AccountKind;
  currency: string;
  opening_balance_cents: number;
  opening_date: string;
  owner: "personal" | "venture";
  venture_id: number | null;
  firewalled: boolean;
  archived: boolean;
  recon_stale_after_days: number | null;
  created_at: string;
}

export interface NewAccount {
  name: string;
  institution: string;
  kind: AccountKind;
  opening_balance_cents: number;
  opening_date: string;
  venture_id?: number | null;
  firewalled: boolean;
}

export interface AccountPatch {
  name?: string;
  institution?: string;
  kind?: AccountKind;
  opening_balance_cents?: number;
  opening_date?: string;
  firewalled?: boolean;
  archived?: boolean;
  recon_stale_after_days?: number | null;
  /** A venture makes the account venture-owned; null hands it back to the person. */
  venture_id?: number | null;
}

export type RootKind =
  "fixed" | "variable" | "irregular" | "debt" | "income" | "transfer" | "venture";

export interface Category {
  id: number;
  parent_id: number | null;
  name: string;
  root_kind: RootKind;
  is_system: boolean;
  system_code: string | null;
  archived: boolean;
  sort_order: number;
  path: string;
}

export interface NewCategory {
  parent_id: number;
  name: string;
}

// ---- import -----------------------------------------------------------------------------------

export interface Profile {
  id: number;
  name: string;
  institution: string;
  format: "csv" | "ofx";
  is_system: boolean;
  spec: unknown;
}

export type ImportSource =
  { kind: "path"; path: string } | { kind: "text"; name: string; text: string };

export type RowStatus = "pending" | "posted";

export interface PreviewRow {
  row: number;
  posted_date: string;
  effective_date: string;
  amount_cents: number;
  payee_raw: string;
  payee_norm: string;
  memo: string;
  status: RowStatus;
  external_id: string | null;
  flags: string[];
  skipped: string | null;
}

export interface Preview {
  file_sha256: string;
  profile: Profile | null;
  candidates: { id: number; name: string; institution: string }[];
  header: string[];
  rows: PreviewRow[];
  total_rows: number;
  blank_rows: number;
  problem: { row: number; column: string; message: string } | null;
  already_imported_batch: number | null;
}

export interface ImportReport {
  batch_id: number;
  reason: string | null;
  rows_read: number;
  blank_rows: number;
  inserted: number[];
  updated: { txn_id: number; row: number; similarity_bps: number; fields: string[] }[];
  skipped: { row: number; matched_txn_id: number | null; by: string }[];
  quarantined: {
    row: number;
    quarantine_id: number;
    suspected_txn_id: number;
    similarity_bps: number;
  }[];
  threshold_bps: number;
  date_from: string | null;
  date_to: string | null;
  profile_name: string;
  summary: string;
  /** The last running balance the file carried, when its profile maps a balance column. */
  file_closing_cents: number | null;
  file_closing_date: string | null;
}

export interface ImportBatch {
  id: number;
  command_id: number;
  file_sha256: string;
  file_name: string;
  account_id: number;
  profile_id: number;
  date_from: string | null;
  date_to: string | null;
  rows_read: number;
  inserted: number;
  updated: number;
  skipped: number;
  quarantined: number;
  dedup_report_json: string;
  created_at: string;
  undone_at: string | null;
}

export interface QuarantineRow {
  id: number;
  import_batch_id: number;
  account_id: number;
  row_json: string;
  source_row_hash: string;
  suspected_txn_id: number | null;
  similarity_bps: number;
  reason: string;
  resolution: "pending" | "inserted" | "discarded";
  resolved_txn_id: number | null;
  resolved_at: string | null;
}

export type QuarantineAction = "insert" | "discard";

export interface UndoReport {
  batch_id: number;
  deleted: number;
  restored: number;
  unlinked: number;
  quarantine_discarded: number;
}

// ---- ledger -----------------------------------------------------------------------------------

export interface Cursor {
  posted_date: string;
  id: number;
}

export interface LedgerRow {
  id: number;
  account_id: number;
  account_name: string;
  parent_id: number | null;
  posted_date: string;
  effective_date: string;
  amount_cents: number;
  payee_raw: string;
  payee_norm: string;
  memo: string;
  category_id: number | null;
  category_path: string | null;
  classification: "manual" | "rule" | "heuristic" | "unclassified";
  rule_id: number | null;
  heuristic_code: string | null;
  user_edited: number;
  status: RowStatus;
  transfer_link_id: number | null;
  refund_link_id: number | null;
  venture_id: number | null;
  flags: number;
  import_batch_id: number | null;
  tags: string[];
}

export interface LedgerPage {
  rows: LedgerRow[];
  next_cursor: Cursor | null;
  total_rows: number;
  total_cents: number;
}

export interface TxnRecord {
  id: number;
  account_id: number;
  parent_id: number | null;
  posted_date: string;
  effective_date: string;
  amount_cents: number;
  payee_raw: string;
  payee_norm: string;
  memo: string;
  category_id: number | null;
  import_batch_id: number | null;
  source_row_hash: string | null;
  external_id: string | null;
  classification: "manual" | "rule" | "heuristic" | "unclassified";
  rule_id: number | null;
  heuristic_code: string | null;
  user_edited: number;
  status: RowStatus;
  transfer_link_id: number | null;
  refund_link_id: number | null;
  venture_id: number | null;
  flags: number;
  created_at: string;
  updated_at: string;
  tags: string[];
}

/** `null` clears a nullable field; an absent key leaves it alone. */
export interface TxnPatch {
  payee_norm?: string;
  memo?: string;
  category_id?: number | null;
  tags?: string[];
  venture_id?: number | null;
  flags?: number;
  effective_date?: string;
  status?: RowStatus;
}

export interface SplitPart {
  amount_cents: number;
  category_id?: number | null;
  memo: string;
}

export interface SavedView {
  id: number;
  name: string;
  query_text: string;
  created_at: string;
}

// ---- rules, links, review, views (M2) -----------------------------------------------------------

export interface Rule {
  id: number;
  position: number;
  name: string;
  enabled: boolean;
  match_payee_contains: string | null;
  match_payee_regex: string | null;
  match_memo_contains: string | null;
  match_amount_min_cents: number | null;
  match_amount_max_cents: number | null;
  match_account_id: number | null;
  action_category_id: number | null;
  action_venture_id: number | null;
  action_flags_set: number;
  action_tag_ids: number[];
  hit_count: number;
  created_at: string;
  updated_at: string;
}

/** A rule as entered; the core requires at least one match and one action. */
export interface RuleInput {
  name: string;
  enabled?: boolean;
  match_payee_contains?: string | null;
  match_payee_regex?: string | null;
  match_memo_contains?: string | null;
  match_amount_min_cents?: number | null;
  match_amount_max_cents?: number | null;
  match_account_id?: number | null;
  action_category_id?: number | null;
  action_venture_id?: number | null;
  action_flags?: string[];
  action_tag_ids?: number[];
}

export interface RuleProposal {
  name: string;
  match_payee_contains: string;
  action_category_id: number | null;
  action_venture_id: number | null;
  would_match: number;
}

export interface AutomationReport {
  considered: number;
  rule_hits: number;
  heuristic_hits: number;
  unclassified: number;
  changed: number;
  transfers_linked: number;
  refunds_linked: number;
  refund_candidates: number;
}

export type TransferKind =
  "internal" | "card_payment" | "loan_repayment" | "venture_contribution" | "venture_withdrawal";

export const TRANSFER_KINDS: TransferKind[] = [
  "internal",
  "card_payment",
  "loan_repayment",
  "venture_contribution",
  "venture_withdrawal",
];

export type LinkConfidence = "user" | "heuristic";

export interface TransferLink {
  id: number;
  out_txn_id: number;
  in_txn_id: number;
  kind: TransferKind;
  confidence: LinkConfidence;
  created_at: string;
}

export interface RefundLink {
  id: number;
  original_txn_id: number;
  refund_txn_id: number;
  confidence: LinkConfidence;
  created_at: string;
}

export interface LinkCandidate {
  txn_id: number;
  account_id: number;
  account_name: string;
  posted_date: string;
  amount_cents: number;
  payee_norm: string;
  days_apart: number;
}

export interface LinkCandidates {
  transfers: LinkCandidate[];
  refunds: { candidate: LinkCandidate; similarity_bps: number }[];
}

export interface LinkDetails {
  transfer: TransferLink | null;
  transfer_other: TxnRecord | null;
  refund: RefundLink | null;
  refund_other: TxnRecord | null;
}

export interface CategoryLine {
  category_id: number | null;
  path: string | null;
  root_kind: RootKind | null;
  outflows_cents: number;
  inflows_cents: number;
  net_cents: number;
  rows: number;
}

export interface SpendingView {
  from: string;
  to: string;
  gross_outflows_cents: number;
  linked_refunds_cents: number;
  reimbursements_cents: number;
  net_spending_cents: number;
  positive_review_cents: number;
  unclassified_outflows_cents: number;
  by_category: CategoryLine[];
}

export interface AccountLine {
  account_id: number;
  account_name: string;
  outflows_cents: number;
  inflows_cents: number;
  net_cents: number;
}

export interface CashView {
  from: string;
  to: string;
  outflows_cents: number;
  inflows_cents: number;
  net_cents: number;
  by_account: AccountLine[];
}

export type VentureStatus = "fund" | "freeze" | "kill";

export interface Venture {
  id: number;
  name: string;
  status: VentureStatus;
  cash_cap_cents: number;
  time_budget_hours: number | null;
  milestone: string;
  milestone_date: string | null;
  stop_condition: string;
  archived: boolean;
  created_at: string;
}

export interface VentureInput {
  name: string;
  status: VentureStatus;
  cash_cap_cents: number;
  time_budget_hours?: number | null;
  milestone?: string;
  milestone_date?: string | null;
  stop_condition?: string;
}

// ---- reconciliation and trust (M3) --------------------------------------------------------------

export type ReconStatus = "balanced" | "off";
export type StatementSource = "user" | "file";

export interface Reconciliation {
  id: number;
  account_id: number;
  period_start: string;
  period_end: string;
  opening_cents: number;
  statement_closing_cents: number;
  statement_source: StatementSource;
  computed_closing_cents: number;
  difference_cents: number;
  status: ReconStatus;
  balanced_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ReconInput {
  account_id: number;
  period_end: string;
  statement_closing_cents: number;
  statement_source: StatementSource;
}

export interface ExplorerRow extends LedgerRow {
  running_cents: number;
}

export interface DifferenceExplorer {
  reconciliation: Reconciliation;
  in_period: ExplorerRow[];
  before: LedgerRow[];
  after: LedgerRow[];
  pending: LedgerRow[];
  quarantine: QuarantineRow[];
  neighbour_days: number;
}

export type TrustStatus = "reconciled" | "never_reconciled" | "stale" | "off";

export interface AccountTrust {
  account_id: number;
  account_name: string;
  kind: AccountKind;
  contributes: boolean;
  status: TrustStatus;
  latest_period_end: string | null;
  difference_cents: number | null;
  days_since: number | null;
  stale_after_days: number;
  reason: string;
}

export interface HeroTrust {
  trusted: boolean;
  untrusted: { account_id: number; account_name: string; status: TrustStatus; reason: string }[];
}

export interface TrustReport {
  as_of: string;
  accounts: AccountTrust[];
  hero: HeroTrust;
}

// ---- plan and the hero (M4) -----------------------------------------------------------------------

export type IncomeKind = "base" | "bonus" | "rsu" | "deferred_comp" | "other";
export type Cycle = "weekly" | "biweekly" | "semimonthly" | "monthly" | "once";
export type Confidence = "confirmed" | "expected" | "rumored";
export type WeekendRule = "none" | "previous_business_day" | "next_business_day";

export interface IncomeStream {
  id: number;
  name: string;
  kind: IncomeKind;
  cycle: Cycle;
  anchor_date: string;
  semimonthly_day_1: number | null;
  semimonthly_day_2: number | null;
  expected_net_cents: number;
  variability_cents: number;
  confidence: Confidence;
  weekend_rule: WeekendRule;
  deposit_account_id: number | null;
  match_payee_contains: string | null;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface IncomeInput {
  name: string;
  kind: IncomeKind;
  cycle: Cycle;
  anchor_date: string;
  semimonthly_day_1?: number | null;
  semimonthly_day_2?: number | null;
  expected_net_cents: number;
  variability_cents?: number;
  confidence: Confidence;
  weekend_rule?: WeekendRule;
  deposit_account_id?: number | null;
  match_payee_contains?: string | null;
  active?: boolean;
}

export interface Receipt {
  income_stream_id: number;
  due_date: string;
  txn_id: number;
  matched_by: "user" | "heuristic";
}

export type ObligationKind = "bill" | "debt_minimum" | "other";
export type ObligationStatus = "candidate" | "confirmed" | "retired";
export type DueRule = "monthly_day" | "nth_weekday" | "biweekly" | "annual" | "once";

export interface Obligation {
  id: number;
  name: string;
  kind: ObligationKind;
  status: ObligationStatus;
  due_rule: DueRule;
  due_day: number | null;
  due_month: number | null;
  due_weekday: number | null;
  due_nth: number | null;
  anchor_date: string | null;
  expected_cents: number;
  variability_cents: number;
  source_account_id: number;
  autopay: boolean;
  category_id: number | null;
  debt_id: number | null;
  match_payee_contains: string | null;
  detected_from_json: string | null;
  created_at: string;
  updated_at: string;
}

export interface ObligationInput {
  name: string;
  kind?: ObligationKind;
  status?: ObligationStatus;
  due_rule: DueRule;
  due_day?: number | null;
  due_month?: number | null;
  due_weekday?: number | null;
  due_nth?: number | null;
  anchor_date?: string | null;
  expected_cents: number;
  variability_cents?: number;
  source_account_id: number;
  autopay?: boolean;
  category_id?: number | null;
  debt_id?: number | null;
  match_payee_contains?: string | null;
}

export interface Payment {
  obligation_id: number;
  due_date: string;
  txn_id: number;
  matched_by: "user" | "heuristic";
}

export type EarmarkKind = "obligation" | "sinking_fund" | "emergency_reserve";
export type EarmarkSchedule = "none" | "monthly" | "per_paycheck" | "by_date";
export type EntryKind = "fund" | "release" | "adjust";

export interface Earmark {
  id: number;
  name: string;
  kind: EarmarkKind;
  funding_account_id: number;
  obligation_id: number | null;
  target_cents: number;
  target_date: string | null;
  schedule: EarmarkSchedule;
  schedule_amount_cents: number | null;
  schedule_day: number | null;
  schedule_income_stream_id: number | null;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface EarmarkInput {
  name: string;
  kind: EarmarkKind;
  funding_account_id: number;
  obligation_id?: number | null;
  target_cents: number;
  target_date?: string | null;
  schedule?: EarmarkSchedule;
  schedule_amount_cents?: number | null;
  schedule_day?: number | null;
  schedule_income_stream_id?: number | null;
  active?: boolean;
}

export interface EarmarkEntry {
  id: number;
  earmark_id: number;
  entry_date: string;
  kind: EntryKind;
  amount_cents: number;
  txn_id: number | null;
  note: string;
  created_at: string;
}

export interface EntryInput {
  entry_date: string;
  kind: EntryKind;
  amount_cents: number;
  txn_id?: number | null;
  note?: string;
}

export interface Policy {
  id: number;
  code: string | null;
  name: string;
  kind: "firewall_exclusion" | "informal_first" | "reminder";
  params_json: string;
  is_system: boolean;
  created_at: string;
}

export interface AvailableAccount {
  account_id: number;
  account_name: string;
  posted_cents: number;
  pending_in_cents: number;
  pending_out_cents: number;
  pending_row_ids: number[];
}

export interface EarmarkItem {
  earmark_id: number;
  name: string;
  kind: EarmarkKind;
  funding_account_id: number;
  remaining_cents: number;
  counted_cents: number;
  entry_ids: number[];
}

export interface NextIncome {
  date: string;
  stream_id: number;
  stream_name: string;
  expected_net_cents: number;
  days_away: number;
}

export interface ObligationItem {
  obligation_id: number;
  name: string;
  due_date: string;
  expected_cents: number;
  earmark_covered_cents: number;
  counted_cents: number;
  overdue: boolean;
}

export interface ExcludedAccount {
  account_id: number;
  account_name: string;
  kind: AccountKind;
  posted_cents: number;
  reason: string;
}

export interface FlaggedInflow {
  txn_id: number;
  account_id: number;
  account_name: string;
  posted_date: string;
  cents: number;
  flags: string[];
  status: RowStatus;
}

export interface SafeToSpend {
  as_of: string;
  safe_cents: number;
  terms: {
    available: { cents: number; accounts: AvailableAccount[] };
    earmarks: { cents: number; items: EarmarkItem[] };
    obligations: {
      cents: number;
      next_income: NextIncome | null;
      window_end: string;
      window_reason: string | null;
      items: ObligationItem[];
    };
    buffer: { cents: number };
  };
  excluded: {
    firewalled_accounts: ExcludedAccount[];
    venture_accounts: ExcludedAccount[];
    pending_flagged_inflows: FlaggedInflow[];
    posted_flagged_inflows: FlaggedInflow[];
  };
  trust: TrustReport;
}

export interface UpcomingObligation {
  obligation_id: number;
  name: string;
  due_date: string;
  days_away: number;
  expected_cents: number;
  variability_cents: number;
  earmark_covered_cents: number;
  autopay: boolean;
  overdue: boolean;
  source_account_id: number;
  source_account_name: string;
}

export interface Upcoming {
  as_of: string;
  horizon_days: number;
  next_income: NextIncome | null;
  obligations: UpcomingObligation[];
}

export interface SurpriseBill {
  date: string;
  cents: number;
}

export interface Scenario {
  downside: boolean;
  surprise_bill: SurpriseBill | null;
}

export type ForecastEventKind = "pending" | "income" | "obligation" | "variable" | "surprise";

export interface ForecastEvent {
  kind: ForecastEventKind;
  name: string;
  cents: number;
  ref_id: number | null;
}

export interface ForecastDay {
  day: number;
  date: string;
  opening_cents: number;
  inflows_cents: number;
  outflows_cents: number;
  closing_cents: number;
  committed_cents: number;
  headroom_cents: number;
  events: ForecastEvent[];
}

export interface ForecastWeek {
  week: number;
  start: string;
  end: string;
  inflows_cents: number;
  outflows_cents: number;
  closing_cents: number;
  lowest_cents: number;
}

export interface ForecastPoint {
  date: string;
  cents: number;
}

export interface PayDate {
  stream_id: number;
  stream_name: string;
  date: string;
  shifted: boolean;
}

export interface PlanOverlay {
  snapshot_id: number;
  taken_at: string;
  civil_date: string;
  days: ForecastPoint[];
}

export interface VariableBucket {
  start: string;
  end: string;
  net_outflow_cents: number;
}

export interface CategoryModel {
  category_id: number;
  code: string | null;
  name: string;
  buckets: VariableBucket[];
  median_cents: number;
  override_cents: number | null;
  per_30_days_cents: number;
}

export interface Forecast {
  as_of: string;
  horizon_days: number;
  scenario: Scenario;
  opening_cents: number;
  inflows_cents: number;
  outflows_cents: number;
  closing_cents: number;
  lowest: ForecastPoint;
  first_shortfall: ForecastPoint | null;
  first_buffer_breach: ForecastPoint | null;
  days: ForecastDay[];
  weeks: ForecastWeek[];
  model: CategoryModel[];
  model_total_cents: number;
  pay_dates: PayDate[];
  plan: PlanOverlay | null;
  trust: TrustReport;
}

export type DebtKind = "credit_card" | "loan" | "informal";
export type InterestMethod = "monthly_nominal" | "actual_365";
export type MinimumRule =
  "fixed" | "percent_of_balance" | "interest_plus_percent" | "full_balance" | "none";
export type Strategy = "avalanche" | "snowball" | "custom";

export interface Debt {
  id: number;
  name: string;
  kind: DebtKind;
  account_id: number | null;
  apr_bps: number;
  promo_apr_bps: number | null;
  promo_end: string | null;
  interest_method: InterestMethod;
  minimum_rule: MinimumRule;
  minimum_fixed_cents: number;
  minimum_bps: number;
  minimum_floor_cents: number;
  due_day: number | null;
  strategy_participation: boolean;
  custom_order: number | null;
  standalone_opening_cents: number | null;
  standalone_opening_date: string | null;
  active: boolean;
  payment_account_id: number | null;
  match_payee_contains: string | null;
  created_at: string;
  updated_at: string;
}

export interface DebtInput {
  name: string;
  kind: DebtKind;
  account_id?: number | null;
  apr_bps?: number;
  promo_apr_bps?: number | null;
  promo_end?: string | null;
  interest_method?: InterestMethod;
  minimum_rule: MinimumRule;
  minimum_fixed_cents?: number;
  minimum_bps?: number;
  minimum_floor_cents?: number;
  due_day?: number | null;
  strategy_participation?: boolean;
  custom_order?: number | null;
  standalone_opening_cents?: number | null;
  standalone_opening_date?: string | null;
  active?: boolean;
  payment_account_id?: number | null;
  match_payee_contains?: string | null;
}

export interface DebtView extends Debt {
  account_name: string | null;
  payment_account_name: string | null;
  owed_cents: number;
  paid_cents: number;
  effective_apr_bps: number;
  next_period_start: string;
  next_period_end: string;
  next_interest_cents: number;
  next_minimum_cents: number;
  obligation_id: number | null;
}

export interface DebtPayment {
  id: number;
  debt_id: number;
  paid_date: string;
  amount_cents: number;
  txn_id: number | null;
  note: string;
  created_at: string;
}

export interface DebtPaymentInput {
  paid_date: string;
  amount_cents: number;
  txn_id?: number | null;
  note?: string;
}

/** `[txn_id, posted_date, payee, amount_cents]`. */
export type PaymentCandidate = [number, string, string, number];

export interface ScheduleRow {
  id: number;
  debt_id: number;
  due_date: string;
  amount_cents: number;
  unpaid_cents: number;
}

export interface InformalLoan {
  debt_id: number;
  name: string;
  counterparty: string;
  original_cents: number;
  borrowed_date: string;
  promised_terms: string;
  promised_date: string | null;
  proceeds_txn_id: number | null;
  note_draft: string;
  strategy_participation: boolean;
  active: boolean;
  payment_account_id: number | null;
  match_payee_contains: string | null;
  schedule: ScheduleRow[];
  repayments: DebtPayment[];
  repaid_cents: number;
  remaining_cents: number;
}

export interface InformalInput {
  counterparty: string;
  original_cents: number;
  borrowed_date: string;
  promised_terms?: string;
  promised_date?: string | null;
  proceeds_txn_id?: number | null;
  payment_account_id?: number | null;
  match_payee_contains?: string | null;
  strategy_participation?: boolean;
  active?: boolean;
}

export interface PeriodRow {
  period: number;
  start: string;
  end: string;
  opening_cents: number;
  interest_cents: number;
  minimum_cents: number;
  payment_cents: number;
  closing_cents: number;
}

export interface DebtSchedule {
  debt_id: number;
  name: string;
  kind: DebtKind;
  informal: boolean;
  owed_cents: number;
  total_interest_cents: number;
  total_paid_cents: number;
  payoff_date: string | null;
  periods: PeriodRow[];
}

export interface StrategyRun {
  strategy: Strategy;
  extra_cents: number;
  budget_cents: number;
  total_interest_cents: number;
  payoff_date: string | null;
  unfinished: boolean;
  debts: DebtSchedule[];
}

export interface InformalScenario {
  extra_cents: number;
  periods: number;
  remaining_cents: number;
  achievable: boolean;
  gap_cents: number;
  payoff_date: string | null;
}

export interface DebtTotals {
  as_of: string;
  total_debt_cents: number;
  informal_remaining_cents: number;
  open_informal: number;
  debts: number;
}

export interface DebtComparison {
  as_of: string;
  extra_cents: number;
  extra_source: "user" | "none";
  informal_first: boolean;
  strategies: StrategyRun[];
  scenario: InformalScenario;
}

export interface VentureBucket {
  cents: number;
  rows: number;
}

export interface VentureAccount {
  account_id: number;
  name: string;
  kind: AccountKind;
  balance_cents: number;
}

export interface VentureRollup extends Venture {
  as_of: string;
  window_start: string;
  customer_revenue: VentureBucket;
  operating_expense: VentureBucket;
  owner_contribution: VentureBucket;
  financing: VentureBucket;
  withdrawal: VentureBucket;
  operating_expense_from_personal_cents: number;
  operating_cash_flow_cents: number;
  cap_used_cents: number;
  cap_remaining_cents: number;
  cap_utilization_bps: number;
  milestone_days: number | null;
  alerts: string[];
  accounts: VentureAccount[];
}

export interface VentureSummary {
  as_of: string;
  window_start: string;
  ventures: VentureRollup[];
  total_cap_cents: number;
  total_cap_used_cents: number;
  total_operating_expense_cents: number;
  take_home_cents: number;
  spend_share_bps: number;
}

export const api = {
  appStatus: () => call<AppStatus>("app_status"),
  chooseDataDir: (path: string) => call<AppStatus>("choose_data_dir", { path }),
  createDatabase: (passphrase: string, confirm: string, remember: boolean) =>
    call<AppStatus>("create_database", { passphrase, confirm, remember }),
  unlock: (passphrase: string, remember: boolean) =>
    call<AppStatus>("unlock", { passphrase, remember }),
  unlockRemembered: () => call<AppStatus>("unlock_remembered"),
  lock: () => call<AppStatus>("lock"),
  rememberPassphrase: () => call<AppStatus>("remember_passphrase"),
  forgetRemembered: () => call<AppStatus>("forget_remembered"),
  getSettings: () => call<Settings>("get_settings"),
  updateSetting: (update: SettingUpdate) =>
    call<Settings>("update_setting", { key: update.key, value: update.value }),

  listAccounts: () => call<Account[]>("list_accounts"),
  createAccount: (account: NewAccount) => call<Account>("create_account", { new: account }),
  updateAccount: (id: number, patch: AccountPatch) =>
    call<Account>("update_account", { id, patch }),
  listCategories: () => call<Category[]>("list_categories"),
  createCategory: (category: NewCategory) => call<Category>("create_category", { new: category }),
  renameCategory: (id: number, name: string) => call<Category>("rename_category", { id, name }),
  archiveCategory: (id: number, archived: boolean) =>
    call<Category>("archive_category", { id, archived }),

  listImportProfiles: () => call<Profile[]>("list_import_profiles"),
  importPreview: (accountId: number, profileId: number | null, source: ImportSource) =>
    call<Preview>("import_preview", { accountId, profileId, source }),
  importCommit: (accountId: number, profileId: number | null, source: ImportSource) =>
    call<ImportReport>("import_commit", { accountId, profileId, source }),
  undoImportBatch: (batchId: number) => call<UndoReport>("undo_import_batch", { batchId }),
  listImportBatches: (limit?: number) =>
    call<ImportBatch[]>("list_import_batches", { limit: limit ?? null }),
  listQuarantine: () => call<QuarantineRow[]>("list_quarantine"),
  resolveQuarantine: (id: number, action: QuarantineAction) =>
    call<TxnRecord | null>("resolve_quarantine", { id, action }),

  ledgerQuery: (filter: LedgerFilter, cursor: Cursor | null, limit?: number) =>
    call<LedgerPage>("ledger_query", { filter, cursor, limit: limit ?? null }),
  ledgerChildren: (parentId: number) => call<TxnRecord[]>("ledger_children", { parentId }),
  getTxn: (id: number) => call<TxnRecord>("get_txn", { id }),
  updateTxn: (id: number, patch: TxnPatch) => call<TxnRecord>("update_txn", { id, patch }),
  recategorize: (ids: number[], categoryId: number | null) =>
    call<number>("recategorize", { ids, categoryId }),
  splitTxn: (parentId: number, parts: SplitPart[]) =>
    call<TxnRecord[]>("split_txn", { parentId, parts }),
  unsplitTxn: (parentId: number) => call<number>("unsplit_txn", { parentId }),
  listSavedViews: () => call<SavedView[]>("list_saved_views"),
  saveView: (name: string, queryText: string) => call<SavedView>("save_view", { name, queryText }),
  deleteSavedView: (id: number) => call<null>("delete_saved_view", { id }),

  listRules: () => call<Rule[]>("list_rules"),
  createRule: (input: RuleInput) => call<Rule>("create_rule", { input }),
  updateRule: (id: number, input: RuleInput) => call<Rule>("update_rule", { id, input }),
  deleteRule: (id: number) => call<null>("delete_rule", { id }),
  reorderRules: (ids: number[]) => call<Rule[]>("reorder_rules", { ids }),
  applyRules: () => call<AutomationReport>("apply_rules"),
  proposeRule: (txnId: number) => call<RuleProposal>("propose_rule", { txnId }),

  linkCandidates: (txnId: number) => call<LinkCandidates>("link_candidates", { txnId }),
  linkDetails: (txnId: number) => call<LinkDetails>("link_details", { txnId }),
  linkTransfer: (outTxnId: number, inTxnId: number, kind: TransferKind | null) =>
    call<TransferLink>("link_transfer", { outTxnId, inTxnId, kind }),
  unlinkTransfer: (linkId: number) => call<null>("unlink_transfer", { linkId }),
  linkRefund: (originalTxnId: number, refundTxnId: number) =>
    call<RefundLink>("link_refund", { originalTxnId, refundTxnId }),
  unlinkRefund: (linkId: number) => call<null>("unlink_refund", { linkId }),
  acknowledgeFirewall: (txnId: number, note: string) =>
    call<TxnRecord>("acknowledge_firewall", { txnId, note }),

  reviewQueue: (limit?: number) => call<LedgerRow[]>("review_queue", { limit: limit ?? null }),
  spendingView: (from: string, to: string) => call<SpendingView>("spending_view", { from, to }),
  cashView: (from: string, to: string) => call<CashView>("cash_view", { from, to }),

  listVentures: () => call<Venture[]>("list_ventures"),
  createVenture: (input: VentureInput) => call<Venture>("create_venture", { input }),
  updateVenture: (id: number, input: VentureInput) =>
    call<Venture>("update_venture", { id, input }),

  listReconciliations: (accountId: number) =>
    call<Reconciliation[]>("list_reconciliations", { accountId }),
  reconcile: (input: ReconInput) => call<Reconciliation>("reconcile", { input }),
  deleteReconciliation: (id: number) => call<null>("delete_reconciliation", { id }),
  differenceExplorer: (id: number) => call<DifferenceExplorer>("difference_explorer", { id }),
  trustStatus: () => call<TrustReport>("trust_status"),

  listIncomeStreams: () => call<IncomeStream[]>("list_income_streams"),
  createIncomeStream: (input: IncomeInput) => call<IncomeStream>("create_income_stream", { input }),
  updateIncomeStream: (id: number, input: IncomeInput) =>
    call<IncomeStream>("update_income_stream", { id, input }),
  listReceipts: (streamId: number) => call<Receipt[]>("list_receipts", { streamId }),
  recordReceipt: (streamId: number, dueDate: string, txnId: number) =>
    call<Receipt>("record_receipt", { streamId, dueDate, txnId }),
  removeReceipt: (streamId: number, dueDate: string) =>
    call<null>("remove_receipt", { streamId, dueDate }),
  listObligations: () => call<Obligation[]>("list_obligations"),
  createObligation: (input: ObligationInput) => call<Obligation>("create_obligation", { input }),
  updateObligation: (id: number, input: ObligationInput) =>
    call<Obligation>("update_obligation", { id, input }),
  setObligationStatus: (id: number, status: ObligationStatus) =>
    call<Obligation>("set_obligation_status", { id, status }),
  deleteObligationCandidate: (id: number) => call<null>("delete_obligation_candidate", { id }),
  detectObligationCandidates: () => call<Obligation[]>("detect_obligation_candidates"),
  listPayments: (obligationId: number) => call<Payment[]>("list_payments", { obligationId }),
  recordPayment: (obligationId: number, dueDate: string, txnId: number) =>
    call<Payment>("record_payment", { obligationId, dueDate, txnId }),
  removePayment: (obligationId: number, dueDate: string) =>
    call<null>("remove_payment", { obligationId, dueDate }),
  listEarmarks: () => call<Earmark[]>("list_earmarks"),
  createEarmark: (input: EarmarkInput) => call<Earmark>("create_earmark", { input }),
  updateEarmark: (id: number, input: EarmarkInput) =>
    call<Earmark>("update_earmark", { id, input }),
  listEarmarkEntries: (earmarkId: number) =>
    call<EarmarkEntry[]>("list_earmark_entries", { earmarkId }),
  addEarmarkEntry: (earmarkId: number, input: EntryInput) =>
    call<EarmarkEntry>("add_earmark_entry", { earmarkId, input }),
  deleteEarmarkEntry: (id: number) => call<null>("delete_earmark_entry", { id }),
  nextOccurrences: (kind: "income" | "obligation", id: number, count?: number) =>
    call<string[]>("next_occurrences", { kind, id, count: count ?? null }),
  listPolicies: () => call<Policy[]>("list_policies"),
  safeToSpend: () => call<SafeToSpend>("safe_to_spend"),
  upcoming: (days?: number) => call<Upcoming>("upcoming", { days: days ?? null }),

  forecast: (scenario: Scenario) => call<Forecast>("forecast", { scenario }),
  variableSpendModel: () => call<CategoryModel[]>("variable_spend_model"),
  setVariableSpendOverride: (categoryId: number, per30DaysCents: number | null) =>
    call<CategoryModel[]>("set_variable_spend_override", { categoryId, per30DaysCents }),
  saveForecastPlan: () => call<PlanOverlay>("save_forecast_plan"),

  listDebts: () => call<DebtView[]>("list_debts"),
  createDebt: (input: DebtInput) => call<Debt>("create_debt", { input }),
  updateDebt: (id: number, input: DebtInput) => call<Debt>("update_debt", { id, input }),
  listDebtPayments: (debtId: number) => call<DebtPayment[]>("list_debt_payments", { debtId }),
  recordDebtPayment: (debtId: number, input: DebtPaymentInput) =>
    call<DebtPayment>("record_debt_payment", { debtId, input }),
  removeDebtPayment: (id: number) => call<null>("remove_debt_payment", { id }),
  debtPaymentCandidates: (debtId: number) =>
    call<PaymentCandidate[]>("debt_payment_candidates", { debtId }),
  listInformalLoans: () => call<InformalLoan[]>("list_informal_loans"),
  createInformalLoan: (input: InformalInput) =>
    call<InformalLoan>("create_informal_loan", { input }),
  updateInformalLoan: (debtId: number, input: InformalInput) =>
    call<InformalLoan>("update_informal_loan", { debtId, input }),
  setInformalNote: (debtId: number, note: string) =>
    call<null>("set_informal_note", { debtId, note }),
  addInformalScheduleRow: (debtId: number, dueDate: string, amountCents: number) =>
    call<number>("add_informal_schedule_row", { debtId, dueDate, amountCents }),
  deleteInformalScheduleRow: (id: number) => call<null>("delete_informal_schedule_row", { id }),
  debtComparison: (extraCents: number | null) =>
    call<DebtComparison>("debt_comparison", { extraCents }),
  debtTotals: () => call<DebtTotals>("debt_totals"),
  ventureSummary: () => call<VentureSummary>("venture_summary"),
};

export const CHANGED_EVENT = "kept://changed";

export interface ChangedPayload {
  entities: string[];
}

/** Subscribe to the core's change notifications. Resolves to the unsubscribe function. */
export function onChanged(handler: (entities: string[]) => void): Promise<UnlistenFn> {
  return listen<ChangedPayload>(CHANGED_EVENT, (event) => {
    handler(event.payload.entities);
  });
}
