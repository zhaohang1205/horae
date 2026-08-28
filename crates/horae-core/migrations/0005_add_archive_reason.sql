-- Record why a task was archived: 'completed' (done) or 'deleted' (not done).
-- NULL = archived before this migration.
ALTER TABLE tasks ADD COLUMN archive_reason TEXT;
