# Changelog

Each milestone ends here with a demo note: what you can do now, what was verified in-session
(command and exit code), and what was not verified on this host.

## M0 — Scaffold, tokens, encrypted database (unreleased)

### Demo

1. `just dev` (or `pnpm tauri dev` with `KEPT_DATA_DIR` set) opens Kept at 1440×900.
2. First run: choose a data folder → `kept.config.json` appears beside the executable;
   `logs/`, `backups/`, `exports/` are created and `logs/kept.<date>.log` starts.
3. Create the database with a passphrase (optionally remembered in the credential store). The
   schema (migration v1, 35 tables + the `txn_leaf` view) and seeds (settings, category roots and
   system codes, two system policies, the generic CSV profile) are applied in one transaction.
4. The dashboard shell renders its nine panels at 1440×900 without scrolling; every panel is an
   empty state naming what is missing and which milestone fills it.
5. Settings: theme swap (dark/light tokens), IANA zone with validation, remember/forget
   passphrase, lock. Every settings change is a command group with an audit row.
6. Lock, then enter a wrong passphrase: SQLCipher rejects the key before anything is read and the
   file is unchanged.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 24 unit, 3 property (`money_sum_conserves`,
  `mul_div_round_is_within_half_a_unit`, `decimal_string_round_trips`), 1 logging, 9 acceptance
  (wrong passphrase fails closed and leaves the file byte-identical; empty DB migrates with
  seeds; tampered checksum refuses to open; newer schema refuses to open; backup round-trips
  under a new passphrase; settings updates validated and audited).
- `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test` (34 Vitest cases) — exit 0.
- `just check` — exit 0 with `check-core: green` followed by the explicit
  `E2E NOT RUN: requires Windows/WebView2` notice (fmt, clippy for both feature sets, all Rust
  suites, tsc, eslint, prettier, vitest, gates).
- `pnpm tauri build --debug --no-bundle` — exit 0; the app launched under Xvfb at 1440×900,
  created a database from typed input, and wrote `logs/kept.2026-10-05.log` with the migration
  and creation events. Screenshots: `docs/screenshots/m0-create-database.png`,
  `docs/screenshots/m0-dashboard.png`.

### Not verified on this host

- The Playwright critical path and the `netstat` no-network observation require Windows/WebView2
  (ADR-0009). The Windows CI job runs the E2E; the netstat observation is a manual step recorded
  here when first performed on a Windows machine.
