-- horae schema v8: index tasks(due_at) for the due-notification window scan
-- (`due_in_range` polls `due_at BETWEEN ? AND ?` every tick).
CREATE INDEX IF NOT EXISTS idx_tasks_due_at ON tasks(due_at);
