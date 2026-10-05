# Kept — Architecture

Status: draft for product-owner approval (planning turn, no application code yet).
Owner: tech lead. Product decisions are Dave's and are quoted from the spec; engineering
decisions are recorded as ADRs in `DECISIONS.md` and referenced here as `ADR-NNNN`.

Kept answers one question every day: **how much can I spend right now without breaking a
commitment I have already made?** Everything below exists to make that number correct,
traceable to source rows, and visibly untrusted when it cannot be trusted.

---

## 1. Module map

### 1.1 Repository layout

```
Kept/
├── ARCHITECTURE.md  DECISIONS.md  MILESTONES.md  CLAUDE.md  CHANGELOG.md  README.md
├── justfile                      # `just check` and friends (ADR-0009)
├── package.json  pnpm-lock.yaml  tsconfig.json  vite.config.ts  eslint.config.js  .prettierrc
├── index.html
├── src/                          # React 18 + TypeScript strict (webview; display only)
│   ├── main.tsx  App.tsx  router.tsx
│   ├── tokens.css                # design tokens — locked before any component (spec)
│   ├── tokens/                   # TS accessors for tokens (charts read CSS variables)
│   ├── lib/                      # ipc.ts, money.ts (format only), dates.ts, query-chips.ts, queries.ts, store.ts
│   ├── components/               # Money, Untrusted, DataTable, Dialog, Palette, Toast, Sparkline, …
│   └── screens/                  # Unlock, Dashboard, Ledger, Import, Reconcile, Plan, Forecast, Debts, Ventures, Review, Settings
├── src-tauri/                    # Rust core — all money math lives here
│   ├── Cargo.toml  tauri.conf.json  build.rs  capabilities/default.json  icons/
│   ├── migrations/               # 0001_init.sql, 0002_… (embedded with include_str!, ADR-0011)
│   ├── src/
│   │   ├── main.rs  lib.rs  error.rs  money.rs  dates.rs  config.rs
│   │   ├── db/        # open/unlock/lock (SQLCipher), pragmas, migrate.rs, audit.rs, repo/*.rs (plain SQL per entity)
│   │   ├── import/    # pipeline: detect → parse → normalize → hash → dedup → link → commit → report; csv.rs, ofx.rs (M9), profile.rs
│   │   ├── recon/     # reconciliation identity, roll-forward, trust status, difference explorer
│   │   ├── rules/     # ordered rules, heuristics, review-queue ordering, "why"; link.rs (transfer / card payment / refund)
│   │   ├── cash/      # balances, available, earmarks, obligations, income occurrences, safe-to-spend terms
│   │   ├── forecast/  # daily/weekly engine, variable-spend model, scenarios, tie-out
│   │   ├── debt/      # interest math, amortization, avalanche/snowball/custom, informal loans
│   │   ├── venture/   # rollup buckets, cap utilization, stop condition
│   │   ├── review/    # guided review, dependable surplus, three actions, snapshots, trends
│   │   ├── export/    # CSV/JSON full export, audit pack, backup/restore (sqlcipher_export)
│   │   └── cmd/       # #[tauri::command] functions, grouped per screen; thin: validate → engine → map error
│   └── tests/         # integration tests against fixtures/ and a temp SQLCipher database; proptest invariants
├── fixtures/                     # synthetic statements + EXPECTED.md (hand-computed answers written FIRST)
├── e2e/                          # Playwright critical path (Windows / WebView2 over CDP, ADR-0009)
├── docs/                         # release.md (signing, manual), profiles.md, data-folder.md
└── .github/workflows/            # ci.yml (lint/test + Windows build & e2e), release.yml (unsigned installer on tag)
```

### 1.2 Rust core modules and their contracts

| Module     | Owns                                                                                                                                                                     | Must never                                                                                                                        |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| `money`    | `Cents(i64)` newtype, checked add/sub/neg, `mul_div_round` (i128 intermediates, half away from zero), bps math, largest-remainder allocation, decimal string for exports | use `f64`; panic on overflow (returns `AppError::Overflow`)                                                                       |
| `dates`    | civil-date helpers in the user's zone, `today()`, pay-cycle and due-rule occurrence generators, business-day rule                                                        | do pay-cycle math on instants                                                                                                     |
| `db`       | open/unlock/lock, pragmas, migrations, audit writer, per-entity SQL                                                                                                      | expose a connection to the webview; write without an audit row                                                                    |
| `import`   | file hashing, profile detection, parsing, normalization, dedup, quarantine, batch report, batch undo                                                                     | overwrite a user-edited field; drop or duplicate a row silently                                                                   |
| `rules`    | rule matching, heuristics, review queue, transfer/card-payment/refund linking                                                                                            | create a rule from a correction without the user accepting it                                                                     |
| `recon`    | identity `opening + inflows − outflows = closing`, roll-forward, status, trust per account                                                                               | call a non-zero difference "balanced"                                                                                             |
| `cash`     | balances, `available`, earmark remaining, obligation occurrences, next confirmed income, safe-to-spend terms                                                             | include firewalled, archived, credit, or venture-owned balances in `available`; count pending borrowing / securities-sale inflows |
| `forecast` | 91-day daily engine (30-day and 13-week views), variable-spend model, scenarios                                                                                          | invent income to avoid a negative balance                                                                                         |
| `debt`     | interest in integer cents, schedules, strategy comparison, informal loans                                                                                                | treat a repayment as an expense or proceeds as income                                                                             |
| `venture`  | bucket rollup, 12-month net cash, cap used, stop-condition alert                                                                                                         | take sunk cost as an input; set the verdict                                                                                       |
| `review`   | review state machine, dependable surplus, exactly three actions, snapshots, trends                                                                                       | complete a review with ≠ 3 actions                                                                                                |
| `export`   | CSV/JSON of every table, audit pack, backup, restore roundtrip verification                                                                                              | open a network connection                                                                                                         |
| `cmd`      | Tauri commands: `Result<T, AppError>`, change-event emission                                                                                                             | compute anything the engines compute                                                                                              |

### 1.3 Frontend modules

| Module                 | Owns                                                                                              | Must never                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `lib/ipc.ts`           | typed `invoke` wrappers, `AppError` type, change-event subscription → TanStack Query invalidation | catch and swallow an error                                |
| `lib/money.ts`         | `formatCents(i64)`, `formatBps`, `parseCentsInput(string) → cents` for forms                      | sum, net, or derive a money figure the core also computes |
| `lib/query-chips.ts`   | chip grammar → `LedgerFilter` JSON (ADR-0029)                                                     | evaluate a filter against rows                            |
| `lib/store.ts`         | Zustand: UI state only (selection, open panels, theme, palette)                                   | hold ledger rows or balances                              |
| `components/Money`     | tabular mono rendering of integer cents, sign and untrusted treatment                             | round                                                     |
| `components/Untrusted` | dashed underline + label naming the unreconciled accounts                                         | rely on color alone                                       |
| `screens/*`            | layout and interaction for one screen each                                                        | derive balances in React                                  |

---

## 2. Data flow: import → hero number

```
 file (CSV / OFX / clipboard CSV)
   │ user action (dialog, drag-drop, paste) → path or text reaches Rust; the webview never parses money
   ▼
 import::hash      sha256(file bytes)  ──► same (account, profile, sha256) already imported?
   │                                        yes → batch recorded as no-op (rows_read=n, inserted=0), report says why
   ▼
 import::detect    header signature → profile (auto) | user picks profile | mapping preview
   ▼
 import::parse     rows → RawRow { posted_date, effective_date?, amount_cents, payee_raw, memo, status, external_id?, balance? }
   │               currency ≠ USD → AppError::Unsupported (whole batch rejected, nothing written)
   │               amounts parsed as integers (no float), dates as civil dates
   ▼
 import::normalize payee_norm (ADR-0018), source_row_hash (ADR-0017)
   ▼
 import::dedup     exact (hash / external_id) → skip
   │               fuzzy: same account, same amount, posted_date ±3 d, similarity ≥ threshold
   │                    incoming is a better observation (pending→posted, gains external_id) → UPDATE system fields only
   │                    otherwise → QUARANTINE (import_quarantine), never inserted, never dropped
   │               new → INSERT
   ▼
 rules + heuristics (ADR-0019)   category / venture / tags / flags with a stored "why"; transfer, card-payment, refund links;
   │                            cash withdrawals and payment-app rows → needs_review (never guessed)
   ▼
 db::audit         one command row + one audit_event per inserted / updated row, same SQLite transaction
   ▼
 recon             recompute computed_closing / status for every reconciliation period touched
   ▼
 cmd emits `kept://changed { entities: ["txn","import_batch","reconciliation"] }`
   ▼
 webview invalidates queries → invokes `safe_to_spend()`
   ▼
 cash::safe_to_spend
     balances per account (posted, pending in, pending out)        ──► available
     earmark remaining per funding account                         ──► earmarks_unfunded
     next confirmed income date → unpaid confirmed obligation
       occurrences due on/before it, net of earmark coverage       ──► obligations_before_next_income
     settings.timing_buffer_cents                                  ──► minimum_buffer
     recon::trust per contributing account                         ──► trusted | untrusted (which accounts, why)
   ▼
 Hero: integer cents + full terms + row ids behind every term. Untrusted → dashed underline + label naming the accounts.
