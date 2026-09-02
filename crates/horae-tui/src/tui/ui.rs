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

/// 优先级的配色：high 红 / medium 黄 / low 蓝，与状态色区分开。
pub fn priority_color(priority: &str) -> Option<Color> {
    match priority {
        "high" => Some(Color::Rgb(243, 139, 168)),   // Red
        "medium" => Some(Color::Rgb(249, 226, 175)), // Yellow
        "low" => Some(Color::Rgb(137, 180, 250)),    // Blue
        _ => None,
    }
}

/// 优先级的展示名 (zh, en)。用于列表/详情/预览的徽标文案。
pub fn priority_label(priority: &str) -> Option<(&'static str, &'static str)> {
    match priority {
        "high" => Some(("高", "High")),
        "medium" => Some(("中", "Medium")),
        "low" => Some(("低", "Low")),
        _ => None,
    }
}

/// 列表行内的迷你微进度条（如 [■■■□□ 3/5]），用于项目/检查单。
fn append_progress_spans(
    spans: &mut Vec<Span<'static>>,
    done: Option<usize>,
    total: Option<usize>,
    theme: &crate::tui::theme::Theme,
    is_ascii: bool,
) {
    if let (Some(d), Some(t)) = (done, total) {
        if let Some(div) = (d * 5).checked_div(t) {
            let filled = div.min(5);
            let empty = 5 - filled;
            let (fill_char, empty_char) = if is_ascii { ("=", "-") } else { ("■", "□") };
            let bar_filled = fill_char.repeat(filled);
            let bar_empty = empty_char.repeat(empty);
            let is_all_done = d == t;
            let bar_color = if is_all_done {
                theme.text_success
            } else {
                theme.accent
            };

            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("[{}{}", bar_filled, bar_empty),
                Style::default().fg(bar_color).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {}/{}]", d, t),
                Style::default().fg(if is_all_done {
                    theme.text_success
                } else {
                    theme.text_dim
                }),
            ));
        }
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

            // 到期：相对时间 + 逾期红色强调 + 今日暖黄强调
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

            let is_overdue =
                !is_archived && !is_done && !is_checked_in && !is_quote && time::is_overdue(r.due);
            let is_today = !is_overdue
                && !is_archived
                && !is_done
                && !is_checked_in
                && !is_quote
                && r.due
                    .map(|d| {
                        let (s, e) = horae_core::time::local_day_bounds(0);
                        d >= s && d <= e
                    })
                    .unwrap_or(false);

            let due_color = if is_archived || is_done || is_checked_in || is_quote {
                app.theme.text_dim
            } else if is_overdue {
                app.theme.text_urgent
            } else if is_today {
                Color::Rgb(249, 226, 175)
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

            // 进度微进度条
            append_progress_spans(
                &mut spans,
                r.done,
                r.total,
                &app.theme,
                app.icon_style == crate::tui::icons::IconStyle::Ascii,
            );

            // 优先级徽标（独立字段，不再作为标签渲染）
            if let Some(ref p) = r.priority {
                if let Some((zh, en)) = priority_label(p) {
                    let c = priority_color(p).unwrap_or(app.theme.hl_fg);
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("!{}", app.lang.tr(zh, en)),
                        Style::default().fg(c).add_modifier(Modifier::BOLD),
                    ));
                }
            }

            // 标签（金句视图隐藏冗余的 @quote）
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
                    Style::default()
                        .fg(due_color)
                        .add_modifier(if is_overdue || is_today {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
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
