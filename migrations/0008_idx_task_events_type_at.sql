-- horae schema v7: index task_events by event_type for habit check-in lookups
-- (`checked_in_today` filters on event_type + at, which otherwise scans the
-- unbounded append-only timeline).
CREATE INDEX IF NOT EXISTS idx_events_type_at ON task_events(event_type, at);