```

Every arrow is a pure function of ledger rows plus settings. Nothing on screen is stored as a
second copy of a balance except `snapshot` rows, which are trends only (ADR-0027).

---

## 3. Core types and conventions

### 3.1 Money

- `Cents(i64)`; SQLite `INTEGER`. No `f64` anywhere on a monetary path (clippy gate greps for `f64|f32|as f` in `src-tauri/src` outside explicitly allow-listed non-money code such as chart scaling, which does not exist in Rust).
- Percentages and APR are basis points: `apr_bps: i32` (1999 = 19.99%). Similarity thresholds are also bps.
- `money::mul_div_round(a: i128, b: i128, d: i128) -> Result<i64>`: `a·b/d` rounded half away from zero; the only rounding primitive. Interest, pro-rata, and percent-of-balance all go through it.
- `money::allocate(total: i64, parts: usize) -> Vec<i64>`: largest-remainder split; `Σ parts == total` always (`money_sum_conserves`).
- SQL: `SUM()` on `INTEGER` is exact and raises on overflow — allowed. `total()`, `AVG()`, division, and any `REAL` on a money column are banned; medians are computed in Rust.
- Decimal strings are created only in `src/lib/money.ts` (UI) and `export` (CSV/JSON). Cents are already integers, so no rounding happens there; `formatBps` rounds half away from zero when it shortens.

### 3.2 Dates

- Civil dates `YYYY-MM-DD` (`TEXT`) for posted / effective / due / period dates. Instants `TEXT` UTC RFC3339 for `*_at` columns.
- `dates::today()` = civil date now in the user's zone (`setting.zone`, default `America/Chicago`). All pay-cycle and due-rule math is civil-date arithmetic.
- Sign convention: `amount_cents` is from the account's point of view: inflow positive, outflow negative. A liability account (credit, loan) therefore carries a non-positive balance; `owed = max(0, −balance)`.

### 3.3 Identifiers

Integer `rowid` primary keys everywhere (ADR-0010). Single user, no sync, no merge; integers are fastest for 200k-row joins and simplest to read in the audit pack.

### 3.4 `txn.flags` bitmask

| bit | name                  | set by                                                                    |
| --- | --------------------- | ------------------------------------------------------------------------- |
| 1   | `needs_review`        | import heuristics, unlinked refund, firewall touch, quarantine resolution |
| 2   | `cash_withdrawal`     | ATM / cash heuristics; always with `needs_review` until classified        |
| 4   | `payment_app_unknown` | every payment-app row on import; purpose is never guessed                 |
| 8   | `borrowing`           | user, or rule; informal-loan proceeds                                     |
| 16  | `securities_sale`     | user, rule, or brokerage profile                                          |
| 32  | `fee`                 | profile/heuristic                                                         |
| 64  | `interest`            | profile/heuristic                                                         |

Query chip `flag:borrowing` compiles to `(flags & 8) <> 0`.

### 3.5 `txn.user_edited` bitmask

| bit | field         | bit  | field                                   |
| --- | ------------- | ---- | --------------------------------------- |
| 1   | `payee_norm`  | 64   | `effective_date`                        |
| 2   | `memo`        | 128  | `status`                                |
| 4   | `category_id` | 256  | `amount_cents` (manual rows and splits) |
| 8   | tags          | 512  | `posted_date`                           |
| 16  | `venture_id`  | 1024 | `account_id`                            |
| 32  | `flags`       |      |                                         |

A re-import may touch a field only if its bit is 0 (`user_edit_survives_reimport`).

### 3.6 Enumerations

Stored as `TEXT` with `CHECK (… IN (…))`; mirrored by Rust enums with `serde(rename_all = "snake_case")`. Adding a variant is a migration.

---

## 4. Schema (migration `0001_init.sql`)

Conventions: every table has `created_at`; derived caches are named and documented as caches;
booleans are `INTEGER` 0/1 with a CHECK. `PRAGMA foreign_keys = ON` is set per connection, not in
the migration. The SQL below is the migration; at M0 it is copied verbatim to
`src-tauri/migrations/0001_init.sql`, which then becomes canonical.

```sql
-- schema bookkeeping ---------------------------------------------------------
CREATE TABLE schema_migration (
  version     INTEGER PRIMARY KEY,
  name        TEXT    NOT NULL,
  sha256      TEXT    NOT NULL,
  applied_at  TEXT    NOT NULL
);

CREATE TABLE setting (
  key         TEXT PRIMARY KEY,          -- zone, timing_buffer_cents, recon_stale_after_days,
  value_json  TEXT NOT NULL,             -- dedup_similarity_bps, theme, backup_keep_daily, …
  updated_at  TEXT NOT NULL
);

-- command / audit ------------------------------------------------------------
CREATE TABLE command (
  id                    INTEGER PRIMARY KEY,
  name                  TEXT NOT NULL,                      -- e.g. 'import.commit', 'txn.recategorize'
  actor                 TEXT NOT NULL CHECK (actor IN ('user','import','system','undo')),
  at                    TEXT NOT NULL,
  undoes_command_id     INTEGER REFERENCES command(id),     -- set on undo commands
  undone_by_command_id  INTEGER REFERENCES command(id)      -- set on the command that was undone
);

CREATE TABLE audit_event (
  id           INTEGER PRIMARY KEY,
  command_id   INTEGER NOT NULL REFERENCES command(id),
  at           TEXT NOT NULL,
  entity       TEXT NOT NULL,                               -- table name
  entity_id    INTEGER NOT NULL,
  action       TEXT NOT NULL CHECK (action IN ('insert','update','delete')),
  before_json  TEXT,                                        -- NULL on insert
  after_json   TEXT                                         -- NULL on delete
);
CREATE INDEX audit_event_command ON audit_event(command_id);
CREATE INDEX audit_event_entity  ON audit_event(entity, entity_id);

-- ventures and accounts ------------------------------------------------------
CREATE TABLE venture (
  id                 INTEGER PRIMARY KEY,
  name               TEXT NOT NULL UNIQUE,
  status             TEXT NOT NULL CHECK (status IN ('fund','freeze','kill')),   -- the user-set verdict
  cash_cap_cents     INTEGER NOT NULL CHECK (cash_cap_cents >= 0),
  time_budget_hours  INTEGER CHECK (time_budget_hours IS NULL OR time_budget_hours >= 0),
  milestone          TEXT NOT NULL DEFAULT '',
  milestone_date     TEXT,
  stop_condition     TEXT NOT NULL DEFAULT '',
  archived           INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  created_at         TEXT NOT NULL
);

CREATE TABLE account (
  id                      INTEGER PRIMARY KEY,
  name                    TEXT NOT NULL UNIQUE,
  institution             TEXT NOT NULL DEFAULT '',
  kind                    TEXT NOT NULL CHECK (kind IN ('checking','savings','credit','brokerage','loan','payment_app','cash','venture')),
  currency                TEXT NOT NULL DEFAULT 'USD' CHECK (currency = 'USD'),
  opening_balance_cents   INTEGER NOT NULL,
  opening_date            TEXT NOT NULL,
  owner                   TEXT NOT NULL DEFAULT 'personal' CHECK (owner IN ('personal','venture')),
  venture_id              INTEGER REFERENCES venture(id),
  firewalled              INTEGER NOT NULL DEFAULT 0 CHECK (firewalled IN (0,1)),
  archived                INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  recon_stale_after_days  INTEGER,                          -- NULL → setting default (45)
  created_at              TEXT NOT NULL,
  CHECK ((owner = 'venture') = (venture_id IS NOT NULL))
);

-- categories, tags, rules ----------------------------------------------------
CREATE TABLE category (
  id           INTEGER PRIMARY KEY,
  parent_id    INTEGER REFERENCES category(id),
  name         TEXT NOT NULL,
  root_kind    TEXT NOT NULL CHECK (root_kind IN ('fixed','variable','irregular','debt','income','transfer','venture')),
  is_system    INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),   -- editable, not deletable
  system_code  TEXT UNIQUE,                                 -- stable code the engines reference (ADR-0033)
  archived     INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  sort_order   INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL,
  UNIQUE (parent_id, name)                                  -- root-level uniqueness enforced in Rust (NULL parent)
);

