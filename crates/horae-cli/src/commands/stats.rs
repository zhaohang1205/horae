use anyhow::Result;
use horae_core::model::event;
use horae_core::repo::tasks::{count_by_status, count_completed_since};
use horae_core::time::{local_day_bounds, now_ms};
use rusqlite::Connection;

fn format_duration_short(ms: i64) -> String {
    let mins = (ms.max(0) / 60_000).max(1);
    if mins < 60 {
        format!("{}m", mins)
    } else if mins < 1440 {
        format!("{:.1}h", mins as f64 / 60.0)
    } else {
        format!("{:.1}d", mins as f64 / 1440.0)
    }
}

pub fn get_stats_lines(conn: &Connection) -> Result<Vec<String>> {
    let (today_start, _) = local_day_bounds(0);
    let week_start = now_ms() - 7 * 24 * 3600 * 1000;

    let completed_tasks = count_completed_since(conn, today_start)?;
    let completed_habits_today: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_HABIT_COMPLETED, today_start],
        |r| r.get(0),
    )?;
    let completed_pomodoros: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_POMODORO, today_start],
        |r| r.get(0),
    )?;
    let aborted_pomodoros: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_POMODORO_ABORTED, today_start],
        |r| r.get(0),
    )?;

    let week_completed = count_completed_since(conn, week_start)?;
    let week_habits: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_HABIT_COMPLETED, week_start],
        |r| r.get(0),
    )?;
    let week_pomodoros: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_POMODORO, week_start],
        |r| r.get(0),
    )?;

    // Time-Datafication: 平均澄清耗时 (clarified_at - created_at)
    let avg_clarify_ms: Option<f64> = conn
        .query_row(
            "SELECT AVG(clarified_at - created_at) FROM tasks \
             WHERE clarified_at IS NOT NULL AND clarified_at >= created_at AND id != '__journal__'",
            [],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let clarify_str = avg_clarify_ms
        .map(|v| format_duration_short(v as i64))
        .unwrap_or_else(|| "-".to_string());

    // Time-Datafication: 拖延指数（非循环未完成任务中被重排程 >= 3 次的任务数）
    let procrastinated_count: usize = conn.query_row(
        "SELECT COUNT(*) FROM (
            SELECT t.id FROM tasks t
            JOIN task_events e ON e.task_id = t.id
            WHERE t.archived_at IS NULL AND t.status != 'done' AND t.rrule IS NULL
              AND e.event_type = ?1
            GROUP BY t.id
            HAVING COUNT(*) >= 3
        )",
        rusqlite::params![event::EV_SCHEDULED],
        |r| r.get(0),
    )?;

    let counts = count_by_status(conn, None)?;
    let inbox = counts.get("inbox").copied().unwrap_or(0);
    let next = counts.get("next").copied().unwrap_or(0);
    let waiting = counts.get("waiting").copied().unwrap_or(0);
    let scheduled = counts.get("scheduled").copied().unwrap_or(0);
    let total_pending = inbox + next + scheduled + waiting;

    // 活跃情境标签 Top 3
    let mut top_tags_stmt = conn.prepare(
        "SELECT g.name, COUNT(*) as cnt FROM task_tags tt
         JOIN tags g ON g.id = tt.tag_id
         JOIN tasks t ON t.id = tt.task_id
         WHERE t.archived_at IS NULL AND t.status != 'done' AND g.name != 'quote'
         GROUP BY g.name
         ORDER BY cnt DESC, g.name ASC
         LIMIT 3",
    )?;
    let top_tags: Vec<String> = top_tags_stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let cnt: usize = r.get(1)?;
            Ok(format!("@{}({})", name, cnt))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let top_contexts_str = if top_tags.is_empty() {
        "-".to_string()
    } else {
        top_tags.join(" ")
    };

    let target = 8;
    let filled = std::cmp::min(completed_pomodoros, target);
    let empty = target - filled;

    let pomo_bar = format!(
        "\x1b[38;2;250;179;135m{}\x1b[38;2;49;50;68m{}\x1b[0m",
        "■ ".repeat(filled),
        "□ ".repeat(empty)
    );
    let total_pomo_attempts = completed_pomodoros + aborted_pomodoros;
    let focus_rate = (completed_pomodoros * 100)
        .checked_div(total_pomo_attempts)
        .map(|rate| format!(" ({rate}% focus rate)"))
        .unwrap_or_default();

    let stats = vec![
        " \x1b[1;38;2;205;214;244mHORAE\x1b[0m".to_string(), // Text
        " \x1b[38;2;245;194;231mGoddess of Time · Time-Datafication\x1b[0m".to_string(), // Pink
        "".to_string(),
        format!(
            " \x1b[38;2;166;227;161mToday's Pomodoros\x1b[0m : {} {}{}",
            completed_pomodoros, pomo_bar, focus_rate
        ), // Green
        format!(
            " \x1b[38;2;166;227;161mTasks Completed\x1b[0m   : {} (+{} habits today)",
            completed_tasks, completed_habits_today
        ),
        format!(
            " \x1b[38;2;137;180;250m7-Day Velocity\x1b[0m    : {} tasks · {} habits · {} 🍅",
            week_completed, week_habits, week_pomodoros
        ),
        format!(
            " \x1b[38;2;203;166;247mClarify Lead Time\x1b[0m : {} avg  |  Rescheduled ≥3x: {}",
            clarify_str, procrastinated_count
        ),
        "".to_string(),
        format!(
            " \x1b[38;2;249;226;175mPending Tasks\x1b[0m     : {}",
            total_pending
        ), // Yellow
        format!(
            "   \x1b[38;2;186;194;222mInbox\x1b[0m           : {}",
            inbox
        ), // Subtext1
        format!("   \x1b[38;2;186;194;222mNext Action\x1b[0m     : {}", next),
        format!(
            "   \x1b[38;2;186;194;222mScheduled\x1b[0m       : {}",
            scheduled
        ),
        format!(
            "   \x1b[38;2;186;194;222mWaiting\x1b[0m         : {}",
            waiting
        ),
        format!(
            "   \x1b[38;2;186;194;222mTop Contexts\x1b[0m    : {}",
            top_contexts_str
        ),
    ];

    Ok(stats)
}

