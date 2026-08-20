use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use crate::tui::App;

use crate::model::task::Status;
use crate::time;

/// 状态缩写（窄字符，用于列表前缀，保持列对齐）。
pub fn status_letter(s: &Status) -> &'static str {
    match s {
        Status::Inbox => ".",
        Status::Next => ">",
        Status::Waiting => "W",
        Status::Scheduled => "#",
        Status::Someday => "?",
        Status::Reference => "*",
        Status::Done => "x",
    }
}

/// 状态语义色：列表、详情、引导栏统一使用，形成稳定的视觉语言。
pub fn status_color(s: &Status) -> Color {
    match s {
        Status::Inbox => Color::Gray,
        Status::Next => Color::Yellow,
        Status::Waiting => Color::LightBlue,
        Status::Scheduled => Color::Cyan,
        Status::Someday => Color::Magenta,
        Status::Reference => Color::White,
        Status::Done => Color::Green,
    }
}

/// 优先级标签的配色：p1 红 / p2 黄 / p3 蓝，与状态色区分开。
pub fn priority_color(tag: &str) -> Option<Color> {
    match tag {
        "p1" => Some(Color::Red),
        "p2" => Some(Color::Yellow),
        "p3" => Some(Color::Blue),
        _ => None,
    }
}

/// 列表行内的迷你进度条（如 [3/5]），用于项目/检查单。
fn progress_text(done: Option<usize>, total: Option<usize>) -> String {
    match (done, total) {
        (Some(d), Some(t)) if t > 0 => format!("[{}/{}]", d, t),
        _ => String::new(),
    }
}

pub fn build_list_items(app: &App) -> Vec<ListItem<'static>> {
    let active_pomo_task_id = if app.pomo.phase != crate::model::pomodoro::Phase::Idle {
        app.pomo.task_id.clone()
    } else {
        None
    };

    app.items
        .iter()
        .map(|r| {
            let status_enum = r.status.parse::<Status>().unwrap_or(Status::Inbox);
            let is_selected = app.selected_ids.contains(&r.id);
            let is_focus_task = active_pomo_task_id.as_deref() == Some(&r.id);
            let is_done = status_enum == Status::Done;
            // 归档箱：用归档原因取代状态语义，不再显示"已完成/逾期"。
            let is_archived = r.archive_reason.is_some();
            // 循环任务今日已打卡：✓ 标记 + 下一次执行时间。
            let is_checked_in = r.checked_in_today;

            let indent = "  ".repeat(r.indent);
            let sel_prefix = if is_selected {
                " [v]"
            } else if is_focus_task {
                " 🎯"
            } else {
                ""
            };

            // 到期：相对时间 + 逾期红色强调（已完成/归档任务显示过去时间，不再标红）
            let reason_cn = if is_archived {
                match r.archive_reason.as_deref() {
                    Some("completed") => crate::tr!(app.lang, "[完成]", "[Done]"),
                    Some("deleted") => crate::tr!(app.lang, "[删除]", "[Deleted]"),
                    _ => crate::tr!(app.lang, "[归档]", "[Archived]"),
                }
            } else {
                ""
            };
            let due_text = if is_archived || is_done {
                time::relative_past(app.lang, r.due)
                    .map(|s| format!("~{}", s))
                    .unwrap_or_default()
            } else if is_checked_in {
                // 已打卡：重点展示下一次执行时间。
                time::relative_due(app.lang, r.due)
                    .map(|s| format!("已打卡·下次:{}", s))
                    .unwrap_or_default()
            } else {
                time::relative_due(app.lang, r.due)
                    .map(|s| format!("~{}", s))
                    .unwrap_or_default()
            };
            let due_color = if is_archived || is_done || is_checked_in {
                Color::DarkGray
            } else if time::is_overdue(r.due) {
                Color::Red
            } else {
                Color::DarkGray
            };

            let (letter, color) = if is_archived {
                match r.archive_reason.as_deref() {
                    Some("completed") => ("√", Color::DarkGray),
                    Some("deleted") => ("×", Color::DarkGray),
                    _ => ("?", Color::DarkGray),
                }
            } else if is_checked_in {
                ("✓", Color::Green)
            } else {
                (status_letter(&status_enum), status_color(&status_enum))
            };

            let mut spans = vec![Span::styled(
                format!("{}{}{} ", sel_prefix, indent, letter),
                Style::default()
                    .fg(if is_focus_task {
                        Color::LightRed
                    } else if is_selected {
                        Color::Yellow
                    } else {
                        color
                    })
                    .add_modifier(if is_focus_task {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )];

            // 标题（选中时加粗，专注任务特殊高亮）
            let title_style = if is_focus_task {
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(r.title.clone(), title_style));

            // 进度
            let prog = progress_text(r.done, r.total);
            if !prog.is_empty() {
                spans.push(Span::styled(
                    format!(" {}", prog),
                    Style::default().fg(Color::Green),
                ));
            }

            // 标签（优先级彩色）
            if !r.tags.is_empty() {
                spans.push(Span::raw(" "));
                let mut first = true;
                for t in &r.tags {
                    if !first {
                        spans.push(Span::raw(","));
                    }
                    first = false;
                    let c = priority_color(t).unwrap_or(app.theme.hl_fg);
                    spans.push(Span::styled(format!("@{}", t), Style::default().fg(c)));
                }
            }

            // 到期 / 归档时间
            if !due_text.is_empty() || !reason_cn.is_empty() {
                spans.push(Span::styled(
                    format!("  {}{}", reason_cn, due_text),
                    Style::default()
                        .fg(due_color)
                        .add_modifier(if due_color == Color::Red {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ));
            }

            let line = Line::from(spans);
            let mut item = ListItem::new(line);
            if is_selected {
                item = item.style(Style::default().bg(Color::DarkGray));
            }
            item
        })
        .collect()
}
