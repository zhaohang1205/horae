-- horae schema v12: composite index for active tasks with due/schedule dates
-- accelerates due scan and daily digest without scanning the entire tasks table.
CREATE INDEX IF NOT EXISTS idx_tasks_overdue ON tasks(archived_at, status, due_at, scheduled_start_at);
