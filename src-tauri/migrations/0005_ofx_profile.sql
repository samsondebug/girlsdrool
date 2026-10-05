-- Kept schema v5: the one OFX/QFX import profile (ADR-0046). The OFX standard fixes every field
-- name, so a single system profile serves every bank; its spec maps TRNTYPE values to row flags
-- (ARCHITECTURE §3.4). Never edit after commit; fix mistakes in 0006+.

INSERT INTO import_profile (name, institution, format, spec_json, is_system, created_at, updated_at) VALUES
  ('ofx_qfx', 'Any bank (OFX/QFX)', 'ofx',
   '{"flags_by_trntype":{"ATM":["cash_withdrawal","needs_review"],"CASH":["cash_withdrawal","needs_review"],"FEE":["fee"],"SRVCHG":["fee"],"INT":["interest"]}}',
   1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
