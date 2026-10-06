# Kept — Decisions (ADR log, append-only)

Rules of this file: entries are appended, never edited or deleted. To change a decision, add a
new ADR that supersedes the old one and mark the old one `Superseded by ADR-NNNN`. Product
decisions come from Dave's spec and are marked **Source: product spec**; they are recorded so
they are not relitigated. Engineering decisions are marked **Source: tech lead**. An entry
marked **Question batched: Mn boundary** has a default in force until Dave answers at that
milestone boundary.

Format: `## ADR-NNNN — Title` · Status · Date · Source · Context · Decision · Consequences.

---

## ADR-0001 — Product frame, scope, and non-negotiables

Status: Accepted · Date: 2026-10-05 · Source: product spec

**Context.** Kept is a cockpit instrument for one user who runs out of cash between paychecks
because of timing, fixed commitments, informal debt, venture leakage, and asset sales used as a
bridge. It answers: how much can I spend right now without breaking a commitment already made?

**Decision.** Working title Kept, not renamed. Analyze and record only: the app never moves
money, places trades, contacts anyone, or opens a network connection except an explicit user
action; default offline; no cloud, account, telemetry, analytics, or crash phone-home. Out of
scope and not stubbed: bank-aggregation APIs, multi-user, mobile, cloud sync,
investment-performance analytics, tax filing, an LLM inside the app (an LLM-facing audit-pack
export is in scope). Non-negotiables may not be weakened to make a milestone easier; an
infeasible one stops the milestone and is written here with the smallest alternative.

**Consequences.** Every later ADR is subordinate to this one. "Product change:" from Dave,
naming the rule, is the only way a non-negotiable moves.

## ADR-0002 — Stack: Tauri 2.x, Rust core, rusqlite + SQLCipher

Status: Accepted · Date: 2026-10-05 · Source: product spec (stack) + tech lead (versions)

**Context.** Money math must not live in the webview; the database must be unreadable without a
key; the only target is Windows 11 x64.

**Decision.** Tauri 2.x (resolved today: `tauri` 2.12.1, MSRV 1.90; a 3.0 alpha exists and is
not used). Rust core. `rusqlite` 0.40 with `bundled-sqlcipher-vendored-openssl` (feature name
verified against crates.io today). `@tauri-apps/plugin-sql` is not used as the data layer: it
does not ship SQLCipher and would put money math in the webview. `keyring` 4.x with the
`windows-native-keyring-store` feature for the optional remembered passphrase. `chrono` +
`chrono-tz`, `serde`, `thiserror`, `tracing` + `tracing-appender`, `proptest`, `strsim`, `sha2`,
`csv`, `zeroize`. Exact versions are pinned in `Cargo.toml`/`Cargo.lock` at M0 and the resolved
set is appended here as ADR-0034 when M0 lands.

**Consequences.** MSRV follows Tauri's current minor; no ancient toolchain. Tauri plugins
registered: `dialog` only (file/folder pickers). No `http`, `shell`, `updater`, `sql`.

## ADR-0003 — Money representation

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (mechanics)

**Decision.** Money is `i64` cents (`Cents` newtype, SQLite `INTEGER`). No `f64` on any
monetary path. Percentages and APR are basis-point integers (`apr_bps: i32`, 1999 = 19.99%).
One rounding primitive, `money::mul_div_round(a, b, d)` with `i128` intermediates, rounds half
away from zero; interest, pro-rata, and percent-of-balance all use it. `money::allocate` splits
a total by largest remainder so parts always sum to the total. Checked arithmetic returns
`AppError::Overflow`; it never wraps or panics. SQL may use `SUM()` on integers (exact, raises
on overflow); `total()`, `AVG()`, division, and `REAL` on money columns are banned; medians are
computed in Rust. Decimal strings exist only in `src/lib/money.ts` (display) and `export`.

**Consequences.** A clippy/grep gate fails the build on `f64`/`f32` in `src-tauri/src`. Tests
pin the rounding behavior on exact half cases (positive and negative).

## ADR-0004 — Dates and time zone

Status: Accepted · Date: 2026-10-05 · Source: product spec

**Decision.** Civil dates `YYYY-MM-DD` for posted/effective/due/period dates; UTC RFC3339 for
instants. User zone `America/Chicago` unless Settings says otherwise. Pay-cycle and due-rule
math is civil-date arithmetic; `today()` is the civil date now in the user's zone.

## ADR-0005 — Ledger discipline

Status: Accepted · Date: 2026-10-05 · Source: product spec

**Decision.** Statements and exports are the primary record; notes never create transactions.
The ledger is append-only in spirit: imports never overwrite a user-edited field (per-field
`user_edited` bitmask); re-imports update system fields only when the incoming row is a better
observation of the same event; nothing is silently dropped or duplicated; every mutation writes
an audit row in the same transaction. A number on screen that cannot be traced to source rows
is a bug. Unreconciled figures are visibly untrusted everywhere, including the hero.

## ADR-0006 — Offline by default, and how it is enforced

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (enforcement)

**Decision.** No network calls on startup or at any time without an explicit user action named
by the spec. Enforcement: no HTTP/updater/shell Tauri plugins; CSP `connect-src` limited to the
IPC origin; fonts self-hosted; `just check` fails on network APIs in `src/` and network crates
in `Cargo.lock` (list in ARCHITECTURE §12); M0 acceptance on Windows records a `netstat`
observation during launch and import.

## ADR-0007 — Windows 11 x64 only; portable layout; data folder

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (layout)

**Decision.** Single target Windows 11 x64, no admin. `Kept.exe` + `kept.config.json` beside it
holding `data_dir`, chosen on first run; env `KEPT_DATA_DIR` overrides (tests, portability). The
data folder holds `kept.db`, `logs/`, `backups/`, `exports/`. WebView2 is a documented
prerequisite (`docs/data-folder.md`, installer notes), not bundled around.

## ADR-0008 — Frontend stack

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (majors)

