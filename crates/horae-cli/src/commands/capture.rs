use rusqlite::Connection;

use anyhow::Result;
use horae_core::model::task;
use horae_core::repo::tasks;
use horae_core::time;

/// 校验 RRULE 可被展开引擎支持；非法值报错并说明支持的频率。
pub fn ensure_rrule_supported(rrule: &str) -> Result<()> {
    if !horae_core::parser::rrule_valid(rrule) {
        anyhow::bail!(
            "invalid rrule `{rrule}`: engine only supports FREQ=DAILY|WEEKLY|MONTHLY|YEARLY"
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
    pub high: bool,
    pub medium: bool,
    pub low: bool,
    pub due: Option<String>,
    pub status: Option<String>,
    pub notes: Option<String>,
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
    let mut notes_text = args.notes.clone();

    if args.clip {
        let clip_text = get_clipboard_text()?;
        let trimmed_clip = clip_text.trim();
        if trimmed_clip.is_empty() {
            anyhow::bail!("clipboard is empty");
        }

        if args.title.trim().is_empty() {
            // 用户没有显式提供标题：从剪贴板第一行提取前 30 个字作为标题，如果内容有多行或超长，全文沉淀到 notes
            let first_line = trimmed_clip.lines().next().unwrap_or("").trim();
            let char_count = first_line.chars().count();
            let has_more_lines = trimmed_clip.lines().nth(1).is_some();

            if char_count > 30 || has_more_lines {
                let truncated: String = first_line.chars().take(30).collect();
                args.title = format!("{}…", truncated.trim_end());
                if notes_text.is_none() {
                    notes_text = Some(trimmed_clip.to_string());
                }
            } else {
                args.title = first_line.to_string();
            }
        } else {
            // 用户显式提供了标题（如 horae c "买书" --clip）：用户输入作为标题，剪贴板全文存入 notes
            if notes_text.is_none() {
                notes_text = Some(trimmed_clip.to_string());
            }
        }
    }

    if args.title.trim().is_empty() {
        anyhow::bail!("task title cannot be empty (provide a title or use --clip)");
    }

    let quick_add = horae_core::parser::parse_quick_add(&args.title);

    // 与 TUI 输入层同源防呆：非法 RRULE（引擎不支持的频率/语法）在写库前报错，
    // 避免循环任务静默退化成一次性任务（watch 手机桥复用本函数，一并覆盖）。
    if let Some(rr) = &quick_add.rrule {
        ensure_rrule_supported(rr)?;
    }

    let mut tag_names: Vec<String> = args.tags.clone();
    tag_names.extend(quick_add.tags);

    // 优先级为独立字段（CLI 标志优先于标题里的 !high）。
    let priority = if args.high {
        Some("high".to_string())
    } else if args.medium {
        Some("medium".to_string())
    } else if args.low {
        Some("low".to_string())
    } else {
        quick_add.priority.clone()
    };

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
        notes: notes_text.unwrap_or_default(),
        status: if parsed_status == task::Status::Inbox && scheduled_start.is_some() {
            task::Status::Scheduled
        } else {
            parsed_status
        },
        due_at,
        tag_names,
        priority,
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
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
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
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
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
    fn capture_accepts_yearly_rrule() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "年度体检 *y".into(),
                clip: false,
                tags: vec![],
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
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
        assert_eq!(task[0].rrule.as_deref(), Some("FREQ=YEARLY"));
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
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
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
                title: "晨跑 *2d[1,3] ~tomorrow 09:00".into(),
                clip: false,
                tags: vec![],
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("2d[1,3]"), "{}", err);
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
                high: false,
                medium: false,
                low: false,
                due: None,
                status: None,
                notes: None,
                json: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "{}", err);
    }
}
