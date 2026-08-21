use crate::repo::tasks;
use rusqlite::Connection;
use std::collections::HashSet;

#[derive(Debug)]
pub enum NotificationEvent {
    DueInOneHour { title: String },
    DueInTenMins { title: String },
    DueNow { id: String, title: String },
}

pub struct NotificationEngine {
    last_tick_ms: i64,
    notified_events: HashSet<String>,
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
        let now = chrono::Local::now().timestamp();
        if now - self.last_tick_ms < 10 {
            return vec![];
        }
        self.last_tick_ms = now;

        // 每日心智维护摘要（合并成一条，同一天至多一次）。
        // We will move this logic out of the engine, or keep it here and let it just run as a side effect?
        // Actually, check() already handles deduplication internally.
        let _ = crate::commands::notify::check(conn);

        let mut events = Vec::new();

        if let Ok(rows) = tasks::due_in_range(conn, (now - 60) * 1000, (now + 3600) * 1000) {
            for (id, title, due) in rows {
                let Some(due) = due else { continue };
                let diff = due / 1000 - now;

                if diff > 3540 && diff <= 3600 {
                    let key = format!("{id}-{due}-1h");
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::DueInOneHour { title });
                    }
                } else if diff > 540 && diff <= 600 {
                    let key = format!("{id}-{due}-10m");
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::DueInTenMins { title });
                    }
                } else if diff <= 0 && diff > -60 {
                    let key = format!("{id}-{due}-now");
                    if !self.notified_events.contains(&key) {
                        self.notified_events.insert(key);
                        events.push(NotificationEvent::DueNow { id, title });
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
