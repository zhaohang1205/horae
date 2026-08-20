use rusqlite::Connection;

use crate::cli::Command;
use crate::model::task::Task;
use anyhow::Result;

mod alarm;
mod backup;
mod capture;
mod list;
pub mod notify;
pub mod pomo;
mod review;
mod show;
mod status;
mod tagging;
mod watch;

pub fn run(cmd: Command, conn: &Connection) -> Result<()> {
    let result = run_inner(cmd, conn);
    // CLI hook：每次命令结束后顺带检查每日心智维护摘要（每天至多一次，已发送则直接跳过）。
    let _ = notify::check(conn);
    result
}

fn run_inner(cmd: Command, conn: &Connection) -> Result<()> {
    match cmd {
        Command::Capture {
            title,
            tag,
            p1,
            p2,
            p3,
            due,
            status,
            json,
        } => capture::run(
            conn,
            capture::CaptureArgs {
                title,
                tags: tag,
                p1,
                p2,
                p3,
                due,
                status,
                json,
            },
        ),
        Command::List {
            status,
            tag,
            due_before,
            json,
        } => list::run(conn, status.as_deref(), &tag, due_before.as_deref(), json),
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
        Command::Tui => crate::tui::run(conn),
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
            },
        ),
        Command::Export { file } => backup::run_export(conn, file.as_deref()),
        Command::Import { file, replace } => backup::run_import(conn, &file, replace),
        Command::Completions { .. } => {
            anyhow::bail!("`gtp completions` is handled before the database is opened")
        }
    }
}

/// The "effective due" of a task: for recurring tasks the slot that the human
/// currently cares about — the most recent occurrence that has already passed
/// without a check-in (missed ⇒ overdue), else the next occurrence on or after
/// now. Otherwise `due_at` or `scheduled_start_at`. Used for sorting/filtering,
/// the alarm window, and the daily digest.
pub(crate) fn effective_due(task: &Task) -> Option<i64> {
    if let Some(rr) = &task.rrule {
        let anchor = task.scheduled_start_at.or(task.due_at);
        if let Some(start) = anchor {
            if let Ok(occ) = crate::time::rrule_occurrences(rr, start, 366) {
                if let Some(d) = effective_due_from_occurrences(&occ) {
                    return Some(d);
                }
            }
            return Some(start);
        }
    }
    task.due_at.or(task.scheduled_start_at)
}

/// 从已经展开的发生序列里挑出「最近一次已错过（逾期）的 slot，否则下一个」。
/// 供缓存了展开结果的路径复用，避免重复展开循环规则。
pub(crate) fn effective_due_from_occurrences(occ: &[i64]) -> Option<i64> {
    let now = crate::time::now_ms();
    if let Some(missed) = occ.iter().rev().find(|m| **m <= now).copied() {
        return Some(missed);
    }
    occ.iter().find(|m| **m >= now).copied()
}
