use super::AppRender;
use crate::tui::app::{App, Mode};
use ratatui::symbols::border;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::canvas::{Canvas, Points},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

impl<'a> App<'a> {
    /// 批量操作确认弹层：归档 / 永久删除的居中醒目确认框。
    pub(super) fn render_confirm_overlay(&mut self, f: &mut Frame, size: Rect) {
        let (title, desc) = match self.mode {
            Mode::ConfirmPurge => (
                tr!(
                    self.lang,
                    " ⚠ 确认永久删除 ",
                    " ⚠ Confirm permanent delete "
                ),
                tr!(
                    self.lang,
                    "将永久删除 {} 项，不可恢复。",
                    "Permanently delete {} item(s). This cannot be undone.",
                    self.pending_purge_ids.len()
                ),
            ),
            Mode::ConfirmProfileDelete => (
                tr!(
                    self.lang,
                    " ⚠ 确认删除 profile ",
                    " ⚠ Confirm delete profile "
                ),
                tr!(
                    self.lang,
                    "从配置移除 profile `{}`（数据库文件保留）。",
                    "Remove profile `{}` from config (db file kept).",
                    self.pending_profile_delete.as_deref().unwrap_or("")
                ),
            ),
            _ => (
                tr!(self.lang, " ⚠ 确认归档 ", " ⚠ Confirm archive "),
                tr!(
                    self.lang,
                    "将归档 {} 项。",
                    "Archive {} item(s).",
                    self.pending_archive_ids.len()
                ),
            ),
        };

        let lines = vec![
            Line::from(Span::styled(
                format!(" {}", desc),
                Style::default().fg(self.theme.fg),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    tr!(self.lang, "  [y/Enter] ", "  [y/Enter] "),
                    Style::default()
                        .fg(self.theme.text_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, "确认  ", "Confirm  ")),
                Span::styled(
                    tr!(self.lang, "[n/Esc] ", "[n/Esc] "),
                    Style::default()
                        .fg(self.theme.text_urgent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, "取消", "Cancel")),
            ]),
        ];
        let height = lines.len() as u16 + 2;
        let area = self.centered_rect(58, height, size);

        f.render_widget(ratatui::widgets::Clear, area);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(
                Style::default()
                    .fg(self.theme.text_urgent)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// 弹出框：模块开关 / 任务到期提醒。
    pub(super) fn render_popups(&mut self, f: &mut Frame, size: Rect) {
        let Some(ref popup) = self.popup else { return };
        match popup {
            crate::tui::app::Popup::ModuleToggles(idx) => {
                let area = self.centered_rect(44, 20, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(self.lang, " 模块显示设置 ", " Module Visibility "))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let mut items = vec![];
                let nerd = matches!(self.icon_style, crate::tui::icons::IconStyle::Nerd);
                let icons_label = if nerd {
                    tr!(self.lang, "图标 (Nerd Font)", "Icons (Nerd Font)")
                } else {
                    tr!(self.lang, "图标 (ASCII 回退)", "Icons (ASCII fallback)")
                };
                let is_ref = matches!(
                    self.completion_style,
                    crate::tui::app::completion::CompletionStyle::Reference
                );
                let completion_label = self.completion_style.label(self.lang);
                let opts = [
                    (self.modules.splash, "开屏页 (Splash)"),
                    (self.modules.reference, "6 参考资料 (Reference)"),
                    (self.modules.done, "7 已完成 (Done)"),
                    (self.modules.archived, "8 归档箱 (Archived)"),
                    (self.modules.tags, "9 标签库 (Tags)"),
                    (self.quotes.enabled, "0 金句 (Quotes)"),
                    (self.modules.review, "r 周回顾 (Review)"),
                    (self.modules.settings, "M 设置 (Settings)"),
                    (nerd, icons_label),
                    (
                        self.start_in_capture,
                        tr!(
                            self.lang,
                            "启动即快速录入 (Capture)",
                            "Start in capture mode"
                        ),
                    ),
                    (is_ref, completion_label),
                    (
                        self.zen_capture,
                        tr!(
                            self.lang,
                            "纯净录入无干扰 (Zen)",
                            "Zen capture (no distractions)"
                        ),
                    ),
                    (
                        self.lunar_enabled,
                        tr!(
                            self.lang,
                            "农历与节气提醒 (Lunar)",
                            "Lunar & holiday reminders (Lunar)"
                        ),
                    ),
                ];
                for (i, (enabled, name)) in opts.iter().enumerate() {
                    let checkbox = if *enabled { "[x]" } else { "[ ]" };
                    let mut style = Style::default();
                    if i == *idx {
                        style = style
                            .fg(self.theme.bg)
                            .bg(self.theme.accent)
                            .add_modifier(Modifier::BOLD);
                    } else if !*enabled {
                        style = style.fg(self.theme.text_dim);
                    }
                    items.push(ratatui::widgets::ListItem::new(Line::from(Span::styled(
                        format!(" {} {} ", checkbox, name),
                        style,
                    ))));
                }

                let list = ratatui::widgets::List::new(items).block(block);
                f.render_widget(list, area);
            }
            crate::tui::app::Popup::TaskDueNow(_, title) => {
                let mut lines = vec![Line::from(tr!(
                    self.lang,
                    "有任务已到期，需要立即处理：",
                    "A task is due now, handle it now:"
                ))];
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!(" 「{}」 ", title),
                    Style::default()
                        .fg(self.theme.text_urgent)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        " [Enter] 一键进入番茄钟  |  [Esc] 忽略 ",
                        " [Enter] start pomodoro  |  [Esc] dismiss "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                let area = self.centered_rect(50, 10, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(self.lang, " ⏰ 任务提醒! ", " ⏰ Task due! "))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.text_urgent));
                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
        }
    }

