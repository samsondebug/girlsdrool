# Kept

A local-only, encrypted cash cockpit for one user. It answers one question every day: how much
can I spend right now without breaking a commitment I have already made?

Tauri 2 · Rust core · SQLite with SQLCipher · React 18 webview (display only). Windows 11 x64.
Offline by default; no cloud, no account, no telemetry.

## What it does

- Imports bank, card, brokerage and payment-app exports (CSV through editable institution
  profiles, OFX/QFX through the standard's own fields), never twice, never over an edit.
- Keeps a ledger you can trust: zero-tolerance reconciliation per statement period, transfers
  and refunds linked so they are never spending, every untrusted figure marked and named.
- Shows one safe-to-spend number with its terms and the rows behind each, the next income, the
  next fourteen days, and a 91-day forecast with the first shortfall and why.
- Plans income, obligations, earmarks and reserves; tracks debts (avalanche, snowball, custom),
  informal loans with schedules, and ventures against a cash cap.
- Walks a weekly review that ends in exactly three actions; snapshots and trends over time.
- Backs itself up (verified, encrypted), restores by comparison, exports plaintext audit packs
  only where you ask.

## Run and check

- Prerequisites: Rust (Tauri 2's MSRV), Node 22, pnpm, `just`, WebView2 (Windows).
- `pnpm install`, then `pnpm tauri dev` (or `just dev` against a scratch data folder).
- `just check` runs everything: rustfmt, clippy, the Rust suite (fixtures with hand-computed
  answers, property invariants, integration tests), tsc, eslint, prettier, vitest, the greppable
  gates, and on Windows the Playwright critical path against the built app.
- `KEPT_DATA_DIR=<dir>` overrides the data folder; `KEPT_LOG=debug` raises the log level.

## Documents

1. `ARCHITECTURE.md` — module map, import → hero data flow, schema, every derived figure,
   engines, IPC, encryption, threat model, testing.
2. `DECISIONS.md` — the ADR log, append-only (ADR-0001..0047).
3. `MILESTONES.md` — M0–M10 with their acceptance checks, and the v1 definition of done.
4. `CLAUDE.md` — how to run and test, where money math lives, the invariants, do-not-touch files.
5. `fixtures/EXPECTED.md` — the fixture's known answers, written before the engines.
6. `docs/data-folder.md` — the portable layout, backups, restore, the passphrase.
7. `docs/undo.md` — every destructive action and its undo.
8. `docs/performance.md` — the 200k-row run and what it means for the UI.
9. `docs/release.md` — the unsigned installer and the manual signing step.
10. `docs/screenshots/` — one or two screenshots per milestone.

## Keyboard

`Ctrl K` palette (screens and commands), `F1` or `?` shortcuts, `Ctrl Shift L` lock, `/` the
Ledger query box, `Esc` closes anything; every action is reachable with Tab.
