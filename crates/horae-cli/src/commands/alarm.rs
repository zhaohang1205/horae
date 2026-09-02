use rusqlite::Connection;
use serde_json::json;
use std::process::Command as StdCommand;

use anyhow::Result;
use horae_core::model::task::{Status, Task};
use horae_core::repo::alarm;
use horae_core::repo::{tags, tasks};
use horae_core::schedule::effective_due;
use horae_core::time;

/// 提前多少分钟触发闹铃提醒。
pub const LEAD_MINUTES: i64 = 5;

/// 过期 handled key 的保留窗口（防止状态文件无限增长）。
const PRUNE_MS: i64 = 7 * 24 * 3600 * 1000;

const CLASS_SOON: &str = "alarm-soon";
const CLASS_NOW: &str = "alarm-now";
const CLASS_OVERDUE: &str = "alarm-overdue";
const CLASS_NONE: &str = "alarm-none";

/// 两个 waybar 闹铃模块共享的刷新信号 (SIGRTMIN+12)。窗口滚动时 (点按跳过 /
/// 数据库变更) 由 `horae alarm` 发出, 让 slot 1 与 slot 2 同时重算、同帧刷新,
/// 避免左右显示错位或重复。
const WAYBAR_SYNC_SIGNAL: &str = "-RTMIN+12";

fn refresh_waybar() {
    // 仅当 waybar 在运行时生效, 否则静默忽略
    let _ = StdCommand::new("pkill")
        .args([WAYBAR_SYNC_SIGNAL, "-x", "waybar"])
        .status();
}

/// Occurrence dedup key: `task_id:occurrence_ms`. 循环任务的每次 occurrence
/// 都会生成新 key, 因此会按周期重新提醒。
fn occ_key(id: &str, occ_ms: i64) -> String {
    format!("{}:{}", id, occ_ms)
}

/// 选出滚动窗口内的任务：非 Done、有 effective_due、且当前 occurrence
/// 未被跳过（skipped）, 按到期时间升序（逾期在前）取前 `limit` 个。已响铃
/// 但未跳过的任务仍保留在窗口内提醒用户。纯函数, 便于单测。
fn window(tasks: &[Task], skipped: &[String], limit: usize) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = tasks
        .iter()
        .filter(|t| t.status != Status::Done)
        .filter_map(|t| effective_due(t).map(|d| (t.id.clone(), d)))
        .filter(|(id, d)| !skipped.contains(&occ_key(id, *d)))
        .collect();
    v.sort_by_key(|(_, d)| *d);
    v.truncate(limit.max(1));
    v
}

/// Window 的 occurrence-key 表示, 用于比较两次计算出的窗口是否相同
/// (窗口是否滚动)。
fn window_keys(win: &[(String, i64)]) -> Vec<String> {
    win.iter().map(|(id, d)| occ_key(id, *d)).collect()
}

/// 到点需要响铃的 occurrence：已进入「截止前 LEAD_MINUTES 分钟」窗口且未
/// 响铃过（rung）。扫描全部任务（不只看窗口内两个）, 避免窗口被逾期项占满时
/// 漏报。纯函数, 便于单测。
fn due_to_ring(tasks: &[Task], rung: &[String], now: i64, lead_ms: i64) -> Vec<(String, i64)> {
    tasks
        .iter()
        .filter(|t| t.status != Status::Done)
        .filter_map(|t| effective_due(t).map(|d| (t.id.clone(), d)))
        .filter(|(id, d)| now >= d - lead_ms && !rung.contains(&occ_key(id, *d)))
        .collect()
}

