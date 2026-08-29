use rusqlite::Connection;

use horae_core::schedule::effective_due;

use anyhow::Result;
use horae_core::repo::{tags, tasks};
use horae_core::time;

pub fn run(
    conn: &Connection,
    status: Option<&str>,
    tags_filter: &[String],
    due_before: Option<&str>,
    date: Option<&str>,
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

    if let Some(ds) = date {
        let (start, end) = time::parse_date_search(ds)?;
        tasks_vec.retain(|t| {
            effective_due(t)
                .map(|due| due >= start && due <= end)
                .unwrap_or(false)
        });
        tasks_vec.sort_by_key(|t| effective_due(t).unwrap_or(i64::MAX));
    }

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

    let mut table = comfy_table::Table::new();
    table
        .load_preset(comfy_table::presets::NOTHING)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
        .set_header(vec!["ID", "STATUS", "DUE", "TAGS", "TITLE"]);

    for t in &tasks_vec {
        let tags_s = tag_map.get(&t.id).map(|v| v.join(",")).unwrap_or_default();
        let short_id = &t.id[..t.id.len().min(8)];
        let due_s = time::format_local(effective_due(t));
        table.add_row(vec![
            short_id,
            &t.status.to_string(),
            &due_s,
            &tags_s,
            &t.title,
        ]);
    }

    println!("{table}");
    Ok(())
}
