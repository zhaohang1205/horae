use rusqlite::Connection;

use crate::repo::tasks;
use crate::schedule::effective_due;
use crate::time;
use anyhow::Result;

pub fn run(conn: &Connection) -> Result<()> {
    let all = tasks::list(
        conn,
        &tasks::ListFilter {
            status: None,
            tags: vec![],
            query: None,
            review_stale: false,
        },
    )?;
    let inbox = all
        .iter()
        .filter(|t| t.status == crate::model::task::Status::Inbox)
        .count();
    let next = all
        .iter()
        .filter(|t| t.status == crate::model::task::Status::Next)
        .count();
    let waiting = all
        .iter()
        .filter(|t| t.status == crate::model::task::Status::Waiting)
        .count();
    let someday = all
        .iter()
        .filter(|t| t.status == crate::model::task::Status::Someday)
        .count();
    let scheduled = all
        .iter()
        .filter(|t| t.status == crate::model::task::Status::Scheduled)
        .count();

    let now = time::now_ms();
    let horizon = 3 * 24 * 3600 * 1000i64;
    let due_soon = all
        .iter()
        .filter(|t| {
            effective_due(t)
                .map(|x| x <= now + horizon)
                .unwrap_or(false)
        })
        .count();

    println!("Weekly Review");
    println!("  inbox     : {}", inbox);
    println!("  next      : {}", next);
    println!("  waiting   : {}", waiting);
    println!("  scheduled : {} ({} due within 3d)", scheduled, due_soon);
    println!("  someday   : {}", someday);
    Ok(())
}
