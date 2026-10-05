/**
 * Typed wrappers over Tauri `invoke`. Shapes mirror the Rust `cmd` module; the Vitest shape
 * test keeps them honest. The webview never computes a money figure the core also computes.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