**Decision.** React 18 (18.3.1 line), TypeScript strict, Vite, Tailwind (v4 line: CSS-first
`@theme` mapping the spec's tokens), TanStack Query / Table / Virtual, Zustand for UI state
only, visx for charts, Radix primitives for focus/menu/dialog behavior. No component-library
skin, no shadcn paste-in. IBM Plex Sans + IBM Plex Mono self-hosted via `@fontsource/*` (OFL).
TypeScript version: the newest whose typescript-eslint and Vite plugin support is stable at M0
(TS 7 exists today; the resolved choice is recorded in ADR-0034).

## ADR-0009 — Test pyramid, `just check`, and the Windows-only E2E

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Context.** The spec asks for `just check` (or `pnpm check` with a reason) running fmt,
clippy `-D warnings`, tsc, eslint, Rust tests, Vitest, and the Playwright critical path.
Playwright drives browsers, not Tauri windows; WebView2 exists only on Windows, which is also the
only target. The dev container used for planning is Linux and has no `just` installed.

**Decision.** Keep `just` (single binary; `winget install Casey.Just`, `cargo install just`, or
`extractions/setup-just` in CI); `pnpm check` is a thin alias to `just check` for discoverability.
E2E runs Playwright against the real app on Windows: launch with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`, attach over CDP, isolate
with `KEPT_DATA_DIR`. `just check` runs every other step on any host and runs `e2e` only when
`os() == "windows"`; elsewhere it ends with an explicit `E2E NOT RUN: requires Windows/WebView2`
line so a Linux run is never mistaken for a full green. CI runs lint/tests on `ubuntu-latest`
and the full `just check` + unsigned build on `windows-latest`; the Windows job is required.
`proptest` invariants are named exactly as the spec names them.

**Consequences.** "`just check` is green" in a milestone note means the Windows run; a Linux run
is reported as "green except E2E not runnable here", never as green.

## ADR-0010 — Schema conventions

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** Hand-written SQL migrations, no ORM. Integer `rowid` primary keys (single user, no
sync or merge; fastest joins at 200k rows; readable in the audit pack). `TEXT` dates, `TEXT`
enums with `CHECK`, `INTEGER` 0/1 booleans with `CHECK`. The transaction table is `txn`
(`transaction` is a SQL keyword). Splits are child rows with `parent_id`; the `txn_leaf` view
excludes parents that have children and every aggregate reads it. Flags and `user_edited` are
documented bitmasks (ARCHITECTURE §3.4–3.5). Tags are a junction table. Link tables carry the
pair explicitly and `txn` carries the back-pointer; the writing command keeps both consistent in
one transaction. Derived caches (`reconciliation.computed_closing_cents`, `status`) are labelled
CACHE in the schema and refreshed in the same transaction as the write that affects them.

## ADR-0011 — Migration strategy

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** `src-tauri/migrations/NNNN_name.sql` embedded with `include_str!`; one
transaction per migration; `schema_migration(version, name, sha256, applied_at)` plus
`PRAGMA user_version`. Startup verifies checksums of applied migrations and refuses to open on a
mismatch or a newer-than-binary version. A `pre_migration` backup precedes any migration.
Forward-only: no down migrations, the backup is the rollback. Table rewrites use
create-copy-drop-rename with `foreign_keys = OFF` for that transaction and `foreign_key_check`
after. Seeds are idempotent inserts keyed on `system_code`. A committed migration file is never
edited; a mistake is fixed by the next migration.

## ADR-0012 — Encryption and key handling

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (mechanics)

**Decision.** SQLCipher 4 defaults (AES-256-CBC, HMAC-SHA512, PBKDF2-HMAC-SHA512 at 256 000
iterations); the passphrase is the key input and SQLCipher's KDF is used as-is (no home-grown
KDF). Open: `PRAGMA key` → read `sqlite_master` → wrong key fails closed as `WrongPassphrase`.
Optional remember stores the passphrase itself (SQLCipher needs it or a raw key; the DB must
remain openable without the keyring) in Windows Credential Manager via `keyring`, service
`Kept`, user `sha256(data_dir)`. Passphrase change: fresh backup then `PRAGMA rekey`. Backups use
`ATTACH … KEY …; SELECT sqlcipher_export(…)`, which re-encrypts under the chosen passphrase and
is the mechanism the tested backup/restore roundtrip uses. `temp_store = MEMORY`; WAL is
encrypted by SQLCipher. `cipher_memory_security` stays off (unlocked-session attacker is out of
scope; the setting costs performance).

## ADR-0013 — One connection behind a mutex

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** `Mutex<Option<rusqlite::Connection>>` in Tauri state; `None` while locked.
SQLite is single-writer anyway, every writing command is one transaction, and 200k-row reads
are milliseconds on one connection. Pragmas per connection: `foreign_keys = ON`,
`journal_mode = WAL`, `synchronous = NORMAL`, `temp_store = MEMORY`, `busy_timeout = 5000`.

## ADR-0014 — IPC contract, error taxonomy, change events

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Decision.** Commands return `Result<T, AppError>`; `AppError` serializes as
`{ kind, message, detail? }` with the kinds in ARCHITECTURE §7. Commands are thin: validate →
engine → map error. After any write the core emits `kept://changed { entities }`; the webview
invalidates matching TanStack Query keys, so the hero recomputes on every relevant write. TS
types for IPC payloads are hand-written and checked by a Vitest shape test against JSON the
Rust tests emit. Display-only formatting of integers may run in TS; nothing else monetary does.

## ADR-0015 — Logging and redaction

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Decision.** `tracing` to a daily-rotating file under `data_dir/logs`, 14 files kept,
non-blocking writer, default level `info`, `KEPT_LOG` raises it. Logged: ids, counts, durations,
command names, error kinds. Never logged: amounts, payees, memos, counterparties, passphrases,
keys, file contents, SQL with bound values. No `println!`.

## ADR-0016 — Audit command groups and undo

Status: Accepted · Date: 2026-10-05 · Source: product spec (undo everywhere) + tech lead

**Decision.** Every write is a `command` row plus one `audit_event` per touched row (before and
after JSON) in the same transaction. Undo applies the inverse of a command's events in reverse
order as a new `undo` command and is refused with `Conflict` if a touched row changed since.
Toasts name the undo ("Undo: import batch 12"). Import batch undo is the same mechanism plus
discarding the batch's quarantine rows.

## ADR-0017 — Import identity and dedup

Status: Accepted · Date: 2026-10-05 · Source: product spec (rules) + tech lead (mechanics)

**Decision.** `source_row_hash = sha256(account_id ‖ posted_date ‖ amount_cents ‖
trim(payee_raw) ‖ trim(memo) ‖ external_id?)`. File-level idempotency on
`(account_id, profile_id, file_sha256)`: a repeat is recorded as a batch with `inserted = 0` and
a report that says why. Row-level: exact match on `external_id` or hash → skip; fuzzy
candidates = same account, same amount, posted date ±3 days, normalized-Levenshtein similarity
≥ `dedup_similarity_bps` (default 8000, recorded per batch). Fuzzy + better observation
(pending→posted, or gains an `external_id`) → update system fields whose `user_edited` bit is 0.
Fuzzy otherwise → `import_quarantine`, never inserted and never dropped; the review queue resolves
it and the reconciliation explorer shows it. Candidates never cross accounts. A posted row dated
in the future or before the account's opening date is a `Validation` error.

**Consequences.** `import_idempotent`, `dedup_no_cross_account_collapse`, and
`user_edit_survives_reimport` are property tests over this algorithm.

## ADR-0018 — Payee normalization

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** `payee_norm` = lowercase, Unicode-NFKC, punctuation → space, collapse whitespace,
strip trailing store numbers / card last-4 / dates (`#1234`, `*1234`, `12/03`), strip known
processor prefixes (`pos debit`, `checkcard`, `sq *`, `tst*`, `paypal *`, `zelle payment to/from`
kept as a `zelle` marker). The transform is pure, versioned (`normalize::VERSION`), and unit
tested with a table; `payee_raw` is always kept.

## ADR-0019 — Transfer, card-payment, and refund linking

Status: Accepted · Date: 2026-10-05 · Source: product spec (not spending) + tech lead (windows)

**Decision.** Transfer pair = two unlinked rows on different accounts with opposite equal
amounts posted within ±3 days; kind by account kinds (`card_payment`, `loan_repayment`,
`venture_contribution`/`venture_withdrawal`, else `internal`); heuristic links are shown as such
and can be undone. Refund = a positive row matching a prior negative row's absolute amount within
90 days with payee similarity ≥ threshold; linked heuristically only on an exact payee+amount
match, otherwise queued for the user. Unlinked refunds sit in the review queue.

## ADR-0020 — Spending view vs cash view

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead (exact sets)

**Decision.** As defined in ARCHITECTURE §5.2: spending = non-transfer, non-income,
non-proceeds leaf rows by category (card purchase counted once, on the card; linked refunds net
against the original); cash = leaf rows on cash-kind accounts by date (card payment counted on
its date; card purchases excluded). `transfer_not_spending` checks both.

## ADR-0021 — Reconciliation tolerance and trust status

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead · **Question batched: M3 boundary**

**Decision.** Zero tolerance: `balanced` iff `difference_cents = 0`. Roll-forward from the prior
balanced closing. An account is `reconciled` iff its latest reconciliation is balanced and its
`period_end` is within `recon_stale_after_days` (default 45; per-account override). The hero is
trusted iff every account in the `available` set is reconciled.

**Question for Dave (M3).** Is 45 days the right staleness window, and should "stale" mark the
hero untrusted (current default: yes)?

## ADR-0022 — Safe-to-spend interpretations

Status: Accepted · Date: 2026-10-05 · Source: product spec (formula) + tech lead (terms) · **Questions batched: M4 boundary**

**Decision.** The formula is the spec's, verbatim (ARCHITECTURE §5.4). Interpretations in force:

1. Account set = personal, non-firewalled, non-archived `checking|savings|cash|payment_app`.
   Venture-owned accounts are excluded (venture cash is capped venture money, not personal cash).
2. Posted borrowing / securities-sale proceeds in a counted account are inside `posted_balance`
   (they are cash and must reconcile); they are flagged, listed on their own panel, and never
   income. Only _pending_ flagged inflows are excluded, per the spec's "not cash until posted".
3. No double subtraction: an obligation linked to an earmark contributes
   `max(0, expected − earmark_remaining)`; the earmark contributes its remaining. Overdue unpaid
   occurrences count.
4. The emergency reserve is an earmark of kind `emergency_reserve` (enters via the earmark
   term); the timing buffer is `minimum_buffer`. Two reserves, stored separately.
5. `next_income_date` = earliest unreceived occurrence (≥ today) of a confirmed stream. With no
   confirmed stream the window is 30 days and the term carries a warning reason.
6. The returned total is computed from the returned terms (`safe_terms_sum` by construction).

**Questions for Dave (M4).** Confirm 1, 2, 4, and 5. Each changes the hero number.

## ADR-0023 — Forecast model

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead · **Questions batched: M5 boundary**

**Decision.** One daily engine over 91 days from today; 30-day and 13-week views read it.
Pending rows land on day 0 (or their effective date) with the safe-to-spend sign rules. Variable
spend per variable category = median of three trailing 30-day bucket totals ending yesterday,
user-overridable, allocated to days by largest remainder. Earmarks feed a `committed` series;
`headroom = balance − committed`. `first_shortfall` is the first day `closing < 0`;
`first_buffer_breach` is the first day `headroom < 0`; both are reported. Downside: next base-pay
occurrence +7 days, no expected/rumored, plus a user surprise bill. No invented income.
`forecast_ties` holds per day and over the horizon for every scenario.

**Questions for Dave (M5).** Median of three 30-day buckets (vs 13 weekly buckets, which zeroes
sporadic categories)? Is "shortfall" the overdraft date or the buffer-breach date on the
dashboard (both are computed; default shows overdraft as shortfall and buffer breach as warning)?

## ADR-0024 — Debt math and strategy ordering

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Decision.** Interest via `mul_div_round`: monthly nominal `balance × apr_bps / 120 000`;
actual/365 `balance × apr_bps × days / 3 650 000`; default monthly nominal; promo APR while
`today ≤ promo_end`. Minimum rules: fixed, percent-of-balance with floor, interest-plus-percent,
full balance, none. A debt's minimum is an `obligation` with `debt_id`, so forecast and
safe-to-spend count it once. Avalanche = highest effective APR (tie → smaller balance); snowball
= smallest balance; custom = user order; `informal_first` policy routes extra to informal loans
by promised date before any strategy. Extra per month defaults to the latest review's dependable
surplus when positive and the UI names the source. Comparison is in interest cents and payoff
dates; the two-debt fixture must match a hand schedule within one cent per period.

## ADR-0025 — Venture buckets and cap utilization

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead · **Question batched: M7 boundary**

**Decision.** Venture rows partition into the five spec buckets by category system code (links
of kind `venture_contribution`/`venture_withdrawal` map to contribution/withdrawal).
`cap_used = owner_contribution + operating expense paid from personal accounts − withdrawal`.
Operating cash flow = revenue − operating expense, trailing 12 months. Verdict (`fund|freeze|kill`)
is user-set; sunk cost is not an input.

**Question for Dave (M7).** Should venture expenses paid directly from personal accounts count
toward the cap (current default: yes — owner cash left the personal pool)?

## ADR-0026 — Review: dependable surplus and exactly three actions

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead · **Question batched: M8 boundary**

**Decision.** Dependable surplus is a monthly-equivalent figure from rows only (ARCHITECTURE
§5.10): confirmed-stream receipts over the trailing 90 days scaled to 30, minus fixed
obligations, debt service (minimums + informal schedule ÷ 12), irregular provisions (annuals ÷
12 + sinking funds), and modeled variable spend. Borrowing and asset sales cannot enter because
only `income_receipt` rows of confirmed streams count. A review completes only with exactly three
non-empty actions (enforced in the completing transaction) and stores what each step showed.

**Question for Dave (M8).** 90 days of receipts as the income base, or three full pay cycles?

## ADR-0027 — Snapshots

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Decision.** "Nightly" is implemented as "once per civil day, on launch, if none exists for
today" because the app may not be running at night; plus on demand and a `plan` kind at review
completion. Snapshots feed trends only and are never read for a current figure.

## ADR-0028 — Fixture discipline

Status: Accepted · Date: 2026-10-05 · Source: product spec

**Decision.** `fixtures/EXPECTED.md` is written by hand, with arithmetic shown, before any engine
that it tests. Integration tests assert its numbers. If the engine disagrees, the engine is wrong
until the fixture is shown wrong, and that showing is a new ADR naming the row. No number in a
test, fixture, or UI string is invented.

## ADR-0029 — Query chips: parsed in TS, executed in Rust

Status: Accepted · Date: 2026-10-05 · Source: product spec (chips) + tech lead (split)

**Decision.** The chip grammar is parsed in `src/lib/query-chips.ts` into a `LedgerFilter` JSON
(Vitest-tested); the core compiles it to indexed SQL (Rust-tested). The webview never filters or
sums rows itself. Saved views store the chip text.

## ADR-0030 — Settings and reserves storage

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** Scalar settings live in `setting(key, value_json)`: zone, timing buffer, staleness
window, dedup threshold, theme, backup retention. The emergency reserve is a single active
`earmark` of kind `emergency_reserve` (ADR-0022 §4). Institution profiles live in
`import_profile`. The data folder path lives outside the database in `kept.config.json`.

## ADR-0031 — Policies and firewall acknowledgments

Status: Accepted · Date: 2026-10-05 · Source: product spec + tech lead

**Decision.** Two system policies ship, non-deletable and always enforced: `firewall_exclusion`
(firewalled accounts never enter `available`; an outflow on a firewalled account requires a
logged `firewall_ack`, otherwise it is flagged `needs_review` with `heuristic_code =
firewall_touch` and listed in Review) and `informal_first` (ADR-0024). User policies of kind
`reminder` are listed, not enforced.

## ADR-0032 — Payment-app rows are never guessed

Status: Accepted · Date: 2026-10-05 · Source: product spec

**Decision.** Every Venmo/payment-app row imports with `payment_app_unknown | needs_review` and
no category; the note is stored in `memo` and is never an input to heuristics. Cash withdrawals
likewise stay `needs_review` until the user classifies them.

## ADR-0033 — Category seeds and system codes

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** Engines reference categories by `system_code`, never by name, so seeded names are
editable. Codes listed in ARCHITECTURE §4. Borrowing proceeds and securities-sale proceeds are
seeded under the `transfer` root (`transfer.borrowing_proceeds`,
`transfer.securities_sale_proceeds`) so they can never be income or spending; they are also
flagged. Informal-loan repayments are `transfer.loan_repayment`.

## ADR-0034 — Resolved dependency versions at M0

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Decision.** Versions resolved at scaffold time (from `Cargo.lock` and `pnpm-lock.yaml`); the
lock files are canonical and this entry is the human-readable record ADR-0002 and ADR-0008
promised.

- Rust (MSRV 1.90, toolchain 1.97): tauri 2.12.1, tauri-build 2.7.1,
  tauri-plugin-dialog 2.8.1, rusqlite 0.40.2 (libsqlite3-sys
  0.38.2, bundled SQLCipher with vendored OpenSSL), keyring-core 1.0.0,
  windows-native-keyring-store 1.1.0, linux-keyutils-keyring-store
  1.0.0, chrono 0.4.45, chrono-tz 0.10.4, serde 1.0.229,
  serde_json 1.0.151, thiserror 1.0.69, tracing 0.1.44, tracing-subscriber
  0.3.23, tracing-appender 0.2.5, sha2 0.10.9, hex 0.4.3,
  zeroize 1.9.0, proptest 1.11.0, tempfile 3.27.0.
- Frontend: react 18.3.1, typescript 5.9.3 (TypeScript 7 exists but typescript-eslint
  supports <6.1), vite 8.3.2, tailwindcss 4.3.3, @tanstack/react-query
  5.104.1, zustand 5.0.15, @tauri-apps/api 2.12.1,
  @tauri-apps/cli 2.12.1, @tauri-apps/plugin-dialog 2.8.1,
  vitest 5.0.3, @playwright/test 1.63.0, eslint 10.12.0,
  typescript-eslint 8.71.0, prettier 3.9.9, @fontsource/ibm-plex-sans
  5.3.0, @fontsource/ibm-plex-mono 5.3.0.
- TanStack Table/Virtual, visx and Radix are added when the first screen uses them (M1+), not
  before, so the dependency graph never carries unused packages.

**Consequences.** `keyring` (the facade crate) is not used: with only a platform store feature
it hits a `compile_error!`; `keyring-core` plus one store crate is the supported shape.

## ADR-0035 — The `--info` token and the light-theme swap

Status: Accepted · Date: 2026-10-05 · Source: tech lead (the spec names the token but gives no value)

**Decision.** `--info` is `#7d97aa` (dark) and `#3f6076` (light): a muted slate that sits
beside the brass accent without competing with it. Light theme values (`src/tokens.css`,
`[data-theme="light"]`): bg `#f3f0e9`, bg-raised `#fbf9f5`, bg-inset `#e8e4db`, line
`#cfc9bc`, text `#1d1c19`, text-dim `#5f5a50`, accent `#7f6134`, positive `#44633f`,
negative `#9c3f33`, warning `#7a5c1a`, untrusted `#8f4d1a`, info `#3f6076`. WCAG contrast
computed at M0: every light pair is ≥ 5.04:1 against bg and raised; every dark pair is ≥ 4.47:1,
the lowest being the spec-locked `--negative` on `--bg-raised` (4.47:1), which is acceptable
because money figures always carry a sign and the mono face, never color alone.

**Consequences.** Tailwind's default palette is wiped (`--color-*: initial`) so only these
tokens exist as utilities; adding a color is an ADR, not a class.

## ADR-0036 — The network gate inspects the dependency graph, not the lock file

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** `Cargo.lock` lists `reqwest` and `hyper` because Tauri depends on them for
mobile targets; `cargo tree -i reqwest` is empty for both the Linux host and
`x86_64-pc-windows-msvc`.

**Decision.** `scripts/gates.sh` fails if any of `reqwest ureq isahc curl tauri-plugin-http
tauri-plugin-updater tauri-plugin-shell tauri-plugin-websocket` appears in the resolved graph
(`cargo tree --all-features -i <crate>`) for the host or the Windows target. The lock file is
not grepped. The webview-side grep for `fetch(`, `XMLHttpRequest`, `WebSocket`,
`sendBeacon`, `navigator.onLine` and `EventSource` stands.

## ADR-0037 — Dedup similarity is Jaro–Winkler; older observations skip; bank-funded app rows skip

Status: Accepted · Date: 2026-10-05 · Source: tech lead (supersedes the similarity metric in ADR-0017)

**Context.** Writing the fixtures showed how exports vary: a descriptor gains a location or a
processor code as a suffix (`JEWEL-OSCO #3421` → `JEWEL-OSCO #3421 CHICAGO`), and a pending
card row posts later with a longer descriptor. Normalised Levenshtein scores such pairs at 0.48;
a prefix-weighted metric scores them at 0.91. Two more cases had no rule: re-importing an
export that still shows a row as pending after the ledger already holds it posted, and a
payment-app export that lists payments funded straight from a bank account.

**Decision.**

1. Payee similarity is Jaro–Winkler on `payee_norm`, threshold 0.85 (`dedup_similarity_bps`
   8500, migration 0002), recorded per batch. The expected values in `fixtures/EXPECTED.md`
   come from an independent implementation in `fixtures/generate.py`.
2. An incoming **pending** row that fuzzy-matches a **posted** ledger row is skipped as an older
   observation (`older_observation`), never quarantined: it carries no new information. Posted
   vs posted still quarantines; pending → posted still updates; a gained `external_id` still
   updates.
3. A profile may declare `skip_when` (column, allowed values, reason): rows outside the allowed
   values are skipped and reported (`profile_rule`). Venmo payments funded from a bank account
   are the case: the bank's own row is the ledger entry, and the Venmo balance never moved.
4. Profiles may also set `row_flags` (every row), `flags_by_type` (type column → flags), an
   `effective_date` column, a counterparty payee (`inflow_column`/`outflow_column` with a
   fallback), a multi-column memo, and `skip_rows` for preambles.
5. A trailing newline is not a row; a record of empty cells is a blank row, counted and reported.

**Consequences.** `import_idempotent` holds for the August-then-September card export pattern.
ADR-0017's hash, file-level idempotency, quarantine and user-field rules stand unchanged.

## ADR-0038 — TanStack Table stays on the v8 line

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** The spec names TanStack Table without a version. At M1 the registry's latest is
9.x, a rewrite of the API (feature and row-model imports) that the author of this code cannot
vouch for from memory; 8.21 is the long-stable line the ledger's needs (column definitions, row
model, row selection) are well served by.