CREATE TABLE tag (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE rule (
  id                      INTEGER PRIMARY KEY,
  position                INTEGER NOT NULL,                 -- evaluation order; renumbered by the engine
  name                    TEXT NOT NULL,
  enabled                 INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  match_payee_contains    TEXT,
  match_payee_regex       TEXT,
  match_memo_contains     TEXT,
  match_amount_min_cents  INTEGER,
  match_amount_max_cents  INTEGER,
  match_account_id        INTEGER REFERENCES account(id),
  action_category_id      INTEGER REFERENCES category(id),
  action_venture_id       INTEGER REFERENCES venture(id),
  action_flags_set        INTEGER NOT NULL DEFAULT 0,
  action_tag_ids_json     TEXT NOT NULL DEFAULT '[]',
  hit_count               INTEGER NOT NULL DEFAULT 0,
  created_at              TEXT NOT NULL,
  updated_at              TEXT NOT NULL,
  CHECK (match_payee_contains IS NOT NULL OR match_payee_regex IS NOT NULL OR match_memo_contains IS NOT NULL
         OR match_amount_min_cents IS NOT NULL OR match_amount_max_cents IS NOT NULL OR match_account_id IS NOT NULL)
);
CREATE INDEX rule_position ON rule(position);

-- import ---------------------------------------------------------------------
CREATE TABLE import_profile (
  id           INTEGER PRIMARY KEY,
  name         TEXT NOT NULL UNIQUE,
  institution  TEXT NOT NULL DEFAULT '',
  format       TEXT NOT NULL CHECK (format IN ('csv','ofx')),
  spec_json    TEXT NOT NULL,                               -- column map, date format, sign convention, header signature (§6.1)
  is_system    INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);

CREATE TABLE import_batch (
  id                 INTEGER PRIMARY KEY,
  command_id         INTEGER NOT NULL REFERENCES command(id),
  file_sha256        TEXT NOT NULL,
  file_name          TEXT NOT NULL,
  account_id         INTEGER NOT NULL REFERENCES account(id),
  profile_id         INTEGER NOT NULL REFERENCES import_profile(id),
  date_from          TEXT,
  date_to            TEXT,
  rows_read          INTEGER NOT NULL,
  inserted           INTEGER NOT NULL,
  updated            INTEGER NOT NULL,
  skipped            INTEGER NOT NULL,
  quarantined        INTEGER NOT NULL,
  dedup_report_json  TEXT NOT NULL,                         -- human-readable report source (§6.3)
  created_at         TEXT NOT NULL,
  undone_at          TEXT
);
CREATE INDEX import_batch_file ON import_batch(account_id, profile_id, file_sha256);

-- the ledger -----------------------------------------------------------------
CREATE TABLE txn (
  id                INTEGER PRIMARY KEY,
  account_id        INTEGER NOT NULL REFERENCES account(id),
  parent_id         INTEGER REFERENCES txn(id),             -- split child when set; children inherit account/date/status
  posted_date       TEXT NOT NULL,
  effective_date    TEXT NOT NULL,                          -- defaults to posted_date
  amount_cents      INTEGER NOT NULL,                       -- signed, account's point of view
  payee_raw         TEXT NOT NULL DEFAULT '',
  payee_norm        TEXT NOT NULL DEFAULT '',
  memo              TEXT NOT NULL DEFAULT '',
  category_id       INTEGER REFERENCES category(id),
  import_batch_id   INTEGER REFERENCES import_batch(id),
  source_row_hash   TEXT,                                   -- NULL for manual rows and split children
  external_id       TEXT,                                   -- FITID / bank reference when the source has one
  classification    TEXT NOT NULL DEFAULT 'unclassified' CHECK (classification IN ('manual','rule','heuristic','unclassified')),
  rule_id           INTEGER REFERENCES rule(id),            -- why (rule)
  heuristic_code    TEXT,                                   -- why (heuristic), e.g. 'card_payment_pair', 'atm_withdrawal'
  user_edited       INTEGER NOT NULL DEFAULT 0,             -- bitmask §3.5
  status            TEXT NOT NULL DEFAULT 'posted' CHECK (status IN ('pending','posted')),
  transfer_link_id  INTEGER REFERENCES transfer_link(id),
  refund_link_id    INTEGER REFERENCES refund_link(id),
  venture_id        INTEGER REFERENCES venture(id),
  flags             INTEGER NOT NULL DEFAULT 0,             -- bitmask §3.4
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK (parent_id IS NULL OR source_row_hash IS NULL)
);
CREATE UNIQUE INDEX txn_source_identity   ON txn(account_id, source_row_hash) WHERE source_row_hash IS NOT NULL;
CREATE UNIQUE INDEX txn_external_identity ON txn(account_id, external_id)     WHERE external_id IS NOT NULL;
CREATE INDEX txn_account_date ON txn(account_id, posted_date);
CREATE INDEX txn_dedup        ON txn(account_id, amount_cents, posted_date);
CREATE INDEX txn_category     ON txn(category_id);
CREATE INDEX txn_venture      ON txn(venture_id);
CREATE INDEX txn_parent       ON txn(parent_id);
CREATE INDEX txn_payee_norm   ON txn(payee_norm);
CREATE INDEX txn_status       ON txn(status);

-- every aggregate reads leaves: a split parent is never counted
CREATE VIEW txn_leaf AS
  SELECT t.* FROM txn t WHERE NOT EXISTS (SELECT 1 FROM txn c WHERE c.parent_id = t.id);

CREATE TABLE txn_tag (
  txn_id  INTEGER NOT NULL REFERENCES txn(id),
  tag_id  INTEGER NOT NULL REFERENCES tag(id),
  PRIMARY KEY (txn_id, tag_id)
);

CREATE TABLE transfer_link (
  id          INTEGER PRIMARY KEY,
  out_txn_id  INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  in_txn_id   INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  kind        TEXT NOT NULL CHECK (kind IN ('internal','card_payment','loan_repayment','venture_contribution','venture_withdrawal')),
  confidence  TEXT NOT NULL CHECK (confidence IN ('user','heuristic')),
  created_at  TEXT NOT NULL,
  CHECK (out_txn_id <> in_txn_id)
);

CREATE TABLE refund_link (
  id               INTEGER PRIMARY KEY,
  original_txn_id  INTEGER NOT NULL REFERENCES txn(id),      -- one original may have several partial refunds
  refund_txn_id    INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  confidence       TEXT NOT NULL CHECK (confidence IN ('user','heuristic')),
  created_at       TEXT NOT NULL
);

CREATE TABLE import_quarantine (
  id                INTEGER PRIMARY KEY,
  import_batch_id   INTEGER NOT NULL REFERENCES import_batch(id),
  account_id        INTEGER NOT NULL REFERENCES account(id),
  row_json          TEXT NOT NULL,                          -- the normalised row exactly as it would have been inserted
  source_row_hash   TEXT NOT NULL,
  suspected_txn_id  INTEGER REFERENCES txn(id),
  similarity_bps    INTEGER NOT NULL,
  reason            TEXT NOT NULL,
  resolution        TEXT NOT NULL DEFAULT 'pending' CHECK (resolution IN ('pending','inserted','discarded')),
  resolved_txn_id   INTEGER REFERENCES txn(id),
  resolved_at       TEXT
);
CREATE INDEX import_quarantine_pending ON import_quarantine(resolution) WHERE resolution = 'pending';

CREATE TABLE saved_view (
  id          INTEGER PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  query_text  TEXT NOT NULL,                                -- chip grammar text, parsed on load
  created_at  TEXT NOT NULL
);

-- plan: income, obligations, earmarks ---------------------------------------
CREATE TABLE income_stream (
  id                     INTEGER PRIMARY KEY,
  name                   TEXT NOT NULL,
  kind                   TEXT NOT NULL CHECK (kind IN ('base','bonus','rsu','deferred_comp','other')),
  cycle                  TEXT NOT NULL CHECK (cycle IN ('weekly','biweekly','semimonthly','monthly','once')),
  anchor_date            TEXT NOT NULL,                     -- a known pay date; for 'once' the expected date
  semimonthly_day_1      INTEGER CHECK (semimonthly_day_1 BETWEEN 1 AND 31),   -- 31 = last day of month
  semimonthly_day_2      INTEGER CHECK (semimonthly_day_2 BETWEEN 1 AND 31),
  expected_net_cents     INTEGER NOT NULL CHECK (expected_net_cents >= 0),
  variability_cents      INTEGER NOT NULL DEFAULT 0 CHECK (variability_cents >= 0),
  confidence             TEXT NOT NULL CHECK (confidence IN ('confirmed','expected','rumored')),
  weekend_rule           TEXT NOT NULL DEFAULT 'previous_business_day' CHECK (weekend_rule IN ('none','previous_business_day','next_business_day')),
  deposit_account_id     INTEGER REFERENCES account(id),
  match_payee_contains   TEXT,                              -- how receipts are recognised
  active                 INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  created_at             TEXT NOT NULL,
  updated_at             TEXT NOT NULL,
  CHECK (cycle <> 'semimonthly' OR (semimonthly_day_1 IS NOT NULL AND semimonthly_day_2 IS NOT NULL))
);

CREATE TABLE income_receipt (                               -- an occurrence that has actually arrived
  income_stream_id  INTEGER NOT NULL REFERENCES income_stream(id),
  due_date          TEXT NOT NULL,
  txn_id            INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  matched_by        TEXT NOT NULL CHECK (matched_by IN ('user','heuristic')),
  PRIMARY KEY (income_stream_id, due_date)
);

CREATE TABLE debt (
  id                        INTEGER PRIMARY KEY,
  name                      TEXT NOT NULL,
  kind                      TEXT NOT NULL CHECK (kind IN ('credit_card','loan','informal')),
  account_id                INTEGER UNIQUE REFERENCES account(id),  -- linked liability account, when any
  apr_bps                   INTEGER NOT NULL DEFAULT 0 CHECK (apr_bps >= 0),
  promo_apr_bps             INTEGER CHECK (promo_apr_bps IS NULL OR promo_apr_bps >= 0),
  promo_end                 TEXT,
  interest_method           TEXT NOT NULL DEFAULT 'monthly_nominal' CHECK (interest_method IN ('monthly_nominal','actual_365')),
  minimum_rule              TEXT NOT NULL CHECK (minimum_rule IN ('fixed','percent_of_balance','interest_plus_percent','full_balance','none')),
  minimum_fixed_cents       INTEGER NOT NULL DEFAULT 0 CHECK (minimum_fixed_cents >= 0),
  minimum_bps               INTEGER NOT NULL DEFAULT 0 CHECK (minimum_bps >= 0),
  minimum_floor_cents       INTEGER NOT NULL DEFAULT 0 CHECK (minimum_floor_cents >= 0),
  due_day                   INTEGER CHECK (due_day BETWEEN 1 AND 31),
  strategy_participation    INTEGER NOT NULL DEFAULT 1 CHECK (strategy_participation IN (0,1)),
  custom_order              INTEGER,
  standalone_opening_cents  INTEGER CHECK (standalone_opening_cents IS NULL OR standalone_opening_cents >= 0),
  standalone_opening_date   TEXT,
  active                    INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  created_at                TEXT NOT NULL,
  updated_at                TEXT NOT NULL,
  CHECK ((account_id IS NULL) = (standalone_opening_cents IS NOT NULL))
);

CREATE TABLE informal_loan (
  debt_id          INTEGER PRIMARY KEY REFERENCES debt(id),
  counterparty     TEXT NOT NULL,
  original_cents   INTEGER NOT NULL CHECK (original_cents > 0),
  borrowed_date    TEXT NOT NULL,
  promised_terms   TEXT NOT NULL DEFAULT '',
  promised_date    TEXT,
  proceeds_txn_id  INTEGER REFERENCES txn(id),            -- the flagged borrowing inflow, when imported
  note_draft       TEXT NOT NULL DEFAULT ''               -- repayment note: stored locally, never sent
);

CREATE TABLE informal_loan_schedule (                      -- promised repayments; the source of informal outflows in the forecast
  id            INTEGER PRIMARY KEY,
  debt_id       INTEGER NOT NULL REFERENCES informal_loan(debt_id),
  due_date      TEXT NOT NULL,
  amount_cents  INTEGER NOT NULL CHECK (amount_cents > 0)
);

CREATE TABLE debt_payment (                                -- repayment log for standalone debts (linked debts read the ledger)
  id            INTEGER PRIMARY KEY,
  debt_id       INTEGER NOT NULL REFERENCES debt(id),
  paid_date     TEXT NOT NULL,
  amount_cents  INTEGER NOT NULL CHECK (amount_cents > 0),
  txn_id        INTEGER UNIQUE REFERENCES txn(id),         -- the outflow row (a transfer to a liability, not an expense)
  note          TEXT NOT NULL DEFAULT '',
  created_at    TEXT NOT NULL
);

CREATE TABLE obligation (
  id                    INTEGER PRIMARY KEY,
  name                  TEXT NOT NULL,
  kind                  TEXT NOT NULL CHECK (kind IN ('bill','debt_minimum','other')),
  status                TEXT NOT NULL DEFAULT 'candidate' CHECK (status IN ('candidate','confirmed','retired')),
  due_rule              TEXT NOT NULL CHECK (due_rule IN ('monthly_day','nth_weekday','biweekly','annual','once')),
  due_day               INTEGER CHECK (due_day BETWEEN 1 AND 31),
  due_month             INTEGER CHECK (due_month BETWEEN 1 AND 12),
  due_weekday           INTEGER CHECK (due_weekday BETWEEN 0 AND 6),      -- 0 = Monday
  due_nth               INTEGER CHECK (due_nth BETWEEN 1 AND 5),          -- 5 = last
  anchor_date           TEXT,                                             -- biweekly anchor / once date
  expected_cents        INTEGER NOT NULL CHECK (expected_cents >= 0),
  variability_cents     INTEGER NOT NULL DEFAULT 0 CHECK (variability_cents >= 0),
  source_account_id     INTEGER NOT NULL REFERENCES account(id),
  autopay               INTEGER NOT NULL DEFAULT 0 CHECK (autopay IN (0,1)),
  category_id           INTEGER REFERENCES category(id),
  debt_id               INTEGER REFERENCES debt(id),                      -- a debt's minimum is an obligation (ADR-0024)
  match_payee_contains  TEXT,
  detected_from_json    TEXT,                                             -- evidence rows for auto-detected candidates
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL
);

CREATE TABLE obligation_payment (                           -- an occurrence that has actually been paid
  obligation_id  INTEGER NOT NULL REFERENCES obligation(id),
  due_date       TEXT NOT NULL,
  txn_id         INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  matched_by     TEXT NOT NULL CHECK (matched_by IN ('user','heuristic')),
  PRIMARY KEY (obligation_id, due_date)
);

CREATE TABLE earmark (
  id                         INTEGER PRIMARY KEY,
  name                       TEXT NOT NULL,
  kind                       TEXT NOT NULL CHECK (kind IN ('obligation','sinking_fund','emergency_reserve')),
  funding_account_id         INTEGER NOT NULL REFERENCES account(id),
  obligation_id              INTEGER UNIQUE REFERENCES obligation(id),
  target_cents               INTEGER NOT NULL CHECK (target_cents >= 0),
  target_date                TEXT,
  schedule                   TEXT NOT NULL CHECK (schedule IN ('none','monthly','per_paycheck','by_date')),
  schedule_amount_cents      INTEGER CHECK (schedule_amount_cents IS NULL OR schedule_amount_cents >= 0),
  schedule_day               INTEGER CHECK (schedule_day BETWEEN 1 AND 31),
  schedule_income_stream_id  INTEGER REFERENCES income_stream(id),       -- per_paycheck
  active                     INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  created_at                 TEXT NOT NULL,
  updated_at                 TEXT NOT NULL,
  CHECK ((kind = 'obligation') = (obligation_id IS NOT NULL))
);

CREATE TABLE earmark_entry (                                -- funding status is derived from these, never stored
  id            INTEGER PRIMARY KEY,
  earmark_id    INTEGER NOT NULL REFERENCES earmark(id),
  entry_date    TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('fund','release','adjust')),
  amount_cents  INTEGER NOT NULL,                           -- positive reserves, negative releases
  txn_id        INTEGER REFERENCES txn(id),                 -- the payment that released it, when any
  note          TEXT NOT NULL DEFAULT '',
  created_at    TEXT NOT NULL
);
CREATE INDEX earmark_entry_earmark ON earmark_entry(earmark_id);