/// Waybar 多闹铃模块: 每个 slot 输出单个 JSON 对象。waybar v0.15 的 custom
/// json 只支持单对象（数组会报 requires objectValue）, 因此用两个模块
/// `horae alarm waybar 1` / `horae alarm waybar 2` 各渲染一个闹铃。
/// 两个模块共享 SIGRTMIN+12 信号: 窗口滚动或点按跳过时 `horae alarm` 发出该
/// 信号, 强制两个模块同一帧重算刷新, 保证左(第一件)/右(第二件)始终同步。
/// 仅 slot 1 负责触发响铃与写状态文件, 避免两个进程并发写 alarm.json。
/// 传 `emit_all = true` 时整窗作为 JSON 数组输出（供 omarchy/waybar 单次拉取多任务）。
pub fn waybar(slot: Option<usize>, limit: Option<usize>, emit_all: bool) -> Result<()> {
    let slot = slot.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(2).clamp(1, 10);
    let conn = horae_core::db::conn::open(None)?;
    let all = tasks::list(
        &conn,
        &tasks::ListFilter {
            status: None,
            tags: Vec::new(),
            query: None,
            review_stale: false,
        },
    )?;
    let now = time::now_ms();
    let lead_ms = LEAD_MINUTES * 60 * 1000;
    let mut state = alarm::get_state().unwrap_or_default();

    let win = window(&all, &state.skipped, limit);
    let win_keys = window_keys(&win);
    let window_changed = state.last_window != win_keys;

    if slot == 1 {
        // 触发到点闹铃（notify-send + 声音）; 已响铃任务仍留在窗口内
        let mut dirty = false;
        for (id, d) in due_to_ring(&all, &state.rung, now, lead_ms) {
            if let Ok(t) = tasks::get(&conn, &id) {
                state.rung.push(occ_key(&id, d));
                crate::commands::pomo::notify(
                    "⏰ 任务提醒",
                    &format!(
                        "5 分钟后要做:\n{}\n截止: {}",
                        t.title,
                        time::format_local(Some(d))
                    ),
                );
                dirty = true;
            }
        }

        // 清理过期 key; 窗口滚动时记录最新窗口, slot 2 据此对齐
        let pre_len = state.rung.len() + state.skipped.len();
        state.rung.retain(|k| key_fresh(k, now));
        state.skipped.retain(|k| key_fresh(k, now));
        let post_len = state.rung.len() + state.skipped.len();
        dirty |= post_len != pre_len;
        if window_changed {
            state.last_window = win_keys;
            dirty = true;
        }

        // 有实际变化才落盘 (rung/skipped 只由 slot 1 写, 避免并发写文件)
        if dirty {
            alarm::save_state(&state)?;
        }
    }

    // Bug6 修复：仅 slot 1 负责发 SIGRTMIN+12 信号，避免两个 slot 进程并发
    // 各发一次导致双重刷新闪烁。slot 2 跳过此步，依赖 slot 1 触发的同帧刷新。
    if window_changed && slot == 1 {
        refresh_waybar();
    }

    if emit_all {
        let items: Vec<serde_json::Value> = win
            .iter()
            .map(|(id, d)| alarm_item(&conn, id, *d, now, lead_ms))
            .collect::<Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string(&items)?);
        return Ok(());
    }

    let item = win
        .get(slot - 1)
        .map(|(id, d)| alarm_item(&conn, id, *d, now, lead_ms))
        .transpose()?
        .unwrap_or_else(empty_item);
    println!("{}", serde_json::to_string(&item)?);
    Ok(())
}

/// 点击闹铃条：把窗口内第 `slot` 个任务的当前 occurrence 标记为 skipped,
/// 窗口滚动补位。slot 缺省为 1。
pub fn next(slot: Option<usize>, limit: Option<usize>) -> Result<()> {
    let slot = slot.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(2).clamp(1, 10);
    let conn = horae_core::db::conn::open(None)?;
    let all = tasks::list(
        &conn,
        &tasks::ListFilter {
            status: None,
            tags: Vec::new(),
            query: None,
            review_stale: false,
        },
    )?;
    let mut state = alarm::get_state().unwrap_or_default();
    if let Some((id, d)) = window(&all, &state.skipped, limit).get(slot - 1) {
        state.skipped.push(occ_key(id, *d));
        alarm::save_state(&state)?;
        // 立即让两个 slot 同帧刷新: 点按后无需等待下一个 interval
        refresh_waybar();
        if let Ok(t) = tasks::get(&conn, id) {
            println!("skipped: {}", t.title);
        }
    }
    Ok(())
}

fn key_fresh(key: &str, now: i64) -> bool {
    key.rsplit_once(':')
        .is_some_and(|(_, m)| m.parse::<i64>().is_ok_and(|ms| ms >= now - PRUNE_MS))
}

