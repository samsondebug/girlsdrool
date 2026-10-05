-- Kept schema v6: what the 200k-row run asked for (ADR-0047). Never edit after commit; fix
-- mistakes in 0007+.
--
-- 1. Transfer detection asks "which row on another account has the opposite amount within the
--    window?"; without an index led by amount_cents that was a full scan per imported row.
CREATE INDEX txn_amount_date ON txn(amount_cents, posted_date);

-- 2. The Ledger pages by (posted_date, id) across every account; without this index each page
--    sorted the whole ledger.
CREATE INDEX txn_date_id ON txn(posted_date, id);

-- 3. Balances and the hero sum one account's posted rows up to a date; this covering index
--    answers from the index alone.
CREATE INDEX txn_account_status_date ON txn(account_id, status, posted_date, amount_cents);

-- 4. Flagged rows (borrowing, securities sales, cash, fees, interest, review) are a small
--    minority; the hero asks for flagged inflows per account and should not read every row
--    to find them. Queries that want this index say `flags <> 0` explicitly.
CREATE INDEX txn_flagged ON txn(account_id, status, posted_date, amount_cents, flags) WHERE flags <> 0;

-- 5. The leaf view excluded split parents with a correlated NOT EXISTS probe per row; the set
--    of parents is tiny, so it is built once per statement instead. Same rows, same meaning:
--    a split parent is never counted.
DROP VIEW txn_leaf;
CREATE VIEW txn_leaf AS
  SELECT t.* FROM txn t
  WHERE t.id NOT IN (SELECT parent_id FROM txn WHERE parent_id IS NOT NULL);
