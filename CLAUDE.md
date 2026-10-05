# CLAUDE.md — Kept

Kept is a local-only, encrypted personal cash cockpit (Tauri 2 + Rust + SQLCipher; React 18
webview for display). One user (Dave, product owner). It answers: how much can I spend right now
without breaking a commitment I have already made? Read `ARCHITECTURE.md` before touching
engines, `DECISIONS.md` before changing anything it covers, `MILESTONES.md` for what is next.

## Instruction hierarchy

The product spec (summarized in `DECISIONS.md` ADR-0001..0008) wins over later messages unless
Dave says "product change:" and names the rule. Never weaken a non-negotiable to finish a
milestone; stop, write the conflict as an ADR, propose the smallest alternative.

## Run

- Prereqs: Rust (MSRV = Tauri 2.x's, currently 1.90), Node 22, pnpm, `just`, WebView2 (Windows).
- `pnpm install` once. `pnpm tauri dev` runs the app. `KEPT_DATA_DIR=<dir>` overrides the data
  folder (tests, scratch); otherwise `kept.config.json` beside the exe names it.
- Logs: `<data_dir>/logs/kept.YYYY-MM-DD.log` (daily rotation, 14 kept). `KEPT_LOG=debug` raises the level.

## Test and check

- `just check` — fmt, clippy `-D warnings`, cargo test, tsc, eslint, prettier, vitest, grep gates,
  then Playwright E2E **on Windows only**. On Linux/macOS it ends with `E2E NOT RUN`; report that
  as "green except E2E not runnable here", never as green.
- `cargo test` in `src-tauri/`; `pnpm vitest run`; `pnpm exec playwright test` (Windows, built app).
- Never claim a command passed unless you ran it in this session and saw the exit code.

## Where money math lives

- Only in `src-tauri/src/` — `money.rs` (Cents, `mul_div_round`, `allocate`), `cash/`,
  `forecast/`, `debt/`, `venture/`, `review/`, `recon/`. The webview never sums, nets, filters,
  or derives a money figure; `src/lib/money.ts` only formats integers the core returned.
- `i64` cents; APR/percent in basis points; `i128` intermediates; half away from zero via
  `mul_div_round`. No `f64` anywhere monetary. SQL: `SUM()` ok; `total()`, `AVG()`, `REAL` banned.
- Civil dates `YYYY-MM-DD` for ledger/plan dates; UTC RFC3339 for `*_at`; zone `America/Chicago`
  default; pay-cycle math on civil dates.

## Invariants (proptest names are exact; see `src-tauri/tests/invariants.rs`)

- `money_sum_conserves` — splitting and recombining rows never loses a cent.
- `import_idempotent` — same file twice: zero new rows, zero changed user fields.
- `dedup_no_cross_account_collapse` — same amount and payee on two accounts are not one event.
- `transfer_not_spending` — a linked pair nets to zero in spending view, right timing in cash view.
- `forecast_ties` — ending = opening + inflows − outflows for every scenario.
- `safe_terms_sum` — hero equals the sum of its returned terms.
- `firewall_excluded` — firewalled balance never appears inside `available`.
- `borrowing_not_income` — borrowing and securities sales never enter income or safe-to-spend.
- `user_edit_survives_reimport` — re-import never overwrites a user-edited field.
- `recon_identity` — for a reconciled fixture period the identity holds to the cent.

## Ledger rules that are easy to get wrong

- Aggregates read `txn_leaf` (split parents are never counted). Sign is the account's point of view.
- Transfers, card payments, loan repayments are not spending. Linked refunds net against the original.
- Borrowing and securities-sale proceeds are flagged, never income; pending ones never enter the hero.
- Cash withdrawals and payment-app rows stay `needs_review`; never guess their purpose.
- Imports never overwrite a `user_edited` field; suspected duplicates go to `import_quarantine`,
  never silently dropped or inserted.
- Reconciliation tolerance is zero. Unreconciled → untrusted marking (dashed underline + label).
- Every write command: one transaction, one `command` row, one `audit_event` per touched row,
  then emit `kept://changed`.

## Do not touch without an ADR

- `src/tokens.css` token values (spec-locked); `fixtures/EXPECTED.md` numbers (engine is wrong
  until the fixture is shown wrong, in an ADR); committed `src-tauri/migrations/*.sql` (add a new
  migration instead); existing entries in `DECISIONS.md` (append only); the safe-to-spend formula
  in `ARCHITECTURE.md` §5.4.

## Quality gates (every commit)

- No `TODO`, no commented-out code, no `any`, no `unwrap()`/`expect()` outside tests, no silent
  `catch`, no `println!`, no network APIs or crates (`just gates` enforces the greppable ones).
- Never invent a number in tests, fixtures, or UI copy. Fixtures have hand-computed answers first.
- Conventional commits (`feat(import): …`, `fix(cash): …`, `docs: …`, `test(forecast): …`).
- Each milestone ends with a commit, a CHANGELOG demo note, and a screenshot if the app launches.
- No emoji, mascots, gradients, or illustration empty states. Empty states say what is missing
  and the command that fixes it. Dashboard fits 1440×900 without page scroll.

## Working method

- Make engineering decisions and log them as ADRs; do not ask what a careful lead would decide.
- Ask Dave only at a milestone boundary, only about money, the safe-to-spend formula, or what data
  is trusted, and batch the questions (listed per milestone in `MILESTONES.md`).
- Default is offline. Never add a network call, plugin, or dependency that opens one.
- Windows 11 x64 is the only target; the E2E and installer run there. Linux is a dev convenience.