CREATE TABLE variable_spend_override (                      -- user override of the modeled variable spend (§5.7)
  category_id          INTEGER PRIMARY KEY REFERENCES category(id),
  per_30_days_cents    INTEGER NOT NULL CHECK (per_30_days_cents >= 0),
  updated_at           TEXT NOT NULL
);

-- policies -------------------------------------------------------------------
CREATE TABLE policy (
  id           INTEGER PRIMARY KEY,
  code         TEXT UNIQUE,                                 -- 'firewall_exclusion', 'informal_first' (system)
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('firewall_exclusion','informal_first','reminder')),
  params_json  TEXT NOT NULL DEFAULT '{}',
  is_system    INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),
  created_at   TEXT NOT NULL
);

CREATE TABLE firewall_ack (                                 -- the logged acknowledgment for an outflow from a firewalled account
  id               INTEGER PRIMARY KEY,
  txn_id           INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  acknowledged_at  TEXT NOT NULL,
  note             TEXT NOT NULL DEFAULT ''
);

-- reconciliation -------------------------------------------------------------
CREATE TABLE reconciliation (
  id                       INTEGER PRIMARY KEY,
  account_id               INTEGER NOT NULL REFERENCES account(id),
  period_start             TEXT NOT NULL,
  period_end               TEXT NOT NULL,
  opening_cents            INTEGER NOT NULL,                -- roll-forward: prior balanced closing, or account opening balance
  statement_closing_cents  INTEGER NOT NULL,
  statement_source         TEXT NOT NULL CHECK (statement_source IN ('user','file')),
  computed_closing_cents   INTEGER NOT NULL,                -- CACHE: refreshed in the same transaction as any write to the period
  difference_cents         INTEGER NOT NULL,                -- CACHE: computed − statement
  status                   TEXT NOT NULL CHECK (status IN ('balanced','off')),   -- CACHE: balanced iff difference = 0
  balanced_at              TEXT,
  created_at               TEXT NOT NULL,
  updated_at               TEXT NOT NULL,
  UNIQUE (account_id, period_end)
);

-- review, snapshots, backups ---------------------------------------------------
CREATE TABLE snapshot (
  id                        INTEGER PRIMARY KEY,
  taken_at                  TEXT NOT NULL,
  civil_date                TEXT NOT NULL,
  kind                      TEXT NOT NULL CHECK (kind IN ('daily','on_demand','plan')),
  safe_cents                INTEGER NOT NULL,
  available_cents           INTEGER NOT NULL,
  earmarks_cents            INTEGER NOT NULL,
  obligations_cents         INTEGER NOT NULL,
  buffer_cents              INTEGER NOT NULL,
  trusted                   INTEGER NOT NULL CHECK (trusted IN (0,1)),
  total_debt_cents          INTEGER NOT NULL,
  informal_remaining_cents  INTEGER NOT NULL,
  venture_cap_used_cents    INTEGER NOT NULL,
  detail_json               TEXT NOT NULL                   -- per-account balances, terms with row ids, forecast series for 'plan'
);
CREATE UNIQUE INDEX snapshot_daily ON snapshot(civil_date) WHERE kind = 'daily';

