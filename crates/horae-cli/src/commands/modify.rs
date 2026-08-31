use rusqlite::Connection;

use anyhow::Result;
use horae_core::repo::tasks;
use horae_core::time;

/// CLI-derived arguments for `modify`.
pub struct ModifyArgs {
    pub id: String,
    pub text: Vec<String>,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub untags: Vec<String>,
    pub clear_tags: bool,
    pub p1: bool,
    pub p2: bool,
    pub p3: bool,
    pub clear_priority: bool,
    pub due: Option<String>,
    pub clear_due: bool,
    pub start: Option<String>,
    pub end: Option<String>,
    pub rrule: Option<String>,
    pub clear_schedule: bool,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub edit_notes: bool,
    pub json: bool,
}

fn edit_notes_interactive(initial: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    let temp_path = std::env::temp_dir().join(format!(
        "horae_notes_{}_{}.md",
        std::process::id(),
        horae_core::time::now_ms()
    ));
    std::fs::write(&temp_path, initial)?;
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"{}\"", editor, temp_path.display()))
        .status();
    let new_content = std::fs::read_to_string(&temp_path).unwrap_or_default();
    let _ = std::fs::remove_file(&temp_path);
    if let Err(e) = status {
        anyhow::bail!("failed to launch editor: {}", e);
    }
    Ok(new_content)
}