**Decision.** `@tanstack/react-table` `^8.21` with `@tanstack/react-virtual` `^3.14`. Moving to
v9 is a deliberate upgrade with its own ADR, not a routine bump. `@radix-ui/react-dialog` `^1.1`
is the first Radix primitive in use (split editor, save-view dialog).

## ADR-0039 — M2 engine decisions: what automation may touch, and what a link changes

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** Writing the M2 fixtures and tests before the engines surfaced five choices the
architecture left implicit: whether a refund link overrides a rule, which rows "apply rules" may
revisit, how undo treats the edits automation makes inside an import, what a user correction does
to the review flags, and whether a transfer-root row that is not one leg of a linked pair is
spending.

**Decision.**

1. **A link never overrides a prior categorisation.** A refund that nothing categorised takes the
   original's category and `refund_match`; a refund a rule or the user already placed keeps that
   category and its reason (the Target return reads `rule:Target`, as `fixtures/EXPECTED.md` wrote
   down). Transfer links do set both legs to the kind's category: a transfer leg is never spending
   or income, whatever a payee rule said.
2. **Automation owns every row the user has not categorised by hand.** `apply_rules` re-evaluates
   all unlinked leaf rows with `classification ≠ manual`: a new or reordered rule can take a row
   from an older rule or from a heuristic (rules outrank heuristics); a deleted rule's rows fall
   through to heuristics or review. The run is idempotent; the report counts outcomes
   (`rule_hits + heuristic_hits + unclassified = considered`) and, separately, `changed`. A rule's
   `hit_count` counts a row once.