CREATE TABLE review (
  id                   INTEGER PRIMARY KEY,
  started_at           TEXT NOT NULL,
  completed_at         TEXT,
  status               TEXT NOT NULL CHECK (status IN ('in_progress','completed','abandoned')),
  period_start         TEXT NOT NULL,
  period_end           TEXT NOT NULL,
  surplus_cents        INTEGER,                             -- dependable surplus (negative = deficit), set at completion
  surplus_detail_json  TEXT,
  steps_json           TEXT NOT NULL DEFAULT '{}',          -- what each step showed and what was acknowledged
  notes                TEXT NOT NULL DEFAULT ''
);

CREATE TABLE review_action (
  id         INTEGER PRIMARY KEY,
  review_id  INTEGER NOT NULL REFERENCES review(id),
  position   INTEGER NOT NULL CHECK (position BETWEEN 1 AND 3),
  text       TEXT NOT NULL CHECK (length(trim(text)) > 0),
  done       INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0,1)),
  done_at    TEXT,
  UNIQUE (review_id, position)
);

CREATE TABLE backup_log (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  kind        TEXT NOT NULL CHECK (kind IN ('daily','manual','pre_migration','pre_restore')),
  bytes       INTEGER NOT NULL,
  verified    INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1)),
  created_at  TEXT NOT NULL
);
```

Seed rows written by `0001_init.sql` (all `is_system = 1`, names editable, not deletable):

- Settings: `zone = "America/Chicago"`, `timing_buffer_cents = 0`, `recon_stale_after_days = 45`, `dedup_similarity_bps = 8000` (raised to 8500 by migration 0002 with the Jaro–Winkler metric, ADR-0037), `theme = "dark"`, `backup_keep_daily = 14`.
- Category roots for each `root_kind`, plus system-coded children the engines reference (ADR-0033): `income.salary`, `income.bonus`, `income.rsu`, `income.deferred_comp`, `income.interest`, `income.other`, `transfer.internal`, `transfer.card_payment`, `transfer.loan_repayment`, `transfer.borrowing_proceeds`, `transfer.securities_sale_proceeds`, `debt.interest`, `debt.fees`, `venture.customer_revenue`, `venture.operating_expense`, `venture.owner_contribution`, `venture.financing`, `venture.withdrawal`, `variable.cash`, `variable.uncategorized`.
- Policies: `firewall_exclusion`, `informal_first`.
- Import profiles: `generic_csv` (Date, Description, Amount). The fixture institutions' profiles (§14) arrive with the fixtures in M1's migration, so no column layout is invented before the files exist.

Forward references (`txn` → `transfer_link`, `refund_link`) are legal in SQLite because foreign
keys are checked at DML time; link tables and their back-pointers on `txn` are kept consistent by
the Rust command that writes both inside one transaction.

---

## 5. Derived figures — exact definitions

All figures are functions of ledger rows + plan rows + settings, computed in Rust, and returned
with the row ids behind them.

### 5.1 Balances (per account)

- `posted_balance = opening_balance_cents + Σ amount_cents of txn_leaf rows with status='posted'`. A posted row dated after `today()` or before `opening_date` is rejected at write time (`Validation`), so no date filter is needed for "now".
- `posted_balance_as_of(d) = opening + Σ posted leaf rows with posted_date ≤ d` (snapshots, reconciliation, trends).
- `pending_in = Σ positive pending leaf rows without flags borrowing|securities_sale`; `pending_in_excluded = Σ positive pending rows with those flags` (listed, not counted).
- `pending_out = Σ negative pending leaf rows` (all of them).
- Liability accounts: `owed = max(0, −posted_balance)`.

### 5.2 Spending view and cash view (ADR-0020)

- **Spending view** (what was consumed): Σ over `txn_leaf` rows where `transfer_link_id IS NULL`, root_kind ∉ {`transfer`, `income`}, and the row is not borrowing/securities-sale proceeds, grouped by category. A linked refund (positive) sits in the original's category and nets against it. A card purchase counts once, on the card, on its posted date. The card payment (a `card_payment` transfer) counts zero.
- **Cash view** (what left the cash accounts, when): Σ over `txn_leaf` rows on cash-kind accounts (`checking|savings|cash|payment_app`), by date, including the card payment as an outflow on its date and excluding card purchases (they are on the credit account). A transfer between two cash accounts appears as −x and +x on their dates and nets to zero in aggregate (`transfer_not_spending`).
- `EXPECTED.md` states both totals for the fixture period; they differ by a known amount (M2 acceptance).

### 5.3 Reconciliation (ADR-0021)

- Period `[period_start, period_end]` per account. `opening_cents` = `account.opening_balance_cents` for the first period (then `period_start = opening_date`), else the prior **balanced** period's `statement_closing_cents` (roll-forward).
- `computed_closing = opening_cents + Σ posted leaf rows with posted_date in [start, end]`; `difference = computed − statement_closing`; `status = balanced iff difference = 0`. No tolerance: a one-cent difference is a missing or wrong row.
- The difference explorer lists: rows in the period, rows just outside the period (±5 days) that could belong, pending rows, quarantined rows for the account, and the running total.
- **Trust per account**: `reconciled` iff the latest reconciliation (by `period_end`) is `balanced` and `period_end ≥ today − stale_after_days` (account override, else setting, default 45). Otherwise `never_reconciled | stale | off`, each with the dates behind it.
- **Hero trust**: trusted iff every account contributing to `available` is `reconciled`. Untrusted rendering names the accounts and the reason.

### 5.4 Safe-to-spend (ADR-0022) — the spec's formula, verbatim

```
available = sum over non-firewalled, non-archived cash accounts
            (posted balance + pending inflows that are not borrowing or securities sales)
            − pending outflows
earmarks_unfunded = sum of earmark remaining that is funded from those accounts
obligations_before_next_income = confirmed obligations with due date
            on or before the next confirmed income date, not yet paid