fn alarm_item(
    conn: &Connection,
    id: &str,
    due_ms: i64,
    now: i64,
    lead_ms: i64,
) -> Result<serde_json::Value> {
    let t = tasks::get(conn, id)?;
    let (class, marker) = if due_ms < now {
        (CLASS_OVERDUE, "⏰")
    } else if due_ms - now <= lead_ms {
        (CLASS_NOW, "🔔")
    } else {
        (CLASS_SOON, "⏰")
    };

    let mut title = t.title.clone();
    if title.chars().count() > 14 {
        let truncated: String = title.chars().take(13).collect();
        title = format!("{}…", truncated);
    }

    let tags_s = tags::get_task_tags(conn, id)?
        .iter()
        .map(|x| x.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let tags_s = if tags_s.is_empty() {
        "-"
    } else {
        tags_s.as_str()
    };

    Ok(json!({
        "text": format!("{} {} {}", marker, compact_time(due_ms), title),
        "alt": class,
        "class": class,
        "tooltip": format!(
            "{}\n状态: {}\n截止: {}\n标签: {}",
            t.title,
            t.status,
            time::format_local(Some(due_ms)),
            tags_s
        ),
        "id": t.id,
        "title": t.title,
        "due": due_ms,
        "status": format!("{}", t.status),
        "tags": tags_s,
    }))
}

fn empty_item() -> serde_json::Value {
    json!({
        "text": "⏰ --",
        "alt": CLASS_NONE,
        "class": CLASS_NONE,
        "tooltip": "暂无即将到来的任务",
    })
}

/// 紧凑时间：今天只显示 `HH:MM`, 跨天显示 `MM-DD HH:MM`。
fn compact_time(ms: i64) -> String {
    use chrono::{Local, TimeZone, Utc};
    match Utc.timestamp_millis_opt(ms).single() {
        Some(dt) => {
            let local = dt.with_timezone(&Local);
            if local.date_naive() == Local::now().date_naive() {
                local.format("%H:%M").to_string()
            } else {
                local.format("%m-%d %H:%M").to_string()
            }
        }
        None => ms.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_task(id: &str, status: Status, due: Option<i64>) -> Task {
        Task {
            id: id.into(),
            title: format!("t-{}", id),
            notes: String::new(),
            status,
            rrule: None,
            priority: None,
            created_at: 0,
            clarified_at: None,
            due_at: due,
            scheduled_start_at: None,
            scheduled_end_at: None,
            completed_at: None,
            archived_at: None,
            archive_reason: None,
            updated_at: 0,
            delegated_to: None,
            checklist: vec![],
        }
    }

    #[test]
    fn window_takes_two_nearest_and_skips_done() {
        let tasks = vec![
            mk_task("a", Status::Next, Some(3000)),
            mk_task("b", Status::Next, Some(1000)),
            mk_task("c", Status::Next, Some(2000)),
            mk_task("d", Status::Done, Some(500)),
        ];
        let w = window(&tasks, &[], 2);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], ("b".to_string(), 1000));
        assert_eq!(w[1], ("c".to_string(), 2000));
    }

    #[test]
    fn window_excludes_skipped_and_advances() {
        let tasks = vec![
            mk_task("a", Status::Next, Some(1000)),
            mk_task("b", Status::Next, Some(2000)),
            mk_task("c", Status::Next, Some(3000)),
        ];
        let skipped = vec![occ_key("a", 1000)];
        let w = window(&tasks, &skipped, 2);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], ("b".to_string(), 2000));
        assert_eq!(w[1], ("c".to_string(), 3000));
    }

    #[test]
    fn window_respects_limit() {
        let tasks = vec![
            mk_task("a", Status::Next, Some(1000)),
            mk_task("b", Status::Next, Some(2000)),
            mk_task("c", Status::Next, Some(3000)),
        ];
        let one = window(&tasks, &[], 1);
        assert_eq!(one, vec![("a".to_string(), 1000)]);
        let three = window(&tasks, &[], 3);
        assert_eq!(three.len(), 3);
    }

    #[test]
    fn window_keys_match_same_window() {
        let w = vec![("a".to_string(), 1000), ("b".to_string(), 2000)];
        assert_eq!(
            window_keys(&w),
            vec![occ_key("a", 1000), occ_key("b", 2000)]
        );
    }

    #[test]
    fn rung_task_stays_in_window() {
        let tasks = vec![
            mk_task("a", Status::Next, Some(1000)),
            mk_task("b", Status::Next, Some(2000)),
        ];
        let w = window(&tasks, &[], 2);
        assert_eq!(w[0].0, "a");
    }

    #[test]
    fn alarm_item_embeds_task_fields() {
        let (_dir, conn) = horae_core::testutil::test_conn();
        let t = horae_core::repo::tasks::create_capture(
            &conn,
            &horae_core::repo::tasks::CaptureInput {
                title: "提交周报".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let due = 1_750_000_000_000;
        let item = alarm_item(&conn, &t.id, due, 1_800_000_000_000, 300_000).unwrap();
        assert_eq!(item["id"], t.id);
        assert_eq!(item["title"], "提交周报");
        assert_eq!(item["due"], due);
        assert_eq!(item["status"], "inbox");
        assert_eq!(item["class"], CLASS_OVERDUE);
        assert!(item["tags"].is_string());
    }

    #[test]
    fn ring_fires_only_within_lead_window() {
        let now = 10_000;
        let lead_ms = 5 * 60 * 1000; // 300_000
        let tasks = vec![
            mk_task("soon", Status::Next, Some(now + 60_000)),
            mk_task("just_out", Status::Next, Some(now + 360_000)),
            mk_task("overdue", Status::Next, Some(now - 1000)),
        ];
        let mut ring = due_to_ring(&tasks, &[], now, lead_ms);
        ring.sort_by_key(|(_, due)| *due);
        let ids: Vec<String> = ring.into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec!["overdue", "soon"]);
    }

    #[test]
    fn ring_skips_rung_occurrence() {
        let now = 10_000;
        let lead_ms = 5 * 60 * 1000;
        let tasks = vec![mk_task("a", Status::Next, Some(now))];
        let rung = vec![occ_key("a", now)];
        assert!(due_to_ring(&tasks, &rung, now, lead_ms).is_empty());
    }
}