3. **Undo works per row, not per audit event.** Automation inside an import touches a freshly
   inserted row again in the same command group. Undo therefore compares a row with the last state
   the command left and restores the first state it found; links the command created come off
   first (the other leg returns to review); links that predate the command stay. A refund linked
   later to a row the batch inserted is a conflict, like any later change.
4. **A correction leaves the queue.** A user-set category clears `needs_review` and
   `payment_app_unknown`; an outflow from a firewalled account keeps `needs_review` until its
   acknowledgment exists (policy 1), whoever categorised it.
5. **Spending excludes transfer-root rows even without a link** (ARCHITECTURE §5.2 as written).
   The fixture generator had implemented "non-transfer" as "not one leg of a pair" and counted the
   two loan repayments to Chris as spending; the engine followed §5.2 and the arithmetic in
   `fixtures/generate.py` was corrected (gross 11,846.20, net 8,162.03, spending − cash
   −3,953.74). This is the first case where the fixture, not the engine, was wrong; the written
   definition decided it, and the correction is recorded in the CHANGELOG.

**Consequences.** `tests/m2_rules_links.rs` and the `transfer_not_spending` property pin all five.
The heuristic code list in ARCHITECTURE §6.4 is the implemented one; the earlier draft's
`internal_transfer_pair` and `firewall_touch` names did not survive (the firewall state is a flag
plus the absence of an acknowledgment, not a classification).

## ADR-0040 — Reconciliation periods: immutable when balanced, refreshed by every write, file closings as drafts

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0021 fixed the tolerance (zero), the roll-forward and the trust rule. Building M3
left four mechanics open: what a period may overwrite, when the cached status is recomputed, how a
statement balance gets from an export into a period, and which accounts the hero's trust is
computed over when none has been reconciled.

**Decision.**

