ALTER TABLE todoist_sync_state ADD COLUMN last_sync_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE todoist_sync_state ADD COLUMN last_imported INTEGER NOT NULL DEFAULT 0;
ALTER TABLE todoist_sync_state ADD COLUMN last_updated INTEGER NOT NULL DEFAULT 0;
ALTER TABLE todoist_sync_state ADD COLUMN last_pushed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE todoist_sync_state ADD COLUMN last_error TEXT;

CREATE TABLE IF NOT EXISTS todoist_conflicts (
    todoist_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    remote_json TEXT NOT NULL,
    detected_at INTEGER NOT NULL,
    resolution TEXT CHECK (resolution IN ('local', 'remote'))
);
