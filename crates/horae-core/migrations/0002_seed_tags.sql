-- horae preset tags (v1, simplified scientific set)
-- Idempotent: INSERT OR IGNORE so re-running migrations is safe.
-- Categories: 'context' (where/which life area). 优先级为 tasks.priority 独立字段，不再作为标签。

INSERT OR IGNORE INTO tags (name, category, is_system, created_at) VALUES
  ('home',     'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('work',     'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('learning', 'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('errands',  'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('calls',    'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000),
  ('computer', 'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000);