1. **Periods are entered in order and a balanced period is immutable.** A new period ends after
   the last balanced period and opens at its statement closing (`period_start` is the day after
   it; the first period opens at the account's opening balance and date). An `off` period can be
   re-entered with another statement; a balanced one is deleted (latest first, since later
   periods roll forward from it) and entered again. Re-entering the same balance is a no-op; a
   different one is `Conflict`.
2. **Every writing transaction refreshes every period** (`recon::refresh_all` runs inside
   `cmd::write`, import commit, undo and quarantine resolution), so the stored status never lags
   the ledger: undoing a batch flips a period to `off`, importing the right file flips it back,
   nothing is re-entered. Each period is recomputed in order against the last balanced period
   before it, so a later period's opening and start follow the fix.
3. **A file's last running balance is a draft, not a reconciliation.** The import report carries
   `file_closing_cents` and its date when the profile maps a balance column; the Reconcile screen
   opens with it prefilled and records `statement_source = 'file'` once the person confirms.
   Nothing reconciles on import.
4. **The hero's set is the cash-kind accounts that are neither firewalled nor archived** (the
   `available` set of §5.1); the hero is trusted only when that set is non-empty and every member
   is `reconciled`. Cards and the firewalled brokerage carry their own status and mark only their
   own figures. A figure that sums rows from several accounts is untrusted when any of them is not
   reconciled: the ledger Σ over the filter's accounts, the spending view over every account, the
   cash view over the cash set.
5. **The difference explorer is a list, not an auto-fix:** the period's posted rows with a running
   balance, posted rows within 5 days on either side, pending rows, and pending quarantine rows for
   the account; rows equal to ±difference are pointed out. The ledger is corrected through the
   Ledger or by undoing a batch, never by editing a period.

**Consequences.** `tests/m3_recon.rs` and the `recon_identity` property pin the mechanics;
`fixtures/recon.json` is `EXPECTED.md`'s machine-readable twin for them. The batched M3 question
(ADR-0021: window 45 days, stale → untrusted) stays open with its defaults in force.

## ADR-0041 — Plan matching, candidate detection and the hero's windows

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0022 fixed what each term of the formula means. Building M4 left open how a
receipt or payment is tied to a ledger row, how far back an unpaid occurrence still counts, how a
recurring bill is proposed, how one earmark's money is spread over several unpaid occurrences, and
what the Plan stores versus derives.

**Decision.**

1. **Matching is a function of the ledger and is re-run by every write.** A receipt (payment) is
   the posted row on the stream's deposit (obligation's source) account whose normalized payee
   contains the match text, whose |amount| is within `expected ± variability`, and which posted
   within `[due − 10, due + 5]` days; the closest row by |posted − due| wins (earlier posted date
   on a tie), each row is used at most once, occurrences are filled in due order. `plan::match_all`
   runs inside `cmd::write`, import commit, undo and quarantine resolution, next to
   `recon::refresh_all`, so a row that arrives later finds its occurrence and an undone import
   releases it (`detach_plan_links` drops the receipt or payment of a deleted row). A receipt or
   payment the person records by hand (`matched_by = user`) is kept; automation only fills
   occurrences that have none.
2. **Windows are bounded by the source account's opening date.** Unpaid occurrences count back
   `OVERDUE_LOOKBACK_DAYS = 120` days, matching looks back the same 120 days, and neither ever
   generates an occurrence before the account existed: the fixture's rent is due on the 1st but the
   account opened on 2026-07-01, so no June occurrence is "overdue".
3. **Candidates come from recurrence, never from names.** `detect_candidates` groups posted
   outflows that are not transfers by `(account, payee_norm)` and proposes one obligation per group
   with ≥ 3 rows, consecutive gaps of 25–36 days, and every amount within 25% (2500 bps) of the
   median: `expected = median`, `variability = max |amount − median|`, `due_day = median day of
month`, name = the payee in title case, `detected_from` lists the row ids. Groups already
   covered by an obligation (same name or matching payee text) are skipped. A candidate is deleted
   outright; a confirmed obligation is retired, never deleted, because its payments are history.
4. **Earmark coverage is distributed in due order.** An earmark's counted remaining
   (`max(0, Σ entries ≤ today)`) covers its obligation's unpaid occurrences one after another until
   it is exhausted; each occurrence counts `expected − covered`. An over-released earmark counts as
   zero and covers nothing; the drill-down labels it.
5. **The hero's drill-down lists what it left out and what it contains.** `excluded` carries the
   firewalled and venture-owned accounts with their balances and reasons, the pending flagged
   inflows that are not counted, and the posted flagged inflows that are inside the balance
   (ADR-0022 §2) so the person can see the borrowing or securities-sale money the figure holds.
6. **"Next 14 days" never hides a bill that is past due.** `upcoming` returns the unpaid
   confirmed occurrences from `today − 120` days to `today + horizon`, overdue ones first and
   flagged `overdue`, so the panel and the hero's obligation term list the same past-due items.
7. **The Plan stores inputs; the dashboard renders outputs.** Streams, obligations, earmarks and
   their entries are the only writes; `safe_to_spend` and `upcoming` return every figure the
   dashboard shows, terms and row ids included. Nothing on the dashboard is summed in React.

**Consequences.** `tests/m4_plan.rs` pins the receipts, payments, hero, next-14-days and candidate
answers of `fixtures/EXPECTED.md` (`fixtures/plan.json` is the machine-readable twin); the
`safe_terms_sum`, `firewall_excluded` and `borrowing_not_income` properties hold by construction.
The batched M4 questions (ADR-0022: venture accounts excluded, posted flagged proceeds inside the
balance, emergency reserve as an earmark, 30-day window without confirmed income) stay open with
their defaults in force.

## ADR-0042 — Forecast mechanics: blocks, committed projection, overdue day 0, pending rows, scenarios

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0023 fixed the model (one daily engine, median of three 30-day buckets, the
downside, `forecast_ties`). Building M5 left open how the model is spread over days, what
"committed" means day by day, where overdue and pending rows land, which rows the buckets read,
and what the plan overlay is.

**Decision.**

1. **The model is spent in 30-day blocks.** Each variable category's per-30-days figure (its
   override, else its median) is allocated over days 0..29 by `money::allocate(figure, 30)` and
   the same allocation repeats for days 30..59 and 60..89; day 90 opens a fourth block. Every
   30-day view therefore spends the model exactly once, to the cent.
2. **Buckets read posted, non-transfer leaf rows only.** `net_outflow = max(0, −Σ amount)` over
   rows with the category whose `status = 'posted'` and `transfer_link_id IS NULL`, so refunds net
   against their category and a pending row is never counted twice (it is a day-0 event instead).
   Rows without a category (review queue) count for nothing until classified. Only the children
   of the variable root have a model; the root itself does not.
3. **Pending rows are events.** A pending leaf row lands on its `effective_date` (day 0 when that
   is today or past), with the §5.1 sign rules: every outflow, inflows unless flagged borrowing or
   securities-sale. One dated beyond the horizon is left out.