minimum_buffer = the buffer setting (default 0 until the user sets it)
safe = available − earmarks_unfunded − obligations_before_next_income − minimum_buffer
```

Precise terms:

- **Account set A** = accounts with `kind ∈ {checking, savings, cash, payment_app}`, `owner = 'personal'`, `firewalled = 0`, `archived = 0`. Credit, brokerage, loan and venture-owned accounts are never in A (`firewall_excluded`; credit available is not cash).
- `available = Σ_A (posted_balance + pending_in − pending_out)`. Posted borrowing or securities-sale proceeds sitting in an A account are in `posted_balance` (they are cash in the bank and must reconcile); they are flagged, listed on their own panel, and never counted as income. Only _pending_ flagged inflows are excluded.
- `earmark_remaining(e) = Σ earmark_entry.amount_cents` for active earmarks; `earmarks_unfunded = Σ earmark_remaining(e)` over earmarks whose `funding_account_id ∈ A`. The emergency reserve is an earmark of kind `emergency_reserve`, so it enters here; the timing buffer is `minimum_buffer`. Two reserves, stored separately, each subtracted once.
- `next_income_date` = earliest occurrence `≥ today()` of an active income stream with `confidence = 'confirmed'` that has no `income_receipt`. `expected`/`rumored` streams never enter. If no confirmed stream exists, the window is `today + 30` and the term carries `window_reason = 'no_confirmed_income'` (warning label, not untrusted).
- `obligations_before_next_income = Σ over occurrences o of confirmed obligations with due_date ≤ next_income_date and no obligation_payment: max(0, expected_cents − earmark_remaining(linked earmark))`. Overdue unpaid occurrences are included. An obligation fully covered by its earmark contributes 0 here and its earmark contributes in the earmark term: each dollar is subtracted exactly once.
- `minimum_buffer = setting.timing_buffer_cents` (default 0).
- `safe = available − earmarks_unfunded − obligations_before_next_income − minimum_buffer` and the returned object satisfies `safe_terms_sum` by construction (the total is computed from the returned terms, not separately).

Returned shape (`cmd::safe_to_spend`):

```
SafeToSpend {
  as_of: CivilDate, safe_cents,
  terms: {
    available:   { cents, accounts: [{ account_id, posted_cents, pending_in_cents, pending_out_cents, pending_row_ids }] },
    earmarks:    { cents, items: [{ earmark_id, remaining_cents, entry_ids }] },
    obligations: { cents, next_income: { date, stream_id } | null, window_reason,
                   items: [{ obligation_id, due_date, expected_cents, earmark_covered_cents, counted_cents }] },
    buffer:      { cents }
  },
  excluded: { firewalled_accounts, venture_accounts, pending_flagged_inflows: [{ txn_id, cents, flag }] },
  trust: { trusted, accounts: [{ account_id, status, last_balanced_period_end }] }
}
```

### 5.5 Pay-cycle and due-rule occurrences (`dates`)

- `weekly|biweekly`: `anchor + 7k | 14k` days. `semimonthly`: the two days each month, 31 = last day. `monthly`: anchor's day clamped to month length. `once`: anchor only. Then `weekend_rule` (income only; default previous business day; US holidays not modeled in v1, the user can override a single occurrence by editing the stream's anchor or recording the receipt).
- Obligations: `monthly_day` clamped; `nth_weekday` (5 = last); `biweekly` from anchor; `annual` on `due_month/due_day`; `once`. No weekend shift: being early is conservative.
- Payment/receipt matching (heuristic, user-overridable): same account, payee match, amount within `expected ± variability`, posted within `[due − 10, due + 5]` days.

### 5.6 Forecast (ADR-0023)

- One daily engine over days `0..=90` from `today()`; the 30-day view is days 0–29, the 13-week view is seven-day buckets from day 0.
- `opening = Σ_A posted_balance`. Day-0 events: pending rows (`effective_date ≤ today → day 0`, else their date) with the §5.1 sign rules.
- Inflows: confirmed income occurrences in the window with no receipt. Outflows: unpaid confirmed obligation occurrences (overdue → day 0), `informal_loan_schedule` rows in the window, and modeled variable spend (§5.7). Debt minimums are obligations (`obligation.debt_id`), so they appear once.
- Earmarks do not move cash; they feed `committed(d) = Σ earmark_remaining projected by schedule + minimum_buffer`, and `headroom(d) = balance(d) − committed(d)`.
- Outputs: per-day rows `{date, opening, inflows, outflows, closing, committed, headroom, events[]}`; `lowest_balance {cents, date}`; `first_shortfall {date, cents}` where `closing < 0`; `first_buffer_breach {date, cents}` where `headroom < 0`.
- Downside scenario: next confirmed base-pay occurrence shifted +7 civil days; `expected`/`rumored` streams excluded (already excluded in baseline; stated so the toggle is honest); a user-entered surprise bill `{date, cents}`.
- Plan overlay: when a `plan` snapshot exists, its stored series is drawn against the baseline.
- `forecast_ties`: for every scenario, `closing(90) = opening + Σ inflows − Σ outflows` and each day `closing = opening + inflows − outflows`.

### 5.7 Variable-spend model

Per category with `root_kind = 'variable'`: three 30-day buckets ending yesterday (`[d−30, d−1]`, `[d−60, d−31]`, `[d−90, d−61]`); net outflow per bucket from `txn_leaf` rows excluding transfer-linked rows, floored at 0; the model is the median (middle value) of the three; `variable_spend_override` replaces it. Daily allocation uses `money::allocate(median, 30)` so every 30 forecast days sum exactly to the modeled figure.

### 5.8 Debt (ADR-0024)

- Balance owed: linked debt → `owed` of its account; standalone → `standalone_opening_cents − Σ debt_payment.amount_cents`.
- Interest per period: `monthly_nominal` → `mul_div_round(balance, apr_bps, 120_000)`; `actual_365` → `mul_div_round(balance, apr_bps × days, 3_650_000)`. Effective APR = `promo_apr_bps` while `today ≤ promo_end`, else `apr_bps`.
- Minimum per period by `minimum_rule`: `fixed`; `percent_of_balance` = `max(floor, mul_div_round(balance, minimum_bps, 10_000))`; `interest_plus_percent` = `interest + max(floor, pct)`; `full_balance`; `none` (informal loans with a schedule).
- Strategy: every participating debt gets its minimum; the monthly `extra_cents` input (defaults to the latest review's dependable surplus if positive; the UI names the source) goes to one target: **avalanche** highest effective APR (tie → smaller balance), **snowball** smallest balance, **custom** `custom_order`. Policy `informal_first`: informal debts absorb extra first, by `promised_date`. Payoff frees that minimum for the next target.
- Output per strategy: schedule rows per debt per period `{period_start, opening, interest, payment, principal, closing}`, total interest cents, payoff date; comparison table in cents. "Informal repaid in 12 months" is a scenario: if the surplus cannot, show the gap and the date it can.

### 5.9 Venture rollup (ADR-0025)

Rows with `venture_id = V` partition into buckets by category system code: `customer_revenue`, `operating_expense`, `owner_contribution`, `financing`, `withdrawal` (transfer links of kind `venture_contribution`/`venture_withdrawal` map to the last two). Derived: `operating_cash_flow = customer_revenue − operating_expense` (trailing 12 months); `cap_used = owner_contribution + operating_expense paid from personal accounts − withdrawal`; `cap_utilization_bps = cap_used / cash_cap_cents`; milestone countdown in civil days; stop-condition alert when `cap_used ≥ cash_cap_cents` or `milestone_date < today`. `venture_spend_share_bps = Σ operating_expense (all ventures, 12 mo) / Σ confirmed base-pay receipts (12 mo)`.

### 5.10 Review and dependable surplus (ADR-0026)

Monthly-equivalent, from rows only: `income = Σ income_receipt amounts of confirmed streams in the trailing 90 days × 30/90` (`mul_div_round`); `fixed = Σ monthly-equivalent expected of confirmed obligations with kind ≠ debt_minimum` (weekly ×52/12, biweekly ×26/12, annual ÷12 via `mul_div_round`); `debt_service = Σ debt_minimum obligations monthly-equivalent + Σ informal_loan_schedule next 12 months ÷ 12`; `irregular = Σ annual obligations ÷ 12 + Σ sinking-fund schedules monthly-equivalent`; `variable = Σ variable-spend model`. `surplus = income − fixed − debt_service − irregular − variable`. Borrowing and asset sales are excluded by construction (only confirmed-stream receipts count as income).

---

## 6. Engines

### 6.1 Import profiles (`import_profile.spec_json`)

```json
{
  "header_signature": ["Date", "Description", "Amount", "Running Bal."],
  "skip_rows": 0,
  "date": { "column": "Date", "format": "%m/%d/%Y" },
  "effective_date": { "column": "Transaction Date", "format": "%m/%d/%Y", "optional": true },
  "amount": { "kind": "single_signed", "column": "Amount" },
  "payee": { "column": "Description" },
  "memo": { "column": "Memo", "optional": true },
  "status": { "column": "Status", "pending_values": ["Pending"], "optional": true },
  "external_id": { "column": "Reference", "optional": true },
  "balance": { "column": "Running Bal.", "optional": true },
  "currency": { "column": "Currency", "optional": true },
  "sign_convention": "account_pov"
}
```

`amount.kind ∈ {single_signed, debit_credit (two columns), amount_with_type (type column lists debit values)}`; `sign_convention ∈ {account_pov, card_statement}` (card statements show purchases positive; the profile negates). Also: `skip_rows` for a preamble, `effective_date` as a second date column, `payee` as a single column, several columns, or a counterparty rule (`inflow_column` / `outflow_column` / `fallback_column`), `memo` as one or several columns, `row_flags` applied to every row, `flags_by_type` (type value → flag names), and `skip_when` (column, allowed values, reason) for rows that belong on another account's ledger (ADR-0037). Detection: fold case and whitespace, compare the header row to each profile's signature; exactly one match → auto; otherwise the user picks and sees the mapping preview (first 20 parsed rows) before commit. Amount parsing is integer-only: strip `$`, `,`, handle `(x)` and leading `-`, split on `.`, at most two decimals, else `Parse { row, column }`. A `currency` column present with any value other than `USD` rejects the batch with `Unsupported`.

Payment-app profiles (Venmo) set `payment_app_unknown | needs_review` on every row; the note is kept in `memo` and is never used by heuristics (ADR-0032).

### 6.2 Identity and dedup (ADR-0017)

- `source_row_hash = sha256(account_id ‖ posted_date ‖ amount_cents ‖ trim(payee_raw) ‖ trim(memo) ‖ external_id?)`, hex.
- File-level idempotency: `(account_id, profile_id, file_sha256)` seen before → the batch is recorded with `inserted = 0`, every row `skipped`, and `dedup_report_json.reason = "duplicate_file_of_batch:<id>"`. Nothing silently dropped: the report says so.
- Row-level: exact match on `(account_id, external_id)` or `(account_id, source_row_hash)` → skip. Else fuzzy candidates: same account, same `amount_cents`, `posted_date` within ±3 days, `similarity_bps(payee_norm_a, payee_norm_b) ≥ setting.dedup_similarity_bps` (Jaro–Winkler, default 8500, ADR-0037). If the candidate is `pending` and the incoming row is `posted`, or the candidate lacks an `external_id` the incoming row has → **update** system fields whose `user_edited` bit is 0 (`status`, `posted_date`, `effective_date`, `payee_norm`, `memo`; `payee_raw`, `external_id` and `source_row_hash` always follow the latest observation) and count `updated`. If the candidate is `posted` and the incoming row is `pending` → **skip** as an older observation. Otherwise → **quarantine**: the row is stored in `import_quarantine` with the suspected twin and the similarity; the review queue resolves it as `inserted` or `discarded`, both audited. The reconciliation difference explorer shows pending quarantine rows for the account, so a real transaction cannot hide there unnoticed.
- `dedup_no_cross_account_collapse`: candidates are always restricted to the same account.
- Batch undo: inside one transaction, delete rows the batch inserted, restore the `before_json` of rows it updated, discard its quarantine rows, set `undone_at`, write audit rows under an `undo` command. Refused with `Conflict` if any inserted row has since been split, linked, or user-edited (the UI lists them).

### 6.3 Dedup report (`dedup_report_json`)

`{ reason?, rows_read, inserted, updated: [{txn_id, fields}], skipped: [{row, matched_txn_id, by: "hash|external_id"}], quarantined: [{quarantine_id, suspected_txn_id, similarity_bps}], threshold_bps, date_range }` rendered as prose: "Read 412 rows. Inserted 380. Updated 12 pending rows that have now posted. Skipped 18 already-imported rows. Held 2 suspected duplicates for review (threshold 80%)."

### 6.4 Rules and heuristics

Order per row: (1) rules by `position`, first match wins, `classification = rule`, `rule_id` set; (2) heuristics in a fixed order, `classification = heuristic`, `heuristic_code` set; (3) else `unclassified` + `needs_review`. Heuristic codes (implemented at M2, `rules/heuristics.rs`): `securities_sale` (flag → `transfer.securities_sale_proceeds`), `fee_charge` (→ `debt.fees` + `fee`), `interest_charge` / `interest_income` (→ `debt.interest` / `income.interest` + `interest`), `atm_withdrawal` (→ `cash_withdrawal | needs_review`, no category), `payment_app_row` (→ `payment_app_unknown | needs_review`, no category). Link detection (`rules/link.rs`) then writes `<kind>_pair` on both legs of a transfer it links (`internal_pair`, `card_payment_pair`, `loan_repayment_pair`, `venture_contribution_pair`, `venture_withdrawal_pair`), `refund_match` on a refund it links to a still-uncategorised inflow, and `refund_candidate` / `transfer_ambiguous` on rows it only flags for the person. A firewall touch is not a code: an outflow on a firewalled account keeps `needs_review` until its `firewall_ack` row exists, whatever classified it. Heuristics never assign a category to cash withdrawals or payment-app rows. The review queue (`needs_review` set, or `unclassified`) orders by `|amount_cents|` descending. A correction (`txn.update` with a category) sets `classification = manual`, clears `needs_review | payment_app_unknown` unless a firewall acknowledgment is pending, and `propose_rule` returns a proposed rule shape (payee contains `payee_norm` → the chosen category, with how many other rows it would match today); nothing is created until the user accepts it. `apply_rules` re-runs rules → heuristics → detection over every unlinked leaf row the user has not categorised by hand; the run is idempotent (`changed = 0` on a settled ledger) and a rule's `hit_count` counts a row once (ADR-0039).

### 6.5 Linking (ADR-0019)

- Transfer: two rows on different accounts, `a.amount = −b.amount`, posted within ±3 days, neither linked; kind `card_payment` when one account is `credit`, `loan_repayment` when `loan`, `venture_contribution/withdrawal` when exactly one account is venture-owned, else `internal`. Confidence `heuristic`; the user can unlink or link manually (`user`).
- Refund: positive row on an expense category's account, matching a prior negative row with `|amount|` equal, payee similarity ≥ threshold, within 90 days → `refund_candidate` in the review queue; linking is a user action unless the payee and amount match exactly (then heuristic link, still shown).
- Linking writes both legs in one transaction (`db/repo/link.rs`): a transfer sets the kind's category on both rows, `classification = heuristic` (detected) or `manual` (user; `user_edited` gains the category bit), `heuristic_code = <kind>_pair`, and clears `needs_review | payment_app_unknown` except on a pending firewall touch. A refund link gives a still-uncategorised refund the original's category and `refund_match`; a refund a rule or the user already categorised keeps that decision (ADR-0039). Unlinking returns the rows to `unclassified` + `needs_review`. A split row is not linkable (link its parts); automation never re-categorises a linked row.
- Undoing an import batch removes the links its automation created first (the other leg returns to review), then deletes or restores the batch's rows (§11).

### 6.6 Export and backup

- Full export: one CSV per table + one JSON document, decimal strings produced by `money::to_decimal_string`, written to a user-chosen folder.
- Audit pack: `ledger.csv`, `reconciliation.csv`, `safe_to_spend.json` (terms with row ids), `forecast.json`, `debt_schedule.csv`, `venture_rollup.csv`, `README.md` explaining each file and the sign convention — enough for an advisor or an LLM session.
- Backup: `ATTACH DATABASE ? AS b KEY ?; SELECT sqlcipher_export('b'); DETACH DATABASE b;` under the current or a new passphrase. Daily rotating on launch (`backups/kept-YYYY-MM-DD.db`, keep `backup_keep_daily`); `pre_migration` before any migration; `pre_restore` before a restore swap. Restore: open the backup with its passphrase → export into a temp data dir → migrate → compare row counts per table and the hero number against the live DB → show the comparison → the user confirms the swap.

---

## 7. IPC contract and error taxonomy

Every command: `#[tauri::command] async fn name(state: State<'_, App>, args: Args) -> Result<T, AppError>`; `T` and `Args` are `serde` structs mirrored by hand-written TS types in `src/lib/ipc.ts` (checked by a Vitest shape test against JSON fixtures the Rust tests emit). Commands that write more than one row run in one `rusqlite` transaction and write their audit rows inside it. After any write, the core emits `kept://changed { entities: [...] }`; the webview maps entities → TanStack Query keys and invalidates; the hero recomputes on every relevant write.

