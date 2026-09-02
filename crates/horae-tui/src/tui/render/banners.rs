use crate::tui::app::{App, Mode};
use crate::tui::icons::Icon;
use crate::tui::keys::status_strip;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

impl<'a> App<'a> {
    /// 顶部横幅：每周回顾进度条，或番茄“成就结清”提示。返回剩余主区域。
    pub(super) fn render_banners(&mut self, f: &mut Frame, size: Rect) -> Rect {
        let mut main_area = size;
        if self.is_reviewing {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(size);

            let step_names = [
                "",
                tr!(self.lang, "清空收件箱", "Clear Inbox"),
                tr!(self.lang, "追踪等待事项", "Follow up Waiting"),
                tr!(self.lang, "重估将来/也许", "Re-evaluate Someday"),
                tr!(self.lang, "检视已完成", "Review Done"),
            ];
            let step_name = step_names.get(self.review_step as usize).unwrap_or(&"");

            let banner = Paragraph::new(Line::from(Span::styled(
                tr!(
                    self.lang,
                    " 🌟 每周回顾 第 {}/4 步: {} (按 'R' 进入下一步, 'Esc' 退出) ",
                    " 🌟 Weekly Review step {}/4: {} ('R' next, 'Esc' exit) ",
                    self.review_step,
                    step_name
                ),
                Style::default()
                    .bg(self.theme.accent)
                    .fg(self.theme.bg)
                    .add_modifier(Modifier::BOLD),
            )));
            f.render_widget(banner, chunks[0]);
            main_area = chunks[1];
        } else if !self.hide_pomo_banner {
            let pomo = &self.pomo;
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let today_active = pomo.last_date.as_deref() == Some(today.as_str());
            // 仅在休息刚结束的窗口内提示“再接再厉”，避免当天每次启动都常驻旧横幅
            let break_prompt_visible = pomo.break_prompt_visible(horae_core::time::now_ms());

            if today_active && pomo.today_count > 0 && break_prompt_visible {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(0)])
                    .split(size);

                let last_title = pomo.last_completed_task_title.as_deref().unwrap_or(tr!(
                    self.lang,
                    "上一任务",
                    "last task"
                ));
                let banner = Paragraph::new(Line::from(vec![
                    Span::styled(
                        tr!(
                            self.lang,
                            " {} 成就结清: 今日已积 {} 个番茄 (Streak {} 连击!)  |  ",
                            " {} Settled: {} tomatoes today (Streak {})  |  ",
                            self.icon(Icon::Achievement),
                            pomo.today_count,
                            pomo.streak
                        ),
                        Style::default()
                            .fg(self.theme.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        tr!(
                            self.lang,
                            "休息已完成  |  再接再厉? {} [Space/P] 开启新一轮专注 [{}] ",
                            "Break done  |  Go again? {} [Space/P] start a new focus [{}] ",
                            self.icon(Icon::Active),
                            last_title
                        ),
                        Style::default()
                            .fg(self.theme.bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().bg(self.theme.text_success));

                f.render_widget(banner, chunks[0]);
                main_area = chunks[1];
            }
        }
        main_area
    }

    /// 状态栏（单行）：[MODE] + 内容区(消息 / F2 提示 / 全局条) | [horae]。
    pub(super) fn render_status_bar(&mut self, f: &mut Frame, area: Rect) {
        let mode_str = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Visual => " VISUAL ",
            Mode::ChecklistFocus => tr!(self.lang, " 检查单 ", " CHECKLIST "),
            _ => " INSERT ",
        };
        let mode_bg = match self.mode {
            Mode::Normal => self.theme.text_success,
            Mode::Visual => self.theme.accent,
            Mode::ChecklistFocus => self.theme.accent,
            _ => self.theme.text_urgent,
        };
        let mode_fg = self.theme.bg;

        let status_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(7)])
            .split(area);

        // 内容区三态：消息(支持 Toast 高亮) > F2 关闭提示 > 全局键条。
        let mut content_spans: Vec<Span> = Vec::new();
        let now_ms = horae_core::time::now_ms();
        let is_active_toast = self.toast.as_ref().is_some_and(|t| {
            now_ms - t.created_at_ms < t.duration_ms && t.message == self.status_message
        });

        if !self.status_message.is_empty() {
            let (fg, bold) = if is_active_toast {
                let is_success = self.toast.as_ref().is_none_or(|t| t.is_success);
                (
                    if is_success {
                        self.theme.text_success
                    } else {
                        self.theme.accent
                    },
                    Modifier::BOLD,
                )
            } else {
                (self.theme.status_fg, Modifier::empty())
            };
            content_spans.push(Span::styled(
                format!(" {}", self.status_message),
                Style::default()
                    .fg(fg)
                    .bg(self.theme.status_bg)
                    .add_modifier(bold),
            ));
        } else if !self.show_shortcut_bar {
            content_spans.push(Span::styled(
                tr!(
                    self.lang,
                    "按 F2 显示快捷键条",
                    "Press F2 to show shortcut bar"
                ),
                Style::default()
                    .fg(self.theme.text_dim)
                    .bg(self.theme.status_bg),
            ));
        } else {
            content_spans.push(Span::styled(
                " ⌘ ",
                Style::default()
                    .fg(self.theme.accent)
                    .bg(self.theme.status_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            let items = status_strip(self.lang);
            for (i, (k, d)) in items.iter().enumerate() {
                // 两种颜色交替，视觉上间隔开每一条。
                let key_color = if i % 2 == 0 {
                    self.theme.accent
                } else {
                    self.theme.text_success
                };
                content_spans.push(Span::styled(
                    format!("{} {}", k, d),
                    Style::default()
                        .fg(key_color)
                        .bg(self.theme.status_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                if i + 1 < items.len() {
                    content_spans.push(Span::styled(
                        " · ",
                        Style::default()
                            .fg(self.theme.text_dim)
                            .bg(self.theme.status_bg),
                    ));
                }
            }
        }

        let mut status_spans = vec![Span::styled(
            mode_str,
            Style::default()
                .fg(mode_fg)
                .bg(mode_bg)
                .add_modifier(Modifier::BOLD),
        )];
        status_spans.extend(content_spans);
        f.render_widget(
            Paragraph::new(Line::from(status_spans))
                .style(Style::default().bg(self.theme.status_bg)),
            status_layout[0],
        );
        f.render_widget(
            Paragraph::new(Span::styled(
                " horae ",
                Style::default()
                    .fg(self.theme.bg)
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            status_layout[1],
        );
    }
}
