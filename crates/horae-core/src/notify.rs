use rusqlite::Connection;

use crate::model::task::Status;
use crate::repo::{notify, tasks};
use crate::schedule::effective_due;
use crate::time;
use anyhow::Result;
use chrono::{Duration, NaiveDate};

/// 桌面通知：直接调用 `notify-send`（与 pomo.rs 一致），避免引入 zbus 依赖栈。
pub fn desktop(summary: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["-u", "normal", "-i", "appointment-soon", summary, body])
        .status();
}

/// 收件箱/等待超期的默认阈值天数。
pub const STALE_DAYS: i64 = 7;

/// 每日聚合摘要中每个类别最多展示的标题数。
const MAX_TITLES: usize = 3;

/// 保留去重 key 的天数（防止 notify.json 无限增长）。
const PRUNE_DAYS: i64 = 7;

/// 每日聚合摘要（纯数据，便于单测）。各字段为 (id, title)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DailyDigest {
    pub inbox: Vec<(String, String)>,
    pub overdue: Vec<(String, String)>,
    pub waiting: Vec<(String, String)>,
}

impl DailyDigest {
    pub fn has_any(&self) -> bool {
        !self.inbox.is_empty() || !self.overdue.is_empty() || !self.waiting.is_empty()
    }
}

/// 计算每日聚合摘要。`now` 与 `days` 注入以便单测。
pub fn collect(conn: &Connection, now: i64, days: i64) -> Result<DailyDigest> {
    let cutoff = now - days * 24 * 3600 * 1000;

    // 逾期：非已完成、非归档，且 effective_due 早于 now。
    let all = tasks::list(
        conn,
        &tasks::ListFilter {
            status: None,
            tags: vec![],
            query: None,
            review_stale: false,
        },
    )?;
    let mut overdue: Vec<(String, String)> = all
        .into_iter()
        .filter(|t| t.status != Status::Done && effective_due(t).is_some_and(|x| x < now))
        .map(|t| (t.id, t.title))
        .collect();
    overdue.sort_by(|a, b| a.1.cmp(&b.1));

    Ok(DailyDigest {
        inbox: tasks::list_stale_inbox(conn, cutoff)?,
        overdue,
        waiting: tasks::list_stale_waiting(conn, cutoff)?,
    })
}

/// 检查并发送每日聚合摘要（合并成一条桌面通知）。同一天至多一次。
pub fn check(conn: &Connection) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let key = format!("digest:{}", today);
    let mut state = notify::get_state()?;
    if state.sent.contains(&key) {
        return Ok(());
    }

    let digest = collect(conn, time::now_ms(), STALE_DAYS)?;
    if !digest.has_any() {
        return Ok(());
    }

    let (summary, body) = render(&digest);
    crate::pomo::notify(&summary, &body);
    state.sent.push(key);
    prune(&mut state, &today);
    notify::save_state(&state)?;
    Ok(())
}

/// 完成动作后的即时反馈：通知今日已完成任务数。
pub fn completed_feedback(conn: &Connection) -> Result<()> {
    let today_start = crate::time::local_day_bounds(0).0;
    let n = tasks::count_completed_since(conn, today_start)?;
    if n == 0 {
        return Ok(());
    }
    crate::pomo::notify(
        "🎉 今日已完成",
        &format!("今天完成了 {} 项任务，继续保持！", n),
    );
    Ok(())
}

fn render(d: &DailyDigest) -> (String, String) {
    let mut total = 0;
    let mut lines = Vec::new();
    if !d.inbox.is_empty() {
        total += d.inbox.len();
        lines.push(format!(
            "📥 收件箱 {} 项滞留超 {} 天：{}",
            d.inbox.len(),
            STALE_DAYS,
            titles(&d.inbox)
        ));
    }
    if !d.overdue.is_empty() {
        total += d.overdue.len();
        lines.push(format!(
            "⏰ 逾期未完成 {} 项：{}",
            d.overdue.len(),
            titles(&d.overdue)
        ));
    }
    if !d.waiting.is_empty() {
        total += d.waiting.len();
        lines.push(format!(
            "⏳ 等待超 {} 天 {} 项：{}",
            STALE_DAYS,
            d.waiting.len(),
            titles(&d.waiting)
        ));
    }
    (
        format!("🧭 GTD 心智维护：{} 项待处理", total),
        lines.join("\n"),
    )
}

fn titles(items: &[(String, String)]) -> String {
    let shown: Vec<String> = items
        .iter()
        .take(MAX_TITLES)
        .map(|(_, title)| format!("\"{}\"", title))
        .collect();
    if items.len() > MAX_TITLES {
        format!("{} 等{}项", shown.join("、"), items.len())
    } else {
        shown.join("、")
    }
}

