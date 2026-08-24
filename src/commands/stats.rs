use crate::model::event;
use crate::repo::tasks::{count_by_status, count_completed_since};
use crate::time::local_day_bounds;
use anyhow::Result;
use rusqlite::Connection;

pub fn get_stats_lines(conn: &Connection) -> Result<Vec<String>> {
    let (today_start, _) = local_day_bounds(0);

    let completed_tasks = count_completed_since(conn, today_start)?;
    let completed_pomodoros: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_POMODORO, today_start],
        |r| r.get(0),
    )?;

    let counts = count_by_status(conn, None)?;
    let inbox = counts.get("inbox").copied().unwrap_or(0);
    let next = counts.get("next").copied().unwrap_or(0);
    let waiting = counts.get("waiting").copied().unwrap_or(0);
    let scheduled = counts.get("scheduled").copied().unwrap_or(0);
    let total_pending = inbox + next + scheduled + waiting;

    let target = 8;
    let filled = std::cmp::min(completed_pomodoros, target);
    let empty = target - filled;

    let pomo_bar = format!(
        "\x1b[38;2;250;179;135m{}\x1b[38;2;49;50;68m{}\x1b[0m",
        "■ ".repeat(filled),
        "□ ".repeat(empty)
    );

    let stats = vec![
        " \x1b[1;38;2;205;214;244mHORAE\x1b[0m".to_string(), // Text
        " \x1b[38;2;245;194;231mGoddess of Time\x1b[0m".to_string(), // Pink
        "".to_string(),
        "".to_string(),
        format!(
            " \x1b[38;2;166;227;161mToday's Pomodoros\x1b[0m : {} {}",
            completed_pomodoros, pomo_bar
        ), // Green
        format!(
            " \x1b[38;2;166;227;161mTasks Completed\x1b[0m   : {}",
            completed_tasks
        ),
        "".to_string(),
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
