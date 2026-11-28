-- Todoist is an opt-in sync tag. Seed it so quick-add autocomplete offers @todoist.
-- Upgrade a tag created by an earlier sync to a non-deletable system tag.
INSERT INTO tags (name, category, is_system, created_at)
VALUES ('todoist', 'context', 1, CAST(strftime('%s','now') AS INTEGER) * 1000)
ON CONFLICT(name) DO UPDATE SET category='context', is_system=1;
