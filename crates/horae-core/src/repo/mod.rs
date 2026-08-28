pub mod alarm;
pub mod backup;
pub mod modules;
pub mod notify;
pub mod pomodoro;
pub mod quotes;
pub mod settings;
pub mod state;
pub mod tags;
pub mod tasks;

use rusqlite::{Connection, Transaction};

/// Run a mutation inside a transaction that owns the audit timestamp.
///
/// Opens the transaction, computes a single `now`, and hands both to the
/// closure. The closure describes the change (any number of SQL statements and
/// `log_event` calls, all sharing `now`); on success the transaction commits,
/// on error it rolls back on drop. Callers never touch transaction lifecycle.
///
/// This is the one place a state change and its audit event are made atomic and
/// timestamp-consistent: the closure's `now` must be used for both the mutation
/// (`updated_at` etc.) and any `log_event`, so they can't drift apart.
pub fn mutate<T>(
    conn: &Connection,
    f: impl FnOnce(&Transaction, i64) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let tx = conn.unchecked_transaction()?;
    let now = crate::time::now_ms();
    let out = f(&tx, now)?;
    tx.commit()?;
    Ok(out)
}

/// Append an entry to the `task_events` audit/timeline log.
/// `at` is the UTC-ms timestamp; inside a [`mutate`] closure, pass the `now`
/// the transaction handed you so the event shares the mutation's instant
/// (time datafication).
pub fn log_event(
    conn: &Connection,
    task_id: &str,
    event_type: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    meta: Option<&str>,
    at: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO task_events (task_id, event_type, from_status, to_status, at, meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![task_id, event_type, from_status, to_status, at, meta],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutate_commits_on_success() {
        let (_dir, conn) = crate::testutil::test_conn();
        mutate(&conn, |tx, _now| {
            tx.execute(
                "INSERT INTO tags (name,category,is_system,created_at) VALUES ('x','custom',0,0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE name='x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "成功路径提交");
    }

    #[test]
    fn mutate_rolls_back_on_error() {
        let (_dir, conn) = crate::testutil::test_conn();
        let res: anyhow::Result<()> = mutate(&conn, |tx, _now| {
            tx.execute(
                "INSERT INTO tags (name,category,is_system,created_at) VALUES ('y','custom',0,0)",
                [],
            )?;
            anyhow::bail!("boom")
        });
        assert!(res.is_err(), "闭包错误向上传播");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE name='y'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "错误路径回滚，不留半写状态");
    }
}
