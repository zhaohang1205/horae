use crate::repo::{state::JsonStateStore, tasks};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// 已发送提醒的持久化状态，存入 `notify_tui.json`。
/// key 格式：`"type:id:due_ms"`，通过提取 due_ms 判断是否过期。
/// Bug3 修复：持久化使 TUI 重启后不重复发送相同提醒。
#[derive(Debug, Default, Serialize, Deserialize)]
struct NotifyTuiState {
    #[serde(default)]
    notified: Vec<String>,
}

fn tui_state_store() -> JsonStateStore<NotifyTuiState> {
    JsonStateStore::new("notify_tui.json")
}

/// 过期窗口：due_ms 早于 now 超过 2 小时的 key 可以丢弃。
/// Bug2 修复：按时间精确过期，替代原来超过 1024 条暴力 clear() 的策略。
const PRUNE_MS: i64 = 2 * 3600 * 1000;

fn key_fresh(key: &str, now: i64) -> bool {
    // key 格式 "type:id:due_ms"，取最后一段为 due_ms
    key.rsplitn(2, ':')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .map(|due_ms| due_ms + PRUNE_MS > now)
        .unwrap_or(true) // 格式不识别时保守保留
}

fn make_key(kind: &str, id: &str, due: i64) -> String {
    format!("{}:{}:{}", kind, id, due)
}

#[derive(Debug)]
pub enum NotificationEvent {
    InOneHour { title: String },
    InTenMins { title: String },
    Now { id: String, title: String },
}

pub struct NotificationEngine {
    last_tick_ms: i64,
    /// 已通知过的 key 集合（内存缓存，与 notify_tui.json 同步）。
    notified: std::collections::HashSet<String>,
}

impl NotificationEngine {
    pub fn new() -> Self {
        // Bug3 修复：从持久化文件恢复状态，避免重启 TUI 后重复通知。
        let saved = tui_state_store().load().unwrap_or_default();
        Self {
            last_tick_ms: 0,
            notified: saved.notified.into_iter().collect(),
        }
    }

    /// Evaluates due tasks and returns a list of events to be notified.
    pub fn tick(&mut self, conn: &Connection) -> Vec<NotificationEvent> {
        let now_ms = crate::time::now_ms();
        if now_ms - self.last_tick_ms < 10_000 {
            return vec![];
        }
        self.last_tick_ms = now_ms;

        // Bug2 修复：按时间过期清理，不再暴力 clear()。
        // 只丢弃 due_ms 早于 now 超过 PRUNE_MS 的 key，保留近期去重记录。
        let before_len = self.notified.len();
        self.notified.retain(|k| key_fresh(k, now_ms));
        let pruned = before_len != self.notified.len();

        let mut events = Vec::new();

        // 加载全部未归档任务，通过 effective_due() 计算有效截止时间。
        // 正确支持带 rrule 的循环任务（每日站会、每周回顾等）。
        let all = match tasks::list(
            conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        ) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut dirty = pruned;

        for task in all {
            // 已完成/已归档的任务不需要提醒
            if task.status == crate::model::task::Status::Done || task.archived_at.is_some() {
                continue;
            }

            let Some(due) = crate::schedule::effective_due(&task) else {
                continue;
            };

            let diff_ms = due - now_ms;

            // 仅处理接下来 1 小时内或刚过期 1 分钟内的任务，过滤掉不相关的远期任务。
            if diff_ms > 3_600_000 || diff_ms < -60_000 {
                continue;
            }

            if diff_ms > 3_540_000 && diff_ms <= 3_600_000 {
                let key = make_key("1h", &task.id, due);
                if self.notified.insert(key) {
                    dirty = true;
                    events.push(NotificationEvent::InOneHour { title: task.title });
                }
            } else if diff_ms > 540_000 && diff_ms <= 600_000 {
                let key = make_key("10m", &task.id, due);
                if self.notified.insert(key) {
                    dirty = true;
                    events.push(NotificationEvent::InTenMins { title: task.title });
                }
            } else if diff_ms <= 0 && diff_ms > -60_000 {
                let key = make_key("now", &task.id, due);
                if self.notified.insert(key) {
                    dirty = true;
                    events.push(NotificationEvent::Now {
                        id: task.id,
                        title: task.title,
                    });
                }
            }
        }

        // Bug3 修复：有变化时才落盘，保证重启 TUI 不重复通知。
        if dirty {
            let state = NotifyTuiState {
                notified: self.notified.iter().cloned().collect(),
            };
            let _ = tui_state_store().save(&state);
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::{Status, Task};
    use crate::repo::tasks::CaptureInput;
    use crate::testutil::test_conn;

    fn mk_task_with_rrule(
        conn: &Connection,
        title: &str,
        rrule: &str,
        scheduled_start_at: Option<i64>,
        due_at: Option<i64>,
    ) -> Task {
        let mut t = crate::repo::tasks::create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE tasks SET rrule = ?1, scheduled_start_at = ?2, due_at = ?3 WHERE id = ?4",
            rusqlite::params![rrule, scheduled_start_at, due_at, &t.id],
        )
        .unwrap();
        t.rrule = Some(rrule.to_string());
        t.scheduled_start_at = scheduled_start_at;
        t.due_at = due_at;
        t
    }

    #[test]
    fn tick_fires_for_recurring_task_due_now() {
        let (_dir, conn) = test_conn();
        let now = crate::time::now_ms();
        // 循环任务：锚点在 30 秒前（刚过期，在 -60s～0 窗口内）
        let anchor = now - 30_000;
        mk_task_with_rrule(&conn, "每日站会", "FREQ=DAILY", Some(anchor), None);

        let mut engine = NotificationEngine::new();
        // 强制 last_tick_ms 为 0，跳过节流
        let events = engine.tick(&conn);
        // 应当触发 Now 事件
        let has_now = events
            .iter()
            .any(|e| matches!(e, NotificationEvent::Now { .. }));
        assert!(has_now, "循环任务到点后应触发 Now 提醒，但未触发");
    }

    #[test]
    fn tick_does_not_fire_for_recurring_task_far_future() {
        let (_dir, conn) = test_conn();
        let now = crate::time::now_ms();
        // 循环任务：锚点在 2 小时后，超出 1 小时窗口
        let anchor = now + 2 * 3600 * 1000;
        mk_task_with_rrule(&conn, "远期任务", "FREQ=DAILY", Some(anchor), None);

        let mut engine = NotificationEngine::new();
        let events = engine.tick(&conn);
        assert!(events.is_empty(), "2 小时后的任务不应触发任何提醒");
    }

    #[test]
    fn tick_deduplicates_recurring_task() {
        let (_dir, conn) = test_conn();
        let now = crate::time::now_ms();
        let anchor = now - 30_000;
        mk_task_with_rrule(&conn, "每日站会", "FREQ=DAILY", Some(anchor), None);

        let mut engine = NotificationEngine::new();
        let first = engine.tick(&conn);
        assert!(!first.is_empty(), "首次应触发");

        // 重置节流计时，模拟 10s 后再次 tick
        engine.last_tick_ms = 0;
        let second = engine.tick(&conn);
        assert!(second.is_empty(), "相同 occurrence 不应重复触发");
    }
}
