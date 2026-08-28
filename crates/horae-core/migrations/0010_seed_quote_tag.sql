-- Seed the system `quote` tag backing the 金句 (Quotes) view.
-- A task tagged @quote (status reference) is managed in the Quotes view.
-- Idempotent: INSERT OR IGNORE so re-running migrations is safe.
INSERT OR IGNORE INTO tags (name, category, is_system, created_at) VALUES
  ('quote', 'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000);