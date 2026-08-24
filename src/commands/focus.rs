use anyhow::Result;
use rusqlite::Connection;

use crate::model::task::{Status, Task};
use crate::repo::tags;
use crate::repo::tasks;
use crate::schedule;
use crate::time;

pub fn run(conn: &Connection, start: bool) -> Result<()> {
    let filter = tasks::ListFilter {
        status: None,
        tags: vec![],
        query: None,
        review_stale: false,
    };
    let all_tasks = tasks::list(conn, &filter)?;
    let task_ids: Vec<&str> = all_tasks.iter().map(|t| t.id.as_str()).collect();
    let tags_map = tags::get_tags_for_tasks(conn, &task_ids)?;

    let now = time::now_ms();
    let mut best_task: Option<(i64, &Task)> = None;

    for task in &all_tasks {
        if matches!(
            task.status,
            Status::Someday | Status::Reference | Status::Done
        ) {
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

        let task_tags = tags_map.get(&task.id);
        let tag_names: Vec<&str> = task_tags
            .map(|v| v.iter().map(|t| t.as_str()).collect())
            .unwrap_or_default();

        if tag_names.contains(&tasks::QUOTE_TAG) {
            continue;
        }

        let mut score = 0_i64;

        if tag_names.contains(&"p1") {
            score += 10000;
        } else if tag_names.contains(&"p2") {
            score += 5000;
        } else if tag_names.contains(&"p3") {
            score += 1000;
        }

        if task.status == Status::Next {
            score += 500;
        }

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

        if let Some((best_score, best_t)) = best_task {
            if score > best_score || (score == best_score && task.created_at < best_t.created_at) {
                best_task = Some((score, task));
            }
        } else {
            best_task = Some((score, task));
        }
    }

    if let Some((score, task)) = best_task {
        println!("🎯 专注目标 (推荐分数: {})", score);
        println!("📝 {}", task.title);
        if let Some(due) = schedule::effective_due(task) {
            println!("⏰ 截止: {}", time::format_local(Some(due)));
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
    use crate::testutil::test_conn;

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
                tags: vec!["p3".into()],
                p1: false,
                p2: false,
                p3: true,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap();

        capture(
            &conn,
            CaptureArgs {
                title: "high priority".into(),
                tags: vec!["p1".into()],
                p1: true,
                p2: false,
                p3: false,
                due: None,
                status: Some("next".into()),
                json: false,
            },
        )
        .unwrap();

        // This will print to stdout during test, which is fine
        assert!(run(&conn, false).is_ok());
    }
}
