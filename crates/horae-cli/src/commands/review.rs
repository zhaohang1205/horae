use rusqlite::Connection;

use anyhow::Result;
use horae_core::model::{event, task::Status};
use horae_core::repo::tasks;
use horae_core::schedule::effective_due;
use horae_core::time;

pub fn build_review_report(conn: &Connection) -> Result<String> {
    let all = tasks::list(
        conn,
        &tasks::ListFilter {
            status: None,
            tags: vec![],
            query: None,
            review_stale: false,
        },
    )?;
    let inbox = all.iter().filter(|t| t.status == Status::Inbox).count();
    let next = all.iter().filter(|t| t.status == Status::Next).count();
    let waiting = all.iter().filter(|t| t.status == Status::Waiting).count();
    let someday = all.iter().filter(|t| t.status == Status::Someday).count();
    let scheduled = all.iter().filter(|t| t.status == Status::Scheduled).count();

    let now = time::now_ms();
    let day_ms = 24 * 3600 * 1000i64;
    let horizon = 3 * day_ms;
    let due_soon = all
        .iter()
        .filter(|t| {
            t.status != Status::Done
                && effective_due(t)
                    .map(|x| x <= now + horizon)
                    .unwrap_or(false)
        })
        .count();

    let week_start = now - 7 * day_ms;
    let week_completed = tasks::count_completed_since(conn, week_start)?;
    let week_habits: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_events WHERE event_type = ?1 AND at >= ?2",
        rusqlite::params![event::EV_HABIT_COMPLETED, week_start],
        |r| r.get(0),
    )?;

    let mut out = String::new();
    out.push_str("Weekly Review\n");
    out.push_str(&format!("  inbox     : {}\n", inbox));
    out.push_str(&format!("  next      : {}\n", next));
    out.push_str(&format!("  waiting   : {}\n", waiting));
    out.push_str(&format!(
        "  scheduled : {} ({} due within 3d)\n",
        scheduled, due_soon
    ));
    out.push_str(&format!("  someday   : {}\n", someday));
    out.push_str(&format!(
        "  7d output : {} completed, {} habit check-ins\n",
        week_completed, week_habits
    ));

    // 1. Overdue tasks
    let mut overdue: Vec<_> = all
        .iter()
        .filter(|t| !matches!(t.status, Status::Done | Status::Reference))
        .filter_map(|t| effective_due(t).filter(|&d| d < now).map(|d| (t, d)))
        .collect();
    overdue.sort_by_key(|(_, d)| *d);
    if !overdue.is_empty() {
        out.push_str(&format!("\n[!] Overdue ({})\n", overdue.len()));
        for (t, d) in overdue.iter().take(10) {
            out.push_str(&format!(
                "  - [{}] {} ({}, due {})\n",
                &t.id[..8.min(t.id.len())],
                t.title,
                t.status,
                time::format_local(Some(*d))
            ));
        }
    }

    // 2. Stale Inbox (> 3 days unclarified)
    let stale_inbox: Vec<_> = all
        .iter()
        .filter(|t| t.status == Status::Inbox && t.created_at < now - 3 * day_ms)
        .collect();
    if !stale_inbox.is_empty() {
        out.push_str(&format!("\n[!] Stale Inbox >3d ({})\n", stale_inbox.len()));
        for t in stale_inbox.iter().take(10) {
            let age_d = ((now - t.created_at) / day_ms).max(1);
            out.push_str(&format!(
                "  - [{}] {} ({}d in inbox)\n",
                &t.id[..8.min(t.id.len())],
                t.title,
                age_d
            ));
        }
    }

    // 3. Aging Waiting (> 7 days without update)
    let aging_waiting: Vec<_> = all
        .iter()
        .filter(|t| t.status == Status::Waiting && t.updated_at < now - 7 * day_ms)
        .collect();
    if !aging_waiting.is_empty() {
        out.push_str(&format!(
            "\n[!] Aging Waiting >7d ({})\n",
            aging_waiting.len()
        ));
        for t in aging_waiting.iter().take(10) {
            let age_d = ((now - t.updated_at) / day_ms).max(1);
            let who = t
                .delegated_to
                .as_deref()
                .map(|w| format!(" @{w}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  - [{}] {}{} (idle {}d)\n",
                &t.id[..8.min(t.id.len())],
                t.title,
                who,
                age_d
            ));
        }
    }

    // 4. Stale Someday / Next (> 14 days untouched)
    let stale_backlog: Vec<_> = all
        .iter()
        .filter(|t| {
            matches!(t.status, Status::Someday | Status::Next) && t.updated_at < now - 14 * day_ms
        })
        .collect();
    if !stale_backlog.is_empty() {
        out.push_str(&format!(
            "\n[?] Stale Next/Someday >14d ({})\n",
            stale_backlog.len()
        ));
        for t in stale_backlog.iter().take(10) {
            let age_d = ((now - t.updated_at) / day_ms).max(1);
            out.push_str(&format!(
                "  - [{}] {} ({}, idle {}d)\n",
                &t.id[..8.min(t.id.len())],
                t.title,
                t.status,
                age_d
            ));
        }
    }

    Ok(out)
}

pub fn run(conn: &Connection) -> Result<()> {
    let report = build_review_report(conn)?;
    print!("{}", report);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horae_core::repo::tasks::{create_capture, CaptureInput};
    use horae_core::testutil::test_conn;

    #[test]
    fn review_report_includes_overdue_and_stale_diagnostics() {
        let (_dir, conn) = test_conn();
        let now = time::now_ms();
        let day_ms = 24 * 3600 * 1000;

        let t1 = create_capture(
            &conn,
            &CaptureInput {
                title: "overdue item".into(),
                status: Status::Next,
                due_at: Some(now - 2 * day_ms),
                ..Default::default()
            },
        )
        .unwrap();

        let t2 = create_capture(
            &conn,
            &CaptureInput {
                title: "old inbox item".into(),
                status: Status::Inbox,
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE tasks SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now - 5 * day_ms, t2.id],
        )
        .unwrap();

        let report = build_review_report(&conn).unwrap();
        assert!(report.contains("Weekly Review"));
        assert!(report.contains("[!] Overdue (1)"));
        assert!(report.contains(&t1.title));
        assert!(report.contains("[!] Stale Inbox >3d (1)"));
        assert!(report.contains(&t2.title));
    }
}
