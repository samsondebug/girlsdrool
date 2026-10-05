-- Kept schema v1. Canonical copy of ARCHITECTURE.md §4. Never edit after it is committed: the
-- runner verifies this file's checksum against every database it opens. Fix mistakes in 0002+.
-- Conventions: money INTEGER cents; civil dates TEXT YYYY-MM-DD; instants TEXT UTC RFC3339;
-- booleans INTEGER 0/1 with CHECK; enums TEXT with CHECK. No BEGIN/COMMIT here: the runner wraps
-- each migration in one transaction.

-- schema bookkeeping ---------------------------------------------------------
CREATE TABLE schema_migration (
  version     INTEGER PRIMARY KEY,
  name        TEXT    NOT NULL,
  sha256      TEXT    NOT NULL,
  applied_at  TEXT    NOT NULL
);

CREATE TABLE setting (
  key         TEXT PRIMARY KEY,
  value_json  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

-- command / audit ------------------------------------------------------------
CREATE TABLE command (
  id                    INTEGER PRIMARY KEY,
  name                  TEXT NOT NULL,
  actor                 TEXT NOT NULL CHECK (actor IN ('user','import','system','undo')),
  at                    TEXT NOT NULL,
  undoes_command_id     INTEGER REFERENCES command(id),
  undone_by_command_id  INTEGER REFERENCES command(id)
);

CREATE TABLE audit_event (
  id           INTEGER PRIMARY KEY,
  command_id   INTEGER NOT NULL REFERENCES command(id),
  at           TEXT NOT NULL,
  entity       TEXT NOT NULL,
  entity_id    INTEGER NOT NULL,
  action       TEXT NOT NULL CHECK (action IN ('insert','update','delete')),
  before_json  TEXT,
  after_json   TEXT
);
CREATE INDEX audit_event_command ON audit_event(command_id);
CREATE INDEX audit_event_entity  ON audit_event(entity, entity_id);

-- ventures and accounts ------------------------------------------------------
CREATE TABLE venture (
  id                 INTEGER PRIMARY KEY,
  name               TEXT NOT NULL UNIQUE,
  status             TEXT NOT NULL CHECK (status IN ('fund','freeze','kill')),
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
  recon_stale_after_days  INTEGER,
  created_at              TEXT NOT NULL,
  CHECK ((owner = 'venture') = (venture_id IS NOT NULL))
);

-- categories, tags, rules ----------------------------------------------------
CREATE TABLE category (
  id           INTEGER PRIMARY KEY,
  parent_id    INTEGER REFERENCES category(id),
  name         TEXT NOT NULL,
  root_kind    TEXT NOT NULL CHECK (root_kind IN ('fixed','variable','irregular','debt','income','transfer','venture')),
  is_system    INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),
  system_code  TEXT UNIQUE,
  archived     INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  sort_order   INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL,
  UNIQUE (parent_id, name)
);

