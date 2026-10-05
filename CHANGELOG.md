# Changelog

Each milestone ends here with a demo note: what you can do now, what was verified in-session
(command and exit code), and what was not verified on this host.

## M10 — Palette, shortcuts, undo coverage, 200k rows, the critical path (unreleased)

### Demo

1. `Ctrl K` anywhere: the palette lists every screen and the main commands (lock, back up now,
   start the weekly review, take a snapshot, import a statement, write the audit pack); type to
   filter, `Enter` runs. `F1` or `?` shows every shortcut; `Ctrl Shift L` locks; `/` focuses the
   Ledger query box; `Esc` closes anything. Every action is a button or a form control, so `Tab`
   reaches it, and the focus ring is the accent color everywhere.
2. Every destructive action names its undo on its toast and runs the inverse command as a new
   audited step: delete a rule, remove a reconciled period, unmatch a receipt or payment, delete
   a candidate, remove an earmark entry, remove a debt payment or a schedule row, unlink a
   transfer or refund, set or clear a forecast override, delete a profile. `docs/undo.md` is the
   full table, including the few actions that have no undo and what stands in for them.
3. 200 000 rows: `KEPT_PERF=1 cargo test --no-default-features --test perf_200k -- --ignored
--nocapture` derives the rows from one formula, imports them through the real pipeline into a
   real encrypted file, and prints the timings `docs/performance.md` records with this host's
   specification.
4. The light theme is checked as a token swap (`src/tokens.test.ts`), and the Playwright critical
   path (create → account → paste July → reconcile with the fixture's 5,647.48 → trusted hero,
   dashboard fits 1440×900; the palette and the overlay) is written for the Windows run.

### Verified in this session (Linux dev container)

- `KEPT_PERF=1 cargo test --release --no-default-features --test perf_200k -- --ignored
--nocapture` — exit 0 on the shipped code: 200 000 rows in 70 batches imported in 40.8 s
  (slowest batch 0.8 s), first page 31 ms, the page at row 100 000 34 ms, the `jewel` text
  filter 319 ms, the hero 92 ms cold and 65 ms warm; every number and the steps that got there
  are in `docs/performance.md`. The first run of this test was quadratic and was stopped after
  40 minutes; migration 0006 (four indexes, the cheaper leaf view), cached statements and a
  64 MB page cache are the fixes.
- `cargo test --no-default-features` — exit 0: 51 unit, 13 property, 1 logging, 9 M0, 6 M1, 3 M2, 3 M3, 3 M4, 5 M5, 5 M6, 3 M7,
  4 M8, 4 M9 backup/export and 3 M9 OFX/profiles acceptance (113), with `perf_200k` ignored
  unless asked for.
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (45: the token-swap test joins the chips and money tests),
  `scripts/gates.sh` — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder: `docs/screenshots/m10-palette.png` (Ctrl K
  with `re` typed: Review, Reconcile, Forecast, Ventures, the review and snapshot commands),
  `m10-shortcuts.png` (F1); the `/` shortcut focused the Ledger query box.

### Not verified on this host