/// 清理过期的去重 key。日期字符串可按字典序比较，`today` 注入以便单测。
fn prune(state: &mut notify::NotifyState, today: &str) {
    let Ok(cutoff) = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map(|d| d - Duration::days(PRUNE_DAYS))
        .map(|d| d.format("%Y-%m-%d").to_string())
    else {
        return;
    };
    state.sent.retain(|k| {
        k.rsplit_once(':')
            .map(|(_, date)| date >= cutoff.as_str())
            .unwrap_or(true)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tasks::{self, CaptureInput};
    use crate::testutil::test_conn;

    fn mk(conn: &Connection, title: &str, status: Status) -> crate::model::task::Task {
        tasks::create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                status,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap()
    }

    /// 把任务时间戳统一往前拨 `shift_ms`，模拟“很久以前创建/更新”。
    fn backdate(conn: &Connection, id: &str, shift_ms: i64) {
        conn.execute(
            "UPDATE tasks SET created_at = created_at - ?1, updated_at = updated_at - ?1 WHERE id = ?2",
            rusqlite::params![shift_ms, id],
        )
        .unwrap();
    }

    #[test]
    fn collect_aggregates_inbox_overdue_and_waiting() {
        let (_dir, conn) = test_conn();
        let now = time::now_ms();
        let day = 24 * 3600 * 1000i64;

        // 滞留收件箱：8 天前创建
        let stale = mk(&conn, "老收件箱", Status::Inbox);
        backdate(&conn, &stale.id, 8 * day);
        // 逾期：next + 过去 2 天的 due
        let over = mk(&conn, "逾期任务", Status::Next);
        tasks::set_due(&conn, &over.id, Some(now - 2 * day)).unwrap();
        // 老化等待：waiting + 8 天没动过
        let wait = mk(&conn, "老等待", Status::Waiting);
        backdate(&conn, &wait.id, 8 * day);
        // 不应计入：新收件箱 / 已完成 / 正常 scheduled
        mk(&conn, "新收件箱", Status::Inbox);
        let done = mk(&conn, "已完成", Status::Next);
        tasks::transition(&conn, &done.id, Status::Done).unwrap();

        let d = collect(&conn, now, 7).unwrap();
        assert_eq!(d.inbox.len(), 1);
        assert_eq!(d.inbox[0].1, "老收件箱");
        assert_eq!(d.overdue.len(), 1);
        assert_eq!(d.overdue[0].1, "逾期任务");
        assert_eq!(d.waiting.len(), 1);
        assert_eq!(d.waiting[0].1, "老等待");
        assert!(d.has_any());
    }

    #[test]
    fn collect_empty_when_nothing_stale() {
        let (_dir, conn) = test_conn();
        let now = time::now_ms();
        mk(&conn, "新鲜任务", Status::Inbox);
        let d = collect(&conn, now, 7).unwrap();
        assert!(!d.has_any());
        assert!(d.inbox.is_empty() && d.overdue.is_empty() && d.waiting.is_empty());
    }

    #[test]
    fn render_merges_into_single_notification() {
        let d = DailyDigest {
            inbox: vec![("a".into(), "买牛奶".into())],
            overdue: vec![("b".into(), "交电费".into()), ("c".into(), "体检".into())],
            waiting: vec![],
        };
        let (summary, body) = render(&d);
        assert_eq!(summary, "🧭 GTD 心智维护：3 项待处理");
        assert!(body.contains("收件箱 1 项滞留超 7 天"));
        assert!(body.contains("\"买牛奶\""));
        assert!(body.contains("逾期未完成 2 项"));
    }

    #[test]
    fn render_truncates_title_list() {
        let items: Vec<(String, String)> = (0..5)
            .map(|i| (format!("id{i}"), format!("任务{i}")))
            .collect();
        let s = titles(&items);
        assert!(s.contains("\"任务0\""));
        assert!(!s.contains("\"任务4\""));
        assert!(s.contains("等5项"));
    }

    #[test]
    fn prune_keeps_only_recent_dates() {
        let mut state = notify::NotifyState {
            sent: vec![
                "digest:2026-08-13".into(),
                "digest:2026-08-06".into(),
                "digest:2026-08-05".into(),
            ],
        };
        prune(&mut state, "2026-08-13");
        assert_eq!(
            state.sent,
            vec![
                "digest:2026-08-13".to_string(),
                "digest:2026-08-06".to_string()
            ]
        );
    }
}
