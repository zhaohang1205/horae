use rusqlite::Connection;

use crate::cli::Command;
use anyhow::Result;

mod alarm;
mod backup;
mod capture;
mod focus;
mod list;
mod log;
pub use horae_core::notify;
mod ntfy;
pub use horae_core::pomo;
pub mod profile;
mod review;
mod show;
pub mod stats;
mod status;
mod tagging;
mod watch;

pub fn run(cmd: Command, conn: &Connection, profile: Option<&str>) -> Result<()> {
    let result = run_inner(cmd, conn, profile);
    // CLI hook：每次命令结束后顺带检查每日心智维护摘要（每天至多一次，已发送则直接跳过）。
    let _ = notify::check(conn);
    result
}

fn run_inner(cmd: Command, conn: &Connection, profile: Option<&str>) -> Result<()> {
    match cmd {
        Command::Capture {
            title,
            clip,
            tag,
            p1,
            p2,
            p3,
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
                p1,
                p2,
                p3,
                due,
                status,
                notes,
                json,
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
        Command::Pomo { action, task_id } => match action.as_str() {
            "start" => {
                if let Some(id) = task_id {
                    pomo::start(conn, &id)
                } else {
                    anyhow::bail!("task_id required for start")
                }
            }
            "stop" => pomo::stop(),
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
        Command::Tui => horae_tui::run(conn, profile),
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
        Command::Stats => stats::run(conn),
        Command::Ntfy { action } => ntfy::run(&action, profile),
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
