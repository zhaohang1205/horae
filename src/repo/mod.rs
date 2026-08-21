pub mod alarm;
pub mod backup;
pub mod notify;
pub mod pomodoro;
pub mod settings;
pub mod state;
pub mod tags;
pub mod tasks;

use rusqlite::Connection;

/// Append an entry to the `task_events` audit/timeline log.
/// `at` is the UTC-ms timestamp, passed in by the caller so the event shares
/// the exact same instant as the task mutation it describes (time datafication).
pub(crate) fn log_event(
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
