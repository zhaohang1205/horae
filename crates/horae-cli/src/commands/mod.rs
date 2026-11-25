use rusqlite::Connection;
use unicode_width::UnicodeWidthStr;

use crate::cli::Command;
use anyhow::Result;

/// Lightweight CJK-aware aligned table printer (replaces `comfy-table`).
pub(crate) fn print_table(headers: Option<&[&str]>, rows: &[Vec<String>]) {
    let cols = headers
        .map(|h| h.len())
        .or_else(|| rows.first().map(|r| r.len()))
        .unwrap_or(0);
    if cols == 0 {
        return;
    }
    let mut widths = vec![0usize; cols];
    if let Some(h) = headers {
        for (i, cell) in h.iter().enumerate() {
            widths[i] = widths[i].max(UnicodeWidthStr::width(*cell));
        }
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let print_row = |cells: &[&str]| {
        let mut line = String::new();
        // Keep a single leading space to match comfy-table NOTHING preset layout
        line.push(' ');
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            if i + 1 < cols {
                let w = UnicodeWidthStr::width(*cell);
                for _ in 0..widths[i].saturating_sub(w) {
                    line.push(' ');
                }
            }
        }
        println!("{}", line.trim_end());
    };

    if let Some(h) = headers {
        print_row(h);
    }
    let mut refs: Vec<&str> = Vec::with_capacity(cols);
    for row in rows {
        refs.clear();
        refs.extend(row.iter().map(String::as_str));
        print_row(&refs);
    }
}

mod alarm;
mod backup;
mod calendar;
mod capture;
mod feishu;
mod focus;
mod list;
mod log;
mod modify;
pub use horae_core::notify;
mod ntfy;
pub mod password;
pub use horae_core::pomo;
pub mod profile;
mod review;
mod show;
pub mod sound;
pub mod stats;
mod status;
mod tagging;
mod todoist;
mod watch;

#[allow(dead_code)]
pub fn run(cmd: Command, conn: &Connection, profile: Option<&str>) -> Result<()> {
    run_with_mode(cmd, conn, profile, horae_tui::LaunchMode::Default)
}

pub fn run_with_mode(
    cmd: Command,
    conn: &Connection,
    profile: Option<&str>,
    launch_mode: horae_tui::LaunchMode,
) -> Result<()> {
    let result = run_inner(cmd, conn, profile, launch_mode);
    // CLI hook：每次命令结束后顺带检查每日心智维护摘要（每天至多一次，已发送则直接跳过）。
    let _ = notify::check(conn);
    result
}

fn run_inner(
    cmd: Command,
    conn: &Connection,
    profile: Option<&str>,
    launch_mode: horae_tui::LaunchMode,
) -> Result<()> {
    match cmd {
        Command::Capture {
            title,
            natural,
            clip,
            tag,
            high,
            medium,
            low,
            due,
            status,
            notes,
            json,
        } => capture::run(
            conn,
            capture::CaptureArgs {
                title: title.join(" "),
                clip,
                tags: tag,
                high,
                medium,
                low,
                due,
                status,
                notes,
                json,
                natural,
            },
        ),
        Command::Voice { text, json } => capture::run(
            conn,
            capture::CaptureArgs {
                title: text.join(" "),
                natural: true,
                json,
                ..Default::default()
            },
        ),
        Command::List {
            status,
            tag,
            due_before,
            date,
            json,
        } => list::run(
            conn,
            status.as_deref(),
            &tag,
            due_before.as_deref(),
            date.as_deref(),
            json,
        ),
        Command::Show { id, json } => show::run(conn, &id, json),
        Command::Modify {
            id,
            text,
            title,
            tag,
            untag,
            clear_tags,
            high,
            medium,
            low,
            clear_priority,
            due,
            clear_due,
            start,
            end,
            rrule,
            clear_schedule,
            status,
            notes,
            edit_notes,
            json,
        } => modify::run(
            conn,
            modify::ModifyArgs {
                id,
                text,
                title,
                tags: tag,
                untags: untag,
                clear_tags,
                high,
                medium,
                low,
                clear_priority,
                due,
                clear_due,
                start,
                end,
                rrule,
                clear_schedule,
                status,
                notes,
                edit_notes,
                json,
            },
        ),
        Command::Next { id } => status::to_status(conn, &id, "next"),
        Command::Wait { id } => status::to_status(conn, &id, "waiting"),
        Command::Someday { id } => status::to_status(conn, &id, "someday"),
        Command::Done { id } => status::to_status(conn, &id, "done"),
        Command::Schedule {
            id,
            start,
            end,
            rrule,
        } => status::schedule(
            conn,
            &id,
            start.as_deref(),
            end.as_deref(),
            rrule.as_deref(),
        ),
        Command::Archive { id } => status::archive(conn, &id),
        Command::Restore { id } => status::restore(conn, &id),
        Command::Purge { id } => status::purge(conn, &id),
        Command::Tag { id, name } => tagging::add(conn, &id, &name),
        Command::Untag { id, name } => tagging::remove(conn, &id, &name),
        Command::Review => review::run(conn),
        Command::Tags => tagging::list(conn),
        Command::Calendar { date, short, json } => calendar::run(date.as_deref(), short, json),
        Command::Pomo { action, task_id } => match action.as_str() {
            "start" => {
                if let Some(id) = task_id {
                    pomo::start(conn, &id)
                } else {
                    anyhow::bail!("task_id required for start")
                }
            }
            "stop" => pomo::stop(Some(conn), false),
            "daemon" => pomo::daemon(),
            "waybar" => pomo::waybar(),
            _ => anyhow::bail!("unknown pomo action"),
        },
        Command::Alarm {
            action,
            slot,
            limit,
            all,
        } => match action.as_str() {
            "waybar" => alarm::waybar(slot, limit, all),
            "next" => alarm::next(slot, limit),
            _ => anyhow::bail!("unknown alarm action"),
        },
        Command::Tui { .. } => horae_tui::run_with_mode(conn, profile, launch_mode),
        Command::Flash => horae_tui::run_with_mode(conn, profile, horae_tui::LaunchMode::Flash),
        Command::Watch {
            dir,
            interval,
            once,
        } => watch::run(
            conn,
            watch::WatchArgs {
                dir: dir.unwrap_or_else(watch::default_sync_dir),
                interval_secs: interval.unwrap_or(watch::DEFAULT_INTERVAL_SECS),
                once,
                profile: profile.map(|s| s.to_string()),
            },
        ),
        Command::Export { file } => backup::run_export(conn, file.as_deref()),
        Command::Import { file, replace } => backup::run_import(conn, &file, replace),
        Command::Sound { action, name } => sound::run(conn, action.as_deref(), name.as_deref()),
        Command::Password { action } => password::run(conn, action),
        Command::Stats => stats::run(conn),
        Command::Ntfy { action } => ntfy::run(&action, profile),
        Command::Feishu { action } => feishu::run(conn, action, profile),
        Command::Todoist { action } => todoist::run(conn, action, profile),
        Command::Focus { start } => focus::run(conn, start),
        Command::Log { message } => log::run(conn, &message),
        Command::Completions { .. } => {
            anyhow::bail!("`horae completions` is handled before the database is opened")
        }
        Command::Profile { .. } => {
            anyhow::bail!("`horae profile` is handled before the database is opened")
        }
    }
}
