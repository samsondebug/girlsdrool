-- Kept schema v2: institution import profiles for the fixture sources (ARCHITECTURE §14) and the
-- Jaro–Winkler dedup threshold (ADR-0037). Never edit after commit; fix mistakes in 0003+.

-- ADR-0037: payee similarity is Jaro–Winkler (prefix-weighted), threshold 0.85.
UPDATE setting SET value_json = '8500', updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE key = 'dedup_similarity_bps' AND value_json = '8000';

INSERT INTO import_profile (name, institution, format, spec_json, is_system, created_at, updated_at) VALUES
  ('northbank_csv', 'Northbank', 'csv',
   '{"header_signature":["Date","Description","Amount","Running Bal."],"skip_rows":0,"date":{"column":"Date","format":"%m/%d/%Y"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"},"balance":{"column":"Running Bal."},"sign_convention":"account_pov"}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

  ('riverside_csv', 'Riverside Bank', 'csv',
   '{"header_signature":["Transaction Date","Posted Date","Description","Debit","Credit","Balance","Currency"],"skip_rows":0,"date":{"column":"Posted Date","format":"%Y-%m-%d"},"effective_date":{"column":"Transaction Date","format":"%Y-%m-%d"},"amount":{"kind":"debit_credit","debit":"Debit","credit":"Credit"},"payee":{"column":"Description"},"balance":{"column":"Balance"},"currency":{"column":"Currency"},"sign_convention":"account_pov"}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

  ('summit_card_csv', 'Summit Card Services', 'csv',
   '{"header_signature":["Transaction Date","Post Date","Description","Type","Amount","Status"],"skip_rows":0,"date":{"column":"Post Date","format":"%m/%d/%Y"},"effective_date":{"column":"Transaction Date","format":"%m/%d/%Y"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"},"status":{"column":"Status","pending_values":["Pending"]},"sign_convention":"card_statement","flags_by_type":{"column":"Type","map":{"Interest":["interest"],"Fee":["fee"]}}}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

  ('harbor_brokerage_csv', 'Harbor Securities', 'csv',
   '{"header_signature":["Date","Activity","Symbol","Quantity","Price","Amount","Cash Balance"],"skip_rows":0,"date":{"column":"Date","format":"%m/%d/%Y"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Activity"},"memo":{"columns":["Symbol","Quantity","Price"]},"balance":{"column":"Cash Balance"},"sign_convention":"account_pov","flags_by_type":{"column":"Activity","map":{"SELL":["securities_sale"]}}}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),

  ('venmo_csv', 'Venmo', 'csv',
   '{"header_signature":["ID","Datetime","Type","Status","Note","From","To","Amount (total)","Funding Source","Destination"],"skip_rows":2,"date":{"column":"Datetime","format":"%Y-%m-%dT%H:%M:%S"},"amount":{"kind":"single_signed","column":"Amount (total)"},"payee":{"inflow_column":"From","outflow_column":"To","fallback_column":"Destination"},"memo":{"column":"Note"},"external_id":{"column":"ID"},"sign_convention":"account_pov","row_flags":["payment_app_unknown","needs_review"],"skip_when":{"column":"Funding Source","not_in":["Venmo balance",""],"reason":"funded from a bank account; that bank''s own row is the ledger"}}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
