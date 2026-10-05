-- Kept schema v3: the everyday category children a finance professional expects to exist
-- (system rows: editable names, never deleted). Codes are what rules and fixtures reference.
-- Never edit after commit; fix mistakes in 0004+.

WITH c(parent_code, name, code, sort_order) AS (VALUES
  ('root.fixed',     'Rent',           'fixed.rent',            10),
  ('root.fixed',     'Utilities',      'fixed.utilities',       20),
  ('root.fixed',     'Internet',       'fixed.internet',        30),
  ('root.fixed',     'Phone',          'fixed.phone',           40),
  ('root.fixed',     'Insurance',      'fixed.insurance',       50),
  ('root.fixed',     'Subscriptions',  'fixed.subscriptions',   60),
  ('root.variable',  'Groceries',      'variable.groceries',    30),
  ('root.variable',  'Dining',         'variable.dining',       40),
  ('root.variable',  'Fuel',           'variable.fuel',         50),
  ('root.variable',  'Transport',      'variable.transport',    60),
  ('root.variable',  'Shopping',       'variable.shopping',     70),
  ('root.variable',  'Health',         'variable.health',       80),
  ('root.variable',  'Entertainment',  'variable.entertainment', 90),
  ('root.variable',  'Personal',       'variable.personal',    100),
  ('root.irregular', 'Insurance',      'irregular.insurance',   10),
  ('root.irregular', 'Taxes',          'irregular.taxes',       20),
  ('root.irregular', 'Travel',         'irregular.travel',      30),
  ('root.irregular', 'Gifts',          'irregular.gifts',       40),
  ('root.irregular', 'Auto repair',    'irregular.auto_repair', 50),
  ('root.irregular', 'Medical',        'irregular.medical',     60)
)
INSERT INTO category (parent_id, name, root_kind, is_system, system_code, sort_order, created_at)
SELECT p.id, c.name, p.root_kind, 1, c.code, c.sort_order, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM c
JOIN category p ON p.system_code = c.parent_code;
