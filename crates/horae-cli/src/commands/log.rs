use anyhow::Result;
use horae_core::model::event::EV_LOGGED;
use horae_core::time;
use rusqlite::Connection;

pub fn run(conn: &Connection, message_parts: &[String]) -> Result<()> {
    if message_parts.is_empty() {
        // List recent logs
        let mut stmt = conn.prepare(
            "SELECT meta, at FROM task_events 
             WHERE task_id = '__journal__' AND event_type = ?1 
             ORDER BY at DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([EV_LOGGED], |r| {
            let meta: Option<String> = r.get(0)?;
            let at: i64 = r.get(1)?;
            Ok((meta.unwrap_or_default(), at))
        })?;

        let mut logs = Vec::new();
        for r in rows {
            logs.push(r?);
        }

        if logs.is_empty() {
            println!("📭 暂无日志记录。使用 `horae log \"你的内容\"` 记录。");
        } else {
            // Reverse to show oldest first at the top, or just print descending?
            // Usually logs are printed oldest to newest if it's a stream, or newest first if it's a list.
            // Let's print newest first.
            for (msg, at) in logs {
                let time_str = time::format_local(Some(at));
                println!("[{}] {}", time_str, msg);
            }
        }
    } else {
        // Add new log
        let message = message_parts.join(" ");

        horae_core::repo::mutate(conn, |tx, now| {
            horae_core::repo::log_event(
                tx,
                horae_core::repo::tasks::SYSTEM_JOURNAL_ID,
                EV_LOGGED,
                None,
                None,
                Some(&message),
                now,
            )
        })?;

        println!("✅ 日志已记录");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horae_core::testutil::test_conn;

    #[test]
    fn test_log_command() {
        let (_dir, conn) = test_conn();
        // test_conn runs migrations, which should include 0012 inserting __journal__

        // Initially empty
        assert!(run(&conn, &[]).is_ok());

        // Log something
        assert!(run(&conn, &["Drank".into(), "water".into()]).is_ok());

        // Check it was inserted
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM task_events WHERE task_id = '__journal__' AND event_type = ?1",
            [EV_LOGGED],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        // List again
        assert!(run(&conn, &[]).is_ok());
    }
}