pub fn run(conn: &Connection) -> Result<()> {
    let stats = get_stats_lines(conn)?;

    // Default left padding for CLI output if not in TUI
    let left_pad_str = " ".repeat(4);

    println!();
    for line in stats {
        println!("{}{}", left_pad_str, line);
    }
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horae_core::model::task::Status;
    use horae_core::repo::tasks::{create_capture, schedule, transition, CaptureInput};
    use horae_core::testutil::test_conn;

    #[test]
    fn stats_reports_time_datafication_metrics() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "procrastinated task".into(),
                status: Status::Inbox,
                tag_names: vec!["work".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let now = now_ms();
        schedule(&conn, &t.id, now + 3_600_000, None, None).unwrap();
        schedule(&conn, &t.id, now + 7_200_000, None, None).unwrap();
        schedule(&conn, &t.id, now + 10_800_000, None, None).unwrap();

        let t2 = create_capture(
            &conn,
            &CaptureInput {
                title: "finished task".into(),
                status: Status::Next,
                tag_names: vec!["work".into()],
                ..Default::default()
            },
        )
        .unwrap();
        transition(&conn, &t2.id, Status::Done).unwrap();

        let lines = get_stats_lines(&conn).unwrap().join("\n");
        assert!(lines.contains("7-Day Velocity"));
        assert!(lines.contains("Clarify Lead Time"));
        assert!(lines.contains("Rescheduled ≥3x: 1"));
        assert!(lines.contains("@work(1)"));
    }
}