CREATE TABLE tag (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE rule (
  id                      INTEGER PRIMARY KEY,
  position                INTEGER NOT NULL,
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
  spec_json    TEXT NOT NULL,
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
  dedup_report_json  TEXT NOT NULL,
  created_at         TEXT NOT NULL,
  undone_at          TEXT
);
CREATE INDEX import_batch_file ON import_batch(account_id, profile_id, file_sha256);

-- the ledger -----------------------------------------------------------------
CREATE TABLE txn (
  id                INTEGER PRIMARY KEY,
  account_id        INTEGER NOT NULL REFERENCES account(id),
  parent_id         INTEGER REFERENCES txn(id),
  posted_date       TEXT NOT NULL,
  effective_date    TEXT NOT NULL,
  amount_cents      INTEGER NOT NULL,
  payee_raw         TEXT NOT NULL DEFAULT '',
  payee_norm        TEXT NOT NULL DEFAULT '',
  memo              TEXT NOT NULL DEFAULT '',
  category_id       INTEGER REFERENCES category(id),
  import_batch_id   INTEGER REFERENCES import_batch(id),
  source_row_hash   TEXT,
  external_id       TEXT,
  classification    TEXT NOT NULL DEFAULT 'unclassified' CHECK (classification IN ('manual','rule','heuristic','unclassified')),
  rule_id           INTEGER REFERENCES rule(id),
  heuristic_code    TEXT,
  user_edited       INTEGER NOT NULL DEFAULT 0,
  status            TEXT NOT NULL DEFAULT 'posted' CHECK (status IN ('pending','posted')),
  transfer_link_id  INTEGER REFERENCES transfer_link(id),
  refund_link_id    INTEGER REFERENCES refund_link(id),
  venture_id        INTEGER REFERENCES venture(id),
  flags             INTEGER NOT NULL DEFAULT 0,
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
  original_txn_id  INTEGER NOT NULL REFERENCES txn(id),
  refund_txn_id    INTEGER NOT NULL UNIQUE REFERENCES txn(id),
  confidence       TEXT NOT NULL CHECK (confidence IN ('user','heuristic')),
  created_at       TEXT NOT NULL
);

CREATE TABLE import_quarantine (
  id                INTEGER PRIMARY KEY,
  import_batch_id   INTEGER NOT NULL REFERENCES import_batch(id),
  account_id        INTEGER NOT NULL REFERENCES account(id),
  row_json          TEXT NOT NULL,
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
  query_text  TEXT NOT NULL,
  created_at  TEXT NOT NULL
);

-- plan: income, debts, obligations, earmarks ---------------------------------
CREATE TABLE income_stream (
  id                     INTEGER PRIMARY KEY,
  name                   TEXT NOT NULL,
  kind                   TEXT NOT NULL CHECK (kind IN ('base','bonus','rsu','deferred_comp','other')),
  cycle                  TEXT NOT NULL CHECK (cycle IN ('weekly','biweekly','semimonthly','monthly','once')),
  anchor_date            TEXT NOT NULL,
  semimonthly_day_1      INTEGER CHECK (semimonthly_day_1 BETWEEN 1 AND 31),
  semimonthly_day_2      INTEGER CHECK (semimonthly_day_2 BETWEEN 1 AND 31),
  expected_net_cents     INTEGER NOT NULL CHECK (expected_net_cents >= 0),
  variability_cents      INTEGER NOT NULL DEFAULT 0 CHECK (variability_cents >= 0),
  confidence             TEXT NOT NULL CHECK (confidence IN ('confirmed','expected','rumored')),
  weekend_rule           TEXT NOT NULL DEFAULT 'previous_business_day' CHECK (weekend_rule IN ('none','previous_business_day','next_business_day')),
  deposit_account_id     INTEGER REFERENCES account(id),
  match_payee_contains   TEXT,
  active                 INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  created_at             TEXT NOT NULL,
  updated_at             TEXT NOT NULL,
  CHECK (cycle <> 'semimonthly' OR (semimonthly_day_1 IS NOT NULL AND semimonthly_day_2 IS NOT NULL))
);

CREATE TABLE income_receipt (
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
  account_id                INTEGER UNIQUE REFERENCES account(id),
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
  proceeds_txn_id  INTEGER REFERENCES txn(id),
  note_draft       TEXT NOT NULL DEFAULT ''
);

CREATE TABLE informal_loan_schedule (
  id            INTEGER PRIMARY KEY,
  debt_id       INTEGER NOT NULL REFERENCES informal_loan(debt_id),
  due_date      TEXT NOT NULL,
  amount_cents  INTEGER NOT NULL CHECK (amount_cents > 0)
);

CREATE TABLE debt_payment (
  id            INTEGER PRIMARY KEY,
  debt_id       INTEGER NOT NULL REFERENCES debt(id),
  paid_date     TEXT NOT NULL,
  amount_cents  INTEGER NOT NULL CHECK (amount_cents > 0),
  txn_id        INTEGER UNIQUE REFERENCES txn(id),
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
  due_weekday           INTEGER CHECK (due_weekday BETWEEN 0 AND 6),
  due_nth               INTEGER CHECK (due_nth BETWEEN 1 AND 5),
  anchor_date           TEXT,
  expected_cents        INTEGER NOT NULL CHECK (expected_cents >= 0),
  variability_cents     INTEGER NOT NULL DEFAULT 0 CHECK (variability_cents >= 0),
  source_account_id     INTEGER NOT NULL REFERENCES account(id),
  autopay               INTEGER NOT NULL DEFAULT 0 CHECK (autopay IN (0,1)),
  category_id           INTEGER REFERENCES category(id),
  debt_id               INTEGER REFERENCES debt(id),
  match_payee_contains  TEXT,
  detected_from_json    TEXT,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL
);

CREATE TABLE obligation_payment (
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
  schedule_income_stream_id  INTEGER REFERENCES income_stream(id),
  active                     INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
  created_at                 TEXT NOT NULL,
  updated_at                 TEXT NOT NULL,
  CHECK ((kind = 'obligation') = (obligation_id IS NOT NULL))
);

CREATE TABLE earmark_entry (
  id            INTEGER PRIMARY KEY,
  earmark_id    INTEGER NOT NULL REFERENCES earmark(id),
  entry_date    TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('fund','release','adjust')),
  amount_cents  INTEGER NOT NULL,
  txn_id        INTEGER REFERENCES txn(id),
  note          TEXT NOT NULL DEFAULT '',
  created_at    TEXT NOT NULL
);
CREATE INDEX earmark_entry_earmark ON earmark_entry(earmark_id);

CREATE TABLE variable_spend_override (
  category_id        INTEGER PRIMARY KEY REFERENCES category(id),
  per_30_days_cents  INTEGER NOT NULL CHECK (per_30_days_cents >= 0),
  updated_at         TEXT NOT NULL
);

-- policies -------------------------------------------------------------------
CREATE TABLE policy (
  id           INTEGER PRIMARY KEY,
  code         TEXT UNIQUE,
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('firewall_exclusion','informal_first','reminder')),
  params_json  TEXT NOT NULL DEFAULT '{}',
  is_system    INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0,1)),
  created_at   TEXT NOT NULL
);

