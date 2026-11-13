//! Timeline semantic decoding for task events.
//!
//! Translates raw `TaskEvent` records into human-readable, Git-commit-like
//! representations with localized badges and descriptions.

use crate::i18n::Lang;
use crate::model::event;
use crate::model::event::TaskEvent;
use crate::model::task::Status;
use crate::tr;

/// A formatted, human-readable timeline entry for Git-like representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub id: i64,
    pub at: i64,
    pub event_type: String,
    pub badge: String,
    pub description: String,
}

/// 聚合后的时间线条目（支持方案一里程碑标记与方案二连续事件智能合并，并记录起止时间段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedTimelineEntry {
    pub id: i64,
    pub at: i64,
    pub end_at: Option<i64>,
    pub event_type: String,
    pub badge: String,
    pub description: String,
    pub is_milestone: bool,
    pub count: usize,
}

/// 任务习惯周期自适应视窗配置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CadenceWindow {
    /// 视窗时长（毫秒）
    pub horizon_ms: i64,
    /// 视窗中文名（如 "21天", "12周", "12个月"）
    pub label_zh: &'static str,
    /// 视窗英文名（如 "21 days", "12 weeks", "12 months"）
    pub label_en: &'static str,
}

/// 根据任务的 rrule 判定自适应习惯追踪视窗：
/// - FREQ=DAILY: 21天 (21 * 86_400_000 ms) 习惯形成黄金周期
/// - FREQ=WEEKLY: 12周 (84 * 86_400_000 ms) 季度追踪周期
/// - FREQ=MONTHLY: 12个月 (365 * 86_400_000 ms) 年度追踪周期
/// - 其他情况/单次任务: None (全生命周期追踪)
pub fn cadence_window(rrule: Option<&str>) -> Option<CadenceWindow> {
    let r = rrule?;
    if r.contains("FREQ=DAILY") {
        Some(CadenceWindow {
            horizon_ms: 21 * 86_400_000,
            label_zh: "21天",
            label_en: "21 days",
        })
    } else if r.contains("FREQ=WEEKLY") {
        Some(CadenceWindow {
            horizon_ms: 84 * 86_400_000,
            label_zh: "12周",
            label_en: "12 weeks",
        })
    } else if r.contains("FREQ=MONTHLY") {
        Some(CadenceWindow {
            horizon_ms: 365 * 86_400_000,
            label_zh: "12个月",
            label_en: "12 months",
        })
    } else {
        None
    }
}

