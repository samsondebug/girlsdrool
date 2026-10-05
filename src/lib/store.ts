/**
 * UI state only (ADR-0008): which screen is open, transient notices, the ledger query text.
 * Ledger rows, balances and settings live in TanStack Query, never here.
 */
import { create } from "zustand";

import type { Theme } from "./ipc";

export type Screen = "dashboard" | "ledger" | "import" | "accounts" | "settings";

export interface Notice {
  id: number;
  tone: "info" | "warning" | "negative" | "positive";
  text: string;
  /** An undo the toast names (spec: toasts that name the undo). */
  undo?: { label: string; run: () => void };
}

interface UiState {
  screen: Screen;
  setScreen: (screen: Screen) => void;
  notices: Notice[];
  pushNotice: (notice: Omit<Notice, "id">) => void;
  dismissNotice: (id: number) => void;
  ledgerQuery: string;
  setLedgerQuery: (text: string) => void;
}

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
}));

/**
 * Apply a theme to the document. Dark is the default before unlock; the stored setting is
 * applied as soon as settings load (a light-theme user sees one dark frame at startup).
 */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset["theme"] = theme;
}
