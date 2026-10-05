# Kept — Milestones

Build order is the milestone order; UI build order follows it. Every milestone ends with:
`just check` green (Windows run; ADR-0009), a conventional commit, a `CHANGELOG.md` demo note,
and a screenshot of the screen the milestone owns once the app can launch. Acceptance checks
are copied from the spec verbatim as the first checkbox group; the engineering checklist below
each is the tech lead's breakdown. "Questions to batch" are asked at that milestone's boundary
only, with the ADR default in force until answered. `main` is never left broken.

Legend: `[ ]` open · `[x]` done and verified in-session (command run, exit code seen).

---

## M0 — Scaffold, tokens, encrypted DB, migrations, keyring, CI, `just check`, CLAUDE.md

**Acceptance (spec)**

- [x] app launches (debug build launched under Xvfb at 1440×900; `docs/screenshots/m0-*.png`; Windows launch pending the first Windows run)
- [x] wrong passphrase fails closed (`tests/m0_acceptance.rs`: `WrongPassphrase`, file byte-identical)
- [x] empty DB migrates (`tests/m0_acceptance.rs`: v1 schema, 35 tables, seeds, integrity and FK checks)
- [x] log file created (`tests/logging.rs` and the Xvfb launch wrote `logs/kept.2026-10-05.log`)
- [x] no network (CSP `connect-src` limited to IPC; no http/shell/updater plugin; gate proves no network crate in the host or Windows dependency graph and no network API in the webview; the Windows `netstat` observation stays open below)

**Engineering checklist**

- [x] Tauri 2.x scaffold: `src-tauri` with `dialog` plugin only; `capabilities/default.json` minimal; CSP per ARCHITECTURE §12
- [x] `src/tokens.css` with the spec's tokens on `:root` and the light swap; Tailwind `@theme` mapping; Plex fonts self-hosted; type scale 12/14/16/20/28; 4px spacing
- [x] `money.rs` (`Cents`, `mul_div_round`, `allocate`, decimal string) with unit tests on exact halves and overflow
- [x] `dates.rs` (`today()` in zone, civil arithmetic helpers) with unit tests
- [x] `config.rs`: `kept.config.json` beside the exe, `KEPT_DATA_DIR` override, first-run folder picker
- [x] `db::open/unlock/lock`: SQLCipher key, `sqlite_master` probe, pragmas, `Mutex<Option<Db>>`
- [x] `db::migrate`: embedded `0001_init.sql` (schema + seeds from ARCHITECTURE §4), checksums, `pre_migration` backup, empty-DB test, integrity and FK checks
- [x] credential store remember/forget behind an opt-in toggle (`keyring-core` + platform store; unlock never depends on it)
- [x] `tracing` daily rotating file under `data_dir/logs`, 14 kept, redaction rule documented in code
- [x] `error.rs` with the full taxonomy; IPC `{ kind, message, detail }`; TS `AppError` type
- [x] Setup, Create-database and Unlock screens, Locked/Unlocked app shell, Settings (theme, zone, passphrase, lock), Dashboard shell with the empty-state copy rule
- [x] `justfile`: `check`, `check-core`, `fmt-check`, `clippy`, `test-rust`, `typecheck`, `lint`, `format-check`, `test-ts`, `gates`, `e2e` (Windows only) / `e2e-notice`, `pnpm check` alias
- [x] `.github/workflows/ci.yml` (ubuntu `check-core` + windows full `check` and unsigned build) and `release.yml` (tag → unsigned NSIS installer on a draft release); `docs/release.md` signing as a manual step
- [x] `CHANGELOG.md` created; `README.md` points at the four documents
- [x] ADR-0034 appended with the resolved dependency versions (plus ADR-0035 tokens, ADR-0036 network gate)
- [ ] Windows acceptance: `netstat` observation during launch recorded in the CHANGELOG note (needs a Windows machine; also the first run of `just e2e`)

**Questions to batch:** none (no money, formula, or trust decision in M0).

---

## M1 — Accounts + CSV import with dedup + ledger

**Acceptance (spec)**

