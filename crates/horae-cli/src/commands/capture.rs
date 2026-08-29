use rusqlite::Connection;

use anyhow::Result;
use horae_core::model::task;
use horae_core::repo::tasks;
use horae_core::time;

/// 校验 RRULE 可被展开引擎支持；非法值报错并说明支持的三种频率。
pub fn ensure_rrule_supported(rrule: &str) -> Result<()> {
    if !horae_core::parser::rrule_valid(rrule) {
        anyhow::bail!(
            "invalid rrule `{rrule}`: engine only supports FREQ=DAILY|WEEKLY|MONTHLY \
             (YEARLY is not implemented)"
        );
    }
    Ok(())
}

/// CLI-derived arguments for `capture`. Keeps `run`'s signature small and
/// separates command parsing from the repo-layer `tasks::CaptureInput`.
pub struct CaptureArgs {
    pub title: String,
    pub clip: bool,
    pub tags: Vec<String>,
    pub p1: bool,
    pub p2: bool,
    pub p3: bool,
    pub due: Option<String>,
    pub status: Option<String>,
    pub json: bool,
}

fn get_clipboard_text() -> Result<String> {
    // 1. 在 Linux Wayland/X11 环境下，优先尝试系统原生 CLI 工具（wl-paste / xclip / xsel）
    // 因为 CLI 进程短生命周期下，纯 X11/Wayland 库直接建立连接偶尔无法接管跨窗口 selection。
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Ok(out) = std::process::Command::new("wl-paste")
            .arg("--no-newline")
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).to_string();
                if !s.trim().is_empty() {
                    return Ok(s);
                }
            }
        }
    }

    if let Ok(out) = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.trim().is_empty() {
                return Ok(s);
            }
        }
    }

    if let Ok(out) = std::process::Command::new("xsel")
        .args(["--clipboard", "--output"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.trim().is_empty() {
                return Ok(s);
            }
        }
    }

    // 2. 跨平台通用库 (arboard) 作为保底
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| anyhow::anyhow!("failed to access clipboard: {e}"))?;
    clipboard
        .get_text()
        .map_err(|e| anyhow::anyhow!("failed to read clipboard text: {e}"))
}

