use rusqlite::Connection;

use crate::model::event;
use crate::model::tag::Tag;
use crate::model::task::Task;
use crate::repo::{tags, tasks};
use crate::time;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct ShowOutput {
    task: Task,
    tags: Vec<Tag>,
    events: Vec<event::TaskEvent>,
}

pub fn run(conn: &Connection, id: &str, json: bool) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    let task = tasks::get(conn, &id)?;
    let tags = tags::get_task_tags(conn, &task.id)?;
    let events = tasks::events(conn, &task.id)?;

    if json {
        let out = ShowOutput { task, tags, events };
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("Task {}", task.id);
    println!("  title     : {}", task.title);
    println!("  status    : {}", task.status);
    println!(
        "  notes     : {}",
        if task.notes.trim().is_empty() {
            "-".to_string()
        } else {
            task.notes.clone()
        }
    );
    if let Some(rr) = &task.rrule {
        println!("  rrule     : {}", rr);
    }
    println!(
        "  captured  : {}",
        time::format_local(Some(task.created_at))
    );
    println!("  clarified : {}", time::format_local(task.clarified_at));
    println!("  due       : {}", time::format_local(task.due_at));
    println!(
        "  scheduled : {} -> {}",
        time::format_local(task.scheduled_start_at),
        time::format_local(task.scheduled_end_at)
    );
    println!("  completed : {}", time::format_local(task.completed_at));
    println!(
        "  tags      : {}",
        tags.iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("Timeline (stored as UTC-ms, shown in local time):");
    for e in &events {
        let raw = e.meta.as_deref().unwrap_or("");
        // 结构化 meta（JSON 对象）提取可读字段展示；旧数据/非 JSON 原样输出。
        let meta = serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|v| v.get("reason").and_then(|r| r.as_str()).map(str::to_string))
            .unwrap_or_else(|| raw.to_string());
        println!(
            "  {}  {:<16} {} -> {}  {}",
            time::format_local(Some(e.at)),
            e.event_type,
            e.from_status.as_deref().unwrap_or("-"),
            e.to_status.as_deref().unwrap_or("-"),
            meta
        );
    }
    Ok(())
}