4. **Unpaid obligations use the hero's window.** Occurrences come from `cash::safe`'s unpaid list
   (`today − 120` days to the horizon, bounded by the account's opening date); an overdue one
   lands on day 0; the forecast spends the full expected amount (earmark coverage is a hero
   concept: the cash still leaves).
5. **Committed is projected by schedule.** `committed(d) = timing_buffer + Σ max(0, remaining_e(d))`
   over active earmarks funded from the hero's accounts. From today's remaining, an earmark is
   released by `min(remaining, expected)` on each day its obligation falls due in the forecast,
   and funded by its schedule: `per_paycheck` on the scenario's pay dates of its stream,
   `monthly` on its day, `by_date` as one funding of the gap on the target date; a positive
   target caps every funding. `headroom = closing − committed`; `first_buffer_breach` is the
   first day it is negative, `first_shortfall` the first day closing is.
6. **Scenarios change inputs only.** The downside shifts the next unreceived occurrence of each
   confirmed base stream by +7 civil days (later ones keep their dates; the earmark funding
   follows the shifted date); a surprise bill is one outflow on its date and must fall inside the
   window with a positive amount. Expected and rumored streams are out of every scenario. The
   same code path produces every scenario, so `forecast_ties` holds for all of them.
7. **The plan overlay is a snapshot.** "Save baseline as plan" stores today's baseline closings
   as a `snapshot` of kind `plan` (with the hero's figures; debt, informal and venture columns
   are 0 until their engines exist); the latest one is drawn dotted against the live series.
8. **Charts are visx.** `@visx/{shape,scale,axis,group,curve}` were added at M5 as ADR-0034
   planned; colors come from the tokens through Tailwind's stroke and fill utilities; nothing is
   computed in the chart.

**Consequences.** `tests/m5_forecast.rs` pins every day and week of the four fixture scenarios,
the model, the override and the plan overlay against `fixtures/forecast.json` (the twin of
`EXPECTED.md` "Forecast (M5)"); the `forecast_ties` property covers random ledgers, pending rows,
the downside and surprise bills. The batched M5 questions (ADR-0023: median of three 30-day
buckets vs weekly; overdraft vs buffer breach as "shortfall") stay open with their defaults in
force: the dashboard shows the overdraft as the shortfall and the buffer breach as a warning.

## ADR-0043 — Debt periods, the constant budget, minimum obligations and informal repayments

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0024 fixed the interest formulas, the minimum rules and the strategy order.
Building M6 left open what a "period" is, how a paid-off debt's minimum is reused, how a minimum
reaches the hero and the forecast, how a standalone balance and an informal loan move with the
ledger, and what the debt engine does with a card that is overpaid.

**Decision.**

1. **Periods are the calendar months after the as-of month.** Period k runs from the first to the
   last day of the k-th month after today's month; interest is computed on the period's opening
   balance (`actual_365` uses the period's day count); the promo rate applies while the period
   starts on or before `promo_end`; the payoff date is the last day of the period that closes at
   zero. The engine stops after 120 periods and reports `unfinished` when a balance remains.
2. **The monthly budget is constant.** `budget = extra + Σ first-period minimums`. Every open debt
   gets its current minimum (never more than opening + interest); the remainder goes to the
   targets in order, so a paid-off debt's minimum and a shrinking percent minimum both roll to
   the next target. Under policy `informal_first` the targets are the informal loans by promised
   date (earliest first), then the strategy's order: avalanche by effective APR this period (tie:
   smaller balance), snowball by opening balance, custom by `custom_order` (unset last).
3. **A minimum is an obligation that starts today.** Every active debt that owes something and
   has a minimum rule, a due day and a payment account carries one confirmed `debt_minimum`
   obligation (expected = the next period's minimum, re-derived by every write, retired when the
   debt owes nothing). Its `anchor_date` is the day it appeared, and for the calendar due rules
   the anchor is now a floor: nothing falls due before an obligation exists. Migration 0004 adds
   `debt.payment_account_id` and `debt.match_payee_contains` so the obligation can match the
   ledger. Card purchases stay variable spend in the forecast; the minimum is the cash that
   leaves for the balance already run up — counting both is the cash view.
4. **Balances move with the ledger.** A linked debt owes `max(0, −posted balance)` of its account
   (an overpaid card owes nothing, has no schedule and no obligation). A standalone debt owes its
   opening minus `debt_payment` rows: every row its minimum obligation matches is adopted as a
   payment once; the person can also record one by hand or from a ledger row. Deleting a row
   detaches its payment.
5. **Informal repayments are found like payments.** An informal loan has no interest, rule `none`,
   schedule rows and a repayment account; each unpaid schedule row (repayments are applied to
   rows in due order) is matched to the closest outflow for exactly its amount on that account
   whose payee contains the match text, posted within `[due − 10, due + 5]`. The forecast counts
   unpaid schedule rows as outflows (`informal` events); the hero's obligation term does not (the
   spec's formula names confirmed obligations). Creating the loan from its proceeds row flags the
   row borrowing, categorises it `transfer.borrowing_proceeds` and takes it out of the review
   queue: never income.
6. **The 12-month scenario is read off the run.** With the given extra: achievable iff every
   informal loan is off within 12 periods; otherwise the gap is what is still owed after period
   12 and the date is when this budget actually gets there (or never). The extra is a user input
   until a review supplies the dependable surplus (M8); the UI names the source.

**Consequences.** `tests/m6_debts.rs` pins every period of every strategy against
`fixtures/debts.json` (the twin of `EXPECTED.md` "Debts and informal loans (M6)"): avalanche beats
snowball by 336.38 of interest on the fixture; the Visa's credit balance owes nothing; the Chris
loan's two Zelle repayments are found and the Mom loan's gap and date are stated. No M6 question
is batched: the fixture exposed no interest-convention ambiguity (monthly nominal stays the
default; the auto loan exercises actual/365).

## ADR-0044 — Venture rollup mechanics: ownership decides contributions, accounts change owner

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0025 fixed the buckets, the cap and the verdict. Building M7 left open how a
transfer becomes a contribution or a withdrawal when the link was made before the account was
marked venture-owned, how the window is cut, and how an existing account becomes a venture's.

**Decision.**

1. **Ownership at rollup time decides.** A transfer whose legs sit on a personal account and on
   an account owned by venture V is an owner contribution (the venture-side leg is an inflow) or
   a withdrawal (an outflow) whatever kind the link stored when it was made: the Summit Amex is
   paid from personal checking, so each card payment is a contribution once the card is marked
   as Ledgerline's. Non-transfer rows tagged to V are bucketed by their venture category code;
   an operating expense on a personal account also counts toward the cap (ADR-0025's default).
2. **The window is the trailing twelve months**, `(same day a year ago, today]`, clamped to the
   month; operating cash flow, cap used and the spend share are all read over it.
3. **An account changes owner through its patch** (`venture_id`: a venture makes it venture-owned,
   null makes it personal again); a venture-owned account never enters the hero's set (ADR-0022
   §1) and its balance is shown on the venture as what it holds or owes, not as personal cash.
4. **Take-home is confirmed base pay received**: Σ `income_receipt` rows of active confirmed
   base streams in the window; `spend_share_bps = Σ operating expense (all ventures) /
take-home`.
5. **Alerts are two, named:** `cap used` when cap used ≥ the cap, `milestone date passed` when
   the milestone date is before today. The verdict (`fund | freeze | kill`) is the person's
   field; the engine never sets it.

**Consequences.** `tests/m7_ventures.rs` pins the fixture's buckets (348.00 of SaaS, 544.18 of
contributions), cap used, the 1088 bps gauge, the 92-day countdown, the 170 bps spend share, the
two alerts and the personal-account expense rule. The batched M7 question (ADR-0025: personal-
account venture expenses count toward the cap) stays open with its default in force.

## ADR-0045 — Review mechanics: the period, what a step stores, plan variance, snapshots on unlock

Status: Accepted · Date: 2026-10-05 · Source: tech lead

**Context.** ADR-0026 fixed the surplus and the three-action rule; ADR-0027 the snapshot cadence.
Building M8 left open what period a review covers, whether history shows what the person saw or
what the engines say now, what "plan variance" compares, and how the nightly snapshot is taken by
an app that may not be running at night.

**Decision.**

1. **The period runs from the last completed review's end to today** (from the first row in the
   ledger for the first review); "since the last review" in the flags step means that start. One
   review is in progress at a time; an abandoned one commits nothing.
2. **A review stores what it showed.** The steps and the surplus with its terms are computed at
   start, on demand, and again inside the completing transaction, and stored on the review
   (`steps_json`, `surplus_detail_json`); history reads them back, never the live engines.
3. **Exactly three non-empty actions, enforced in the completing transaction**: two, four, or
   blanks leave the review in progress and nothing stored; drafts of up to three can be saved
   before the commit. Actions are ticked later from history.
4. **Plan variance compares today's available cash with the plan's balance entering today**, the
   latest `plan` snapshot's closing for yesterday; on the plan's first day there is nothing to
   compare yet. Completing a review stores a `plan` snapshot carrying the baseline forecast's
   closings and the review id, so the next review and the Forecast overlay both read it.
5. **Snapshots are one module.** `review::snapshot::take` writes the hero's terms, total debt,
   informal remaining, venture cap used and every account's balance; `daily` is taken once per
   civil day on unlock (a failure is logged and never blocks the unlock), `on_demand` from the
   Review screen, `plan` by "Save baseline as plan" and by review completion. Trends are one point
   per civil day, the latest snapshot of the day, and read snapshots only.
