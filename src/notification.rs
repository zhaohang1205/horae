use crate::repo::tasks;
use rusqlite::Connection;
use std::collections::HashSet;

#[derive(Debug)]
pub enum NotificationEvent {
    InOneHour { title: String },
    InTenMins { title: String },
    Now { id: String, title: String },
}

#[derive(Hash, Eq, PartialEq)]
pub enum NotificationKey {
    OneHour { id: String, due: i64 },
    TenMins { id: String, due: i64 },
    Now { id: String, due: i64 },
}

pub struct NotificationEngine {
    last_tick_ms: i64,
    notified_events: HashSet<NotificationKey>,
}

impl NotificationEngine {
    pub fn new() -> Self {
        Self {
            last_tick_ms: 0,
            notified_events: HashSet::new(),
        }
    }

    /// Evaluates due tasks and returns a list of events to be notified.
    pub fn tick(&mut self, conn: &Connection) -> Vec<NotificationEvent> {
        let now_ms = crate::time::now_ms();
        if now_ms - self.last_tick_ms < 10_000 {
            return vec![];
        }
        self.last_tick_ms = now_ms;

        let mut events = Vec::new();

        if let Ok(rows) = tasks::due_in_range(conn, now_ms - 60_000, now_ms + 3_600_000) {
            for (id, title, due) in rows {
                let Some(due) = due else { continue };
                let diff_ms = due - now_ms;

                if diff_ms > 3_540_000 && diff_ms <= 3_600_000 {
                    let key = NotificationKey::OneHour { id: id.clone(), due };
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::InOneHour { title });
                    }
                } else if diff_ms > 540_000 && diff_ms <= 600_000 {
                    let key = NotificationKey::TenMins { id: id.clone(), due };
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::InTenMins { title });
                    }
                } else if diff_ms <= 0 && diff_ms > -60_000 {
                    let key = NotificationKey::Now { id: id.clone(), due };
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::Now { id, title });
                    }
                }
            }
        }

        if self.notified_events.len() > 1024 {
            self.notified_events.clear();
        }

        events
    }
}
