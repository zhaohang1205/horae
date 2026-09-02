-- horae schema v1
-- All timestamps stored as UTC milliseconds (INTEGER).

CREATE TABLE IF NOT EXISTS tasks (
  id                TEXT    PRIMARY KEY,           -- uuid v4
  title             TEXT    NOT NULL,
  notes             TEXT    NOT NULL DEFAULT '',
  kind              TEXT    NOT NULL DEFAULT 'action', -- 'action' | 'project'
  parent_id         TEXT    REFERENCES tasks(id) ON DELETE CASCADE,
  status            TEXT    NOT NULL DEFAULT 'inbox',
  rrule             TEXT,                          -- recurrence rule, NULL = none
  priority          TEXT,                          -- high|medium|low|NULL (独立优先级，取代原 p1/p2/p3 标签)
  created_at        INTEGER NOT NULL,             -- = captured_at (UTC ms)
  clarified_at      INTEGER,                      -- set when leaving inbox
  organized_at      INTEGER,                      -- set when assigned to a project / organized
  due_at            INTEGER,                      -- deadline (calendar time)
  scheduled_start_at INTEGER,                    -- planned start (schedule)
  scheduled_end_at  INTEGER,                      -- planned end
  started_at        INTEGER,                      -- work actually began (later Pomodoro phase)
  completed_at      INTEGER,                      -- completed
  archived_at       INTEGER,                      -- soft-deleted / archived
  updated_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_id);

-- Append-only timeline / audit log. Every state transition is recorded here.
CREATE TABLE IF NOT EXISTS task_events (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id     TEXT    NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  event_type  TEXT    NOT NULL,  -- event-type consts live in src/model/event.rs and must stay in sync:
                                -- captured|clarified|organized|status_changed|scheduled|
                                -- completed|archived|restored|tag_added|tag_removed|habit_completed|pomodoro|logged|checklist
  from_status TEXT,
  to_status   TEXT,
  at          INTEGER NOT NULL, -- UTC ms
  meta        TEXT               -- json (e.g. tag added/removed, before/after values)
);
CREATE INDEX IF NOT EXISTS idx_events_task ON task_events(task_id, at);

-- Tag catalog. is_system=1 tags are the preset scientific set.
CREATE TABLE IF NOT EXISTS tags (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT    NOT NULL UNIQUE,
  category    TEXT    NOT NULL,  -- context|custom
  is_system   INTEGER NOT NULL DEFAULT 0,
  color       TEXT,
  icon        TEXT,
  description TEXT,
  created_at  INTEGER NOT NULL
);

-- task <-> tag association
CREATE TABLE IF NOT EXISTS task_tags (
  task_id   TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  tag_id    INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  added_at  INTEGER NOT NULL,
  PRIMARY KEY (task_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_task_tags_tag ON task_tags(tag_id);
