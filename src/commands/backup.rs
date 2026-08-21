use rusqlite::Connection;

use crate::model::backup::BackupData;
use crate::repo;
use anyhow::Result;
use std::path::Path;

/// `gtp export [--file PATH]` — write a full backup JSON (default
/// `gtp-backup-<YYYY-MM-DD>.json` in the current directory).
pub fn run_export(conn: &Connection, file: Option<&str>) -> Result<()> {
    let data = repo::backup::export_all(conn)?;
    let json = repo::backup::to_json(&data)?;
    let path = match file {
        Some(p) => p.to_string(),
        None => {
            let date = crate::time::format_local(Some(crate::time::now_ms()))
                .replace(' ', "T")
                .replace(':', "");
            format!("gtp-backup-{}.json", &date[..10])
        }
    };
    std::fs::write(&path, json)?;
    println!(
        "exported {} tasks / {} events / {} tags -> {}",
        data.tasks.len(),
        data.events.len(),
        data.tags.len(),
        path
    );
    Ok(())
}

/// `gtp import <FILE> [--replace]` — merge (or, with `--replace`, fully
/// restore) a backup into the current database.
pub fn run_import(conn: &Connection, file: &str, replace: bool) -> Result<()> {
    if !Path::new(file).exists() {
        anyhow::bail!("backup file not found: {}", file);
    }
    let content = std::fs::read_to_string(file)?;
    let data: BackupData = repo::backup::from_json(&content)?;

    if replace {
        // 双确认：--replace 会清空当前任务数据，输出里明确提示。
        println!(
            "replacing current database with backup ({:?}) ...",
            Path::new(file)
        );
    }

    let stats = repo::backup::import_all(conn, &data, replace)?;
    println!(
        "imported: {} tasks created, {} skipped, {} events, {} tags created, \
         {} tag links, {} settings{}",
        stats.tasks_created,
        stats.tasks_skipped,
        stats.events_imported,
        stats.tags_created,
        stats.task_links,
        stats.settings_imported,
        if stats.pomo_restored {
            ", pomodoro state restored"
        } else {
            ""
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::state::set_test_override;
    use crate::repo::{tags, tasks};
    use crate::testutil::test_conn;

    fn seed(conn: &Connection) -> (String, String) {
        let t = tasks::create_capture(
            conn,
            &tasks::CaptureInput {
                title: "买牛奶".into(),
                status: crate::model::task::Status::Next,
                tag_names: vec!["home".into(), "errands".into()],
                ..Default::default()
            },
        )
        .unwrap();
        tags::add_tag_to_task(conn, &t.id, "custom").unwrap();
        let done = tasks::create_capture(
            conn,
            &tasks::CaptureInput {
                title: "写周报".into(),
                status: crate::model::task::Status::Next,
                ..Default::default()
            },
        )
        .unwrap();
        tasks::transition(conn, &done.id, crate::model::task::Status::Done).unwrap();
        tasks::archive(conn, &done.id).unwrap();
        (t.id, done.id)
    }

    fn export_roundtrip(conn: &Connection) -> BackupData {
        let data = repo::backup::export_all(conn).unwrap();
        repo::backup::from_json(&repo::backup::to_json(&data).unwrap()).unwrap()
    }

    #[test]
    fn roundtrip_preserves_everything() {
        let (_dir, conn) = test_conn();
        set_test_override();
        let (live, archived) = seed(&conn);
        let data = export_roundtrip(&conn);

        // 导入到全新的空库，模拟换机还原
        let (_dir2, conn2) = test_conn();
        set_test_override();
        let stats = repo::backup::import_all(&conn2, &data, false).unwrap();
        assert_eq!(stats.tasks_skipped, 0, "空库合并不应跳过任何任务");
        assert_eq!(stats.events_imported, data.events.len());

        // 逐表校验
        let live_t = tasks::get(&conn2, &live).unwrap();
        assert_eq!(live_t.title, "买牛奶");
        let tags = tags::get_task_tags(&conn2, &live).unwrap();
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"home") && names.contains(&"custom"));
        let archived_t = tasks::get(&conn2, &archived).unwrap();
        assert_eq!(archived_t.status, crate::model::task::Status::Done);
        assert_eq!(archived_t.archive_reason.as_deref(), Some("completed"));

        // 事件时间线还原
        let events = tasks::events(&conn2, &archived).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.event_type == crate::model::event::EV_COMPLETED),
            "completed 事件被还原"
        );
        assert!(events
            .iter()
            .any(|e| e.event_type == crate::model::event::EV_ARCHIVED));
    }

    #[test]
    fn merge_skips_existing_and_adds_new() {
        let (_dir, conn) = test_conn();
        set_test_override();
        seed(&conn);
        let data = export_roundtrip(&conn);

        // 再导入一份额外任务，验证合并只加不重复
        let extra = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "额外任务".into(),
                status: crate::model::task::Status::Inbox,
                ..Default::default()
            },
        )
        .unwrap();

        // 在已含所有任务的库里再合并原备份 → 全部跳过，不产生重复
        let stats = repo::backup::import_all(&conn, &data, false).unwrap();
        assert_eq!(stats.tasks_created, 0);
        assert_eq!(stats.tasks_skipped, data.tasks.len());
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
        let titles: Vec<&str> = all.iter().map(|t| t.title.as_str()).collect();
        assert!(titles.contains(&"买牛奶") && titles.contains(&extra.title.as_str()));
        assert!(!titles.contains(&"写周报"), "归档任务不出现，未被重复导入");
    }

    #[test]
    fn replace_restores_exactly() {
        let (_dir, conn) = test_conn();
        set_test_override();
        let (live, _archived) = seed(&conn);
        let data = export_roundtrip(&conn);

        // 弄乱当前库：把 live 任务归档再彻底删除
        tasks::archive(&conn, &live).unwrap();
        tasks::purge(&conn, &live).unwrap();
        assert!(tasks::get(&conn, &live).is_err(), "当前库已删除该任务");

        let stats = repo::backup::import_all(&conn, &data, true).unwrap();
        assert_eq!(stats.tasks_skipped, 0, "replace 模式下无任务可跳过");
        assert_eq!(stats.tasks_created, data.tasks.len());
        assert!(tasks::get(&conn, &live).is_ok(), "任务被还原");
        assert_eq!(
            tasks::get(&conn, &live).unwrap().status,
            crate::model::task::Status::Next,
            "还原到备份时的状态"
        );
        let tags = tags::get_task_tags(&conn, &live).unwrap();
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"home") && names.contains(&"custom"));
    }

    #[test]
    fn rejects_unknown_version() {
        let (_dir, conn) = test_conn();
        let mut data = export_roundtrip(&conn);
        data.version = 999;
        let err = repo::backup::import_all(&conn, &data, false).unwrap_err();
        assert!(err.to_string().contains("version"), "{}", err);

        let mut data2 = export_roundtrip(&conn);
        data2.format = "nope".into();
        let err2 = repo::backup::import_all(&conn, &data2, false).unwrap_err();
        assert!(err2.to_string().contains("format"), "{}", err2);
    }

    #[test]
    fn import_missing_file_errors() {
        let (_dir, conn) = test_conn();
        let err = run_import(&conn, "/nonexistent/gtp.json", false).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }
}