- [x] fixture CSVs import (`tests/m1_import.rs`: 17 files, 104 rows, 21 monthly closings equal `EXPECTED.md`)
- [x] second import is a no-op (`tests/m1_import.rs` and the `import_idempotent` property)
- [x] ledger virtualizes (TanStack Virtual over keyset pages of 200; the 200k-row timing is M10's)
- [x] query chips work (`src/lib/query-chips.test.ts`; compiled to SQL in `db/repo/ledger.rs`)

**Engineering checklist**

- [x] `fixtures/EXPECTED.md` written FIRST by hand (`fixtures/generate.py` emits it from one explicit row list with running balances; the M4/M5 answers are appended at those milestones from the plan inputs it defines)
- [x] Fixture CSVs for the seven sources (ARCHITECTURE §14) matching `EXPECTED.md`
- [x] Accounts CRUD (kind, opening balance/date, owner, firewalled, archived) with audit rows
- [x] Import profiles: `spec_json` schema, five institution profiles (migration 0002) + `generic_csv`, header-signature auto-detect, mapping preview
- [x] CSV parser: integer amount parsing, civil dates, status, external id, currency guard (`Unsupported`), `Parse { row, column }`
- [x] `payee_norm` (ADR-0018) with table tests
- [x] Dedup: file-level idempotency, hash/external-id skip, fuzzy update, older-observation skip, quarantine (ADR-0017, ADR-0037); human-readable report
- [x] Batch undo with `Conflict` on later edits
- [x] Ledger screen: TanStack Table + Virtual, keyset pagination, inline edit (sets `user_edited` bits), multi-select recategorize, splits editor (`Validation` when children ≠ parent), saved views
- [x] Query chips parser (TS) + SQL compiler (Rust), both tested
- [x] Import screen: drop zone, profile pick, mapping preview, dedup report, commit, undo of that batch, quarantine resolution
- [x] Property tests: `money_sum_conserves`, `import_idempotent`, `dedup_no_cross_account_collapse`, `user_edit_survives_reimport`
- [x] Integration test: all fixture files import; re-import inserts zero rows; per-account closing balances equal `EXPECTED.md`

**Questions to batch:** none expected; if a fixture scenario forces a dedup-rule choice that changes which rows are trusted, it is written here and asked.

---

## M2 — Rules, review queue, transfer and refund linking

**Acceptance (spec)**

- [x] fixture transfer pairs link (`tests/m2_rules_links.rs`: the 11 pairs of `fixtures/automation.json` with their kinds, plus the Target refund)
- [x] spending view and cash view differ by the known amount in EXPECTED.md (gross spending 11,846.20 − cash outflows 15,799.94 = −3,953.74)

**Engineering checklist**

- [x] Rule engine (ordered, first match, `rule_id` stored), rule editor UI, reorder (`rules/mod.rs`; Rules screen with Apply rules now)
- [x] Heuristics with `heuristic_code` (ARCHITECTURE §6.4); cash withdrawals and payment-app rows never categorized
- [x] Review queue ordered by `|amount|`; corrections propose a rule, never create one (`propose_rule`; the toast's "Create rule" is the only way a proposal becomes a rule)
- [x] Transfer/card-payment/refund detection and link editor (ADR-0019); unlinked refunds in the queue (`refund_candidate`; Link… dialog on Ledger and Review)
- [x] Spending view and cash view queries (ADR-0020) exposed as commands with row ids (`spending_view` per category, `cash_view` per account; the strip above the review queue)
- [x] Property test: `transfer_not_spending`
- [x] Integration test: fixture pairs link; spending − cash difference equals `EXPECTED.md`

**Questions to batch:** none. One fixture correction is recorded instead (ADR-0039 §5): the generator counted loan repayments as spending; ARCHITECTURE §5.2 and the engine do not.

---

## M3 — Reconciliation + untrusted marking

**Acceptance (spec)**

- [x] a balanced fixture reconciles (`tests/m3_recon.rs`: the 21 monthly periods balance against the `EXPECTED.md` closings)
- [x] a mutated fixture shows the difference and marks the hero untrusted (`northbank_checking_2026-08_mutated.csv`: August off by −9.00, September rolls it forward; the hero names Northbank Checking)

**Engineering checklist**

- [x] Reconciliation periods: statement balance entry (user or file), roll-forward, identity, zero-tolerance status, cache refresh in the writing transaction (`cash/recon.rs`; `refresh_all` inside every write, ADR-0040)
- [x] Difference explorer: rows in period, ±5-day neighbors, pending, quarantine
- [x] Trust status per account and hero trust (ADR-0021); `Untrusted` component (dashed underline + label naming accounts) used everywhere a figure appears (`MarkedMoney` on the ledger Σ and the spending and cash views; the dashboard health panel)
- [x] Reconcile screen
- [x] Property test: `recon_identity`
- [x] Integration tests: balanced fixture → `balanced`; fixture with one row's amount mutated → `off` with the exact difference, hero untrusted naming the account

**Questions to batch (ADR-0021):** staleness window 45 days? does "stale" mark the hero untrusted? — asked at this boundary with both defaults in force (45 days; yes).

---

## M4 — Earmarks, obligations, income streams, safe-to-spend, dashboard

**Acceptance (spec)**

- [ ] hero matches EXPECTED.md
- [ ] drill-down terms sum

**Engineering checklist**

- [ ] Income streams (cycles, semimonthly days, weekend rule, confidence) and receipt matching
- [ ] Obligations (due rules, candidate → confirmed, payment matching) and auto-detected candidates from recurring rows
- [ ] Earmarks with derived remaining (`earmark_entry`), schedules, emergency reserve as earmark, timing buffer setting
- [ ] `cash::safe_to_spend` returning terms with row ids (ARCHITECTURE §5.4); excluded items listed with reasons
- [ ] Dashboard at 1440×900 without page scroll: hero + drill-down, next confirmed income, next 14 days of obligations, reconciliation health, debt total, informal-loan remaining, venture cap, firewall status (forecast sparkline slot wired in M5)
- [ ] Plan screen: earmarks, obligations, income streams, two reserves, policy list
- [ ] Property tests: `safe_terms_sum`, `firewall_excluded`, `borrowing_not_income`
- [ ] Integration test: hero for the fixture's stated as-of date, next pay date, and buffer equals `EXPECTED.md`

**Questions to batch (ADR-0022):** venture-owned accounts excluded from `available`; posted flagged proceeds inside the balance; emergency reserve as an earmark; 30-day window when no confirmed income exists.

---

## M5 — Forecast + scenarios

**Acceptance (spec)**

- [ ] lowest balance date matches the fixture
- [ ] downside toggle moves it in the expected direction
- [ ] tie-out test green

**Engineering checklist**

- [ ] Daily engine over 91 days; 30-day table and 13-week table; variable-spend model with override (ARCHITECTURE §5.6–5.7)
- [ ] Scenarios: downside (pay +7 days, no expected/rumored, surprise bill), plan overlay from `plan` snapshot
- [ ] Forecast screen: visx chart with lowest point marked, table, toggles; dashboard sparkline
- [ ] Property test: `forecast_ties` for every scenario
- [ ] Integration tests: lowest balance and date, first shortfall date and amount equal `EXPECTED.md`; downside moves the lowest point down and/or earlier

**Questions to batch (ADR-0023):** median of three 30-day buckets vs weekly; shortfall shown as overdraft date vs buffer breach.

---

## M6 — Debts, informal loans, avalanche vs snowball in interest cents

**Acceptance (spec)**

- [ ] a two-debt fixture matches a hand-computed schedule within one cent per period

**Engineering checklist**

- [ ] Debt model (linked/standalone, APR/promo, interest method, minimum rules, participation, custom order)
- [ ] Informal loans: counterparty, original, promised terms/date, schedule rows, repayment log, local-only note draft
- [ ] Amortization per debt; avalanche / snowball / custom; `informal_first`; comparison in interest cents and payoff dates; 12-month informal scenario with gap and achievable date
- [ ] Debts and loans screen
- [ ] Integration test: two-debt fixture schedule vs `EXPECTED.md` within one cent per period; informal repayments appear as transfers, never expenses

**Questions to batch:** none unless the fixture exposes an interest-convention ambiguity (then: monthly nominal vs actual/365 default).

---

## M7 — Ventures

**Acceptance (spec)**

- [ ] fixture SaaS and owner contribution land in the right buckets
- [ ] cap gauge matches

**Engineering checklist**

- [ ] Venture CRUD (status/verdict, cap, time budget, milestone, stop condition)
- [ ] Rollup buckets, operating cash flow, cap used, utilization, milestone countdown, stop-condition alert, venture spend share of take-home (ADR-0025)
- [ ] Ventures screen: cards, cap gauges, rollup, verdict, stop condition
- [ ] Integration test: fixture SaaS charge → `operating_expense` and cap used; owner contribution → bucket and cap; gauge value equals `EXPECTED.md`

**Questions to batch (ADR-0025):** personal-account venture expenses count toward the cap.

---

## M8 — Weekly review, snapshots, trends

**Acceptance (spec)**

- [ ] a review cannot be completed with fewer or more than three committed actions
- [ ] history persists

**Engineering checklist**

- [ ] Review state machine and steps: balances, unreviewed rows, obligations in 14 days, plan variance, debt and informal progress, venture cap, borrowing flags and firewall touches since last review
- [ ] Dependable surplus/deficit after debt service and irregular provisions, excluding borrowing and asset sales (ADR-0026), shown and stored
- [ ] Exactly three actions enforced in the completing transaction; actions editable before commit; history list
- [ ] Snapshots: daily-on-launch, on demand, `plan` at review completion; trends charts from snapshots only (ADR-0027)
- [ ] Review screen
- [ ] Integration tests: completion with 2 or 4 actions → `Validation`; completed review and actions persist across unlock; snapshot uniqueness per civil day

**Questions to batch (ADR-0026):** income base for the surplus (90 days vs three pay cycles).

---

## M9 — OFX/QFX, institution profiles, backup/restore roundtrip, audit-pack export

**Acceptance (spec)**

- [ ] restore into a temp data dir matches row counts and the hero number

**Engineering checklist**

- [ ] OFX/QFX parser (SGML and XML forms), `FITID` → `external_id`, bank-provided ledger balance → reconciliation statement source `file`
- [ ] Institution profile editor in Settings (create from a sample file, edit mapping, test against a file)
- [ ] Daily rotating backups on launch; manual backup; `pre_migration` and `pre_restore`; `backup_log`
- [ ] Restore flow: open backup with its passphrase → temp data dir → migrate → compare per-table row counts and hero → confirm swap (ADR-0012)
- [ ] Passphrase change via `rekey` with fresh backup; keyring update
- [ ] Full export (CSV per table + JSON) and the audit pack with its README (ARCHITECTURE §6.6)
- [ ] Integration test: backup → restore into temp dir → row counts per table and hero equal the source; restore under a new passphrase opens only with the new one

**Questions to batch:** none.

---

## M10 — Palette, shortcuts, empty states, 200k-row performance, installer

**Acceptance (spec)**

- [ ] critical Playwright path green on the installer build

**Engineering checklist**

- [ ] Ctrl+K palette (commands and navigation), F1 shortcut overlay, keyboard reachability pass, visible focus everywhere
- [ ] Undo for every destructive action with named toasts (audit of coverage)
- [ ] Empty states on every screen: what is missing and the command that fixes it
- [ ] 200k-row generated fixture (deterministic generator, numbers derived not invented); ledger scroll without jank; import time, page fetch, and hero recompute recorded with the machine spec in `docs/performance.md`
- [ ] Installer build (unsigned NSIS) from `release.yml`; WebView2 prerequisite documented; `docs/release.md` signing steps
- [ ] Playwright critical path (import → reconcile → safe-to-spend) against the installer build on Windows; dashboard no-scroll assertion at 1440×900
- [ ] Light theme verified as a token swap only

**Questions to batch:** none.

---

## Definition of done for v1 (spec)

- [ ] import a year of statements from every account in under five minutes
- [ ] see a reconciled ledger I trust
- [ ] read one safe-to-spend number with full drill-down
- [ ] see the first shortfall date and why
- [ ] run a weekly review in 15 minutes
- [ ] never wonder whether a figure is real
- [ ] informal loans and the brokerage firewall are first-class, not notes
