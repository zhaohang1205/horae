use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use crate::tui::{App, View};

use horae_core::model::task::Status;
use horae_core::time;

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

/// 状态语义色：列表、详情、引导栏统一使用，形成稳定的视觉语言（正统 Catppuccin）。
pub fn status_color(s: &Status, is_dark: bool) -> Color {
    match (s, is_dark) {
        // Mocha（深色）
        (Status::Inbox, true) => Color::Rgb(147, 153, 178), // Overlay2
        (Status::Next, true) => Color::Rgb(166, 227, 161),  // Green
        (Status::Waiting, true) => Color::Rgb(249, 226, 175), // Yellow
        (Status::Scheduled, true) => Color::Rgb(137, 180, 250), // Blue
        (Status::Someday, true) => Color::Rgb(203, 166, 229), // Mauve
        (Status::Reference, true) => Color::Rgb(148, 226, 213), // Teal
        (Status::Done, true) => Color::Rgb(127, 132, 156),  // Overlay1
        // Latte（浅色）
        (Status::Inbox, false) => Color::Rgb(138, 143, 161), // Overlay1
        (Status::Next, false) => Color::Rgb(64, 160, 43),    // Green
        (Status::Waiting, false) => Color::Rgb(223, 142, 29), // Yellow
        (Status::Scheduled, false) => Color::Rgb(30, 102, 245), // Blue
        (Status::Someday, false) => Color::Rgb(136, 58, 234), // Mauve
        (Status::Reference, false) => Color::Rgb(23, 146, 153), // Teal
        (Status::Done, false) => Color::Rgb(138, 143, 161),  // Overlay1
    }
}

/// 优先级标签的配色：p1 红 / p2 黄 / p3 蓝，与状态色区分开。
pub fn priority_color(tag: &str) -> Option<Color> {
    match tag {
        "p1" => Some(Color::Rgb(243, 139, 168)), // Red
        "p2" => Some(Color::Rgb(249, 226, 175)), // Yellow
        "p3" => Some(Color::Rgb(137, 180, 250)), // Blue
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

pub(crate) fn build_list_items(app: &App) -> Vec<ListItem<'static>> {
    let active_pomo_task_id = if app.pomo.phase != horae_core::model::pomodoro::Phase::Idle {
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
            // 金句视图：@quote 行显示为引用（前缀 "、创建时间），不显示逾期。
            let is_quote = app.view == View::Quotes
                && r.tags
                    .iter()
                    .any(|t| t == horae_core::repo::tasks::QUOTE_TAG);

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
                    Some("completed") => tr!(app.lang, "[完成]", "[Done]"),
                    Some("deleted") => tr!(app.lang, "[删除]", "[Deleted]"),
                    _ => tr!(app.lang, "[归档]", "[Archived]"),
                }
            } else {
                ""
            };
            let due_text = if is_archived || is_done || is_quote {
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
            let due_color = if is_archived || is_done || is_checked_in || is_quote {
                app.theme.text_dim
            } else if time::is_overdue(r.due) {
                app.theme.text_urgent
            } else {
                app.theme.text_dim
            };

            let (letter, color) = if is_archived {
                match r.archive_reason.as_deref() {
                    Some("completed") => ("√", app.theme.text_dim),
                    Some("deleted") => ("×", app.theme.text_dim),
                    _ => ("?", app.theme.text_dim),
                }
            } else if is_checked_in {
                ("✓", app.theme.text_success)
            } else if is_quote {
                ("\"", app.theme.accent)
            } else {
                (
                    status_letter(&status_enum),
                    status_color(&status_enum, app.theme.is_dark),
                )
            };

            let mut spans = vec![Span::styled(
                format!("{}{}{} ", sel_prefix, indent, letter),
                Style::default()
                    .fg(if is_focus_task {
                        app.theme.text_urgent
                    } else if is_selected {
                        app.theme.accent
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
                    .fg(app.theme.text_urgent)
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
                    Style::default().fg(app.theme.text_success),
                ));
            }

            // 标签（优先级彩色；金句视图隐藏冗余的 @quote）
            let shown_tags: Vec<&String> = r
                .tags
                .iter()
                .filter(|t| !(is_quote && t.as_str() == horae_core::repo::tasks::QUOTE_TAG))
                .collect();
            if !shown_tags.is_empty() {
                spans.push(Span::raw(" "));
                for (i, t) in shown_tags.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::raw(","));
                    }
                    let c = priority_color(t).unwrap_or(app.theme.hl_fg);
                    spans.push(Span::styled(format!("@{}", t), Style::default().fg(c)));
                }
            }

            // 到期 / 归档时间
            if !due_text.is_empty() || !reason_cn.is_empty() {
                spans.push(Span::styled(
                    format!("  {}{}", reason_cn, due_text),
                    Style::default().fg(due_color).add_modifier(
                        if due_color == app.theme.text_urgent {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        },
                    ),
                ));
            }

            let line = Line::from(spans);
            let mut item = ListItem::new(line);
            if is_selected {
                item = item.style(Style::default().bg(app.theme.hl_bg));
            }
            item
        })
        .collect()
}