/// Describe a `TaskEvent` into a `TimelineEntry` for the given language.
pub fn describe_event(event: &TaskEvent, lang: Lang) -> TimelineEntry {
    let (badge, desc) = match event.event_type.as_str() {
        event::EV_CAPTURED => {
            let b = tr!(lang, "创建", "Init");
            let d = match event.to_status.as_deref() {
                Some("inbox") | None => {
                    tr!(lang, "捕获入收件箱", "Captured into inbox").to_string()
                }
                Some(st) => {
                    let st_label = parse_status_label(st, lang);
                    tr!(lang, "捕获并进入 {}", "Captured into {}", st_label)
                }
            };
            (b.to_string(), d)
        }
        event::EV_CLARIFIED => {
            let b = tr!(lang, "理清", "Clarify");
            let d = match event.to_status.as_deref() {
                Some(to) => {
                    let to_lbl = parse_status_label(to, lang);
                    let inbox_lbl = Status::Inbox.label(lang);
                    format!("{} → {}", inbox_lbl, to_lbl)
                }
                None => tr!(lang, "理清并移出收件箱", "Clarified out of inbox").to_string(),
            };
            (b.to_string(), d)
        }
        event::EV_STATUS_CHANGED => {
            let b = tr!(lang, "流转", "Status");
            let from_lbl = event
                .from_status
                .as_deref()
                .map(|s| parse_status_label(s, lang))
                .unwrap_or_else(|| "-".to_string());
            let to_lbl = event
                .to_status
                .as_deref()
                .map(|s| parse_status_label(s, lang))
                .unwrap_or_else(|| "-".to_string());
            (b.to_string(), format!("{} → {}", from_lbl, to_lbl))
        }
        event::EV_COMPLETED => {
            let b = tr!(lang, "完成", "Done");
            let d = match event.from_status.as_deref() {
                Some(from) => {
                    let from_lbl = parse_status_label(from, lang);
                    let done_lbl = Status::Done.label(lang);
                    format!("{} → {}", from_lbl, done_lbl)
                }
                None => tr!(lang, "标记为已完成", "Marked as completed").to_string(),
            };
            (b.to_string(), d)
        }
        event::EV_HABIT_COMPLETED => {
            let b = tr!(lang, "习惯", "Habit");
            let d = tr!(
                lang,
                "习惯打卡完成，已排程至下个周期",
                "Habit completed, rescheduled to next cycle"
            )
            .to_string();
            (b.to_string(), d)
        }
        event::EV_SCHEDULED => {
            let b = tr!(lang, "排期", "Sched");
            let d = if let Some(meta) = event.meta.as_deref() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(meta) {
                    if let Some(rrule) = v.get("rrule").and_then(|r| r.as_str()) {
                        tr!(lang, "循环规则: {}", "Rrule: {}", rrule)
                    } else {
                        tr!(lang, "排期已更新", "Schedule updated").to_string()
                    }
                } else {
                    tr!(lang, "排期已更新", "Schedule updated").to_string()
                }
            } else {
                tr!(lang, "排期已更新", "Schedule updated").to_string()
            };
            (b.to_string(), d)
        }
        event::EV_DUE => {
            let b = tr!(lang, "截止", "Due");
            let d = tr!(lang, "截止日期变更", "Deadline updated").to_string();
            (b.to_string(), d)
        }
        event::EV_PRIORITY => {
            let b = tr!(lang, "优级", "Prio");
            let d = tr!(lang, "优先级变更", "Priority updated").to_string();
            (b.to_string(), d)
        }
        event::EV_TAG_ADDED => {
            let b = tr!(lang, "标签", "+Tag");
            let tag = parse_json_field(event.meta.as_deref(), "name")
                .unwrap_or_else(|| event.meta.clone().unwrap_or_default());
            let d = format!("+@{}", tag);
            (b.to_string(), d)
        }
        event::EV_TAG_REMOVED => {
            let b = tr!(lang, "标签", "-Tag");
            let tag = parse_json_field(event.meta.as_deref(), "name")
                .unwrap_or_else(|| event.meta.clone().unwrap_or_default());
            let d = format!("-@{}", tag);
            (b.to_string(), d)
        }
        event::EV_POMODORO => {
            let b = tr!(lang, "专注", "Focus");
            let duration = format_duration_secs(event.meta.as_deref());
            let d = tr!(lang, "专注完成 ({})", "Focus session done ({})", duration);
            (b.to_string(), d)
        }
        event::EV_POMODORO_STARTED => {
            let b = tr!(lang, "专注", "Focus");
            let d = tr!(lang, "开始专注", "Started focus session").to_string();
            (b.to_string(), d)
        }
        event::EV_POMODORO_ABORTED => {
            let b = tr!(lang, "中断", "Abort");
            let duration = format_duration_secs(event.meta.as_deref());
            let d = tr!(lang, "专注中断 ({})", "Focus aborted ({})", duration);
            (b.to_string(), d)
        }
        event::EV_CHECKLIST => {
            let b = tr!(lang, "清单", "Check");
            let meta = event.meta.as_deref().unwrap_or("");
            let d = if let Some(title) = meta.strip_prefix("toggle:") {
                format!("✓ {}", title)
            } else if let Some(title) = meta.strip_prefix("add:") {
                format!("+ {}", title)
            } else if let Some(title) = meta.strip_prefix("delete:") {
                format!("- {}", title)
            } else if let Some(title) = meta.strip_prefix("rename:") {
                tr!(lang, "重命名步骤: {}", "Renamed item: {}", title)
            } else {
                meta.to_string()
            };
            (b.to_string(), d)
        }
        event::EV_ARCHIVED => {
            let b = tr!(lang, "归档", "Arch");
            let reason = parse_json_field(event.meta.as_deref(), "reason");
            let d = match reason.as_deref() {
                Some("completed") => tr!(lang, "已完成归档", "Archived (completed)").to_string(),
                Some("deleted") => tr!(lang, "已删除归档", "Archived (deleted)").to_string(),
                _ => tr!(lang, "任务归档", "Task archived").to_string(),
            };
            (b.to_string(), d)
        }
        event::EV_RESTORED => {
            let b = tr!(lang, "恢复", "Restore");
            let d = tr!(lang, "从归档箱恢复", "Restored from archive").to_string();
            (b.to_string(), d)
        }
        event::EV_LOGGED => {
            let b = tr!(lang, "日志", "Log");
            let d = event.meta.clone().unwrap_or_default();
            (b.to_string(), d)
        }
        other => {
            let b = other.to_string();
            let d = if let (Some(f), Some(t)) =
                (event.from_status.as_deref(), event.to_status.as_deref())
            {
                format!(
                    "{} → {}",
                    parse_status_label(f, lang),
                    parse_status_label(t, lang)
                )
            } else {
                event.meta.clone().unwrap_or_default()
            };
            (b, d)
        }
    };

    TimelineEntry {
        id: event.id,
        at: event.at,
        event_type: event.event_type.clone(),
        badge,
        description: desc,
    }
}

