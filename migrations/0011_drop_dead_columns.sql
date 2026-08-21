-- horae schema v10: drop dead columns the domain layer never writes.
-- These columns were only ever read via the backup round-trip and are always
-- NULL/default in live data:
--   kind ('action'), parent_id (self-FK), organized_at, started_at, project_type
-- `delegated_to` is KEPT: it is written on capture and shown in the UI.
--
-- parent_id has an index and a self-referential FK, so we drop the index and
-- disable foreign keys around the column drops (SQLite rejects dropping a
-- column that is named in an index or FK constraint).

DROP INDEX IF EXISTS idx_tasks_parent;
PRAGMA foreign_keys=OFF;

ALTER TABLE tasks DROP COLUMN kind;
ALTER TABLE tasks DROP COLUMN parent_id;
ALTER TABLE tasks DROP COLUMN organized_at;
ALTER TABLE tasks DROP COLUMN started_at;
ALTER TABLE tasks DROP COLUMN project_type;

PRAGMA foreign_keys=ON;