6. **Surplus terms, as built:** income = confirmed-stream receipts posted in the trailing 90 days
   × 30/90; fixed = confirmed non-minimum obligations with a monthly, nth-weekday (×1), biweekly
   (×26/12) or weekly (×52/12) rule; debt service = debt-minimum obligations + unpaid informal
   schedule rows due within 365 days ÷ 12; irregular = annual obligations ÷ 12 + sinking-fund
   schedules (per paycheck by the stream's cycle, monthly as is, by date the gap over the months
   left); variable = the variable-spend model. The queue screen became "Queue" so "Review" names
   the walkthrough, as the spec does.

**Consequences.** `tests/m8_review.rs` pins the fixture's surplus (3,149.51) and every step, the
two-or-four refusal, the stored snapshot, persistence across unlock on a real file database, and
daily uniqueness. The batched M8 question (ADR-0026: 90 days of receipts vs three pay cycles)
stays open with its default in force.

## ADR-0046 — OFX/QFX, the profile editor, backups that verify, restore by comparison, exports

Status: Accepted · Date: 2026-10-05 · Source: product spec (M9) + tech lead (mechanics)

**Decision.**

1. **One OFX/QFX profile.** The OFX standard names every field, so a single system profile
   (`ofx_qfx`, migration 0005, format `ofx`) serves every bank; its only knob is `TRNTYPE` →
   row flags (`ATM`/`CASH` → cash withdrawal + needs review, `FEE`/`SRVCHG` → fee, `INT` →
   interest). Detection is by the bytes (`OFXHEADER`, `<?OFX`, `<OFX>`), never by profile. One
   tolerant tokenizer reads both the SGML form (leaf tags left open) and the XML form. Mapping:
   `FITID` → `external_id` (required), `DTPOSTED` → posted date, `DTUSER` → effective date,
   `TRNAMT` → amount as signed by the bank (account's point of view), `NAME` → payee, `MEMO` →
   memo, `CHECKNUM` → payee when `NAME` is absent; rows are posted (OFX has no pending state);
   dates are the first eight digits as the bank states them; `CURDEF` other than USD is
   refused; a file with more than one account is refused. `<LEDGERBAL>` becomes the file's
   closing, which Reconcile takes as statement source `file`. A CSV running balance is now
   reported the same way (`ParsedFile.closing`), so both formats hand Reconcile one figure.
2. **Dedup across formats follows ADR-0017 unchanged.** An OFX row that matches a CSV-imported
   row (same account, amount, ±3 days, payee above the threshold) is a better observation and
   updates it with the FITID; user-edited fields are never touched; nothing is quarantined. The
   same file twice is a duplicate batch. The fixture pins both.
3. **Profiles are data the person edits.** `import_profile.spec` is `Spec::Csv(ProfileSpec)` or
   `Spec::Ofx(OfxSpec)` by the `format` column. CSV profiles are created from a sample file
   (`draft_from_sample`: the first plausible header within ten preamble rows, columns guessed by
   name, the date format from the first data row), corrected in a form or as JSON, tested against
   a file without touching the database (`test_spec`: signature match, then the parse), and
   stored only after `validate_spec` (every named column in the signature, every flag known).
   Built-in profiles are read-only; a profile an import batch used cannot be deleted so the
   batch report keeps its meaning. Every change is one audit row.
4. **Every backup is verified before it is logged**: opened with its passphrase and compared
   table by table with the live row counts at export time. Daily copies are
   `backups/kept-YYYY-MM-DD.db`, one per civil day on the day's first unlock, rotated to
   `backup_keep_daily` by file name only (manual, pre-migration and pre-restore copies never
   rotate); a failure is logged and never blocks the unlock. `sqlcipher_export` does not copy
   `user_version`, so the writer sets it on the copy, and the migration runner restores a
   missing `user_version` from `schema_migration` so copies made by earlier builds still open.
5. **Restore compares before it swaps.** The backup is opened with its own passphrase and
   exported into `<data>/restore-staging/` under the live passphrase, migrated there, and
   compared with the live database: every table's row count and the hero as of today. Confirming
   takes a verified pre-restore copy (`kept-pre-restore-<stamp>.db`), closes the live database,
   removes its WAL sidecars, renames the staged file into place, reopens it and logs the
   pre-restore copy there. If the restored file does not open, the pre-restore copy is put back.
   The restored database keeps the live passphrase, so a remembered credential stays valid.
6. **Passphrase change** = fresh verified manual backup under the current passphrase, WAL
   checkpoint, `PRAGMA rekey`, credential store updated when remembered. Earlier backups keep
   the passphrase they were taken with; the Settings panel says so.
7. **Exports never overwrite** (`create_new`) and are written only where asked (a stamped folder
   under `exports/` by default). Full export: one CSV per user table plus `kept.json`; a column
   stored as `*_cents` is exported without the suffix as a decimal string. Audit pack: ledger
   (leaf rows with account, category path, flags, venture, link ids), reconciliation periods,
   `safe_to_spend.json` (the hero's terms with row ids), `forecast.json` (the baseline),
   `debt_schedule.csv` (avalanche at minimums only, no hypothetical extra), `venture_rollup.csv`
   and a README stating the sign convention. Nothing is recomputed in the exporter.

**Consequences.** `tests/m9_ofx_profiles.rs` pins both OFX forms against the CSV rows, the
insert / duplicate-file / update-with-FITID outcomes and a file-sourced reconciliation, and the
draft → test → create → update → delete path; `tests/m9_backup_export.rs` pins the spec's
acceptance (restore into a temporary data folder matches every table's row count and the hero
14,165.22), a backup under a new passphrase opening only with it, daily rotation, rekey, and the
export file list with the fixture's row counts. The latent `OpenFlags` gap (no `CREATE` on an
existing database, which made an attached backup uncreatable after a plain unlock) is closed.

## ADR-0047 — The keyboard layer, undo by inverse command, the 200k-row fixture

Status: Accepted · Date: 2026-10-05 · Source: product spec (M10) + tech lead (mechanics)

**Decision.**

1. **One keyboard layer in the shell.** `Ctrl K` opens a palette listing every screen ("Go to")
   and the main commands ("Do": lock, back up now, start the weekly review, take a snapshot,
   import a statement, write the audit pack, keyboard shortcuts); typing filters by every word,
   `↑ ↓ Enter` run an entry. `F1` or `?` (outside a field) opens the shortcut overlay,
   `Ctrl Shift L` locks, `/` focuses the Ledger query box; `Esc` closes any dialog. Key events
   that start in an input, textarea, select or editable element are left alone except `Ctrl K`,
   `Ctrl Shift L` and `F1`. Every action in Kept is a `button` or a form control, so Tab reaches
   it, and `:focus-visible` draws the accent ring on every element (`src/index.css`).
2. **Undo is the inverse command.** There is no generic journal replay: a destructive action's
   toast names an undo that runs the inverse command (recreate the rule, re-enter the period,
   re-record the payment, relink the rows, restore the previous override) as a new audited command
   group. The row returns with the same fields and a new id; the audit log keeps both steps.
   `docs/undo.md` lists every destructive action, its undo, and the few that have none
   (quarantine decisions, review abandon, restore, passphrase change) with what stands in.
3. **The 200k-row fixture is derived, never typed.** `tests/perf_200k.rs` builds 200 000 rows from
   integer arithmetic over the row index (day = `i × days / n`, outflow = `−(100 + (i × 37) mod
20000)` cents, every 25th row an inflow of `150000 + (i × 53) mod 100000`, payee from the
   fixture's list), spread over ten years and the seven fixture accounts, and imports them through
   the real pipeline into a real SQLCipher file so dedup, rules, reconciliation refresh and the
   audit log all run. The amount cycle keeps any seven-day window free of equal amounts, so every
   row inserts and the generator's running sum is the ledger's. Timings go to
   `docs/performance.md` with the host's specification; the test is ignored by default and runs
   with `KEPT_PERF=1`.
4. **What the 200k-row run found, and the fixes (migration 0006).** The first run was quadratic:
   transfer detection looks for the opposite amount on another account within three days, and
   with no index led by `amount_cents` that was a full ledger scan per imported row
   (`txn_amount_date`). Every Ledger page sorted the whole ledger because nothing was indexed on
   `(posted_date, id)` (`txn_date_id`, and the keyset cursor is a row-value comparison so the
   index seeks instead of scanning to the page). Balances summed through a correlated
   `NOT EXISTS` probe per row; the leaf view now builds the tiny set of split parents once per
   statement, and a covering index `(account_id, status, posted_date, amount_cents)` answers the
   hero's sums from the index. Flagged rows are a small minority, so a partial index
   `WHERE flags <> 0` serves the hero's flagged-inflow lookup and the `needs:review` / `flag:`
   chips; those queries state `flags <> 0` so SQLite can use it. Hot import statements are
   cached (`prepare_cached`), and the per-connection pragmas of ADR-0013 gain
   `cache_size = -65536`: 64 MB of decrypted pages, because every page read from disk costs an
   AES decrypt and an HMAC check and the hot indexes of a 200k-row ledger are a few megabytes.
   The budget in ARCHITECTURE §15 (page < 50 ms, hero < 100 ms, import < 60 s) is judged on the
   release-profile run, which meets it; `docs/performance.md` keeps every step from the
   quadratic first run to the final numbers. Most of the file is the audit log (full before and
   after JSON per touched row); compacting it is the next lever if the import ever matters.
5. **The light theme is checked as a token swap.** `src/tokens.test.ts` asserts the light block
   redefines only custom properties the dark root declares, holds nothing but values, and never
   touches size, spacing, radius or font tokens.
6. **The E2E critical path is written here and run on Windows** (`e2e/m10-critical-path.spec.ts`:
   create → add the fixture's first account → paste the July statement → reconcile with the
   fixture closing 5,647.48 → the hero is trusted and the dashboard fits 1440×900; plus the
   palette and overlay). The Linux container has no WebView2, so `just check` ends with
   `E2E NOT RUN` here and CI's Windows job carries the proof (ADR-0009).

**Consequences.** The installer build and signing steps stay as `docs/release.md` and
`release.yml` describe; nothing in M10 changed them. The v1 definition of done in
`MILESTONES.md` is ticked against what this host could verify, with the Windows-only items
named as such.

## ADR-0048 — The post-v1 review: six core defects, the webview stops deriving money

**Status.** Accepted (2026-10-05).

**Context.** With M0–M10 complete, a read-only review of the core and of the webview against
`CLAUDE.md`, `ARCHITECTURE.md` and the ADRs found defects no fixture had exercised, and the
first CI runs on GitHub showed the workflows had never executed (a pnpm version named in both
the workflow and `package.json`, then the vendored OpenSSL build using Git Bash's perl).

**Decision.**

1. **A linked row cannot be split.** `txn::split` refuses a row with a transfer or refund link
   (`Conflict`): split children carried no link and the parent left `txn_leaf`, so a split
   transfer became spending in both views and automation would recategorise its parts.
   §6.5 states the rule in both directions now; `tests/m2_rules_links.rs` pins the refusal.
2. **A receipt is a posted, unlinked, unflagged inflow.** `income::candidate_receipt` adds
   `status = 'posted'`, `transfer_link_id IS NULL` and `(flags & (borrowing | securities_sale))
= 0`, which ADR-0041 §1 and §5.5 already stated; without them a late paycheck could be
   stood in for by a borrowing-flagged Zelle, a pending row or an internal transfer, and the
   review's income and the venture spend share would carry borrowed money. The payment and
   repayment matchers add `status = 'posted'` (transfers stay allowed there: card and loan
   payments are transfers). `tests/m4_plan.rs` pins all three exclusions and the recovery.
3. **One account set for the hero and its trust.** `recon::contributes` delegates to
   `safe::contributes`: trust was judged over cash accounts of any owner, so an unreconciled
   venture-owned checking account marked a hero it was not part of untrusted, and a data set
   whose only cash accounts were venture-owned read as a trusted hero over an empty set.
   `tests/m3_recon.rs` pins the venture account's exclusion.
4. **A file's closing is the last transaction of its latest day in statement order.** The CSV
   parser took the last row among equal latest dates, which is the closing only in an
   oldest-first file; newest-first exports (the common bank order) prefilled Reconcile with the
   balance before the last transaction. The parser decides the order from the first and last
   dated rows; a unit test covers both orders with the same rows.
5. **A user edit cannot post a row dated after today.** `apply_user_patch` takes today's civil
   date (`cmd::write_dated`) and refuses `status = posted` on a later `posted_date`, the check
   imports already made; `tests/m1_import.rs` pins it.
6. **Import undo re-matches the plan** (`plan::match_all`), as ADR-0041 §1 lists for every
   writing transaction; an occurrence freed by the undo was otherwise re-filled only by the next
   write.
7. **The webview derives no money figure.** Every sum, difference or net the screens computed
   moves into the core's DTOs: `AvailableAccount.net_cents`, `Upcoming.total_expected_cents`,
   `DebtsStep.total_debt_delta_cents` and `informal_delta_cents`, `Earmark.held_cents` (Σ its
   entries, by SQL `SUM`), forecast `Day.variable_cents`, and `views::compare` with
   `gross_minus_cash_outflows_cents` behind one `compare_views` command. The three places that
   divided basis points or cents by 100 use `formatBps` / `formatCents`; `formatBpsInput` gives
   a percent input its starting text.
8. **The cache empties on lock and unlock.** Every cached read except the status leaves when
   the status changes: the next unlock may open another folder, and an unlock takes the daily
   snapshot and backup. A write invalidates receipts, payments and next occurrences too, since
   every write re-matches (ADR-0041 §1), and recording a debt payment invalidates its
   candidates. The Ledger's `venture:` chip resolves against the ventures list.
9. **Notices are bounded.** The stack keeps the newest five; a notice with nothing to undo or
   act on leaves after eight seconds; the column scrolls past 60% of the window. A notice that
   names an undo stays until dismissed, as ADR-0047 requires.
10. **A render error stops one screen, not the window.** A boundary around the active screen
    names the error and offers the dashboard; the profile JSON editor accepts only a value
    shaped like a CSV mapping, so typing `{}` or `null` mid-edit is an error in the textarea
    rather than a blank window.
11. **Marks and kinds.** The review's per-account balances and the trend table's hero, and the
    forecast tables' closing, headroom and lowest figures carry the untrusted mark when the
    balances behind them are unreconciled; the forecast event kind `informal` reaches the
    webview's type and tone table; the dashboard's venture and firewall lists scroll inside
    their panels; the surprise-bill form takes today's date once the forecast has loaded.
12. **CI.** `pnpm/action-setup` reads the version from `package.json`; the Windows jobs set
    `OPENSSL_SRC_PERL` to Strawberry Perl because `just` runs under Git Bash.

**Consequences.** No fixture number changed; the new tests state the rules the fixtures had
not reached. `EXPECTED.md` stands. The five derived fields are additive, so saved exports and
the audit pack are unchanged.

## ADR-0049 — The installer is gated on `check-core`; the hosted E2E is advisory

**Status.** Accepted (2026-10-06). Amends ADR-0009.

**Context.** ADR-0009 made the Playwright critical path part of `just check` on Windows and the
Windows CI job the proof of a milestone. With every other check green on GitHub's hosted
Windows runner, Playwright cannot attach to WebView2 there: the app starts, writes its logs
and keeps running, but nothing listens on the remote-debugging port, whether the switch comes
from `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` or from the window builder
(`KEPT_E2E_CDP_PORT`, `3c21e10`). Dave needs the installer now.

**Decision.** The Windows job runs `just check-core`, then the critical path as a step marked
`continue-on-error` whose outcome is printed on the run and whose report is uploaded when it is
not green, then `pnpm tauri build` and the `kept-windows-unsigned-installer` artifact.
`release.yml` gates the tag build on `check-core` the same way. `just check` itself is
unchanged: on a Windows machine with a desktop session it still runs the E2E and a red E2E
is still red there; that machine, not the hosted runner, is where the two open boxes in
`MILESTONES.md` close.

**Consequences.** The installer no longer waits on the hosted runner's desktop session. The
E2E's state is visible on every run rather than hidden; when it goes green on the hosted
runner the step can drop `continue-on-error` and this ADR is superseded.