CREATE TABLE firewall_ack (
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
  opening_cents            INTEGER NOT NULL,
  statement_closing_cents  INTEGER NOT NULL,
  statement_source         TEXT NOT NULL CHECK (statement_source IN ('user','file')),
  computed_closing_cents   INTEGER NOT NULL,
  difference_cents         INTEGER NOT NULL,
  status                   TEXT NOT NULL CHECK (status IN ('balanced','off')),
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
  detail_json               TEXT NOT NULL
);
CREATE UNIQUE INDEX snapshot_daily ON snapshot(civil_date) WHERE kind = 'daily';

CREATE TABLE review (
  id                   INTEGER PRIMARY KEY,
  started_at           TEXT NOT NULL,
  completed_at         TEXT,
  status               TEXT NOT NULL CHECK (status IN ('in_progress','completed','abandoned')),
  period_start         TEXT NOT NULL,
  period_end           TEXT NOT NULL,
  surplus_cents        INTEGER,
  surplus_detail_json  TEXT,
  steps_json           TEXT NOT NULL DEFAULT '{}',
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

-- seeds ------------------------------------------------------------------------
-- settings (ADR-0030): defaults the user can change in Settings / Plan
INSERT INTO setting (key, value_json, updated_at) VALUES
  ('zone',                   '"America/Chicago"', strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('timing_buffer_cents',    '0',                 strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('recon_stale_after_days', '45',                strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('dedup_similarity_bps',   '8000',              strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('theme',                  '"dark"',            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('backup_keep_daily',      '14',                strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- category roots: one per root_kind (names editable, rows not deletable)
INSERT INTO category (parent_id, name, root_kind, is_system, system_code, sort_order, created_at) VALUES
  (NULL, 'Fixed',     'fixed',     1, 'root.fixed',     10, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Variable',  'variable',  1, 'root.variable',  20, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Irregular', 'irregular', 1, 'root.irregular', 30, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Debt',      'debt',      1, 'root.debt',      40, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Income',    'income',    1, 'root.income',    50, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Transfers', 'transfer',  1, 'root.transfer',  60, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  (NULL, 'Ventures',  'venture',   1, 'root.venture',   70, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- system-coded children the engines reference by code (ADR-0033)
WITH c(parent_code, name, code, sort_order) AS (VALUES
  ('root.income',   'Salary',                    'income.salary',                     10),
  ('root.income',   'Bonus',                     'income.bonus',                      20),
  ('root.income',   'RSU',                       'income.rsu',                        30),
  ('root.income',   'Deferred comp',             'income.deferred_comp',              40),
  ('root.income',   'Interest income',           'income.interest',                   50),
  ('root.income',   'Other income',              'income.other',                      60),
  ('root.transfer', 'Internal transfer',         'transfer.internal',                 10),
  ('root.transfer', 'Card payment',              'transfer.card_payment',             20),
  ('root.transfer', 'Loan repayment',            'transfer.loan_repayment',           30),
  ('root.transfer', 'Borrowing proceeds',        'transfer.borrowing_proceeds',       40),
  ('root.transfer', 'Securities sale proceeds',  'transfer.securities_sale_proceeds', 50),
  ('root.debt',     'Interest',                  'debt.interest',                     10),
  ('root.debt',     'Fees',                      'debt.fees',                         20),
  ('root.venture',  'Customer revenue',          'venture.customer_revenue',          10),
  ('root.venture',  'Operating expense',         'venture.operating_expense',         20),
  ('root.venture',  'Owner contribution',        'venture.owner_contribution',        30),
  ('root.venture',  'Financing',                 'venture.financing',                 40),
  ('root.venture',  'Withdrawal',                'venture.withdrawal',                50),
  ('root.variable', 'Cash',                      'variable.cash',                     10),
  ('root.variable', 'Uncategorized',             'variable.uncategorized',            20)
)
INSERT INTO category (parent_id, name, root_kind, is_system, system_code, sort_order, created_at)
SELECT p.id, c.name, p.root_kind, 1, c.code, c.sort_order, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM c
JOIN category p ON p.system_code = c.parent_code;

-- system policies (ADR-0031): always enforced, never deletable
INSERT INTO policy (code, name, kind, params_json, is_system, created_at) VALUES
  ('firewall_exclusion', 'Firewalled accounts are not available cash; outflows need an acknowledgment',
   'firewall_exclusion', '{}', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
  ('informal_first', 'Informal loans are scheduled before accelerated debt paydown',
   'informal_first', '{}', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- the generic CSV profile (Date, Description, Amount; ISO dates; signed amounts from the
-- account's point of view). Institution profiles arrive with the fixtures in M1.
INSERT INTO import_profile (name, institution, format, spec_json, is_system, created_at, updated_at) VALUES
  ('generic_csv', '', 'csv',
   '{"header_signature":["Date","Description","Amount"],"skip_rows":0,"date":{"column":"Date","format":"%Y-%m-%d"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"},"sign_convention":"account_pov"}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
