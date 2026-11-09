use anyhow::Result;
use rusqlite::Connection;

use horae_core::model::task::{Status, Task};
use horae_core::repo::tags;
use horae_core::repo::tasks;
use horae_core::schedule;
use horae_core::time;

pub fn pick_focus_task<'a>(
    conn: &Connection,
    all_tasks: &'a [Task],
) -> Result<Option<(i64, &'a Task)>> {
    let task_ids: Vec<&str> = all_tasks.iter().map(|t| t.id.as_str()).collect();
    let tags_map = tags::get_tags_for_tasks(conn, &task_ids)?;
    let today_start = time::local_day_bounds(0).0;
    let checked_today: std::collections::HashSet<String> =
        tasks::checked_in_today(conn, today_start)
            .unwrap_or_default()
            .into_iter()
            .collect();

    let now = time::now_ms();
    let mut best_task: Option<(i64, &'a Task)> = None;

    for task in all_tasks {
        if matches!(
            task.status,
            Status::Someday | Status::Reference | Status::Done
        ) {
            continue;
        }
        if task.rrule.is_some() && checked_today.contains(&task.id) {
            continue;
        }

        let due = schedule::effective_due(task);

        if task.status == Status::Waiting {
            // Only care about waiting if it is overdue
            if due.map(|d| d > now).unwrap_or(true) {
                continue;
            }
        }
        if task.status == Status::Scheduled {
            // Only care if it is past its scheduled start time, or overdue
            let started = task.scheduled_start_at.map(|s| s <= now).unwrap_or(false);
            let is_overdue = due.map(|d| d <= now).unwrap_or(false);
            if !started && !is_overdue {
                continue;
            }
        }

        let has_quote = tags_map
            .get(&task.id)
            .map(|tags| tags.iter().any(|t| t == tasks::QUOTE_TAG))
            .unwrap_or(false);
        if has_quote {
            continue;
        }

        let mut score = 0_i64;

        match task.priority.as_deref() {
            Some("high") => score += 10000,
            Some("medium") => score += 5000,
            Some("low") => score += 1000,
            _ => {}
        }

        if task.status == Status::Next {
            score += 500;
        }

        // 区分真正的截止时间/周期槽位 (due_at / rrule) 与普通排程起点 (scheduled_start_at)，
        // 避免无 due_at 的排程任务被 effective_due 与 scheduled_start_at 双重计分。
        if task.due_at.is_some() || task.rrule.is_some() {
            if let Some(d) = due {
                if d < now {
                    score += 2000;
                    let days_overdue = (now - d) / (1000 * 60 * 60 * 24);
                    score += days_overdue.min(50) * 10;
                } else {
                    let ms_to_due = d - now;
                    let days_to_due = ms_to_due / (1000 * 60 * 60 * 24);
                    if days_to_due == 0 {
                        score += 1000;
                    }
                }
            }
        } else if let Some(s) = task.scheduled_start_at {
            if s <= now {
                score += 1500;
                let days_started = (now - s) / (1000 * 60 * 60 * 24);
                score += days_started.min(50) * 5;
            }
        }

        if let Some((best_score, best_t)) = best_task {
            if score > best_score || (score == best_score && task.created_at < best_t.created_at) {
                best_task = Some((score, task));
            }
        } else {
            best_task = Some((score, task));
        }
    }

    Ok(best_task)
}

pub fn run(conn: &Connection, start: bool) -> Result<()> {
    let filter = tasks::ListFilter {
        status: None,
        tags: vec![],
        query: None,
        review_stale: false,
    };
    let all_tasks = tasks::list(conn, &filter)?;
    let best_task = pick_focus_task(conn, &all_tasks)?;

    if let Some((score, task)) = best_task {
        println!("🎯 专注目标 (推荐分数: {})", score);
        println!("📝 {}", task.title);
        if let Some(due) = schedule::effective_due(task) {
            let label = if task.due_at.is_some() {
                "截止"
            } else if task.rrule.is_some() {
                "周期发生"
            } else {
                "计划开始"
            };
            println!("⏰ {}: {}", label, time::format_local(Some(due)));
        }

        if start {
            println!("🚀 启动番茄钟...");
            // Use existing pomo start logic
            crate::commands::pomo::start(conn, &task.id)?;
        } else {
            println!("\n💡 提示: 附加 --start 直接开启番茄钟，或在 TUI 中按 P");
        }
    } else {
        println!("🎉 没有紧迫任务，太棒了！");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::capture::{run as capture, CaptureArgs};
    use horae_core::testutil::test_conn;

    #[test]
    fn test_focus_empty() {
        let (_dir, conn) = test_conn();
        assert!(run(&conn, false).is_ok());
    }

    #[test]
    fn test_focus_picks_best_task() {
        let (_dir, conn) = test_conn();
        capture(
            &conn,
            CaptureArgs {
                title: "low priority".into(),
                clip: false,
                tags: vec![],
                high: false,
                medium: false,
                low: true,
                due: None,
                status: None,
                notes: None,
                json: false,
                natural: false,
            },
        )
        .unwrap();

        capture(
            &conn,
            CaptureArgs {
                title: "high priority".into(),
                clip: false,
                tags: vec![],
                high: true,
                medium: false,
                low: false,
                due: None,
                status: Some("next".into()),
                notes: None,
                json: false,
                natural: false,
            },
        )
        .unwrap();

        // This will print to stdout during test, which is fine
        assert!(run(&conn, false).is_ok());
    }

    #[test]
    fn test_focus_picks_scheduled_task() {
        let (_dir, conn) = test_conn();
        // A normal task
        capture(
            &conn,
            CaptureArgs {
                title: "normal task".into(),
                clip: false,
                tags: vec![],
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
                json: false,
                natural: false,
            },
        )
        .unwrap();

        // A scheduled task that is past its start time
        capture(
            &conn,
            CaptureArgs {
                title: "scheduled task ~2000-01-01 09:00".into(),
                clip: false,
                tags: vec![],
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
                json: false,
                natural: false,
            },
        )
        .unwrap();

        // The scheduled task should have a higher score (+1500 + 50*5 = 1750) than the normal task (0),
        // without double-counting the +2000 due_at penalty.
        let all = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        let (score, picked) = pick_focus_task(&conn, &all).unwrap().unwrap();
        assert_eq!(picked.title, "scheduled task");
        assert_eq!(score, 1750);
        assert!(run(&conn, false).is_ok());
    }
}
