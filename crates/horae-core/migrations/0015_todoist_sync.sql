-- Per-profile Todoist incremental sync state and task identity mapping.
CREATE TABLE IF NOT EXISTS todoist_sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    sync_token TEXT NOT NULL,
    completed_since_ms INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS task_todoist_links (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    todoist_id TEXT NOT NULL UNIQUE,
    local_hash TEXT NOT NULL,
    remote_hash TEXT NOT NULL,
    remote_checked INTEGER NOT NULL DEFAULT 0,
    remote_deleted INTEGER NOT NULL DEFAULT 0,
    out_of_scope INTEGER NOT NULL DEFAULT 0,
    last_synced_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS todoist_pending_commands (
    command_uuid TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    temp_id TEXT,
    command_json TEXT NOT NULL,
    local_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
