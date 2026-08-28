use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: String,
    pub event_type: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub at: i64,
    pub meta: Option<String>,
}

// Event types (kept as consts so they can't drift from the schema).
pub const EV_CAPTURED: &str = "captured";
pub const EV_CLARIFIED: &str = "clarified";
pub const EV_STATUS_CHANGED: &str = "status_changed";
pub const EV_SCHEDULED: &str = "scheduled";
pub const EV_DUE: &str = "due";

pub const EV_COMPLETED: &str = "completed";
pub const EV_ARCHIVED: &str = "archived";
pub const EV_RESTORED: &str = "restored"; // archived task brought back (soft-delete undone)
pub const EV_TAG_ADDED: &str = "tag_added";
pub const EV_TAG_REMOVED: &str = "tag_removed";

// Events not tied to a status transition:
pub const EV_HABIT_COMPLETED: &str = "habit_completed"; // a recurring task's occurrence done; rescheduled
pub const EV_POMODORO: &str = "pomodoro"; // a Pomodoro work session completed
pub const EV_LOGGED: &str = "logged"; // a pure journal/log event
pub const EV_CHECKLIST: &str = "checklist"; // a checklist item added/toggled/deleted/renamed/moved