    /// 专注模式的圆形进度环 Canvas（点阵图案 + 外围刻度）。
    pub(super) fn render_focus_ring(
        &self,
        f: &mut Frame,
        canvas_area: Rect,
        elapsed_fraction: f64,
        ring_color: Color,
        dim_color: Color,
        bg_color: Color,
    ) {
        let cw = canvas_area.width as f64;
        let ch = canvas_area.height as f64;
        let y_range = (100.0 * ch * 2.0 / cw).max(10.0);
        let cx_c = 50.0_f64;
        let cy_c = y_range / 2.0;
        let max_r = cy_c.min(50.0) * 0.88;
        let outer_r = max_r;
        let inner_r = max_r * 0.68;

        let ef = elapsed_fraction;
        let rc = ring_color;
        let dc = dim_color;
        let bgc = bg_color;

        let canvas = Canvas::default()
            .marker(ratatui::symbols::Marker::Braille)
            .x_bounds([0.0, 100.0])
            .y_bounds([0.0, y_range])
            .background_color(bgc)
            .paint(move |ctx| {
                let steps = 1440_usize;
                let ring_steps = ((outer_r - inner_r) * 3.0) as usize + 1;

                let mut rem_pts: Vec<(f64, f64)> = Vec::with_capacity(steps * ring_steps);
                let mut ela_pts: Vec<(f64, f64)> = Vec::with_capacity(steps * ring_steps);

                for i in 0..steps {
                    let angle_deg = i as f64 * 360.0 / steps as f64;
                    let angle_rad = (90.0_f64 - angle_deg).to_radians();
                    let frac = angle_deg / 360.0;

                    for ri in 0..=ring_steps {
                        let r = inner_r + ri as f64 * (outer_r - inner_r) / ring_steps as f64;
                        let x = cx_c + r * angle_rad.cos();
                        let y = cy_c + r * angle_rad.sin();
                        if !(0.5..=99.5).contains(&x) || y < 0.5 || y > y_range - 0.5 {
                            continue;
                        }
                        if frac >= ef {
                            rem_pts.push((x, y));
                        } else {
                            ela_pts.push((x, y));
                        }
                    }
                }
                ctx.draw(&Points {
                    coords: &ela_pts,
                    color: dc,
                });
                ctx.draw(&Points {
                    coords: &rem_pts,
                    color: rc,
                });

                // 外围刻度点
                let mut tick_pts = vec![];
                for t in 0..12 {
                    let deg = t as f64 * 30.0;
                    let rad = (90.0_f64 - deg).to_radians();
                    for ri in 0..=3 {
                        let r = outer_r + 2.0 + ri as f64 * 0.8;
                        let x = cx_c + r * rad.cos();
                        let y = cy_c + r * rad.sin();
                        tick_pts.push((x, y));
                    }
                }
                ctx.draw(&Points {
                    coords: &tick_pts,
                    color: self.theme.text_dim,
                });
            });
        f.render_widget(canvas, canvas_area);
    }
}
