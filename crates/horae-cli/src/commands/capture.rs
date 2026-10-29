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
#[derive(Debug, Clone, Default)]
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
    pub natural: bool,
}

fn get_clipboard_text() -> Result<String> {
    // 优先按当前桌面环境依次尝试系统原生剪贴板工具（Wayland / X11 / macOS / Windows / WSL）
    let mut candidates: Vec<(&str, &[&str])> = Vec::with_capacity(5);
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        candidates.push(("wl-paste", &["--no-newline"]));
    }
    candidates.extend_from_slice(&[
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["--clipboard", "--output"]),
        ("pbpaste", &[]),
        (
            "powershell.exe",
            &["-NoProfile", "-Command", "Get-Clipboard"],
        ),
    ]);

    for (bin, args) in candidates {
        if let Ok(out) = std::process::Command::new(bin).args(args).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).into_owned();
                if !s.trim().is_empty() {
                    return Ok(s);
                }
            }
        }
    }

    anyhow::bail!("failed to read clipboard text (install wl-paste, xclip, xsel, or pbpaste)")
}

pub fn run(conn: &Connection, mut args: CaptureArgs) -> Result<()> {
    let mut notes_text = args.notes.clone();

    // 语音/自然语言模式：若 title 为空或为 "-"，支持从标准输入 (stdin) 读取管道文本
    if args.natural && (args.title.trim().is_empty() || args.title.trim() == "-") {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            use std::io::Read;
            let mut stdin_buf = String::new();
            if std::io::stdin().read_to_string(&mut stdin_buf).is_ok() {
                args.title = stdin_buf.trim().to_string();
            }
        }
    }

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

    let quick_add = if args.natural {
        let nat = horae_core::parser::natural::parse_natural_language(&args.title);
        if args.due.is_none() && nat.due_str.is_some() {
            args.due = nat.due_str.clone();
        }
        nat.to_quick_add()
    } else {
        horae_core::parser::parse_quick_add(&args.title)
    };

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
    let mut t = tasks::create_capture(conn, &input)?;
    if let Some(start) = scheduled_start {
        let _ = tasks::schedule(conn, &t.id, start, None, quick_add.rrule)?;
        if let Ok(updated) = tasks::get(conn, &t.id) {
            t = updated;
        }
    }

    let now = time::now_ms();
    if t.rrule.is_none() {
        if let Some(ms) = t.scheduled_start_at.or(t.due_at) {
            if time::is_past(ms, now) {
                eprintln!(
                    "warning: 时间已过 ({})，任务已处于逾期状态",
                    time::format_local(Some(ms))
                );
            }
        }
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
        let mut details = Vec::new();
        if let Some(s) = t.scheduled_start_at {
            details.push(format!("scheduled: {}", time::format_local(Some(s))));
        }
        if let Some(d) = t.due_at {
            details.push(format!("due: {}", time::format_local(Some(d))));
        }
        if let Some(ref rr) = t.rrule {
            details.push(format!("rrule: {rr}"));
        }
        if let Some(ref p) = t.priority {
            details.push(format!("priority: {p}"));
        }
        if !input.tag_names.is_empty() {
            let tags_s = input
                .tag_names
                .iter()
                .map(|tg| format!("@{tg}"))
                .collect::<Vec<_>>()
                .join(" ");
            details.push(format!("tags: {tags_s}"));
        }
        if !details.is_empty() {
            println!("  parsed -> {}", details.join(" | "));
        }
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
                natural: false,
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
                natural: false,
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
                natural: false,
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
                natural: false,
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
                natural: false,
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
                natural: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("cannot be empty"), "{}", err);
    }

    #[test]
    fn capture_natural_voice_input() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "提醒我明天下午三点半去参加架构评审会，非常重要，标签是工作".into(),
                natural: true,
                ..Default::default()
            },
        )
        .unwrap();
        let tasks = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.title, "去参加架构评审会");
        assert_eq!(t.priority.as_deref(), Some("high"));
        assert_eq!(t.status, task::Status::Scheduled);
        let tags = horae_core::repo::tags::get_task_tags(&conn, &t.id).unwrap();
        assert!(tags.iter().any(|tag| tag.name == "work"));
    }

    #[test]
    fn capture_natural_deadline_sets_due_without_scheduled_start() {
        let (_dir, conn) = test_conn();
        run(
            &conn,
            CaptureArgs {
                title: "下周五下班前必须提交财报分析".into(),
                natural: true,
                ..Default::default()
            },
        )
        .unwrap();
        let tasks = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.title, "提交财报分析");
        assert!(t.due_at.is_some());
        assert_eq!(t.scheduled_start_at, None);
        assert_eq!(t.status, task::Status::Inbox);
        assert_eq!(t.priority.as_deref(), Some("high"));
    }
}