- The Playwright critical path and the unsigned NSIS installer build (Windows/WebView2 only, run
  by CI's Windows job); scroll smoothness is a judgment the Windows run makes by eye, the
  virtualizer and keyset paging are what make it possible.

## M9 — OFX/QFX, institution profiles, backup and restore, exports (unreleased)

### Demo

1. Import an OFX or QFX file: drop `fixtures/riverside/riverside_checking_2026-08.qfx` (SGML form)
   or `northbank/northbank_savings_2026-09.ofx` (XML form) on the Import screen. The preview names
   the account in the file, its date range and the ledger balance the bank states; every row gets
   its `FITID` as external id. Imported after the same month's CSV, each row is matched to the row
   already there and gains its FITID; nothing is inserted twice (`fixtures/EXPECTED.md` "OFX/QFX,
   backup and restore, audit pack (M9)"). After the import, Reconcile takes the file's balance as
   statement source `file` and the period balances.
2. Settings → Institution profiles: "New from sample file…" reads a CSV's header, guesses the
   mapping, and opens the editor; correct the columns (or the whole mapping as JSON), test it
   against the file (first rows, or the exact row and column that fails), save. Built-in
   profiles stay read-only; a profile an import used cannot be deleted.
3. Settings → Backups: the day's first unlock writes `backups/kept-YYYY-MM-DD.db` (the newest
   `backup_keep_daily` kept); Back up now writes a manual copy. Every copy is verified table by
   table before it is listed. Restore from backup… opens any copy with its own passphrase, stages
   it beside the live database, migrates it and shows every table's row count and the hero side
   by side; "Replace live data with this backup" takes a verified pre-restore copy first.
4. Settings → Passphrase: change it; a verified backup under the previous passphrase is taken
   first, then the database is rekeyed in place and the remembered credential follows.
5. Settings → Exports: the full export (one CSV per table plus `kept.json`) and the audit pack
   (ledger, reconciliation, safe-to-spend terms, forecast, debt schedule, venture rollup, README)
   go to a stamped folder under `exports/` or a folder of your choice; never over existing files.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 51 unit, 13 property, 1 logging, 9 M0, 6 M1,
  3 M2, 3 M3, 3 M4, 5 M5, 5 M6, 3 M7, 4 M8, 3 M9 OFX/profiles (`tests/m9_ofx_profiles.rs`: both
  OFX forms parse to the CSV rows; insert, duplicate file, update-with-FITID and a file-sourced
  reconciliation; draft → test → create → update → delete) and 4 M9 backup/export
  (`tests/m9_backup_export.rs`: the restore roundtrip matches every table's row count and the
  hero 14,165.22 on a real SQLCipher file, the wrong passphrase fails closed, daily rotation,
  rekey, the export file list with the fixture's row counts).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42), `scripts/gates.sh` — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb: opening the M8 seed folder migrated it v4 → v5 with a verified
  pre-migration backup and took the day's backup; `docs/screenshots/m9-settings.png` (Backups,
  profiles, exports), `m9-restore.png` (the pre-migration copy compared with live data: schema
  v4 → v5, same hero, two bookkeeping tables differ), `m9-import-ofx.png` (the QFX preview; its
  commit updated the five CSV rows with their FITIDs and offered the ledger balance to
  Reconcile); an audit pack written from Settings (7 files).

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); Windows Credential Manager update after a
  passphrase change (the Linux keyring store path ran); the Windows netstat observation.

## M8 — Weekly review, snapshots, trends (unreleased)

### Demo

1. Review (the guided walkthrough; the row queue moved to "Queue"): Start review opens the period
   since the last completed review and walks seven steps with their figures — balances with
   reconciliation status, unreviewed rows largest first, obligations in the next 14 days, plan
   variance against the last plan snapshot, debt and informal-loan progress with the change since
   the last review, venture cap, and the borrowing, securities-sale and firewall flags since the
   last review. Recompute refreshes every figure; Abandon commits nothing.
2. The dependable surplus, monthly equivalent and from rows only: on the fixture as of 2026-09-30,
   income 6,825.54 (six confirmed receipts in 90 days × 30/90) − fixed 2,729.99 − debt service
   307.00 − irregular 107.00 − variable 532.04 = **3,149.51**, every term listed with its items;
   borrowing and asset sales cannot enter (`fixtures/EXPECTED.md` "Weekly review (M8)").
3. Exactly three actions: the commit button stays off until three are written, and the core
   refuses two, four or blanks inside the completing transaction (nothing stored). A completed
   review keeps what it showed, its surplus and its actions in History, where each action is ticked
   off later; it also stores a `plan` snapshot that the Forecast draws against and the next review's
   plan variance reads.
