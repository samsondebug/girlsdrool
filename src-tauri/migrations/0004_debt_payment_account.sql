-- M6: a debt names the cash account its payments leave from and the payee text that finds them,
-- so its minimum can be an obligation that matches the ledger (ADR-0043).
ALTER TABLE debt ADD COLUMN payment_account_id INTEGER REFERENCES account(id);
ALTER TABLE debt ADD COLUMN match_payee_contains TEXT;
