use rusqlite::Connection;

use crate::model::task;
use crate::repo::tasks;
use crate::time;
use anyhow::Result;

/// CLI-derived arguments for `capture`. Keeps `run`'s signature small and
/// separates command parsing from the repo-layer `tasks::CaptureInput`.
pub struct CaptureArgs {
    pub title: String,
    pub tags: Vec<String>,
    pub p1: bool,
    pub p2: bool,
    pub p3: bool,
    pub due: Option<String>,
    pub status: Option<String>,
    pub json: bool,
}

pub fn run(conn: &Connection, args: CaptureArgs) -> Result<()> {
    let quick_add = crate::parser::parse_quick_add(&args.title);

    let mut tag_names: Vec<String> = args.tags.clone();
    tag_names.extend(quick_add.tags);
    if let Some(p) = &quick_add.priority {
        tag_names.push(p.clone());
    }
    if args.p1 {
        tag_names.push("p1".into());
    }
    if args.p2 {
        tag_names.push("p2".into());
    }
    if args.p3 {
        tag_names.push("p3".into());
    }

    // `--due` 仍是软截止（due_at）；一句话里的 `~time` 是排程起点（scheduled_start_at）。
    let due_at = match args.due {
        Some(d) => Some(time::parse_time(&d)?),
        None => None,
    };
    let scheduled_start = match &quick_add.time_str {
        Some(t) => Some(time::parse_time(t)?),
        None => None,
    };
    let status_str = args.status.as_deref().unwrap_or("inbox");
    let parsed_status: task::Status = status_str.parse().map_err(|e| anyhow::anyhow!("{}", e))?;

    // ~time 存在 → 排程起点（创建后 schedule 设 scheduled_start_at, 状态 Scheduled, 无终点）。
    let input = tasks::CaptureInput {
        title: quick_add.title,
        status: if parsed_status == task::Status::Inbox && scheduled_start.is_some() {
            task::Status::Scheduled
        } else {
            parsed_status
        },
        due_at,
        tag_names,
        rrule: if scheduled_start.is_some() {
            None
        } else {
            quick_add.rrule.clone()
        },
        ..Default::default()
    };
    let t = tasks::create_capture(conn, &input)?;
    if let Some(start) = scheduled_start {
        let _ = tasks::schedule(conn, &t.id, start, None, quick_add.rrule)?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&t)?);
    } else {
        println!(
            "captured [{}] {}  (status: {})",
            &t.id[..t.id.len().min(8)],
            t.title,
            t.status
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::test_conn;

    #[test]
    fn capture_keeps_quick_add_rrule() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "晨跑 *d".into(),
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap();
        let task = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(task.len(), 1);
        assert_eq!(task[0].title, "晨跑");
        assert_eq!(task[0].rrule.as_deref(), Some("FREQ=DAILY"));
        assert_eq!(task[0].status, task::Status::Inbox, "无时间则留在收件箱");
    }

    #[test]
    fn capture_quick_add_time_schedules() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "买牛奶 ~tomorrow 09:00".into(),
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap();
        let task = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(task.len(), 1);
        let t = &task[0];
        assert_eq!(t.status, task::Status::Scheduled, "~time → 排程起点");
        assert_eq!(t.due_at, None, "~time 不再设软截止");
        let expect = time::parse_time("tomorrow 09:00").unwrap();
        assert_eq!(
            time::format_local(t.scheduled_start_at),
            time::format_local(Some(expect)),
            "scheduled_start_at = 明天 09:00"
        );
        assert_eq!(t.scheduled_end_at, None, "只设起点, 不设终点");
    }
}