4. Snapshots: one `daily` per civil day on the day's first unlock, `on_demand` from Trends, `plan`
   at review completion; Trends is one point per day (safe to spend, available, total debt,
   informal remaining, venture cap used) from snapshots only, never live figures.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 42 unit, 13 property, 1 logging, 9 M0, 6 M1,
  3 M2, 3 M3, 3 M4, 5 M5, 5 M6, 3 M7 and 4 M8 acceptance (`tests/m8_review.rs`: the surplus and
  every step; two, four and blank actions refused and three stored with the snapshot; history
  persists across lock and unlock on a real file database; daily snapshots unique per civil day
  and trends read them).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder (`seed_fixture_data_folder` now completes the
  fixture's review): `docs/screenshots/m8-review.png` (a review in progress) and
  `m8-history.png` (history and trends).

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M7 — Ventures (unreleased)

### Demo

1. Ventures: one card per venture with its verdict (`fund | freeze | kill`, the person's field),
   the cash cap gauge, the five buckets over the trailing twelve months, operating cash flow,
   the milestone countdown, the stop condition written down, and the venture-owned accounts.
   Accounts gain an Owner column: marking the Summit Amex as Ledgerline's makes each card payment
   from personal checking an owner contribution, whatever the link's stored kind.
2. The fixture (`fixtures/EXPECTED.md` "Ventures (M7)"): six SaaS charges → operating expense
   348.00; three card payments → owner contribution 544.18; cap used 544.18 of 5,000.00 =
   1088 bps on the gauge; operating cash flow −348.00; milestone "First paying customer" in 92
   days; no alert; venture spend 170 bps of the 20,476.62 of confirmed base pay received.
3. Alerts are named: lowering the cap under what is used raises `cap used`; a milestone date in
   the past raises `milestone date passed`. Tagging a personal-card row as a venture expense adds
   it to operating expense and to the cap (ADR-0025's default, still batched for Dave).
4. Dashboard: the Venture cap panel is a gauge per venture with its verdict and alerts, and the
   spend share of take-home.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 42 unit, 13 property, 1 logging, 9 M0, 6 M1,
  3 M2, 3 M3, 3 M4, 5 M5, 5 M6 and 3 M7 acceptance (`tests/m7_ventures.rs`: buckets, cap and
  gauge, countdown, spend share and the hero's account set; both alerts and the verdict; a
  personal-account expense in the cap).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder (`seed_fixture_data_folder` now installs the
  venture and marks the Amex as its own): `docs/screenshots/m7-ventures.png` and
  `m7-dashboard.png`.

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M6 — Debts, informal loans, avalanche vs snowball in interest cents (unreleased)

### Demo

1. Debts: each card or loan, linked to its account (what it owes is the account's balance; the
   fixture's Visa carries a credit balance and owes nothing) or standalone with an opening
   balance; APR, promo rate and end, interest method (monthly nominal or actual/365), minimum
   rule (fixed, % of balance with a floor, interest + %, full balance), due day, the account it
   is paid from and the payee text that finds the payments. Every debt that owes something gets a
   confirmed `debt_minimum` obligation (expected = next period's minimum, re-derived by every
   write, starting the day it appears), so the hero and the forecast count it once: the fixture's
   three minimums (Amex 116.00 on the 22nd, auto loan 95.00 on the 15th, balance transfer 96.00
   on the 5th) add 921.00 of outflows to the 91-day forecast and nothing to today's hero.
2. Strategy comparison: enter the monthly extra (a user input until a review supplies the
   dependable surplus) and read avalanche, snowball and custom in interest cents with payoff
   dates, then each debt's period-by-period schedule. On the fixture with 300.00 extra: avalanche
   957.77 of interest, snowball and custom 1,294.15, every period equal to `EXPECTED.md` to the
   cent; the budget (607.00) is constant and a paid-off minimum rolls to the next target.
3. Informal loans: the loan from Chris (600.00 on Venmo, flagged borrowing and categorised
   borrowing proceeds — never income — and out of the review queue) with its two scheduled
   repayments found in the ledger (the Zelle transfers, never expenses) and a remaining of 0.00;
   the loan from Mom (2,000.00, no schedule). Policy `informal_first` sends the extra to informal
   loans first: "repaid within 12 months" is a scenario — achievable by 2027-03-31 with 300.00
   extra; with no extra the gap after 12 months is 670.15 and the budget gets there 2028-03-31.
   Each loan keeps a repayment note draft that never leaves the machine.
4. Dashboard: Debt total (8,116.00 across 4 debts) and Informal loans (2,000.00 still owed on 1).

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 42 unit, 13 property, 1 logging, 9 M0, 6 M1,
  3 M2, 3 M3, 3 M4, 5 M5 and 5 M6 acceptance (`tests/m6_debts.rs`: balances, minimum
  obligations and informal loans; every strategy's every period to the cent; the 12-month
  scenario; minimums in the forecast but not today's hero; standalone payments and detached rows).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder (`seed_fixture_data_folder` now installs the
  debts): `docs/screenshots/m6-debts.png` (the Debts screen) and `m6-dashboard.png`.

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M5 — Forecast + scenarios (unreleased)

### Demo

1. Forecast: one daily engine over the next 91 days from today, drawn as a line with the lowest
   point marked (red dot and label), the committed line dashed (earmarks projected by their
   schedules + the timing buffer), the zero line when the balance dips below it. Inflows are the
   confirmed streams' unreceived occurrences; outflows are unpaid confirmed obligations (an
   overdue one lands today), pending rows on the day they post, and the variable-spend model.
   Nothing is invented to avoid a low point.
2. Variable spend: per category, three 30-day buckets ending yesterday with the median as the
   model — on the fixture ledger 532.04 per 30 days (groceries 367.67, fuel 50.33, shopping
   42.99, dining 29.15, transport 23.14, health 18.76; `fixtures/EXPECTED.md` "Forecast (M5)").
   Each 30 forecast days spend it exactly, by largest remainder. An override replaces one
   category's figure and is audited like every write; Clear returns to the median.
3. Scenarios: the downside toggle delays the next confirmed base pay by seven days (the fixture's
   lowest point moves from 26,629.68 on 2026-10-01 to 26,380.38 on 2026-10-08); a surprise bill
   is one outflow on a date inside the window. Both report the first shortfall (closing < 0) and
   the first buffer breach (headroom < 0): with the downside, 15,000.00 on 2026-10-05 dents the
   buffer by 941.37 without overdrawing; 30,000.00 on the baseline overdraws by 28.60 that day.
4. Tables: the next 30 days (inflows, outflows, closing, headroom, the events behind each day)
   and 13 weeks (inflows, outflows, closing, lowest). "Save baseline as plan" stores today's
   series as a `plan` snapshot; later forecasts draw it dotted against the live line.
5. Dashboard: the Forecast panel is the baseline's sparkline with its lowest point, the lowest
   balance and date (with the untrusted marking whenever an account behind it is not reconciled),
   and the first shortfall or buffer breach if there is one.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 42 unit, 13 property (new `forecast_ties` over
  random ledgers, pending rows, the downside and surprise bills), 1 logging, 9 M0, 6 M1, 3 M2,
  3 M3, 3 M4 and 5 M5 acceptance (`tests/m5_forecast.rs`: the model's buckets and medians; every
  day and week of the four fixture scenarios with their lowest point, shortfall and breach; the
  downside and bill directions; the override; the plan overlay).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder: `docs/screenshots/m5-forecast.png` (the
  Forecast screen) and `m5-dashboard.png` (the sparkline panel).

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M4 — Earmarks, obligations, income streams, safe-to-spend, dashboard (unreleased)

### Demo

1. Plan › Income streams: add the base pay cycle (weekly, biweekly, semimonthly, monthly, once;
   weekend rule; net amount ± variability; deposit account; payee text), mark it confirmed. Every
   write re-matches receipts: the fixture's Meridian payroll finds its six deposits
   (`fixtures/EXPECTED.md`, "Receipts matched"); the next three occurrences are listed; a receipt
   can be recorded by hand from the account's rows around the due date, and that choice is kept.
2. Plan › Obligations: the six fixture bills (monthly day, nth weekday, biweekly, annual, once)
   find their 16 payments. "Detect candidates" proposes obligations from recurring rows — 11
   payees on the fixture ledger, 6 once the plan's bills exist (Shell, Uber, Netflix, the Visa
   interest charge, Linear, Vercel) — each showing the rows it came from; Confirm or Delete. A
   confirmed obligation is retired, never deleted.
3. Plan › Earmarks: remaining is derived from entries (fund, release, adjust; each may point at the
   ledger row that moved the money), never stored. The fixture's Rent earmark holds 1,200.00 from
   one per-paycheck funding; the Emergency reserve is an earmark of kind `emergency_reserve`
   holding 12,000.00 in savings. Plan › Reserves sets the timing buffer (500.00 in the fixture)
   and shows the reserve; Plan › Policies lists the two system policies.
4. Dashboard: the hero is `safe_to_spend` as of today with the formula's four terms in order —
   available 29,065.22 across nbc, nbs, rvc, vm; − earmarks 13,200.00; − obligations due by the
   next confirmed income 1,200.00 (Rent 2,400.00 on 2026-10-01, earmark covers 1,200.00);
   − buffer 500.00 = **14,165.22** as of 2026-09-30, exactly `EXPECTED.md`'s answer. Click a
   term: the accounts with posted and pending figures and the firewalled brokerage it leaves out,
   each earmark, each unpaid occurrence with its coverage and an `overdue` chip, the buffer
   setting. The hero carries the dashed untrusted marking with the account names whenever an
   account behind it is not reconciled; without a confirmed stream it shows a 30-day-window chip.
5. Next confirmed income (2026-10-02, Meridian payroll, 3,412.77, 2 days away as of the fixture
   date) and Next 14 days (Rent 10-01, ComEd 10-07, Xfinity 10-12 with autopay and earmark chips,
   Σ expected); an unpaid occurrence that is past due is listed first, never hidden.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 42 unit (incl. occurrence generators), 12 property
  (new `safe_terms_sum`, `firewall_excluded`, `borrowing_not_income`), 1 logging, 9 M0, 6 M1,
  3 M2, 3 M3 and 3 M4 acceptance (`tests/m4_plan.rs`: receipts, payments, hero terms and total,
  trust, a manual receipt; next income and the next 14 days; candidates before and after the plan).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm test` (42) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder (`seed_fixture_data_folder` now installs the
  plan): `docs/screenshots/m4-dashboard.png` (hero, drill-down of the available term, next
  income, next 14 days) and `m4-plan.png` (the Plan screen).

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M3 — Reconciliation + untrusted marking (unreleased)

### Demo

1. Reconcile: pick an account, enter the statement's closing balance for a period end. The first
   period opens at the account's opening balance; each next one rolls forward from the last
   balanced period. `computed = opening + Σ posted rows`; the difference is shown to the cent and
   the status is `balanced` only at exactly zero. All 21 fixture periods balance against the
   closings `fixtures/EXPECTED.md` lists.
2. The difference explorer for a period that is off: the period's rows with a running balance (a
   row equal to ±difference is pointed out), posted rows within five days on either side, pending
   rows, and rows still held in quarantine — the places a missing or wrong row hides.
3. The mutated fixture (`northbank_checking_2026-08_mutated.csv`, one amount transposed): August
   is off by −9.00; September, entered next, rolls forward from July and is off by −9.00 too.
   Undoing that batch and importing the real file flips both periods to balanced without
   re-entering anything, because every write refreshes every period.
4. Trust: per account `reconciled | stale | off | never reconciled` (stale window 45 days by
   default, per-account override on the Reconcile screen, the default in Settings); the hero is
   trusted only when every cash account outside the firewall is reconciled. The dashboard's
   Reconciliation health panel shows the statuses and names the untrusted accounts; the ledger Σ
   and the spending and cash views carry the dashed-underline marking with the account names
   whenever an account behind them is not reconciled.
5. An import report ends with the file's last running balance and a "Reconcile with it" hand-off
   that opens Reconcile prefilled (`statement_source = file`).

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 38 unit, 9 property (new `recon_identity`), 1
  logging, 9 M0, 6 M1, 3 M2 and 3 M3 acceptance (`tests/m3_recon.rs`: 21 balanced periods,
  immutability and ordering rules, delete-latest-only; the mutated scenario's figures, explorer
  counts and hero naming; stale, override and never-reconciled statuses).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`, `pnpm test`,
  `scripts/gates.sh` — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against the seeded folder (`seed_fixture_data_folder` now enters every
  period): `docs/screenshots/m3-reconcile.png` (balanced periods and the explorer),
  `m3-explorer.png` (a period off by +9.00 after a wrong statement, with the explorer),
  `m3-dashboard.png` (the health panel).

### Not verified on this host

- Playwright critical path (Windows/WebView2 only); the Windows netstat observation.

## M2 — Rules, review queue, transfer and refund linking (unreleased)

### Demo

1. Import now runs automation in the same transaction: the 25 fixture rules (`fixtures/rules.json`)
   first, heuristics next (securities sale, fees, interest, ATM, payment-app rows), then link
   detection. Every row keeps why: `rule:Target`, `heuristic:atm_withdrawal`,
   `heuristic:card_payment_pair`. The import report ends with the automation summary.
2. Review: the queue is the six rows `fixtures/EXPECTED.md` lists, largest first — the firewalled
   brokerage transfer (Acknowledge, with a note that lands in the audit log), a Venmo inflow and
   two ATM withdrawals (say what they were), a Venmo payment and a small Venmo inflow. Choosing a
   category proposes a rule on the toast with how many other rows it would match today; Create
   rule makes it, Rules › Apply rules now runs it over the ledger.
3. Links: 11 transfer pairs (6 card payments, 5 internal) and the Target return are linked on
   import. Link… on any row (Ledger or Review) shows its links and the candidates: opposite
   amounts on other accounts within 3 days, and for an inflow the same-size purchases on the same
   account within 90 days with payee similarity. Unlink sends both rows back to the queue; a link
   made by hand is `user` and detection leaves it alone.
4. Spending view vs cash view for a date range at the top of Review: gross 11,846.20, linked
   refunds 84.17, same-category reimbursements 3,600.00, net 8,162.03, against cash outflows
   15,799.94 and inflows 28,600.61 — the difference (−3,953.74) is card purchases counted on the
   cards when bought versus card payments counted on the bank when paid.
5. Rules screen: ordered list (first match wins), enable, reorder, edit — payee contains / regex,
   memo, amount band, account → category, venture, flags — delete with confirmation, hit counts,
   Apply rules now with its report.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 38 unit (incl. the heuristic table mirrored from
  `EXPECTED.md`), 8 property (new `transfer_not_spending`), 1 logging, 9 M0, 6 M1 and 3 M2
  acceptance (`tests/m2_rules_links.rs`: every row's category, reason and flags equal
  `fixtures/automation.json`; 11 pairs + 1 refund with the stated kinds; the queue in order; both
  views' totals and per-category lines; corrections propose a rule; unlink, relink by hand,
  firewall acknowledgment, idempotent re-run).
- `cargo clippy -D warnings` for both feature sets, `pnpm typecheck`, `pnpm lint`, `pnpm test`
  (42), `scripts/gates.sh` — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice.
- Debug app under Xvfb against a data folder seeded by `seed_fixture_data_folder` (now with the
  fixture rules and venture installed before the imports): `docs/screenshots/m2-review.png`,
  `m2-rules.png`, `m2-links.png`.

### Fixture correction (ADR-0039 §5)

- `fixtures/generate.py` counted the two loan repayments to Chris (`transfer.loan_repayment`) as
  spending; ARCHITECTURE §5.2 excludes transfer-root rows whether or not they are linked. The
  generator now follows the definition; `EXPECTED.md`'s spending totals moved from 12,446.20 /
  8,762.03 to 11,846.20 / 8,162.03. No other number changed.

### Not verified on this host

- Playwright critical path (Windows/WebView2 only) and the Windows netstat observation.

## M1 — Accounts, CSV import with dedup, ledger (unreleased)

### Demo

1. Accounts: add the seven fixture accounts (name, institution, kind, opening balance and date,
   firewalled). Accounts are archived, never deleted; kind and opening figures lock once rows exist.
2. Import: drop, choose or paste a statement export; the profile is detected from the header
   (five institution profiles plus `generic_csv`), the mapping preview shows the first 20 rows as
   the ledger will see them, and commit writes one batch in one transaction with a prose report:
   "Read 32 rows. Inserted 0. Skipped 31 already-imported rows. Held 1 suspected duplicate for
   review (similarity ≥ 85.00%)."
3. Dedup, exactly as `fixtures/EXPECTED.md` wrote down first: a byte-identical file is a recorded
   no-op; an overlapping export skips 31 rows by hash and holds one descriptor variant for review
   at Jaro–Winkler 0.9111; a card row that was pending in August and posted in September is
   updated, not duplicated; a bank-funded Venmo payment is skipped by the profile's rule; a EUR
   file is rejected before any write. Suspected duplicates are resolved (insert / discard) in
   the Import screen; batches are undone from the batch list or the toast.
4. Ledger: 104 fixture rows virtualized, query chips (`account:`, `cat:`, `tag:`, `>100`,
   `needs:review`, `flag:borrowing`, `status:pending`, `date:2026-09`, free text), the core's
   row count and Σ for the filter, inline edits of payee and memo (a dot marks a user-edited
   field), per-row category select, multi-select recategorize, a split editor whose parts must
   sum to the row, saved views.

### Verified in this session (Linux dev container)

- `cargo test --no-default-features` — exit 0: 37 unit (profile spec, CSV parsing, normalisation
  table, dedup decisions, report prose), 7 property (`money_sum_conserves` incl. splits,
  `import_idempotent`, `dedup_no_cross_account_collapse`, `user_edit_survives_reimport`, …),
  1 logging, 9 M0 acceptance, 6 M1 acceptance (every fixture file imports; all 21 monthly
  closings equal `EXPECTED.md`; duplicate/overlap/pending/skip/EUR outcomes; re-import of
  everything inserts nothing and changes no user field; undo order and conflicts; preview).
- `pnpm typecheck`, `pnpm lint`, `pnpm test` (42 Vitest cases incl. the chip grammar) — exit 0.
- `just check` — exit 0 with `check-core: green` and the explicit `E2E NOT RUN` notice (fmt, clippy
  for both feature sets, every Rust suite, tsc, eslint, prettier, vitest, gates).
- `pnpm tauri build --debug --no-bundle` — exit 0; launched under Xvfb against a data folder seeded
  with every fixture (`seed_fixture_data_folder`, an ignored test) and screenshotted:
  `docs/screenshots/m1-ledger.png` (104 rows, Σ $16,257.00 from the core), `m1-import.png`
  (one suspected duplicate held, 18 batches with undo), `m1-accounts.png`.

### Not verified on this host

- The Playwright critical path (Windows/WebView2 only) now needs its import → ledger steps written
  against these screens; that lands with the reconciliation step in M3 when the path is complete.

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