fn parse_status_label(s: &str, lang: Lang) -> String {
    s.parse::<Status>()
        .map(|st| st.label(lang).to_string())
        .unwrap_or_else(|_| s.to_string())
}

fn parse_json_field(meta: Option<&str>, field: &str) -> Option<String> {
    meta.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| {
            v.get(field)
                .and_then(|val| val.as_str())
                .map(str::to_string)
        })
}

fn format_duration_secs(raw: Option<&str>) -> String {
    if let Some(s) = raw {
        if let Ok(secs) = s.parse::<i64>() {
            let mins = (secs + 30) / 60;
            return format!("{}m", mins.max(1));
        }
    }
    "25m".to_string()
}

fn parse_secs(raw: Option<&str>) -> i64 {
    raw.and_then(|s| s.parse::<i64>().ok()).unwrap_or(1500)
}

fn format_total_duration(total_secs: i64) -> String {
    let mins = (total_secs + 30) / 60;
    if mins >= 60 {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h {}m", h, m)
        }
    } else {
        format!("{}m", mins.max(1))
    }
}

/// 将时间线原始事件按连续同类逻辑智能聚合，并标注关键生命周期里程碑（方案一 + 方案二）。
pub fn aggregate_timeline_events(events: &[TaskEvent], lang: Lang) -> Vec<AggregatedTimelineEntry> {
    let mut result: Vec<AggregatedTimelineEntry> = Vec::new();
    let mut i = 0;

    while i < events.len() {
        let cur = &events[i];

        // 1. 连续番茄钟聚合 (EV_POMODORO)
        if cur.event_type == event::EV_POMODORO {
            let mut j = i + 1;
            let mut total_secs = parse_secs(cur.meta.as_deref());
            while j < events.len() && events[j].event_type == event::EV_POMODORO {
                total_secs += parse_secs(events[j].meta.as_deref());
                j += 1;
            }
            let count = j - i;
            let start_at = cur.at;
            let last_at = events[j - 1].at;
            let end_at = if count > 1 && last_at != start_at {
                Some(last_at)
            } else {
                None
            };

            let badge = tr!(lang, "专注", "Focus").to_string();
            let desc = if count == 1 {
                let duration = format_duration_secs(cur.meta.as_deref());
                tr!(lang, "专注完成 ({})", "Focus session done ({})", duration)
            } else {
                let dur_str = format_total_duration(total_secs);
                tr!(
                    lang,
                    "连续完成 {} 个番茄钟 (共 {})",
                    "{} consecutive focus sessions done ({})",
                    count,
                    dur_str
                )
            };

            result.push(AggregatedTimelineEntry {
                id: events[j - 1].id,
                at: start_at,
                end_at,
                event_type: event::EV_POMODORO.to_string(),
                badge,
                description: desc,
                is_milestone: true,
                count,
            });
            i = j;
            continue;
        }

        // 2. 连续习惯打卡聚合 (EV_HABIT_COMPLETED)
        if cur.event_type == event::EV_HABIT_COMPLETED {
            let mut j = i + 1;
            while j < events.len() && events[j].event_type == event::EV_HABIT_COMPLETED {
                j += 1;
            }
            let count = j - i;
            let start_at = cur.at;
            let last_at = events[j - 1].at;
            let end_at = if count > 1 && last_at != start_at {
                Some(last_at)
            } else {
                None
            };

            let badge = tr!(lang, "习惯", "Habit").to_string();
            let desc = if count == 1 {
                tr!(
                    lang,
                    "习惯打卡完成，已排程至下个周期",
                    "Habit completed, rescheduled to next cycle"
                )
                .to_string()
            } else {
                tr!(
                    lang,
                    "连续打卡完成 {} 次",
                    "{} consecutive habit check-ins done",
                    count
                )
            };

            result.push(AggregatedTimelineEntry {
                id: events[j - 1].id,
                at: start_at,
                end_at,
                event_type: event::EV_HABIT_COMPLETED.to_string(),
                badge,
                description: desc,
                is_milestone: true,
                count,
            });
            i = j;
            continue;
        }

        // 3. 连续检查单操作聚合 (EV_CHECKLIST)
        if cur.event_type == event::EV_CHECKLIST {
            let mut j = i + 1;
            while j < events.len() && events[j].event_type == event::EV_CHECKLIST {
                j += 1;
            }
            let count = j - i;
            if count >= 3 {
                let start_at = cur.at;
                let last_at = events[j - 1].at;
                let end_at = if last_at != start_at {
                    Some(last_at)
                } else {
                    None
                };
                let badge = tr!(lang, "清单", "Check").to_string();
                let desc = tr!(
                    lang,
                    "连续更新 {} 项清单步骤",
                    "{} checklist steps updated",
                    count
                );
                result.push(AggregatedTimelineEntry {
                    id: events[j - 1].id,
                    at: start_at,
                    end_at,
                    event_type: event::EV_CHECKLIST.to_string(),
                    badge,
                    description: desc,
                    is_milestone: false,
                    count,
                });
                i = j;
                continue;
            }
        }

        // 4. 其他单个事件处理并判断是否是关键里程碑
        let entry = describe_event(cur, lang);
        let is_milestone = matches!(
            cur.event_type.as_str(),
            event::EV_CAPTURED
                | event::EV_CLARIFIED
                | event::EV_STATUS_CHANGED
                | event::EV_COMPLETED
                | event::EV_ARCHIVED
                | event::EV_RESTORED
                | event::EV_SCHEDULED
                | event::EV_DUE
        );

        result.push(AggregatedTimelineEntry {
            id: entry.id,
            at: entry.at,
            end_at: None,
            event_type: entry.event_type,
            badge: entry.badge,
            description: entry.description,
            is_milestone,
            count: 1,
        });
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_captured() {
        let e = TaskEvent {
            id: 1,
            task_id: "t1".into(),
            event_type: event::EV_CAPTURED.into(),
            from_status: None,
            to_status: Some("inbox".into()),
            at: 1000,
            meta: None,
        };
        let zh = describe_event(&e, Lang::Zh);
        assert_eq!(zh.badge, "创建");
        assert_eq!(zh.description, "捕获入收件箱");

        let en = describe_event(&e, Lang::En);
        assert_eq!(en.badge, "Init");
        assert_eq!(en.description, "Captured into inbox");
    }

    #[test]
    fn test_describe_status_changed() {
        let e = TaskEvent {
            id: 2,
            task_id: "t1".into(),
            event_type: event::EV_STATUS_CHANGED.into(),
            from_status: Some("inbox".into()),
            to_status: Some("next".into()),
            at: 2000,
            meta: None,
        };
        let zh = describe_event(&e, Lang::Zh);
        assert_eq!(zh.badge, "流转");
        assert_eq!(zh.description, "收件箱 → 下一步");

        let en = describe_event(&e, Lang::En);
        assert_eq!(en.badge, "Status");
        assert_eq!(en.description, "Inbox → Next");
    }

    #[test]
    fn test_describe_pomodoro() {
        let e = TaskEvent {
            id: 3,
            task_id: "t1".into(),
            event_type: event::EV_POMODORO.into(),
            from_status: None,
            to_status: None,
            at: 3000,
            meta: Some("1500".into()),
        };
        let zh = describe_event(&e, Lang::Zh);
        assert_eq!(zh.badge, "专注");
        assert_eq!(zh.description, "专注完成 (25m)");
    }

    #[test]
    fn test_describe_checklist() {
        let e = TaskEvent {
            id: 4,
            task_id: "t1".into(),
            event_type: event::EV_CHECKLIST.into(),
            from_status: None,
            to_status: None,
            at: 4000,
            meta: Some("toggle:写单测".into()),
        };
        let zh = describe_event(&e, Lang::Zh);
        assert_eq!(zh.badge, "清单");
        assert_eq!(zh.description, "✓ 写单测");
    }

    #[test]
    fn test_describe_archived() {
        let e = TaskEvent {
            id: 5,
            task_id: "t1".into(),
            event_type: event::EV_ARCHIVED.into(),
            from_status: None,
            to_status: None,
            at: 5000,
            meta: Some("{\"reason\":\"completed\"}".into()),
        };
        let zh = describe_event(&e, Lang::Zh);
        assert_eq!(zh.badge, "归档");
        assert_eq!(zh.description, "已完成归档");
    }

    #[test]
    fn test_aggregate_consecutive_pomodoro_and_habits() {
        let events = vec![
            TaskEvent {
                id: 1,
                task_id: "t1".into(),
                event_type: event::EV_CAPTURED.into(),
                from_status: None,
                to_status: Some("inbox".into()),
                at: 1000,
                meta: None,
            },
            TaskEvent {
                id: 2,
                task_id: "t1".into(),
                event_type: event::EV_POMODORO.into(),
                from_status: None,
                to_status: None,
                at: 2000,
                meta: Some("1500".into()),
            },
            TaskEvent {
                id: 3,
                task_id: "t1".into(),
                event_type: event::EV_POMODORO.into(),
                from_status: None,
                to_status: None,
                at: 3000,
                meta: Some("1500".into()),
            },
            TaskEvent {
                id: 4,
                task_id: "t1".into(),
                event_type: event::EV_HABIT_COMPLETED.into(),
                from_status: None,
                to_status: None,
                at: 4000,
                meta: None,
            },
            TaskEvent {
                id: 5,
                task_id: "t1".into(),
                event_type: event::EV_HABIT_COMPLETED.into(),
                from_status: None,
                to_status: None,
                at: 5000,
                meta: None,
            },
            TaskEvent {
                id: 6,
                task_id: "t1".into(),
                event_type: event::EV_HABIT_COMPLETED.into(),
                from_status: None,
                to_status: None,
                at: 6000,
                meta: None,
            },
        ];

        let agg = aggregate_timeline_events(&events, Lang::Zh);
        // 原 6 个事件应被聚合为 3 个节点：1 个创建 + 1 个聚合番茄钟(2个) + 1 个聚合打卡(3次)
        assert_eq!(agg.len(), 3);

        // 创建
        assert_eq!(agg[0].badge, "创建");
        assert!(agg[0].is_milestone);
        assert_eq!(agg[0].count, 1);

        // 连续番茄钟
        assert_eq!(agg[1].badge, "专注");
        assert_eq!(agg[1].count, 2);
        assert_eq!(agg[1].at, 2000);
        assert_eq!(agg[1].end_at, Some(3000));
        assert!(agg[1].is_milestone);
        assert!(agg[1].description.contains("连续完成 2 个番茄钟 (共 50m)"));

        // 连续打卡
        assert_eq!(agg[2].badge, "习惯");
        assert_eq!(agg[2].count, 3);
        assert_eq!(agg[2].at, 4000);
        assert_eq!(agg[2].end_at, Some(6000));
        assert!(agg[2].is_milestone);
        assert!(agg[2].description.contains("连续打卡完成 3 次"));
    }

    #[test]
    fn test_cadence_window_durations() {
        // 每日任务自适应 21 天黄金习惯周期
        let daily = cadence_window(Some("FREQ=DAILY")).expect("daily window");
        assert_eq!(daily.horizon_ms, 21 * 86_400_000);
        assert_eq!(daily.label_zh, "21天");

        // 每周任务自适应 12 周季度追踪周期
        let weekly = cadence_window(Some("FREQ=WEEKLY;BYDAY=MO,WE")).expect("weekly window");
        assert_eq!(weekly.horizon_ms, 84 * 86_400_000);
        assert_eq!(weekly.label_zh, "12周");

        // 每月任务自适应 12 个月年度周期
        let monthly = cadence_window(Some("FREQ=MONTHLY;BYMONTHDAY=15")).expect("monthly window");
        assert_eq!(monthly.horizon_ms, 365 * 86_400_000);
        assert_eq!(monthly.label_zh, "12个月");

        // 单次任务无视窗截断
        assert!(cadence_window(None).is_none());
        assert!(cadence_window(Some("FREQ=YEARLY")).is_none());
    }
}
