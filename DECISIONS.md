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
   income. Only *pending* flagged inflows are excluded, per the spec's "not cash until posted".
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
