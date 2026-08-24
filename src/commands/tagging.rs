use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::model::tag::Tag;
use crate::repo::{tags, tasks};
use anyhow::Result;

pub fn add(conn: &Connection, id: &str, name: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    let _ = tasks::get(conn, &id)?;
    tags::add_tag_to_task(conn, &id, name)?;
    println!("tagged {} with {}", &id[..id.len().min(8)], name);
    Ok(())
}

pub fn remove(conn: &Connection, id: &str, name: &str) -> Result<()> {
    let id = tasks::resolve_id(conn, id)?;
    let _ = tasks::get(conn, &id)?;
    tags::remove_tag_from_task(conn, &id, name)?;
    println!("untagged {} from {}", &id[..id.len().min(8)], name);
    Ok(())
}

pub fn list(conn: &Connection) -> Result<()> {
    let all = tags::list_tags(conn)?;
    let mut by_cat: BTreeMap<String, Vec<Tag>> = BTreeMap::new();
    for t in all {
        by_cat.entry(t.category.clone()).or_default().push(t);
    }
    for (cat, items) in by_cat {
        println!("[{}]", cat);
        for t in items {
            let kind = if t.is_system { "sys" } else { "usr" };
            println!("  {}  ({})", t.name, kind);
        }
    }
    Ok(())
}
