use rusqlite::Connection;

use anyhow::Result;
use horae_core::repo::tasks;
use horae_core::time;

pub fn to_status(conn: &Connection, id: &str, to: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    let parsed_status: horae_core::model::task::Status =
        to.parse().map_err(|e| anyhow::anyhow!("{}", e))?;
    let t = tasks::transition(conn, &id, parsed_status)?;
    println!("{} -> {}", &t.id[..t.id.len().min(8)], t.status);
    if to == "done" {
        let _ = crate::commands::notify::completed_feedback(conn);
    }
    if to == "next" {
        let missing_time = t.due_at.is_none() && t.scheduled_start_at.is_none();
        if missing_time {
            println!("  tip: 建议补充时间 — `horae schedule <id> --start <时间>`");
        }
    }
    Ok(())
}

pub fn schedule(
    conn: &Connection,
    id: &str,
    start: Option<&str>,
    end: Option<&str>,
    rrule: Option<&str>,
) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    let start_ms = match start {
        Some(s) => time::parse_time(s)?,
        None => anyhow::bail!("schedule requires --start <when>"),
    };
    if let Some(rr) = rrule {
        crate::commands::capture::ensure_rrule_supported(rr)?;
    }
    let end_ms = match end {
        Some(e) => Some(time::parse_time(e)?),
        None => None,
    };
    let t = tasks::schedule(conn, &id, start_ms, end_ms, rrule.map(|s| s.to_string()))?;
    println!(
        "scheduled {} at {}",
        &t.id[..t.id.len().min(8)],
        time::format_local(Some(start_ms))
    );
    if let Some(rr) = &t.rrule {
        println!("  rrule: {}", rr);
    }
    Ok(())
}

pub fn archive(conn: &Connection, id: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    tasks::archive(conn, &id)?;
    println!("archived {}", &id[..id.len().min(8)]);
    Ok(())
}

pub fn restore(conn: &Connection, id: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    tasks::unarchive(conn, &id)?;
    println!("restored {}", &id[..id.len().min(8)]);
    Ok(())
}

pub fn purge(conn: &Connection, id: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    tasks::purge(conn, &id)?;
    println!("purged {}", &id[..id.len().min(8)]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Command;
    use horae_core::model::task::Status;
    use horae_core::repo::tasks::{self, CaptureInput};
    use horae_core::testutil::test_conn;

    fn mk_task(conn: &Connection, title: &str) -> horae_core::model::task::Task {
        tasks::create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                status: Status::Inbox,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn status_of(conn: &Connection, id: &str) -> Status {
        tasks::get(conn, id).unwrap().status
    }

    #[test]
    fn cli_status_transitions_persist() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "流转");

        // 经完整命令分发驱动，覆盖 mod.rs 分发 + status.rs 处理
        crate::commands::run(Command::Next { id: t.id.clone() }, &conn, None).unwrap();
        assert_eq!(status_of(&conn, &t.id), Status::Next);

        crate::commands::run(Command::Wait { id: t.id.clone() }, &conn, None).unwrap();
        assert_eq!(status_of(&conn, &t.id), Status::Waiting);

        crate::commands::run(Command::Someday { id: t.id.clone() }, &conn, None).unwrap();
        assert_eq!(status_of(&conn, &t.id), Status::Someday);

        crate::commands::run(Command::Done { id: t.id.clone() }, &conn, None).unwrap();
        assert_eq!(status_of(&conn, &t.id), Status::Done);
    }

    #[test]
    fn cli_accepts_unique_id_prefix() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "前缀解析");
        let prefix: String = t.id.chars().take(8).collect();

        crate::commands::run(Command::Done { id: prefix }, &conn, None).unwrap();
        assert_eq!(status_of(&conn, &t.id), Status::Done);
    }

    #[test]
    fn schedule_requires_start_and_persists_fields() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "排程");

        // 缺 --start 必须报错
        let err = crate::commands::run(
            Command::Schedule {
                id: t.id.clone(),
                start: None,
                end: None,
                rrule: None,
            },
            &conn,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--start"), "{}", err);

        crate::commands::run(
            Command::Schedule {
                id: t.id.clone(),
                start: Some("+1d".into()),
                end: Some("+2d".into()),
                rrule: Some("FREQ=DAILY".into()),
            },
            &conn,
            None,
        )
        .unwrap();

        let task = tasks::get(&conn, &t.id).unwrap();
        assert!(task.scheduled_start_at.is_some(), "start 应落库");
        assert!(task.scheduled_end_at.is_some(), "end 应落库");
        assert_eq!(task.rrule.as_deref(), Some("FREQ=DAILY"));
        assert_eq!(task.status, Status::Scheduled);
    }

    #[test]
    fn schedule_rejects_yearly_rrule_without_touching_task() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "年度体检");

        // FREQ=YEARLY 引擎无法展开，必须报错而非静默存入
        let err = crate::commands::run(
            Command::Schedule {
                id: t.id.clone(),
                start: Some("+1d".into()),
                end: None,
                rrule: Some("FREQ=YEARLY".into()),
            },
            &conn,
            None,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("FREQ=YEARLY"), "{msg}");
        assert!(msg.contains("FREQ=DAILY|WEEKLY|MONTHLY"), "{msg}");

        // 任务保持原样：未排程、无 rrule
        let task = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(task.status, Status::Inbox);
        assert_eq!(task.rrule, None);
        assert!(task.scheduled_start_at.is_none());
    }

    #[test]
    fn archive_restore_roundtrip_via_cli() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "归档");

        crate::commands::run(Command::Archive { id: t.id.clone() }, &conn, None).unwrap();
        assert!(tasks::get(&conn, &t.id).unwrap().archived_at.is_some());

        crate::commands::run(Command::Restore { id: t.id.clone() }, &conn, None).unwrap();
        assert!(tasks::get(&conn, &t.id).unwrap().archived_at.is_none());
    }

    #[test]
    fn purge_requires_archive_then_deletes_permanently() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "清除");

        // 未归档直接 purge 应被拒绝
        let err =
            crate::commands::run(Command::Purge { id: t.id.clone() }, &conn, None).unwrap_err();
        assert!(err.to_string().contains("not archived"), "{}", err);
        assert!(tasks::get(&conn, &t.id).is_ok());

        crate::commands::run(Command::Archive { id: t.id.clone() }, &conn, None).unwrap();
        crate::commands::run(Command::Purge { id: t.id.clone() }, &conn, None).unwrap();
        assert!(
            tasks::get(&conn, &t.id).is_err(),
            "purge 后任务应被永久删除"
        );
    }

    #[test]
    fn tag_and_untag_via_cli_dispatch() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "打标签");

        crate::commands::run(
            Command::Tag {
                id: t.id.clone(),
                name: "urgent".into(),
            },
            &conn,
            None,
        )
        .unwrap();
        assert_eq!(
            horae_core::repo::tags::get_task_tags(&conn, &t.id)
                .unwrap()
                .len(),
            1
        );

        crate::commands::run(
            Command::Untag {
                id: t.id.clone(),
                name: "urgent".into(),
            },
            &conn,
            None,
        )
        .unwrap();
        assert!(horae_core::repo::tags::get_task_tags(&conn, &t.id)
            .unwrap()
            .is_empty());
    }
}
