/**
 * UI state only (ADR-0008): which screen is open, transient notices, the ledger query text.
 * Ledger rows, balances and settings live in TanStack Query, never here.
 */
import { create } from "zustand";

import type { Scenario, Theme } from "./ipc";

export type Screen =
  | "dashboard"
  | "ledger"
  | "review"
  | "rules"
  | "reconcile"
  | "plan"
  | "forecast"
  | "import"
  | "accounts"
  | "settings";

/** A statement balance handed from an import report to the Reconcile screen. */
export interface ReconcileDraft {
  accountId: number;
  periodEnd: string;
  statementClosingCents: number;
}

export interface Notice {
  id: number;
  tone: "info" | "warning" | "negative" | "positive";
  text: string;
  /** An undo the toast names (spec: toasts that name the undo). */
  undo?: { label: string; run: () => void };
  /** A follow-up the toast offers, such as creating the rule a correction proposed. */
  action?: { label: string; run: () => void };
}

interface UiState {
  screen: Screen;
  setScreen: (screen: Screen) => void;
  notices: Notice[];
  pushNotice: (notice: Omit<Notice, "id">) => void;
  dismissNotice: (id: number) => void;
  ledgerQuery: string;
  setLedgerQuery: (text: string) => void;
  reconcileDraft: ReconcileDraft | null;
  setReconcileDraft: (draft: ReconcileDraft | null) => void;
  /** The scenario the Forecast screen shows; the dashboard always shows the baseline. */
  forecastScenario: Scenario;
  setForecastScenario: (scenario: Scenario) => void;
}

export const BASELINE: Scenario = { downside: false, surprise_bill: null };

let nextNoticeId = 1;

export const useUiStore = create<UiState>((set) => ({
  screen: "dashboard",
  setScreen: (screen) => {
    set({ screen });
  },
  notices: [],
  pushNotice: (notice) => {
    const id = nextNoticeId++;
    set((state) => ({ notices: [...state.notices, { ...notice, id }] }));
  },
  dismissNotice: (id) => {
    set((state) => ({ notices: state.notices.filter((n) => n.id !== id) }));
  },
  ledgerQuery: "",
  setLedgerQuery: (ledgerQuery) => {
    set({ ledgerQuery });
  },
  reconcileDraft: null,
  setReconcileDraft: (reconcileDraft) => {
    set({ reconcileDraft });
  },
  forecastScenario: BASELINE,
  setForecastScenario: (forecastScenario) => {
    set({ forecastScenario });
  },
}));

/**
 * Apply a theme to the document. Dark is the default before unlock; the stored setting is
 * applied as soon as settings load (a light-theme user sees one dark frame at startup).
 */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset["theme"] = theme;
}
