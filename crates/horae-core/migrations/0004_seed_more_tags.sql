-- Add preset tags: quick, focus
INSERT OR IGNORE INTO tags (name, category, is_system, created_at) VALUES
  ('quick',  'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('focus',  'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000);