pub fn run(conn: &Connection, mut args: CaptureArgs) -> Result<()> {
    if args.clip {
        let clip_text = get_clipboard_text()?;
        let normalized = clip_text.trim().replace(['\r', '\n'], " ");
        if normalized.is_empty() {
            anyhow::bail!("clipboard is empty");
        }
        if args.title.trim().is_empty() {
            args.title = normalized;
        } else {
            args.title.push(' ');
            args.title.push_str(&normalized);
        }
    }

    if args.title.trim().is_empty() {
        anyhow::bail!("task title cannot be empty (provide a title or use --clip)");
    }

    let quick_add = horae_core::parser::parse_quick_add(&args.title);

    // 与 TUI 输入层同源防呆：非法 RRULE（含引擎不支持的 YEARLY）在写库前报错，
    // 避免循环任务静默退化成一次性任务（watch 手机桥复用本函数，一并覆盖）。
    if let Some(rr) = &quick_add.rrule {
        ensure_rrule_supported(rr)?;
    }

    let mut tag_names: Vec<String> = args.tags.clone();
    tag_names.extend(quick_add.tags);
    if let Some(p) = &quick_add.priority {
        tag_names.push(p.clone());
    }
    if args.p1 {
        tag_names.push("p1".into());
    }
    if args.p2 {
        tag_names.push("p2".into());
    }
    if args.p3 {
        tag_names.push("p3".into());
    }

    // `--due` 仍是软截止（due_at）；一句话里的 `~time` 是排程起点（scheduled_start_at）。
    let due_at = match args.due {
        Some(d) => Some(time::parse_time(&d)?),
        None => None,
    };
    let scheduled_start = match &quick_add.time_str {
        Some(t) => Some(time::parse_time(t)?),
        None => None,
    };
    let status_str = args.status.as_deref().unwrap_or("inbox");
    let parsed_status: task::Status = status_str.parse().map_err(|e| anyhow::anyhow!("{}", e))?;

    // ~time 存在 → 排程起点（创建后 schedule 设 scheduled_start_at, 状态 Scheduled, 无终点）。
    let input = tasks::CaptureInput {
        title: quick_add.title,
        status: if parsed_status == task::Status::Inbox && scheduled_start.is_some() {
            task::Status::Scheduled
        } else {
            parsed_status
        },
        due_at,
        tag_names,
        rrule: if scheduled_start.is_some() {
            None
        } else {
            quick_add.rrule.clone()
        },
        ..Default::default()
    };
    let t = tasks::create_capture(conn, &input)?;
    if let Some(start) = scheduled_start {
        let _ = tasks::schedule(conn, &t.id, start, None, quick_add.rrule)?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&t)?);
    } else {
        println!(
            "captured [{}] {}  (status: {})",
            &t.id[..t.id.len().min(8)],
            t.title,
            t.status
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horae_core::testutil::test_conn;

    #[test]
    fn capture_keeps_quick_add_rrule() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "晨跑 *d".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap();
        let task = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(task.len(), 1);
        assert_eq!(task[0].title, "晨跑");
        assert_eq!(task[0].rrule.as_deref(), Some("FREQ=DAILY"));
        assert_eq!(task[0].status, task::Status::Inbox, "无时间则留在收件箱");
    }

    #[test]
    fn capture_quick_add_time_schedules() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "买牛奶 ~tomorrow 09:00".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap();
        let task = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(task.len(), 1);
        let t = &task[0];
        assert_eq!(t.status, task::Status::Scheduled, "~time → 排程起点");
        assert_eq!(t.due_at, None, "~time 不再设软截止");
        let expect = time::parse_time("tomorrow 09:00").unwrap();
        assert_eq!(
            time::format_local(t.scheduled_start_at),
            time::format_local(Some(expect)),
            "scheduled_start_at = 明天 09:00"
        );
        assert_eq!(t.scheduled_end_at, None, "只设起点, 不设终点");
    }

    fn empty_list(conn: &Connection) -> Vec<task::Task> {
        tasks::list(
            conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap()
    }

    // ---------- rrule 校验（与 TUI 输入层同源防呆） ----------

    #[test]
    fn capture_rejects_yearly_rrule_without_creating_task() {
        let (_dir, conn) = test_conn();
        let err = run(
            &conn,
            CaptureArgs {
                title: "年度体检 *y".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("FREQ=YEARLY"), "{msg}");
        assert!(msg.contains("FREQ=DAILY|WEEKLY|MONTHLY"), "{msg}");
        assert!(
            empty_list(&conn).is_empty(),
            "非法 rrule 不得落库（否则静默退化成一次性任务）"
        );
    }

    #[test]
    fn capture_rejects_unrecognized_rrule_word() {
        let (_dir, conn) = test_conn();
        // `*` 开头但无法识别的词：fallback 原样保留（"sometimes"），必须被拦截
        let err = run(
            &conn,
            CaptureArgs {
                title: "晨跑 *sometimes".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("sometimes"), "{}", err);
        assert!(empty_list(&conn).is_empty());
    }

    #[test]
    fn capture_with_time_rejects_bad_rrule_before_schedule() {
        // ~time 分支走 tasks::schedule，同样必须先被校验拦下
        let (_dir, conn) = test_conn();
        let err = run(
            &conn,
            CaptureArgs {
                title: "年度体检 *y ~tomorrow 09:00".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("FREQ=YEARLY"), "{}", err);
        assert!(empty_list(&conn).is_empty());
    }

    #[test]
    fn capture_rejects_empty_title_without_clip() {
        let (_dir, conn) = test_conn();
        let err = run(
            &conn,
            CaptureArgs {
                title: "".into(),
                clip: false,
                tags: vec![],
                p1: false,
                p2: false,
                p3: false,
                due: None,
                status: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "{}", err);
    }
}
