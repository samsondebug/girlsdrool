/** Every shortcut Kept answers to, by scope (ADR-0047). The overlay and the docs read this. */
export interface ShortcutRow {
  keys: string;
  does: string;
}

export const SHORTCUTS: { scope: string; rows: ShortcutRow[] }[] = [
  {
    scope: "Everywhere",
    rows: [
      { keys: "Ctrl K", does: "Command palette: go to a screen or run a command" },
      { keys: "F1 or ?", does: "This list" },
      { keys: "Esc", does: "Close a dialog, the palette or this list" },
      { keys: "Tab / Shift Tab", does: "Move between controls; every action is reachable" },
      { keys: "Enter / Space", does: "Press the focused button or link" },
      { keys: "Ctrl Shift L", does: "Lock Kept" },
    ],
  },
  {
    scope: "Ledger",
    rows: [
      { keys: "/", does: "Focus the query box (chips: account:, cat:, >100, needs:review …)" },
      { keys: "Enter", does: "Save an inline edit; Esc cancels it" },
    ],
  },
  {
    scope: "Palette",
    rows: [
      { keys: "↑ ↓", does: "Move the highlight" },
      { keys: "Enter", does: "Run the highlighted entry" },
    ],
  },
];
