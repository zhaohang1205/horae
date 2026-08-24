use rusqlite::Connection;

use crate::schedule::effective_due;

use crate::repo::{tags, tasks};
use crate::time;
use anyhow::Result;

pub fn run(
    conn: &Connection,
    status: Option<&str>,
    tags_filter: &[String],
    due_before: Option<&str>,
    json: bool,
) -> Result<()> {
    let f = tasks::ListFilter {
        status: status
            .map(|s| s.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("{}", e))?,
        tags: tags_filter.to_vec(),
        query: None,
        review_stale: false,
    };
    let mut tasks_vec = tasks::list(conn, &f)?;

    if let Some(db) = due_before {
        let before = time::parse_time(db)?;
        tasks_vec.retain(|t| effective_due(t).map(|d| d <= before).unwrap_or(false));
        tasks_vec.sort_by_key(|t| effective_due(t).unwrap_or(i64::MAX));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&tasks_vec)?);
        return Ok(());
    }

    if tasks_vec.is_empty() {
        println!("(no tasks)");
        return Ok(());
    }
    // 一次批量取所有行的标签，避免逐行 `get_task_tags`（N+1）。
    let ids: Vec<&str> = tasks_vec.iter().map(|t| t.id.as_str()).collect();
    let tag_map = tags::get_tags_for_tasks(conn, &ids)?;
    println!(
        "{:<8} {:<9} {:<17} {:<22} TITLE",
        "ID", "STATUS", "DUE", "TAGS"
    );
    for t in &tasks_vec {
        let tags_s = tag_map.get(&t.id).map(|v| v.join(",")).unwrap_or_default();
        println!(
            "{:<8} {:<9} {:<17} {:<22} {}",
            &t.id[..t.id.len().min(8)],
            t.status,
            time::format_local(effective_due(t)),
            tags_s,
            t.title
        );
    }
    Ok(())
}
