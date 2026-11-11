use crate::tui::app::{App, Mode};
use crate::tui::icons::{Icon, IconStyle};
use crate::tui::status_cn;
use crate::tui::ui;
use chrono::{Local, TimeZone, Utc};
use horae_core::model::event;
use horae_core::repo::tasks::describe_event;
use horae_core::time;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn format_compact_time(ms: i64) -> String {
    match Utc.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.with_timezone(&Local).format("%m-%d %H:%M").to_string(),
        None => "-".to_string(),
    }
}

/// 将单行文本按指定首行与后续行最大显示宽度折叠为多行，支持中英文混排、标点与长单词安全折断。
pub(crate) fn wrap_text(text: &str, first_width: usize, cont_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let first_width = first_width.max(1);
    let cont_width = cont_width.max(1);

    // 将文本分词：
    // - 连续空白符
    // - CJK / 宽字符 / 非 ASCII（宽度 >= 2 或非 ASCII）按单个字符拆分
    // - 常用分隔标点（; , : / - _ ； ， ：）作为单独断点
    // - ASCII 单词（连续字母数字等）
    let mut tokens: Vec<&str> = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        if c.is_whitespace() {
            while let Some(&(_, next_c)) = chars.peek() {
                if next_c.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            tokens.push(&text[idx..end]);
        } else if UnicodeWidthChar::width(c).unwrap_or(0) >= 2 || !c.is_ascii() {
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            tokens.push(&text[idx..end]);
        } else if c == ';' || c == ',' || c == ':' || c == '/' || c == '-' || c == '_' {
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            tokens.push(&text[idx..end]);
        } else {
            while let Some(&(_, next_c)) = chars.peek() {
                if next_c.is_whitespace()
                    || UnicodeWidthChar::width(next_c).unwrap_or(0) >= 2
                    || !next_c.is_ascii()
                    || next_c == ';'
                    || next_c == ','
                    || next_c == ':'
                    || next_c == '/'
                    || next_c == '-'
                    || next_c == '_'
                {
                    break;
                }
                chars.next();
            }
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            tokens.push(&text[idx..end]);
        }
    }

    let mut lines = Vec::new();
    let mut cur_line = String::new();
    let mut cur_w = 0;
    let mut cap = first_width;

    for token in tokens {
        let is_ws = token.chars().all(|c| c.is_whitespace());
        let token_w = UnicodeWidthStr::width(token);

        if cur_w == 0 && is_ws {
            continue;
        }

        if cur_w + token_w <= cap {
            cur_line.push_str(token);
            cur_w += token_w;
        } else if cur_w > 0 {
            let trimmed = cur_line.trim_end();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
            cur_line = String::new();
            cur_w = 0;
            cap = cont_width;

            if !is_ws {
                if token_w <= cap {
                    cur_line.push_str(token);
                    cur_w = token_w;
                } else {
                    for c in token.chars() {
                        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                        if cur_w + cw > cap && cur_w > 0 {
                            let tr = cur_line.trim_end();
                            if !tr.is_empty() {
                                lines.push(tr.to_string());
                            }
                            cur_line = String::new();
                            cur_w = 0;
                            cap = cont_width;
                        }
                        cur_line.push(c);
                        cur_w += cw;
                    }
                }
            }
        } else {
            for c in token.chars() {
                let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                if cur_w + cw > cap && cur_w > 0 {
                    let tr = cur_line.trim_end();
                    if !tr.is_empty() {
                        lines.push(tr.to_string());
                    }
                    cur_line = String::new();
                    cur_w = 0;
                    cap = cont_width;
                }
                cur_line.push(c);
                cur_w += cw;
            }
        }
    }

    let trimmed = cur_line.trim_end();
    if !trimmed.is_empty() || lines.is_empty() {
        lines.push(trimmed.to_string());
    }

    lines
}