pub fn run(conn: &Connection, args: ModifyArgs) -> Result<()> {
    let id = tasks::resolve_id(conn, &args.id)?;
    let current = tasks::get(conn, &id)?;

    let text_joined = args.text.join(" ");
    let quick_add = if !text_joined.trim().is_empty() {
        Some(horae_core::parser::parse_quick_add(&text_joined))
    } else {
        None
    };

    let mut input = tasks::ModifyInput::default();

    // 1. Title
    if let Some(t) = args.title {
        input.title = Some(t);
    } else if let Some(ref qa) = quick_add {
        if !qa.title.is_empty() {
            input.title = Some(qa.title.clone());
        }
    }

    // 2. Notes
    if args.edit_notes {
        let base = args.notes.as_deref().unwrap_or(&current.notes);
        let edited = edit_notes_interactive(base)?;
        input.notes = Some(edited);
    } else if let Some(n) = args.notes {
        input.notes = Some(n);
    }

    // 3. Due Date
    if args.clear_due {
        input.due_at = Some(None);
    } else if let Some(ref d) = args.due {
        if d.eq_ignore_ascii_case("none") || d.eq_ignore_ascii_case("clear") {
            input.due_at = Some(None);
        } else {
            let ms = time::parse_time(d)?;
            input.due_at = Some(Some(ms));
        }
    }

    // 4. Schedule & Recurrence
    if args.clear_schedule {
        input.scheduled_start_at = Some(None);
        input.scheduled_end_at = Some(None);
        input.rrule = Some(None);
    } else {
        if let Some(ref s) = args.start {
            if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("clear") {
                input.scheduled_start_at = Some(None);
            } else {
                let ms = time::parse_time(s)?;
                input.scheduled_start_at = Some(Some(ms));
            }
        } else if let Some(ref qa) = quick_add {
            if let Some(ref ts) = qa.time_str {
                let ms = time::parse_time(ts)?;
                input.scheduled_start_at = Some(Some(ms));
            }
        }

        if let Some(ref e) = args.end {
            if e.eq_ignore_ascii_case("none") || e.eq_ignore_ascii_case("clear") {
                input.scheduled_end_at = Some(None);
            } else {
                let ms = time::parse_time(e)?;
                input.scheduled_end_at = Some(Some(ms));
            }
        }

        if let Some(ref r) = args.rrule {
            if r.eq_ignore_ascii_case("none") || r.eq_ignore_ascii_case("clear") {
                input.rrule = Some(None);
            } else {
                let normalized = horae_core::parser::parse_rrule_shorthand(r);
                crate::commands::capture::ensure_rrule_supported(&normalized)?;
                input.rrule = Some(Some(normalized));
            }
        } else if let Some(ref qa) = quick_add {
            if let Some(ref r) = qa.rrule {
                crate::commands::capture::ensure_rrule_supported(r)?;
                input.rrule = Some(Some(r.clone()));
            }
        }
    }

    // 5. Status
    if let Some(ref s) = args.status {
        let st: horae_core::model::task::Status =
            s.parse().map_err(|e| anyhow::anyhow!("{}", e))?;
        input.status = Some(st);
    } else if quick_add.as_ref().is_some_and(|qa| qa.time_str.is_some())
        && current.status == horae_core::model::task::Status::Inbox
        && input.scheduled_start_at.is_some()
    {
        input.status = Some(horae_core::model::task::Status::Scheduled);
    }

    // 6. Tags & Priorities
    input.clear_tags = args.clear_tags;
    for t in args.tags {
        if !input.add_tags.contains(&t) {
            input.add_tags.push(t);
        }
    }
    if let Some(ref qa) = quick_add {
        for t in &qa.tags {
            if !input.add_tags.contains(t) {
                input.add_tags.push(t.clone());
            }
        }
    }
    for u in args.untags {
        if !input.remove_tags.contains(&u) {
            input.remove_tags.push(u);
        }
    }

    if args.clear_priority {
        input.remove_tags.push("p1".to_string());
        input.remove_tags.push("p2".to_string());
        input.remove_tags.push("p3".to_string());
    } else if args.p1 || args.p2 || args.p3 {
        input.remove_tags.push("p1".to_string());
        input.remove_tags.push("p2".to_string());
        input.remove_tags.push("p3".to_string());
        let p = if args.p1 {
            "p1"
        } else if args.p2 {
            "p2"
        } else {
            "p3"
        };
        input.add_tags.push(p.to_string());
    } else if let Some(ref qa) = quick_add {
        if let Some(ref p) = qa.priority {
            input.remove_tags.push("p1".to_string());
            input.remove_tags.push("p2".to_string());
            input.remove_tags.push("p3".to_string());
            input.add_tags.push(p.clone());
        }
    }

    let updated = tasks::modify(conn, &id, &input)?;

    if args.json {
        let tags = horae_core::repo::tags::get_task_tags(conn, &id)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "task": updated,
                "tags": tags,
            }))?
        );
    } else {
        let short_id = &updated.id[..updated.id.len().min(8)];
        println!(
            "modified [{}] {} (status: {})",
            short_id, updated.title, updated.status
        );
        let tags = horae_core::repo::tags::get_task_tags(conn, &id).unwrap_or_default();
        if !tags.is_empty() {
            let tags_str = tags
                .iter()
                .map(|t| format!("@{}", t.name))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  tags: {}", tags_str);
        }
        if let Some(due) = updated.due_at {
            println!("  due: {}", time::format_local(Some(due)));
        }
        if let Some(start) = updated.scheduled_start_at {
            println!("  scheduled: {}", time::format_local(Some(start)));
        }
        if let Some(ref rr) = updated.rrule {
            println!("  rrule: {}", rr);
        }
        if !updated.notes.trim().is_empty() {
            let first_line = updated.notes.lines().next().unwrap_or("").trim();
            if updated.notes.lines().count() > 1 {
                println!("  notes: {}…", first_line);
            } else {
                println!("  notes: {}", first_line);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horae_core::model::task::Status;
    use horae_core::repo::tasks::CaptureInput;
    use horae_core::testutil::test_conn;

    fn mk_task(conn: &Connection, title: &str) -> horae_core::model::task::Task {
        tasks::create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                status: Status::Inbox,
                tag_names: vec!["initial_tag".into()],
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn modify_via_quick_add_tokens() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "original");

        run(
            &conn,
            ModifyArgs {
                id: t.id.clone(),
                text: vec![
                    "renamed".into(),
                    "task".into(),
                    "@work".into(),
                    "~tomorrow 10:00".into(),
                    "!b".into(),
                ],
                title: None,
                tags: vec![],
                untags: vec![],
                clear_tags: false,
                p1: false,
                p2: false,
                p3: false,
                clear_priority: false,
                due: None,
                clear_due: false,
                start: None,
                end: None,
                rrule: None,
                clear_schedule: false,
                status: None,
                notes: None,
                edit_notes: false,
                json: false,
            },
        )
        .unwrap();

        let updated = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(updated.title, "renamed task");
        assert_eq!(updated.status, Status::Scheduled);
        assert!(updated.scheduled_start_at.is_some());

        let tags: Vec<String> = horae_core::repo::tags::get_task_tags(&conn, &t.id)
            .unwrap()
            .into_iter()
            .map(|tg| tg.name)
            .collect();
        assert!(tags.contains(&"work".to_string()));
        assert!(tags.contains(&"p2".to_string()));
        assert!(tags.contains(&"initial_tag".to_string()));
    }

    #[test]
    fn modify_via_explicit_flags_overrides_and_clears() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "original");

        // Set due and notes
        run(
            &conn,
            ModifyArgs {
                id: t.id.clone(),
                text: vec![],
                title: Some("explicit title".into()),
                tags: vec!["extra".into()],
                untags: vec!["initial_tag".into()],
                clear_tags: false,
                p1: true,
                p2: false,
                p3: false,
                clear_priority: false,
                due: Some("tomorrow 15:00".into()),
                clear_due: false,
                start: None,
                end: None,
                rrule: None,
                clear_schedule: false,
                status: Some("next".into()),
                notes: Some("explicit notes".into()),
                edit_notes: false,
                json: false,
            },
        )
        .unwrap();

        let updated = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(updated.title, "explicit title");
        assert_eq!(updated.notes, "explicit notes");
        assert_eq!(updated.status, Status::Next);
        assert!(updated.due_at.is_some());

        let tags: Vec<String> = horae_core::repo::tags::get_task_tags(&conn, &t.id)
            .unwrap()
            .into_iter()
            .map(|tg| tg.name)
            .collect();
        assert!(tags.contains(&"extra".to_string()));
        assert!(tags.contains(&"p1".to_string()));
        assert!(!tags.contains(&"initial_tag".to_string()));

        // Clear due and priority
        run(
            &conn,
            ModifyArgs {
                id: t.id.clone(),
                text: vec![],
                title: None,
                tags: vec![],
                untags: vec![],
                clear_tags: false,
                p1: false,
                p2: false,
                p3: false,
                clear_priority: true,
                due: None,
                clear_due: true,
                start: None,
                end: None,
                rrule: None,
                clear_schedule: false,
                status: None,
                notes: None,
                edit_notes: false,
                json: false,
            },
        )
        .unwrap();

        let updated2 = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(updated2.due_at, None);
        let tags2: Vec<String> = horae_core::repo::tags::get_task_tags(&conn, &t.id)
            .unwrap()
            .into_iter()
            .map(|tg| tg.name)
            .collect();
        assert!(!tags2.contains(&"p1".to_string()));
        assert!(tags2.contains(&"extra".to_string()));
    }
}
