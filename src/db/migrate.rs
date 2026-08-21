use rusqlite::Connection;

/// Run schema + seed migrations. Both are idempotent (IF NOT EXISTS /
/// INSERT OR IGNORE) so this is safe to call on every startup.
pub fn run(conn: &mut Connection) -> anyhow::Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let current_version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;

    if current_version < 1 {
        let sql1 = include_str!("../../migrations/0001_init.sql");
        conn.execute_batch(sql1)?;
        let sql2 = include_str!("../../migrations/0002_seed_tags.sql");
        conn.execute_batch(sql2)?;
        conn.pragma_update(None, "user_version", 1)?;
    }

    if current_version < 2 {
        let sql3 = include_str!("../../migrations/0003_add_gtd_advanced.sql");
        conn.execute_batch(sql3)?;
        conn.pragma_update(None, "user_version", 2)?;
    }

    if current_version < 3 {
        let sql4 = include_str!("../../migrations/0004_seed_more_tags.sql");
        conn.execute_batch(sql4)?;
        conn.pragma_update(None, "user_version", 3)?;
    }

    if current_version < 4 {
        let sql5 = include_str!("../../migrations/0005_add_archive_reason.sql");
        conn.execute_batch(sql5)?;
        conn.pragma_update(None, "user_version", 4)?;
    }

    if current_version < 5 {
        let sql6 = include_str!("../../migrations/0006_settings.sql");
        conn.execute_batch(sql6)?;
        conn.pragma_update(None, "user_version", 5)?;
    }

    if current_version < 6 {
        let sql7 = include_str!("../../migrations/0007_normalize_bare_rrule.sql");
        conn.execute_batch(sql7)?;
        conn.pragma_update(None, "user_version", 6)?;
    }

    if current_version < 7 {
        let sql8 = include_str!("../../migrations/0008_idx_task_events_type_at.sql");
        conn.execute_batch(sql8)?;
        conn.pragma_update(None, "user_version", 7)?;
    }

    if current_version < 8 {
        let sql9 = include_str!("../../migrations/0009_idx_tasks_due_at.sql");
        conn.execute_batch(sql9)?;
        conn.pragma_update(None, "user_version", 8)?;
    }

    if current_version < 9 {
        let sql10 = include_str!("../../migrations/0010_seed_quote_tag.sql");
        conn.execute_batch(sql10)?;
        conn.pragma_update(None, "user_version", 9)?;
    }

    if current_version < 10 {
        let sql11 = include_str!("../../migrations/0011_drop_dead_columns.sql");
        conn.execute_batch(sql11)?;
        conn.pragma_update(None, "user_version", 10)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_v5(conn: &Connection) {
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0002_seed_tags.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0003_add_gtd_advanced.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0004_seed_more_tags.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0005_add_archive_reason.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/0006_settings.sql"))
            .unwrap();
        conn.pragma_update(None, "user_version", 5).unwrap();
    }

    fn insert_task(conn: &Connection, id: &str, rrule: Option<&str>) {
        conn.execute(
            "INSERT INTO tasks (id, title, status, rrule, created_at, updated_at) \
             VALUES (?1, ?2, 'scheduled', ?3, 1, 1)",
            rusqlite::params![id, id, rrule],
        )
        .unwrap();
    }

    fn rrule_of(conn: &Connection, id: &str) -> Option<String> {
        conn.query_row("SELECT rrule FROM tasks WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migration_0007_normalizes_bare_rrule() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply_v5(&conn);

        insert_task(&conn, "t-d", Some("d"));
        insert_task(&conn, "t-w", Some("w"));
        insert_task(&conn, "t-full", Some("FREQ=DAILY;INTERVAL=3"));
        insert_task(&conn, "t-none", None);

        let mut conn = conn;
        run(&mut conn).unwrap();

        assert_eq!(
            rrule_of(&conn, "t-d").as_deref(),
            Some("FREQ=DAILY"),
            "裸 'd' 规范化为 FREQ=DAILY"
        );
        assert_eq!(rrule_of(&conn, "t-w").as_deref(), Some("FREQ=WEEKLY"));
        assert_eq!(
            rrule_of(&conn, "t-full").as_deref(),
            Some("FREQ=DAILY;INTERVAL=3"),
            "合法 rrule 保持不变"
        );
        assert_eq!(rrule_of(&conn, "t-none"), None, "NULL rrule 保持不变");
        let v: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 10, "迁移版本推进到 10");

        // 幂等：再次运行不改变任何值
        run(&mut conn).unwrap();
        assert_eq!(rrule_of(&conn, "t-d").as_deref(), Some("FREQ=DAILY"));
    }

    #[test]
    fn migration_0010_seeds_quote_tag() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let (category, is_system): (String, i64) = conn
            .query_row(
                "SELECT category, is_system FROM tags WHERE name = 'quote'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(category, "context");
        assert_eq!(is_system, 1, "quote 是系统标签，不可删除");

        // 幂等：再次运行不重复插入
        run(&mut conn).unwrap();
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE name = 'quote'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(c, 1);
    }
}
