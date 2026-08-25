use crate::model::event;
use crate::time;
use crate::tui::app::{App, Mode};
use crate::tui::icons::Icon;
use crate::tui::status_cn;
use crate::tui::ui;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

impl<'a> App<'a> {
    /// 详情面板内容行。
    pub(super) fn detail_lines(
        &self,
        d: &crate::tui::app::DetailData,
        width: u16,
    ) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = vec![];

        // 标题
        lines.push(Line::from(vec![
            Span::styled(
                crate::tr!(self.lang, "标题: ", "Title: "),
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                d.task.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));

        // 状态 / 归档（归档任务只展示原因与时间，不再显示已完成/逾期）
        if let Some(reason) = &d.task.archive_reason {
            let reason_cn = match reason.as_str() {
                "completed" => crate::tr!(self.lang, "[完成]", "[Done]"),
                "deleted" => crate::tr!(self.lang, "[删除]", "[Deleted]"),
                _ => crate::tr!(self.lang, "[归档]", "[Archived]"),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, "归档: ", "Archived: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("{} {}", reason_cn, time::format_local(d.task.archived_at)),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]));
        } else {
            let st_color = ui::status_color(&d.task.status, self.theme.is_dark);
            lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, "状态: ", "Status: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    status_cn(self.lang, d.task.status),
                    Style::default().fg(st_color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // 截止时间
        lines.push(Line::from(vec![
            Span::styled(
                crate::tr!(self.lang, "截止: ", "Due: "),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::raw(time::format_local(d.task.due_at)),
        ]));

        // 计划时间：具体时间显示排程起点；循环任务显示最近一次计划执行日期
        if d.task.scheduled_start_at.is_some()
            || d.task.scheduled_end_at.is_some()
            || (d.task.rrule.is_some()
                && (d.task.scheduled_start_at.is_some() || d.task.due_at.is_some()))
        {
            let planned = if d.task.rrule.is_some() {
                // 循环任务：错过 slot 取最近一次已错过（逾期），否则下一次执行
                match crate::schedule::effective_due(&d.task) {
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
            lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, "计划: ", "Planned: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(planned),
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
            lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, "循环: ", "Rrule: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(shown_rr),
            ]));
        }

        // 标签
        if !d.tags.is_empty() {
            let mut tag_spans = vec![Span::styled(
                crate::tr!(self.lang, "标签: ", "Tags: "),
                Style::default().fg(self.theme.text_dim),
            )];
            for (i, tg) in d.tags.iter().enumerate() {
                let c = ui::priority_color(&tg.name).unwrap_or(self.theme.hl_fg);
                tag_spans.push(Span::styled(
                    format!("@{}", tg.name),
                    Style::default().fg(c),
                ));
                if i < d.tags.len() - 1 {
                    tag_spans.push(Span::raw(" "));
                }
            }
            lines.push(Line::from(tag_spans));
        }

        // 委派
        if let Some(del) = &d.task.delegated_to {
            lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, "委派: ", "Delegated: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(del.clone()),
            ]));
        }

        // 检查单
        if !d.task.checklist.is_empty() {
            lines.push(Line::from(Span::styled(
                crate::tr!(self.lang, "检查单:", "Checklist:"),
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
                let prefix = if is_cursor { "▶ " } else { "  " };
                lines.push(Line::from(Span::styled(
                    format!("{}{} {}", prefix, check, item.title),
                    Style::default().fg(c).add_modifier(if is_cursor {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                )));
            }
            // 全勾选提示（不自动改任务状态）：引导用户用 x 标记完成。
            if d.task.checklist.iter().all(|i| i.done) {
                lines.push(Line::from(Span::styled(
                    crate::tr!(
                        self.lang,
                        "✓ 所有步骤已完成 — 按 x 标记任务完成",
                        "✓ all steps done — press x to complete the task"
                    ),
                    Style::default().fg(self.theme.text_success),
                )));
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
                    crate::tr!(self.lang, "专注: ", "Focus: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("{} ({})", tomatoes, pomo_count),
                    Style::default().fg(self.theme.text_urgent),
                ),
            ]));
        }

        // 分隔线
        lines.push(Line::from("─".repeat((width.saturating_sub(4)) as usize)));

        // 备注
        if d.task.notes.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                crate::tr!(self.lang, "备注: -", "Notes: -"),
                Style::default().fg(self.theme.text_dim),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                crate::tr!(self.lang, "备注:", "Notes:"),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for ln in d.task.notes.split('\n') {
                lines.push(Line::from(format!("  {}", ln)));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            crate::tr!(self.lang, "时间线", "Timeline"),
            Style::default().add_modifier(Modifier::UNDERLINED),
        )));
        for e in d.events.iter().rev().take(6).rev() {
            let event_cn = match e.event_type.as_str() {
                "created" => crate::tr!(self.lang, "创建", "created"),
                "status_change" => crate::tr!(self.lang, "流转", "changed"),
                event::EV_COMPLETED => crate::tr!(self.lang, "完成", "completed"),
                event::EV_ARCHIVED => crate::tr!(self.lang, "归档", "archived"),
                event::EV_POMODORO => crate::tr!(self.lang, "专注", "focus"),
                event::EV_HABIT_COMPLETED => crate::tr!(self.lang, "习惯", "habit"),
                event::EV_RESTORED => crate::tr!(self.lang, "恢复", "restored"),
                _ => &e.event_type,
            };

            let from_cn = e
                .from_status
                .as_deref()
                .unwrap_or("-")
                .parse::<crate::model::task::Status>()
                .map(|s| status_cn(self.lang, s))
                .unwrap_or("-");
            let to_cn = e
                .to_status
                .as_deref()
                .unwrap_or("-")
                .parse::<crate::model::task::Status>()
                .map(|s| status_cn(self.lang, s))
                .unwrap_or("-");

            let action = if e.event_type == "status_change" {
                format!("{} -> {}", from_cn, to_cn)
            } else if e.event_type == event::EV_POMODORO {
                "🍅 +1".to_string()
            } else {
                "".to_string()
            };

            lines.push(Line::from(format!(
                "  {} {} {}",
                time::format_local(Some(e.at)),
                event_cn,
                action
            )));
        }

        // 检查单管理模式（ChecklistFocus）的操作提示，吸引注意并告知按键。
        if self.mode == Mode::ChecklistFocus {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "▶ 检查单管理：j/k 移动 · Space 勾选 · d 删除 · J/K 排序 · e 改名 · Tab 退出",
                    "▶ managing: j/k move · Space tick · d delete · J/K reorder · e rename · Tab exit"
                ),
                Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
            )));
        }

        lines
    }
}
