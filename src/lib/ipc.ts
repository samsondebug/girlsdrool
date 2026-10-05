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