`AppError` is `thiserror` on the Rust side and serializes as `{ kind, message, detail? }`:

| kind                       | raised when                                                                                     | UI                                       |
| -------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------- |
| `Locked`                   | a command runs with no open database                                                            | route to Unlock                          |
| `WrongPassphrase`          | SQLCipher key check fails (`SQLITE_NOTADB`)                                                     | inline error, fail closed                |
| `Db`                       | any other SQLite error (message included, SQL never echoed with values)                         | toast + log                              |
| `Migration`                | checksum mismatch, failed step, newer schema than the binary                                    | blocking dialog, do not open             |
| `Io`                       | data folder, backup, export, log file                                                           | toast with path                          |
| `Parse { row, column }`    | CSV/OFX cell cannot be parsed as date, integer cents, or status                                 | import screen row marker                 |
| `Unsupported`              | non-USD currency, unknown file format                                                           | import blocked with reason               |
| `Validation { field }`     | business rule: future posted date, split sum ≠ parent, three actions, archived account write, … | field error                              |
| `Conflict`                 | undo blocked by later edits; concurrent edit of a reconciled period                             | dialog listing the rows                  |
| `NotFound`                 | id does not exist                                                                               | toast                                    |
| `PolicyBlocked { policy }` | firewalled outflow without acknowledgment                                                       | acknowledgment dialog, logged on confirm |
| `Overflow`                 | checked money arithmetic overflowed                                                             | toast + log; never a wrong number        |
| `Internal`                 | invariant violated (e.g. cache drift detected)                                                  | toast; logged at error with context      |

No `unwrap()`/`expect()` outside tests; `?` everywhere; the webview never swallows an error (every `catch` either shows it or rethrows).

---

## 8. Migration strategy (ADR-0011)

- `src-tauri/migrations/NNNN_name.sql`, embedded with `include_str!`, applied in order inside one transaction each; `schema_migration` records version, name, sha256 of the file text, and `applied_at`; `PRAGMA user_version` mirrors the latest version.
- Startup: verify checksums of applied migrations (mismatch → `Migration`, refuse to open: the file in the binary differs from what built this database); refuse to open a database whose version is newer than the binary knows; take a `pre_migration` backup before applying anything; then apply pending migrations.
- Forward-only. No down migrations; the backup is the rollback. SQLite's limited `ALTER` means table rewrites use create-copy-drop-rename inside the migration with `PRAGMA foreign_keys = OFF` for that transaction and `PRAGMA foreign_key_check` after.
- Seeds live in migrations too (idempotent `INSERT OR IGNORE` on `system_code`).
- Tests: every migration applied to an empty DB (M0 acceptance), to the previous version's fixture DB, and `PRAGMA integrity_check` + `foreign_key_check` after.

---

## 9. Encryption, keys, and the data folder (ADR-0012, ADR-0030)

- Portable layout: `Kept.exe` + `kept.config.json` beside it (`{ "data_dir": "…" }`, chosen on first run via the dialog plugin; env `KEPT_DATA_DIR` overrides for tests). Data folder: `kept.db` (+ `-wal`, `-shm`), `logs/`, `backups/`, `exports/`.
- SQLCipher 4 defaults (AES-256-CBC, HMAC-SHA512, PBKDF2-HMAC-SHA512, 256 000 iterations, per-page IV). Open sequence: `PRAGMA key = ?` → `SELECT count(*) FROM sqlite_master` (wrong key → `SQLITE_NOTADB` → `WrongPassphrase`, nothing else happens) → `PRAGMA foreign_keys = ON; journal_mode = WAL; synchronous = NORMAL; temp_store = MEMORY; busy_timeout = 5000` → checksum check → migrations.
- Remember passphrase (opt-in): `keyring` 4.x with the `windows-native-keyring-store` feature stores the passphrase itself under service `Kept`, user `sha256(data_dir)`; Windows Credential Manager protects it with the user's logon (DPAPI). Forget = delete the entry. The passphrase is held in a `zeroize`d buffer and dropped on lock.
- Change passphrase: fresh `manual` backup → `PRAGMA rekey = ?` → update keyring if remembered. Backups re-encrypt via `sqlcipher_export` under the chosen passphrase (§6.6).
- One `rusqlite::Connection` behind `Mutex<Option<Connection>>` in Tauri state (ADR-0013): `None` while locked; commands take the lock for the duration of their transaction.

---

## 10. Threat model

