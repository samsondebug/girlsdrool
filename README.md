# Kept

A local-only, encrypted cash cockpit for one user. It answers one question every day: how much
can I spend right now without breaking a commitment I have already made?

Tauri 2 · Rust core · SQLite with SQLCipher · React 18 webview (display only). Windows 11 x64.
Offline by default; no cloud, no account, no telemetry.

Planning documents (read in this order):

1. `ARCHITECTURE.md` — module map, import → hero data flow, schema, safe-to-spend formula, forecast, threat model
2. `DECISIONS.md` — ADR log (append-only)
3. `MILESTONES.md` — M0–M10 with acceptance checks
4. `CLAUDE.md` — how to run, how to test, where money math lives, invariants, do-not-touch files

Status: planning approved pending product-owner review of `ARCHITECTURE.md`. No application
code yet.