impl<'a> App<'a> {
    /// 详情面板内容行。
    pub(crate) fn detail_lines(
        &self,
        d: &crate::tui::app::DetailData,
        width: u16,
        height: u16,
    ) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = vec![];
        let inner_width = (width.saturating_sub(4)).max(1) as usize;

        // 标题
        let title_prefix = Span::styled(
            tr!(self.lang, "标题: ", "Title: "),
            Style::default()
                .fg(self.theme.text_dim)
                .add_modifier(Modifier::BOLD),
        );
        let prefix_w = UnicodeWidthStr::width(title_prefix.content.as_ref());
        let cont_prefix = " ".repeat(prefix_w);
        let title_style = Style::default().add_modifier(Modifier::BOLD);

        let title_chunks = wrap_text(
            &d.task.title,
            inner_width.saturating_sub(prefix_w),
            inner_width.saturating_sub(prefix_w),
        );
        for (i, chunk) in title_chunks.into_iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    title_prefix.clone(),
                    Span::styled(chunk, title_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(cont_prefix.clone()),
                    Span::styled(chunk, title_style),
                ]));
            }
        }

        // 状态 / 归档（归档任务只展示原因与时间，不再显示已完成/逾期）
        if let Some(reason) = &d.task.archive_reason {
            let reason_cn = match reason.as_str() {
                "completed" => tr!(self.lang, "[完成]", "[Done]"),
                "deleted" => tr!(self.lang, "[删除]", "[Deleted]"),
                _ => tr!(self.lang, "[归档]", "[Archived]"),
            };
            let prefix = Span::styled(
                tr!(self.lang, "归档: ", "Archived: "),
                Style::default().fg(self.theme.text_dim),
            );
            let prefix_w = UnicodeWidthStr::width(prefix.content.as_ref());
            let cont_prefix = " ".repeat(prefix_w);
            let val_str = format!("{} {}", reason_cn, time::format_local(d.task.archived_at));
            let chunks = wrap_text(
                &val_str,
                inner_width.saturating_sub(prefix_w),
                inner_width.saturating_sub(prefix_w),
            );
            for (i, chunk) in chunks.into_iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![
                        prefix.clone(),
                        Span::styled(chunk, Style::default().fg(self.theme.text_dim)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(cont_prefix.clone()),
                        Span::styled(chunk, Style::default().fg(self.theme.text_dim)),
                    ]));
                }
            }
        } else {
            let st_color = self.theme.status_color(&d.task.status);
            lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, "状态: ", "Status: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    status_cn(self.lang, d.task.status),
                    Style::default().fg(st_color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // 计划时间：具体时间显示排程起点；循环任务显示最近一次计划执行日期
        let has_schedule = d.task.scheduled_start_at.is_some()
            || d.task.scheduled_end_at.is_some()
            || (d.task.rrule.is_some()
                && (d.task.scheduled_start_at.is_some() || d.task.due_at.is_some()));

        if has_schedule {
            let planned = if d.task.rrule.is_some() {
                // 循环任务：错过 slot 取最近一次已错过（逾期），否则下一次执行
                match horae_core::schedule::effective_due(&d.task) {
                    Some(ms) => time::format_local(Some(ms)),
                    None => "-".to_string(),
                }
            } else {
                match d.task.scheduled_end_at {
                    Some(_) => format!(
                        "{} -> {}",
                        time::format_local(d.task.scheduled_start_at),
                        time::format_local(d.task.scheduled_end_at)
                    ),
                    None => time::format_local(d.task.scheduled_start_at),
                }
            };
            let prefix = Span::styled(
                tr!(self.lang, "计划: ", "Planned: "),
                Style::default().fg(self.theme.text_dim),
            );
            let prefix_w = UnicodeWidthStr::width(prefix.content.as_ref());
            let cont_prefix = " ".repeat(prefix_w);
            let chunks = wrap_text(
                &planned,
                inner_width.saturating_sub(prefix_w),
                inner_width.saturating_sub(prefix_w),
            );
            for (i, chunk) in chunks.into_iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![prefix.clone(), Span::raw(chunk)]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(cont_prefix.clone()),
                        Span::raw(chunk),
                    ]));
                }
            }
        }

        // 截止时间：仅在明确指定了 due_at 时展示，避免一般录入任务被误导
        if let Some(due) = d.task.due_at {
            lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, "截止: ", "Due: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(time::format_local(Some(due))),
            ]));
        } else if !has_schedule {
            lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, "时间: ", "Time: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw("-"),
            ]));
        }

        // 循环规则
        if let Some(rr) = &d.task.rrule {
            let shown_rr = if self.lang.is_zh() {
                rr.replace("FREQ=DAILY", "每天")
                    .replace("FREQ=WEEKLY", "每周")
                    .replace("FREQ=MONTHLY", "每月")
                    .replace("INTERVAL=", "间隔=")
                    .replace("COUNT=", "次数=")
                    .replace("UNTIL=", "直到=")
            } else {
                rr.clone()
            };
            let prefix = Span::styled(
                tr!(self.lang, "循环: ", "Rrule: "),
                Style::default().fg(self.theme.text_dim),
            );
            let prefix_w = UnicodeWidthStr::width(prefix.content.as_ref());
            let cont_prefix = " ".repeat(prefix_w);
            let chunks = wrap_text(
                &shown_rr,
                inner_width.saturating_sub(prefix_w),
                inner_width.saturating_sub(prefix_w),
            );
            for (i, chunk) in chunks.into_iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![prefix.clone(), Span::raw(chunk)]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(cont_prefix.clone()),
                        Span::raw(chunk),
                    ]));
                }
            }
        }

        // 优先级（独立字段，不再作为标签）
        if let Some(ref p) = d.task.priority {
            if let Some((zh, en)) = ui::priority_label(p) {
                let c = self.theme.priority_color(p).unwrap_or(self.theme.hl_fg);
                lines.push(Line::from(vec![
                    Span::styled(
                        tr!(self.lang, "优先级: ", "Priority: "),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(format!("!{}", self.lang.tr(zh, en)), Style::default().fg(c)),
                ]));
            }
        }

        // 标签
        if !d.tags.is_empty() {
            let prefix = Span::styled(
                tr!(self.lang, "标签: ", "Tags: "),
                Style::default().fg(self.theme.text_dim),
            );
            let prefix_w = UnicodeWidthStr::width(prefix.content.as_ref());
            let cont_prefix = " ".repeat(prefix_w);

            let mut tag_lines: Vec<Vec<Span<'static>>> = Vec::new();
            let mut cur_spans = vec![prefix];
            let mut cur_w = prefix_w;

            for (i, tg) in d.tags.iter().enumerate() {
                let c = self
                    .theme
                    .priority_color(&tg.name)
                    .unwrap_or(self.theme.hl_fg);
                let tag_str = format!("@{}", tg.name);
                let tag_w = UnicodeWidthStr::width(tag_str.as_str());
                let space_w = if i < d.tags.len() - 1 { 1 } else { 0 };

                if cur_w + tag_w + space_w > inner_width && cur_spans.len() > 1 {
                    tag_lines.push(cur_spans);
                    cur_spans = vec![Span::raw(cont_prefix.clone())];
                    cur_w = prefix_w;
                }

                cur_spans.push(Span::styled(tag_str, Style::default().fg(c)));
                cur_w += tag_w;
                if space_w > 0 {
                    cur_spans.push(Span::raw(" "));
                    cur_w += 1;
                }
            }
            if !cur_spans.is_empty() {
                tag_lines.push(cur_spans);
            }
            for tl in tag_lines {
                lines.push(Line::from(tl));
            }
        }

        // 委派
        if let Some(del) = &d.task.delegated_to {
            let prefix = Span::styled(
                tr!(self.lang, "委派: ", "Delegated: "),
                Style::default().fg(self.theme.text_dim),
            );
            let prefix_w = UnicodeWidthStr::width(prefix.content.as_ref());
            let cont_prefix = " ".repeat(prefix_w);
            let chunks = wrap_text(
                del,
                inner_width.saturating_sub(prefix_w),
                inner_width.saturating_sub(prefix_w),
            );
            for (i, chunk) in chunks.into_iter().enumerate() {
                if i == 0 {
                    lines.push(Line::from(vec![prefix.clone(), Span::raw(chunk)]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(cont_prefix.clone()),
                        Span::raw(chunk),
                    ]));
                }
            }
        }

        // 检查单
        if !d.task.checklist.is_empty() {
            lines.push(Line::from(Span::styled(
                tr!(self.lang, "检查单:", "Checklist:"),
                Style::default().fg(self.theme.text_dim),
            )));
            for (i, item) in d.task.checklist.iter().enumerate() {
                let is_cursor = self.checklist_cursor == Some(i);
                let check = if item.done { "[x]" } else { "[ ]" };
                let c = if item.done {
                    self.theme.text_success
                } else {
                    self.theme.text_dim
                };
                let prefix_glyph = if is_cursor { "▶ " } else { "  " };
                let prefix_str = format!("{}{} ", prefix_glyph, check);
                let prefix_w = UnicodeWidthStr::width(prefix_str.as_str());
                let cont_prefix = " ".repeat(prefix_w);

                let item_style = Style::default().fg(c).add_modifier(if is_cursor {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });

                let chunks = wrap_text(
                    &item.title,
                    inner_width.saturating_sub(prefix_w),
                    inner_width.saturating_sub(prefix_w),
                );
                for (ci, chunk) in chunks.into_iter().enumerate() {
                    if ci == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(prefix_str.clone(), item_style),
                            Span::styled(chunk, item_style),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(cont_prefix.clone()),
                            Span::styled(chunk, item_style),
                        ]));
                    }
                }
            }
            // 全勾选提示（不自动改任务状态）：引导用户用 x 标记完成。
            if d.task.checklist.iter().all(|i| i.done) {
                let msg = tr!(
                    self.lang,
                    "✓ 所有步骤已完成 — 按 x 标记任务完成",
                    "✓ all steps done — press x to complete the task"
                );
                let chunks = wrap_text(msg, inner_width, inner_width);
                for chunk in chunks {
                    lines.push(Line::from(Span::styled(
                        chunk,
                        Style::default().fg(self.theme.text_success),
                    )));
                }
            }
        }

        // 番茄钟计数
        let pomo_count = d
            .events
            .iter()
            .filter(|e| e.event_type == event::EV_POMODORO)
            .count();
        if pomo_count > 0 {
            let tomatoes = format!("{} ", self.icon(Icon::Tomato)).repeat(pomo_count);
            lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, "专注: ", "Focus: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("{} ({})", tomatoes, pomo_count),
                    Style::default().fg(self.theme.text_urgent),
                ),
            ]));
        }

        // 分隔线
        lines.push(Line::from("─".repeat(inner_width)));

        // 备注
        if d.task.notes.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                tr!(self.lang, "备注: -", "Notes: -"),
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                tr!(self.lang, "备注:", "Notes:"),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            let md_lines = crate::tui::render::markdown::render_markdown(
                &d.task.notes,
                &self.theme,
                inner_width,
                self.icon_style,
            );

            const NOTES_FOLD_THRESHOLD: usize = 8;
            let total_md_lines = md_lines.len();

            if total_md_lines > NOTES_FOLD_THRESHOLD && !self.notes_expanded {
                lines.extend(md_lines.into_iter().take(NOTES_FOLD_THRESHOLD));
                let collapsed_count = total_md_lines - NOTES_FOLD_THRESHOLD;
                let fold_hint = tr!(
                    self.lang,
                    "  ... (折叠剩余 {} 行 · 按 z 展开备注)",
                    "  ... (collapsed {} remaining lines · press z to expand)",
                    collapsed_count
                );
                lines.push(Line::from(Span::styled(
                    fold_hint,
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::ITALIC),
                )));
            } else if total_md_lines > NOTES_FOLD_THRESHOLD && self.notes_expanded {
                lines.extend(md_lines);
                let collapse_hint = tr!(
                    self.lang,
                    "  ▲ (已展开全文 · 按 z 收起备注)",
                    "  ▲ (expanded all · press z to collapse)"
                );
                lines.push(Line::from(Span::styled(
                    collapse_hint,
                    Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::ITALIC),
                )));
            } else {
                lines.extend(md_lines);
            }
        }

        lines.push(Line::from(""));
        let git_icon = self.icon(Icon::Timeline);
        let header_prefix = if git_icon.is_empty() {
            "".to_string()
        } else {
            format!("{} ", git_icon)
        };
        lines.push(Line::from(vec![
            Span::styled(header_prefix, Style::default().fg(self.theme.accent)),
            Span::styled(
                tr!(self.lang, "时间线 (Git Log)", "Timeline (Git Log)"),
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if d.events.is_empty() {
            lines.push(Line::from(Span::styled(
                tr!(self.lang, "  (暂无历史记录)", "  (no history recorded)"),
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            let is_ascii = self.icon_style == IconStyle::Ascii;
            let node_head = if is_ascii { "*" } else { "●" };
            let node_other = if is_ascii { "o" } else { "○" };
            let rail = if is_ascii { "|" } else { "│" };
            let collapsed_rail = if is_ascii { ":" } else { "┊" };

            // Available vertical space in detail pane
            let used_so_far = lines.len();
            let inner_height = (height.saturating_sub(2)) as usize;
            let available = inner_height.saturating_sub(used_so_far);

            // d.events is chronological: index 0 is root, last is HEAD.
            let total = d.events.len();

            // When total > 6 and space is limited and not expanded: show newest 3, collapsed summary, and 1 root
            let (display_groups, collapsed_count) =
                if total > 6 && available < total * 2 && !self.timeline_expanded {
                    let newest = d.events.iter().rev().take(3).cloned().collect::<Vec<_>>();
                    let root = d.events.first().cloned().unwrap();
                    let collapsed = total - 4;
                    (vec![(newest, true), (vec![root], false)], collapsed)
                } else {
                    let all_rev = d.events.iter().rev().cloned().collect::<Vec<_>>();
                    (vec![(all_rev, true)], 0)
                };

            let total_displayed = if collapsed_count > 0 { 4 } else { total };
            let show_rail_lines = available > total_displayed * 2;

            for (group_idx, (group_events, is_first_group)) in display_groups.iter().enumerate() {
                for (item_idx, event) in group_events.iter().enumerate() {
                    let is_head = *is_first_group && item_idx == 0;
                    let is_root = (!is_first_group && item_idx == group_events.len() - 1)
                        || (*is_first_group
                            && collapsed_count == 0
                            && item_idx == group_events.len() - 1);

                    let entry = describe_event(event, self.lang);
                    let time_str = if inner_width < 28 {
                        // 紧凑模式下仅显示时:分，留出空间展示事件徽章与描述
                        match Utc.timestamp_millis_opt(entry.at).single() {
                            Some(dt) => dt.with_timezone(&Local).format("%H:%M").to_string(),
                            None => "-".to_string(),
                        }
                    } else {
                        format_compact_time(entry.at)
                    };

                    let (node_glyph, node_color) = if is_head {
                        let color = if entry.event_type == event::EV_COMPLETED
                            || entry.event_type == event::EV_HABIT_COMPLETED
                        {
                            self.theme.text_success
                        } else {
                            self.theme.accent
                        };
                        (node_head, color)
                    } else if entry.event_type == event::EV_COMPLETED
                        || entry.event_type == event::EV_HABIT_COMPLETED
                    {
                        (node_other, self.theme.text_success)
                    } else if entry.event_type == event::EV_POMODORO
                        || entry.event_type == event::EV_POMODORO_STARTED
                    {
                        (node_other, self.theme.rrule_fg)
                    } else {
                        (node_other, self.theme.text_dim)
                    };

                    let (badge_color, badge_bold) = match entry.event_type.as_str() {
                        event::EV_COMPLETED | event::EV_HABIT_COMPLETED => {
                            (self.theme.text_success, true)
                        }
                        event::EV_POMODORO | event::EV_POMODORO_STARTED => {
                            (self.theme.rrule_fg, true)
                        }
                        event::EV_POMODORO_ABORTED | event::EV_ARCHIVED => {
                            (self.theme.text_urgent, false)
                        }
                        event::EV_STATUS_CHANGED | event::EV_SCHEDULED => {
                            (self.theme.accent, false)
                        }
                        event::EV_CHECKLIST => (self.theme.hl_fg, false),
                        _ => (self.theme.text_dim, false),
                    };

                    let mut badge_style = Style::default().fg(badge_color);
                    if badge_bold {
                        badge_style = badge_style.add_modifier(Modifier::BOLD);
                    }

                    let ref_badge = if is_head && is_root {
                        Some((
                            "(HEAD, root)",
                            Style::default()
                                .fg(self.theme.accent)
                                .add_modifier(Modifier::BOLD),
                        ))
                    } else if is_head {
                        Some((
                            "(HEAD)",
                            Style::default()
                                .fg(self.theme.accent)
                                .add_modifier(Modifier::BOLD),
                        ))
                    } else if is_root {
                        Some(("(root)", Style::default().fg(self.theme.text_dim)))
                    } else {
                        None
                    };

                    let node_span = Span::styled(
                        format!("  {} ", node_glyph),
                        Style::default().fg(node_color),
                    );
                    let time_span = Span::styled(
                        format!("{} ", time_str),
                        Style::default().fg(self.theme.text_dim),
                    );
                    let badge_span = Span::styled(format!("[{}] ", entry.badge), badge_style);

                    let node_w = UnicodeWidthStr::width(node_span.content.as_ref());
                    let time_w = UnicodeWidthStr::width(time_span.content.as_ref());
                    let badge_w = UnicodeWidthStr::width(badge_span.content.as_ref());

                    let ref_badge_w = ref_badge
                        .map(|(s, _)| UnicodeWidthStr::width(s) + 1)
                        .unwrap_or(0);
                    let desc_w = UnicodeWidthStr::width(entry.description.as_str());

                    let header_w = node_w + time_w + badge_w + ref_badge_w;
                    let total_single_line_w = header_w + desc_w;

                    if total_single_line_w <= inner_width {
                        // 单行足够容纳：整行展示，紧凑美观
                        let mut line_spans = vec![node_span, time_span, badge_span];
                        if let Some((tag_str, tag_style)) = ref_badge {
                            line_spans.push(Span::styled(format!("{} ", tag_str), tag_style));
                        }
                        line_spans.push(Span::styled(
                            entry.description,
                            Style::default().fg(self.theme.fg),
                        ));
                        lines.push(Line::from(line_spans));
                    } else {
                        // 空间不足单行容纳：采用 Git Log 结构化折叠，保持树状轨线（rail）不断裂
                        let header_fits = header_w <= inner_width;
                        let mut line_spans = vec![node_span, time_span, badge_span];

                        // 如果连 ref_badge 都放不下，则将 ref_badge 移到折叠行
                        let (ref_badge_on_line2, desc_text) = if header_fits {
                            if let Some((tag_str, tag_style)) = ref_badge {
                                line_spans.push(Span::styled(format!("{} ", tag_str), tag_style));
                            }
                            (None, entry.description)
                        } else {
                            (ref_badge, entry.description)
                        };
                        lines.push(Line::from(line_spans));

                        // 描述内容及折叠行前缀
                        // 非 root 节点有向下的轨道 `  │   `；root 节点底部无轨，用 6 空格缩进对齐 `      `
                        let cont_indent_w = 6;
                        let avail_desc_w = inner_width.saturating_sub(cont_indent_w).max(1);

                        let desc_chunks = wrap_text(&desc_text, avail_desc_w, avail_desc_w);

                        for (ci, chunk) in desc_chunks.into_iter().enumerate() {
                            let mut cont_spans = Vec::new();
                            if !is_root {
                                cont_spans.push(Span::styled(
                                    format!("  {}", rail),
                                    Style::default().fg(self.theme.border_inactive),
                                ));
                                cont_spans.push(Span::raw("   "));
                            } else {
                                cont_spans.push(Span::raw("      "));
                            }

                            // 如果第一行没放下 ref_badge，在折叠第一段加上
                            if ci == 0 {
                                if let Some((tag_str, tag_style)) = ref_badge_on_line2 {
                                    cont_spans
                                        .push(Span::styled(format!("{} ", tag_str), tag_style));
                                }
                            }

                            cont_spans
                                .push(Span::styled(chunk, Style::default().fg(self.theme.fg)));
                            lines.push(Line::from(cont_spans));
                        }
                    }

                    let is_last_item_overall =
                        group_idx == display_groups.len() - 1 && item_idx == group_events.len() - 1;
                    if show_rail_lines && !is_last_item_overall {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", rail),
                            Style::default().fg(self.theme.border_inactive),
                        )));
                    }
                }

                if group_idx == 0 && collapsed_count > 0 {
                    if show_rail_lines {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", rail),
                            Style::default().fg(self.theme.border_inactive),
                        )));
                    }
                    let collapse_msg = tr!(
                        self.lang,
                        "... (折叠 {} 项更早记录 · 按 Z 展开时间线)",
                        "... (collapsed {} earlier events · press Z to expand)",
                        collapsed_count
                    );
                    let prefix_str = format!("  {} ", collapsed_rail);
                    let prefix_w = UnicodeWidthStr::width(prefix_str.as_str());
                    let avail_w = inner_width.saturating_sub(prefix_w).max(1);
                    let c_chunks = wrap_text(&collapse_msg, avail_w, avail_w);
                    for chunk in c_chunks {
                        let mut c_spans = Vec::new();
                        c_spans.push(Span::styled(
                            format!("  {} ", collapsed_rail),
                            Style::default().fg(self.theme.border_inactive),
                        ));
                        c_spans.push(Span::styled(
                            chunk,
                            Style::default()
                                .fg(self.theme.accent)
                                .add_modifier(Modifier::ITALIC),
                        ));
                        lines.push(Line::from(c_spans));
                    }
                    if show_rail_lines {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", rail),
                            Style::default().fg(self.theme.border_inactive),
                        )));
                    }
                }
            }

            if self.timeline_expanded && total > 6 && available < total * 2 {
                let collapse_hint = tr!(
                    self.lang,
                    "  ┊ ▲ (已展开全部时间线 · 按 Z 收起)",
                    "  ┊ ▲ (all events expanded · press Z to collapse)"
                );
                lines.push(Line::from(Span::styled(
                    collapse_hint,
                    Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }

        // 检查单管理模式（ChecklistFocus）的操作提示，吸引注意并告知按键。
        if self.mode == Mode::ChecklistFocus {
            lines.push(Line::from(""));
            let hint = tr!(
                self.lang,
                "▶ 检查单管理：j/k 移动 · Space 勾选 · d 删除 · J/K 排序 · e 改名 · Tab 退出",
                "▶ managing: j/k move · Space tick · d delete · J/K reorder · e rename · Tab exit"
            );
            let chunks = wrap_text(hint, inner_width, inner_width.saturating_sub(2));
            for (ci, chunk) in chunks.into_iter().enumerate() {
                let prefix = if ci == 0 { "" } else { "  " };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, chunk),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                )));
            }
        }

        lines
    }
}