Assets: the ledger (complete financial history, counterparties, informal debts), the passphrase, backups, exports, logs.

| Attacker                                                                                         | Capability                                | Outcome                                                                                                                                                                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------------------------------ | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **In scope:** local attacker with the database file (and `-wal`, backups) but not the passphrase | copies files, runs offline tools          | Cannot read pages: SQLCipher 4 full-database encryption incl. WAL; no plaintext temp files (`temp_store = MEMORY`); logs carry no amounts, payees, counterparties, or key material (§11); the keyring entry is bound to the Windows user's logon, not to the file. Brute force is bounded by PBKDF2 at 256k iterations — passphrase strength is the user's lever and the Unlock screen says so. |
| In scope: the same attacker also holding an **export or audit pack**                             | reads plaintext by design                 | Exports are written only where the user chooses, the UI states they are unencrypted, and they are never created automatically.                                                                                                                                                                                                                                                                  |
| In scope: a hostile **import file**                                                              | malformed CSV/OFX, huge rows, path tricks | Streaming parser with row/column error reporting; no formulas evaluated; paths come from the dialog or drag-drop only; the batch is one transaction — any error leaves nothing written.                                                                                                                                                                                                         |
| In scope: **network exfiltration**                                                               | malicious dependency or script            | CSP `connect-src` limited to the IPC origin; no HTTP/updater/shell plugins; `just check` greps for network APIs in `src/` and network crates in `Cargo.lock`; fonts self-hosted; Windows acceptance records a `netstat` check.                                                                                                                                                                  |
| **Out of scope:** attacker with the unlocked session, the user's Windows logon, or a keylogger   | reads memory, uses the keyring            | Explicitly out of scope per spec; `cipher_memory_security` stays off for performance and this is recorded.                                                                                                                                                                                                                                                                                      |

---

## 11. Logging, audit, undo

- `tracing` → `tracing_appender` daily rotation into `data_dir/logs/kept.YYYY-MM-DD.log`, non-blocking, `max_log_files = 14`, level `info` (env `KEPT_LOG` raises it). Fields are ids, counts, durations, error kinds, command names. **Never logged:** `amount_cents`, `payee_*`, `memo`, counterparties, passphrases, keys, file contents. No `println!`.
- Audit: every write command creates one `command` row and one `audit_event` per touched row (before/after JSON of the row) in the same transaction (`db::audit`). The Ledger shows "why" (rule / heuristic / user / import batch) from these columns.
- Undo (ADR-0016): a command group is undone by applying the inverse of its audit events in reverse order as a new command with `actor = 'undo'`, refused with `Conflict` when a touched row changed since. Every destructive action is a command group; the toast names it ("Undo: recategorize 14 rows"). Automation inside an import belongs to the import's command group and may touch a row several times, so undo works per row: it compares the row with the last state the command left and restores the first state it found; links the command created are removed before its rows are deleted or restored (ADR-0039).

---

## 12. Offline guarantees

Default is offline and stays offline. No updater, HTTP, or shell plugin is registered. `tauri.conf.json` CSP: `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:; font-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'` (the `style-src` allowance is for visx inline style attributes; verified at M0 and tightened with hashes if possible). The only outbound actions are those the spec names and the user triggers; none exist in v1 scope except file import. `just check` fails on `fetch(`, `XMLHttpRequest`, `WebSocket`, `sendBeacon`, `navigator.onLine` in `src/`, and on `reqwest|hyper|ureq|tauri-plugin-http|tauri-plugin-updater|tauri-plugin-shell` in `Cargo.lock`.

---

## 13. Testing architecture and `just check` (ADR-0009)

| Layer       | Tool                                           | Scope                                                                                                                                                                                                                                                   |
| ----------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Unit        | `cargo test`                                   | money rounding and allocation, date rules, payee normalization, amount parsing, interest math                                                                                                                                                           |
| Property    | `proptest`                                     | the ten named invariants in `src-tauri/tests/invariants.rs`, each test named exactly as the spec names it                                                                                                                                               |
| Integration | `cargo test` + temp SQLCipher DB + `fixtures/` | import → dedup → link → recon → safe-to-spend → forecast → debt → venture, asserting `EXPECTED.md` numbers                                                                                                                                              |
| TS unit     | Vitest                                         | `formatCents`, `formatBps`, `parseCentsInput`, chip parser, IPC type shapes                                                                                                                                                                             |
| E2E         | Playwright                                     | critical path import → reconcile → safe-to-spend against the real app: Windows only, WebView2 launched with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`, Playwright attaches over CDP, `KEPT_DATA_DIR` points at a temp folder |

`just check` = `fmt-check`, `clippy -D warnings`, `cargo test`, `tsc --noEmit`, `eslint`, `prettier --check`, `vitest run`, the grep gates (no `TODO`, no commented-out code markers, no `any`, no `unwrap()`/`expect(` outside tests, no `println!`, no network APIs), then `e2e` **when the host is Windows**. On a non-Windows host `check` ends with an explicit `E2E NOT RUN: requires Windows/WebView2` line and a non-zero `e2e` recipe exit, never a silent green. CI: `ci.yml` runs lint/tests on `ubuntu-latest` for speed and the full `just check` + unsigned build on `windows-latest`; the Windows job is the required one. `release.yml` builds the unsigned NSIS installer on `v*` tags; signing is a documented manual `signtool` step (`docs/release.md`).

---

## 14. Fixture plan (`fixtures/`, written before the engines — M1 starts here)

Sources (one CSV profile each, synthetic names): three banks `northbank_checking`, `northbank_savings`, `riverside_checking`; two cards `summit_visa`, `summit_amex`; one brokerage `harbor_brokerage` (firewalled); one payment app `venmo`. Period: three statement months. `fixtures/EXPECTED.md` is written first, by hand, with the arithmetic shown, and holds: opening and closing per account per month, spending-view total, cash-view total, the transfer pairs, safe-to-spend for a stated as-of date, next pay date and buffer, the first shortfall date, the two-debt schedule, the venture buckets. Scenarios mapped to files:

| Scenario (spec)                       | Where                                                                                                                       |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| duplicate export of the same month    | `northbank_checking` month 2 exported twice (identical) and once overlapping months 2–3                                     |
| pending that later posts              | `summit_visa` month 1 export has a pending row; month 2 export has it posted with a longer descriptor                       |
| a refund                              | `summit_visa` purchase then partial refund 11 days later                                                                    |
| an internal transfer                  | `northbank_checking → northbank_savings`                                                                                    |
| a card payment                        | `northbank_checking → summit_visa`                                                                                          |
| rent split via Zelle                  | `northbank_checking` Zelle outflow (rent) and Zelle inflow from roommate (needs_review, not income until classified)        |
| a foreign ATM fee                     | `riverside_checking` ATM withdrawal (cash_withdrawal, needs_review) + separate fee row (fee)                                |
| a securities sale used to cover a gap | `harbor_brokerage` sale (securities_sale, firewalled) → transfer to `northbank_checking` (flagged proceeds, firewall touch) |
| a friend-loan inflow                  | `venmo` inflow from a named friend → user flags borrowing, creates informal loan                                            |
| a venture SaaS charge                 | `summit_amex` SaaS charge tagged to venture `Ledgerline`                                                                    |
| an annual insurance renewal           | `northbank_checking` annual premium (irregular)                                                                             |

No number in `EXPECTED.md` is produced by the engine. If the engine disagrees, the engine is wrong until the fixture is shown wrong, and that showing is an ADR.

---

## 15. Performance budget

200k leaf rows must scroll without jank (M10, machine documented). Ledger reads are keyset-paginated on `(posted_date, id)` with the chip filter compiled to indexed `WHERE` clauses; counts via `COUNT(*)` on the same filter; TanStack Virtual renders ~40 rows. Import of 200k rows: one transaction, prepared statements, dedup lookups on `txn_dedup`. Safe-to-spend and reconciliation are index-backed `SUM()`s per account. Targets to record at M10: ledger page fetch < 50 ms, hero recompute < 100 ms, 200k-row import < 60 s.

---

## 16. Frontend architecture

- `src/tokens.css` holds the spec's tokens on `:root` (dark) and the light swap under `[data-theme="light"]`; Tailwind 4 `@theme` maps them to utilities; charts read the same CSS variables through `src/tokens/`. IBM Plex Sans/Mono self-hosted from `@fontsource/*` (OFL). 4px spacing scale; type 12/14/16/20/28; every money figure uses `font-variant-numeric: tabular-nums` and the mono face.
- Untrusted = dashed underline + label naming the accounts, everywhere a figure appears, including the hero.
- Query chips (ADR-0029): `account:`, `cat:`, `tag:`, `venture:`, `>100` / `<50` / `>=` / `<=` / `=`, `needs:review`, `flag:<name>`, `status:pending`, `date:YYYY-MM-DD..YYYY-MM-DD`, free text → payee/memo contains. Parsed in TS to a `LedgerFilter` JSON; compiled to SQL in Rust; both sides tested.
- State: TanStack Query for everything from the core; Zustand for UI state only; no balance is ever derived in React.
- Dashboard fits 1440×900 without page scroll (Playwright asserts `document.scrollingElement.scrollHeight <= 900` at that viewport). Dark by default; light is a token swap. No emoji, mascot, gradient, or illustration; empty states name what is missing and the command that fixes it.
