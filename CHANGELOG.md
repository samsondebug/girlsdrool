# Changelog

Each milestone ends here with a demo note: what you can do now, what was verified in-session
(command and exit code), and what was not verified on this host.

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
