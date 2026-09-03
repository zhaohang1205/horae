//! TUI 渲染，按渲染职责拆分：
//! - [`banners`] — 顶部横幅（周回顾进度 / 番茄成就）与状态栏。
//! - [`input`] — 输入/编辑弹层、快速录入实时解析预览与 Tab 补全下拉。
//! - [`popups`] — 确认弹层、今日任务/到期提醒弹窗与专注模式进度环。
//! - [`help`] — 语法说明面板与左侧引导栏内容。
//! - [`detail`] — 右侧任务详情面板内容。
//!
//! `AppRender` trait 与其实现（视图分发、三栏布局、列表/工作流/详情/抽屉）
//! 是一个整体，留在本文件；各子模块通过 `pub(super)` 方法供其调用。

use super::app::{App, Mode, Pane, View};
use super::icons::Icon;
use super::keys::{ctx_of, help_rows, KeyGroup};
use super::ui::build_list_items;
use ratatui::symbols::border;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, Paragraph},
    Frame,
};

pub(crate) trait AppRender {
    fn render(&mut self, f: &mut Frame);
    fn render_focus_mode(&mut self, f: &mut Frame, area: Rect);
    fn render_help_drawer(&self, f: &mut Frame, area: Rect);
    fn render_syntax_drawer(&self, f: &mut Frame, area: Rect);
    fn centered_rect(&self, percent_x: u16, percent_y: u16, r: Rect) -> Rect;
    fn render_guide(&self, f: &mut Frame, area: Rect);
    fn render_list(&mut self, f: &mut Frame, area: Rect);
    fn render_workflow(&self, f: &mut Frame, area: Rect);
    fn workflow_lines(&self) -> Vec<Line<'static>>;
    fn render_workflow_side(&self, f: &mut Frame, area: Rect);
    fn workflow_side_lines(&self) -> Vec<Line<'static>>;
    fn render_detail(&mut self, f: &mut Frame, area: Rect);
}

impl<'a> AppRender for App<'a> {
    fn render(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // ── 番茄专注模式：全屏接管 ──
        {
            let is_active = self.pomo.phase != horae_core::model::pomodoro::Phase::Idle;
            if is_active {
                self.hide_pomo_banner = false; // reset so it shows up next time it becomes Idle
                self.render_focus_mode(f, size);
                return;
            }
        }

        // ── 快速录入纯净专注模式：全屏接管，消除多栏与横幅等背景干扰 ──
        if self.is_zen_capturing() {
            f.render_widget(ratatui::widgets::Clear, size);
            let bg_block = Block::default().style(Style::default().bg(self.theme.bg));
            f.render_widget(bg_block, size);

            // 屏幕最底部单行 Zen 快捷键指示
            let hint_line = Line::from(vec![
                Span::styled(
                    " [Enter] ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tr!(self.lang, "保存入库", "save"),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    "  •  [Tab] ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tr!(self.lang, "智能补全", "complete"),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    "  •  [Ctrl+P] ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if self.show_syntax {
                        tr!(self.lang, "收起语法", "hide syntax")
                    } else {
                        tr!(self.lang, "语法速查", "syntax")
                    },
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    "  •  [Esc] ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tr!(self.lang, "退出", "exit"),
                    Style::default().fg(self.theme.text_dim),
                ),
            ]);
            let hint_area = Rect {
                x: 0,
                y: size.height.saturating_sub(1),
                width: size.width,
                height: 1,
            };
            f.render_widget(
                Paragraph::new(hint_line).alignment(Alignment::Center),
                hint_area,
            );

            if self.show_syntax {
                let (_, syntax_area_opt) = self.capture_layout_geometry(size);
                if let Some(syntax_area) = syntax_area_opt {
                    self.render_syntax_drawer(f, syntax_area);
                }
            }

            self.render_input_overlay(f, size);

            if self.popup.is_some() {
                self.render_popups(f, size);
            }
            return;
        }

        let main_area = self.render_banners(f, size);

        self.list_state.select(Some(self.selected));
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(main_area);

        // 三栏：引导栏 | 列表 | 详情
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(22),
                Constraint::Percentage(46),
                Constraint::Percentage(32),
            ])
            .split(chunks[0]);

        self.render_guide(f, body[0]);
        self.render_list(f, body[1]);
        self.render_detail(f, body[2]);

        self.render_status_bar(f, chunks[1]);

        if self.show_syntax {
            let syntax_area = if self.mode.is_input() {
                let (_, syntax_area_opt) = self.capture_layout_geometry(size);
                syntax_area_opt.unwrap_or_else(|| self.centered_rect(76, 30, size))
            } else {
                self.centered_rect(76, 30, size)
            };
            self.render_syntax_drawer(f, syntax_area);
        }

        match self.mode {
            Mode::ConfirmArchive | Mode::ConfirmPurge | Mode::ConfirmProfileDelete => {
                self.render_confirm_overlay(f, size);
            }
            Mode::Normal | Mode::Visual | Mode::ChecklistFocus => {}
            _ => self.render_input_overlay(f, size),
        }

        if self.show_help {
            let help_area = self.centered_rect(72, size.height.saturating_sub(4), size);
            self.render_help_drawer(f, help_area);
        }

        if self.popup.is_some() {
            self.render_popups(f, size);
        }
    }

    fn render_focus_mode(&mut self, f: &mut Frame, area: Rect) {
        use horae_core::model::pomodoro::Phase;

        let pomo = &self.pomo;
        let now = horae_core::time::now_ms();

        // ── 时间计算 ──
        let start_ts = pomo.start_ts.unwrap_or(now);
        let end_ts = pomo.end_ts.unwrap_or(now);
        let total_ms = (end_ts - start_ts).max(1) as f64;
        let elapsed_fraction = ((now - start_ts) as f64 / total_ms).clamp(0.0, 1.0);

        let diff_secs = ((end_ts - now) / 1000).max(0);
        let mins = diff_secs / 60;
        let secs = diff_secs % 60;
        let time_str = format!("{:02}:{:02}", mins, secs);

        // ── 阶段配色（正统 Catppuccin：Red / Green / Teal）──
        let (phase_icon, ring_color, dim_color, bg_color) = match &pomo.phase {
            Phase::Work => (
                tr!(self.lang, "🍅 专注", "🍅 Focus"),
                Color::Rgb(243, 139, 168), // Red
                mix_toward(self.theme.bg, Color::Rgb(243, 139, 168), 0.55),
                self.theme.bg,
            ),
            Phase::ShortBreak => (
                tr!(self.lang, "☕ 小休", "☕ Short break"),
                Color::Rgb(166, 227, 161), // Green
                mix_toward(self.theme.bg, Color::Rgb(166, 227, 161), 0.55),
                self.theme.bg,
            ),
            Phase::LongBreak => (
                tr!(self.lang, "🌿 长休", "🌿 Long break"),
                Color::Rgb(148, 226, 213), // Teal
                mix_toward(self.theme.bg, Color::Rgb(148, 226, 213), 0.55),
                self.theme.bg,
            ),
            Phase::Idle => return,
        };

        // ── 检查单子项与上下文卡片判定 ──
        let active_task = pomo
            .task_id
            .as_deref()
            .and_then(|tid| horae_core::repo::tasks::get(self.conn, tid).ok());

        let has_checklist = active_task
            .as_ref()
            .map(|t| !t.checklist.is_empty())
            .unwrap_or(false);

        let show_checklist = has_checklist && area.height >= 26;
        let checklist_constraint = if show_checklist {
            Constraint::Length(2)
        } else {
            Constraint::Length(1)
        };

        // ── 布局（垂直分区）──
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // 顶部留白
                Constraint::Length(2), // 任务标题
                Constraint::Min(8),    // Canvas 番茄进度环
                Constraint::Length(7), // 大数字倒计时
                checklist_constraint,  // 检查单行内卡片 / 留白
                Constraint::Length(1), // 统计栏
                Constraint::Length(1), // 操作提示
            ])
            .split(area);

        // ── 1. 当前任务与状态 ──
        let task_title = pomo
            .task_title
            .as_deref()
            .unwrap_or(tr!(self.lang, "无标题", "untitled"));
        let title_line = Line::from(vec![
            Span::styled(
                format!(" {} ", phase_icon),
                Style::default().fg(ring_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                task_title,
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        f.render_widget(
            Paragraph::new(title_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[1],
        );

        // ── 2. Canvas 圆形进度环 (点阵图案) ──
        self.render_focus_ring(
            f,
            rows[2],
            elapsed_fraction,
            ring_color,
            dim_color,
            bg_color,
        );

        // ── 3. 大数字倒计时 (秒针呼吸联动) ──
        let blink = secs % 2 == 0;
        let big_lines = build_big_time(&time_str, ring_color, bg_color, blink);
        f.render_widget(
            Paragraph::new(big_lines)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[3],
        );

        // ── 4. 检查单行内沉浸式步骤卡片 ──
        if show_checklist {
            if let Some(ref task) = active_task {
                let total = task.checklist.len();
                let done = task.checklist.iter().filter(|i| i.done).count();
                let next_item = task.checklist.iter().find(|i| !i.done);

                let filled = (done * 5).checked_div(total).unwrap_or(0).min(5);
                let empty = 5 - filled;
                let bar = format!(
                    "[{}{}] {}/{}",
                    "■".repeat(filled),
                    "□".repeat(empty),
                    done,
                    total
                );

                let item_text = if let Some(item) = next_item {
                    format!("▶ [ ] {}", item.title)
                } else {
                    tr!(self.lang, "✓ 所有子项已达成", "✓ All steps completed").to_string()
                };

                let cl_line = Line::from(vec![
                    Span::styled(
                        bar,
                        Style::default()
                            .fg(if done == total {
                                self.theme.text_success
                            } else {
                                ring_color
                            })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        item_text,
                        Style::default()
                            .fg(self.theme.fg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]);

                f.render_widget(
                    Paragraph::new(cl_line)
                        .alignment(Alignment::Center)
                        .style(Style::default().bg(bg_color)),
                    rows[4],
                );
            }
        }

        // ── 5. 克制的统计信息 & 快捷键 ──
        let hints = if matches!(pomo.phase, Phase::ShortBreak | Phase::LongBreak) {
            tr!(
                self.lang,
                "{} [Space/P] 下一轮  |  [S] 结束专注",
                "{} [Space/P] next round  |  [S] end focus",
                self.icon(Icon::Active)
            )
        } else if has_checklist {
            tr!(
                self.lang,
                "{} [Space/=] 打卡子项  |  [x] 完成任务  |  [S] 停止番茄钟",
                "{} [Space/=] Tick step  |  [x] Complete task  |  [S] Stop pomodoro",
                self.icon(Icon::Active)
            )
        } else {
            tr!(
                self.lang,
                "{} [x] 完成任务  |  [S] 停止番茄钟",
                "{} [x] Complete task  |  [S] Stop pomodoro",
                self.icon(Icon::Active)
            )
        };

        let stats_line = Line::from(vec![
            Span::styled(
                tr!(
                    self.lang,
                    " 🏆 今日完成: {} ",
                    " 🏆 Today: {} ",
                    pomo.today_count
                ),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::styled(" • ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                tr!(self.lang, " 🔥 连击: {} ", " 🔥 Streak: {} ", pomo.streak),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);

        f.render_widget(
            Paragraph::new(stats_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[5],
        );

        f.render_widget(
            Paragraph::new(hints)
                .alignment(Alignment::Center)
                .style(Style::default().fg(self.theme.border_inactive).bg(bg_color)),
            rows[6],
        );
    }

    fn render_help_drawer(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        f.render_widget(ratatui::widgets::Clear, area);
        let keys_block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(self.theme.accent))
            .title(tr!(
                self.lang,
                " 快捷键指南 (F1/?) · hjkl 滚动 · Esc 关闭 ",
                " Shortcuts (F1/?) · hjkl scroll · Esc close "
            ));

        let ctx = ctx_of(self);
        let rows = help_rows(&ctx, self.lang);
        let mut lines: Vec<Line> = Vec::new();
        let mut last_group: Option<KeyGroup> = None;
        for (g, k, desc, applicable) in rows {
            if last_group != Some(g) {
                lines.push(Line::from(Span::styled(
                    g.title(self.lang),
                    Style::default()
                        .fg(self.theme.text_dim)
                        .add_modifier(Modifier::BOLD),
                )));
                last_group = Some(g);
            }
            if applicable {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:>6} ", k),
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(desc),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:>6} ", k),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(desc, Style::default().fg(self.theme.text_dim)),
                ]));
            }
        }

        let content_h = area.height.saturating_sub(2) as usize;
        let scroll = self.help_scroll.min(lines.len().saturating_sub(content_h));
        f.render_widget(
            Paragraph::new(lines)
                .scroll((scroll as u16, 0))
                .block(keys_block),
            area,
        );
    }

    fn render_syntax_drawer(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        f.render_widget(ratatui::widgets::Clear, area);
        let scroll_hint = if self.syntax_scroll > 0 {
            tr!(self.lang, " · PgUp/PgDn 翻卷", " · PgUp/PgDn")
        } else {
            ""
        };
        let title_str = format!(
            "{}{}",
            tr!(
                self.lang,
                " 语法说明指南 (Ctrl+P 收起) ",
                " Syntax guide (Ctrl+P to close) "
            ),
            scroll_hint
        );
        let syntax_block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(self.theme.accent))
            .title(title_str);
        let syntax = self.syntax_lines();
        let para = Paragraph::new(syntax)
            .block(syntax_block)
            .scroll((self.syntax_scroll as u16, 0))
            .wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(para, area);
    }

    fn centered_rect(&self, percent_x: u16, height: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(r.height.saturating_sub(height) / 2),
                Constraint::Length(height),
                Constraint::Min(0),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }

    fn render_guide(&self, f: &mut ratatui::Frame, area: Rect) {
        let lines = self.guide_lines(area);
        let border_color = if self.pane == Pane::Left {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .padding(ratatui::widgets::Padding::horizontal(1))
                    .border_style(Style::default().fg(border_color))
                    .title(tr!(self.lang, " 引导 ", " Guide ")),
            ),
            area,
        );
    }

    fn render_list(&mut self, f: &mut ratatui::Frame, area: Rect) {
        if self.view == View::Workflow {
            self.render_workflow(f, area);
            return;
        }
        let border_color = if self.pane == Pane::Center {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        let items = build_list_items(self);
        let title = tr!(
            self.lang,
            " 任务 · {}{} ",
            " Tasks · {}{} ",
            super::view_label(self.lang, self.view),
            if let Some(ref tf) = self.tag_filter {
                format!(" [@{}]", tf)
            } else {
                String::new()
            }
        );
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .padding(ratatui::widgets::Padding::horizontal(1))
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            )
            .highlight_style(if self.theme.is_dark {
                // 暗色主题：活动行深蓝背景（与标签亮蓝区分），不覆盖任务文本原色。
                if self.pane == Pane::Center {
                    Style::default().bg(self.theme.row_active_bg)
                } else {
                    Style::default().bg(self.theme.hl_bg)
                }
            } else if self.pane == Pane::Center {
                Style::default()
                    .bg(self.theme.hl_bg)
                    .fg(self.theme.hl_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(self.theme.hl_bg)
            });
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// GTD 工作流与决策树视图（中心面板）。
    fn render_workflow(&self, f: &mut ratatui::Frame, area: Rect) {
        let border_color = if self.pane == Pane::Center {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        let title = tr!(
            self.lang,
            " GTD 工作流 · 决策树与五步闭环 ",
            " GTD Workflow · Decision Tree & 5 Steps "
        );
        let lines = self.workflow_lines();
        let content_h = area.height.saturating_sub(2) as usize;
        let top_pad = if content_h > lines.len() {
            ((content_h - lines.len()) / 3).min(3) as u16
        } else {
            0
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::new(1, 1, top_pad, 0))
            .border_style(Style::default().fg(border_color))
            .title(title);
        let usable_h = content_h.saturating_sub(top_pad as usize);
        let scroll = self
            .workflow_scroll
            .min(lines.len().saturating_sub(usable_h));
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .scroll((scroll as u16, 0)),
            area,
        );
    }

    /// GTD 决策树与五步闭环说明文本（中英双语，精炼版）。
    fn workflow_lines(&self) -> Vec<Line<'static>> {
        use Icon::*;
        let s_step = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let s_dim = Style::default().fg(self.theme.text_dim);
        let s_map = Style::default().fg(self.theme.hl_fg);
        let s_urgent = Style::default()
            .fg(self.theme.text_urgent)
            .add_modifier(Modifier::BOLD);
        let s_success = Style::default()
            .fg(self.theme.text_success)
            .add_modifier(Modifier::BOLD);
        let s_bold = Style::default().add_modifier(Modifier::BOLD);

        let step = |s: &'static str| Span::styled(s.to_string(), s_step);
        let dim = |s: &'static str| Span::styled(s.to_string(), s_dim);
        let map = |s: &'static str| Span::styled(s.to_string(), s_map);
        let urgent = |s: &'static str| Span::styled(s.to_string(), s_urgent);

        let mut lines: Vec<Line> = Vec::new();

        // 1. 收集
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", self.icon(Inbox)), s_step),
            step(tr!(self.lang, "1. 收集 (Capture)", "1. Capture")),
        ]));
        lines.push(Line::from(vec![
            dim("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "杂事 100% 入收件箱 (",
                    "Capture 100% stuff into Inbox ("
                ),
                s_dim,
            ),
            map(tr!(self.lang, "1 / a", "1 / a")),
            Span::styled(
                tr!(self.lang, ")，清空大脑，不留杂念。", "). Free your mind."),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 2. 厘清与决策树
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", self.icon(Next)), s_step),
            step(tr!(self.lang, "2. 厘清 (Clarify)", "2. Clarify")),
            dim("  ──  "),
            Span::styled(
                tr!(self.lang, "逐条判定：是否可行动？", "Is it actionable?"),
                s_bold,
            ),
        ]));
        // 不可行动
        lines.push(Line::from(vec![
            dim(tr!(self.lang, "   ├── 否 (No)  ──→ ", "   ├── No → ")),
            urgent(tr!(self.lang, "🗑 归档", "🗑 Archive")),
            dim(tr!(self.lang, " (8/A)  ·  ", " (8/A)  ·  ")),
            map(tr!(self.lang, "💡 将来也许", "💡 Someday")),
            dim(tr!(self.lang, " (5/s)  ·  ", " (5/s)  ·  ")),
            map(tr!(self.lang, "📚 参考资料", "📚 Reference")),
            dim(tr!(self.lang, " (6/0)", " (6/0)")),
        ]));
        // 可行动
        lines.push(Line::from(vec![
            dim(tr!(self.lang, "   └── 是 (Yes) ──→ ", "   └── Yes → ")),
            Span::styled(
                tr!(
                    self.lang,
                    "多步骤立项目 (Shift+C 检查单)",
                    "Multi-step? Project + Checklist (Shift+C)"
                ),
                s_success,
            ),
        ]));
        lines.push(Line::from(vec![
            dim(tr!(
                self.lang,
                "       ├── 耗时 < 2分钟 → ⚡ ",
                "       ├── < 2 min → ⚡ "
            )),
            urgent(tr!(
                self.lang,
                "立即做完 (两分钟原则)",
                "Do it now (2-Minute Rule)"
            )),
        ]));
        lines.push(Line::from(vec![
            dim(tr!(
                self.lang,
                "       ├── 委派他人 → 👥 ",
                "       ├── Delegate → 👥 "
            )),
            map(tr!(self.lang, "等待中 (3/w)", "Waiting For (3/w)")),
        ]));
        lines.push(Line::from(vec![dim(tr!(
            self.lang,
            "       └── 延迟/我来做 (Defer/Do):",
            "       └── Defer/Do:"
        ))]));
        lines.push(Line::from(vec![
            dim(tr!(
                self.lang,
                "           ├── 固定时间 → 📅 ",
                "           ├── Specific time → 📅 "
            )),
            map(tr!(
                self.lang,
                "已排程 (4/~/* 习惯循环)",
                "Scheduled (4/~/* Habits)"
            )),
        ]));
        lines.push(Line::from(vec![
            dim(tr!(
                self.lang,
                "           └── 尽快执行 → 🎯 ",
                "           └── ASAP → 🎯 "
            )),
            map(tr!(
                self.lang,
                "下一步行动 (2/Enter)",
                "Next Actions (2/Enter)"
            )),
        ]));
        lines.push(Line::from(""));

        // 3. 组织
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", self.icon(Scheduled)), s_step),
            step(tr!(self.lang, "3. 组织 (Organize)", "3. Organize")),
        ]));
        lines.push(Line::from(vec![
            dim("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "情境标签 (@work/@focus) 与 任务优先级 (!high/!medium/!low 或 !1/!2/!3)。",
                    "Context tags (@work/@focus) & task priority (!high/!medium/!low or !1/!2/!3)."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 4. 回顾
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", self.icon(Review)), s_step),
            step(tr!(self.lang, "4. 回顾 (Reflect)", "4. Reflect")),
        ]));
        lines.push(Line::from(vec![
            dim("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "晨间 horae do 智能聚焦  ·  晚间清零 (Inbox Zero)  ·  周末按 r 周回顾。",
                    "Morning horae do  ·  Evening Inbox Zero  ·  Weekly review (r)."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 5. 执行
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", self.icon(Done)), s_step),
            step(tr!(self.lang, "5. 执行 (Engage)", "5. Engage")),
        ]));
        lines.push(Line::from(vec![
            dim("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "四维选事法则 (情境 / 时间 / 精力 / 优先级)  ·  按 P 开启番茄专注钟。",
                    "Pick actions by 4-criteria (Context/Time/Energy/Priority)  ·  P for Pomodoro."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            tr!(
                self.lang,
                "💡 提示：按 h/l 切换面板，按 j/k 滚动，按 g/G 置顶/置底。",
                "Tip: press h/l to switch panels, j/k to scroll, g/G for top/bottom."
            ),
            s_dim,
        )));

        lines
    }

    /// GTD 哲学与 David Allen 介绍（右侧面板）。
    fn render_workflow_side(&self, f: &mut ratatui::Frame, area: Rect) {
        let border_color = if self.pane == Pane::Right {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        let title = tr!(
            self.lang,
            " David Allen · GTD 核心哲学 ",
            " David Allen · GTD Philosophy "
        );
        let lines = self.workflow_side_lines();
        let content_h = area.height.saturating_sub(2) as usize;
        let top_pad = if content_h > lines.len() {
            ((content_h - lines.len()) / 3).min(3) as u16
        } else {
            0
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::new(1, 1, top_pad, 0))
            .border_style(Style::default().fg(border_color))
            .title(title);
        let usable_h = content_h.saturating_sub(top_pad as usize);
        let scroll = self
            .workflow_side_scroll
            .min(lines.len().saturating_sub(usable_h));
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .scroll((scroll as u16, 0)),
            area,
        );
    }

    /// David Allen 介绍与 GTD 核心哲学文本（中英双语，100 字精炼版）。
    #[allow(clippy::vec_init_then_push)]
    fn workflow_side_lines(&self) -> Vec<Line<'static>> {
        let s_sec = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let s_quote_title = Style::default()
            .fg(self.theme.rrule_fg)
            .add_modifier(Modifier::BOLD);
        let s_quote_text = Style::default()
            .fg(self.theme.text_urgent)
            .add_modifier(Modifier::ITALIC);
        let s_dim = Style::default().fg(self.theme.text_dim);
        let s_bold = Style::default().add_modifier(Modifier::BOLD);

        let mut lines: Vec<Line> = Vec::new();

        // 导师简介
        lines.push(Line::from(Span::styled(
            tr!(self.lang, "◆ 导师简介 (About)", "◆ About David Allen"),
            s_sec,
        )));
        lines.push(Line::from(vec![
            Span::styled(
                tr!(
                    self.lang,
                    "David Allen (1945~)：",
                    "David Allen (born 1945): "
                ),
                s_bold,
            ),
            Span::styled(
                tr!(
                    self.lang,
                    "GTD (Getting Things Done / 尽管去做) 创始人，全球顶尖效能导师，《时代周刊》效率大师。",
                    "Creator of Getting Things Done (GTD) and world-renowned productivity pioneer."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 核心哲学
        lines.push(Line::from(Span::styled(
            tr!(
                self.lang,
                "◆ 核心心法 (Core Principles)",
                "◆ Core Principles"
            ),
            s_sec,
        )));
        lines.push(Line::from(""));

        // 心法 1：大脑是 CPU 不是硬盘
        lines.push(Line::from(vec![
            Span::styled("🧠 ", s_bold),
            Span::styled(
                tr!(
                    self.lang,
                    "大脑是 CPU，不是硬盘",
                    "Mind for ideas, not holding them"
                ),
                s_quote_title,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "“Your mind is for having ideas, not holding them.”",
                    "“Your mind is for having ideas, not holding them.”"
                ),
                s_quote_text,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "将一切杂事 100% 外部化到系统，彻底清空工作记忆与认知负荷。",
                    "Externalize all loops to system to free mental bandwidth and stress."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 心法 2：心如止水
        lines.push(Line::from(vec![
            Span::styled("🌊 ", s_bold),
            Span::styled(
                tr!(self.lang, "心如止水 (Mind Like Water)", "Mind Like Water"),
                s_quote_title,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "对纷至沓来的事务恰如其分地反应，事毕即复归平静与深度专注。",
                    "Respond appropriately to input, then return to calm and deep focus."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 心法 3：两分钟原则
        lines.push(Line::from(vec![
            Span::styled("⚡ ", s_bold),
            Span::styled(
                tr!(self.lang, "两分钟原则 (2-Minute Rule)", "2-Minute Rule"),
                s_quote_title,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "耗时 < 2 分钟的事立即动手做完，不留任何推迟、排程与记录开销。",
                    "If an action takes less than 2 minutes, do it immediately."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 心法 4：聚焦物理下一步
        lines.push(Line::from(vec![
            Span::styled("🎯 ", s_bold),
            Span::styled(
                tr!(
                    self.lang,
                    "明确物理下一步 (Next Physical Action)",
                    "Next Physical Action"
                ),
                s_quote_title,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "拖延往往源于步骤模糊。将复杂任务拆解为无需思考即可执行的第一步。",
                    "Clarify the immediate next visible physical activity to eliminate friction."
                ),
                s_dim,
            ),
        ]));
        lines.push(Line::from(""));

        // 心法 5：多维高度俯瞰
        lines.push(Line::from(vec![
            Span::styled("🔭 ", s_bold),
            Span::styled(
                tr!(
                    self.lang,
                    "多维专注高度 (Horizons of Focus)",
                    "Horizons of Focus"
                ),
                s_quote_title,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                tr!(
                    self.lang,
                    "5万英尺(愿景与人生目标) → 跑道(日常微行动)，自上而下对齐，自下而上掌控。",
                    "50,000 ft (Vision) to Runway (Actions) — align top-down, control bottom-up."
                ),
                s_dim,
            ),
        ]));

        lines
    }

    fn render_detail(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        if self.view == View::Workflow {
            self.render_workflow_side(f, area);
            return;
        }
        let border_color = if self.pane == Pane::Right {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(border_color))
            .title(tr!(self.lang, " 任务详情 ", " Task Details "));

        match &self.detail {
            None => {
                let empty_para = Paragraph::new(tr!(self.lang, " 未选中任务", " No task selected"))
                    .style(Style::default().fg(self.theme.text_dim))
                    .block(block);
                f.render_widget(empty_para, area);
            }
            Some(d) => {
                let lines = self.detail_lines(d, area.width);
                let para = Paragraph::new(lines)
                    .style(Style::default().fg(self.theme.fg))
                    .block(block)
                    .wrap(ratatui::widgets::Wrap { trim: false });
                f.render_widget(para, area);
            }
        }
    }
}

// ── 大数字字体辅助（5 行 × 4 列，纯 █ 字符）──

/// 将 `over` 朝 `base` 方向按比例 `t`（0..1）混合，得到与背景协调的暗化/亮化色。
/// 用于番茄钟进度环的“轨道”底色，保证两者同属一套 Catppuccin 色相。
fn mix_toward(base: Color, over: Color, t: f32) -> Color {
    if let (Color::Rgb(br, bg, bb), Color::Rgb(or, og, ob)) = (base, over) {
        let m = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t).round() as u8;
        Color::Rgb(m(br, or), m(bg, og), m(bb, ob))
    } else {
        over
    }
}

fn big_digit_rows(c: char, blink: bool) -> [&'static str; 5] {
    match c {
        '0' => [" ██ ", "█  █", "█  █", "█  █", " ██ "],
        '1' => [" ▐█ ", " ██ ", "  █ ", "  █ ", " ███"],
        '2' => [" ██ ", "   █", " ██ ", "█   ", "████"],
        '3' => ["███ ", "   █", " ██ ", "   █", "███ "],
        '4' => ["█  █", "█  █", "████", "   █", "   █"],
        '5' => ["████", "█   ", "███ ", "   █", "███ "],
        '6' => [" ██ ", "█   ", "███ ", "█  █", " ██ "],
        '7' => ["████", "   █", "  █ ", " █  ", " █  "],
        '8' => [" ██ ", "█  █", " ██ ", "█  █", " ██ "],
        '9' => [" ██ ", "█  █", " ███", "   █", " ██ "],
        ':' if blink => ["    ", " ██ ", "    ", " ██ ", "    "],
        ':' => ["    ", "    ", "    ", "    ", "    "],
        _ => ["    ", "    ", "    ", "    ", "    "],
    }
}

/// 将形如 "23:45" 的字符串渲染为 5 行大数字（每行是一个 Span）。
fn build_big_time(s: &str, color: Color, bg: Color, blink: bool) -> Vec<Line<'static>> {
    let chars: Vec<char> = s.chars().collect();
    let mut rows: [String; 5] = Default::default();
    for &c in &chars {
        let digit = big_digit_rows(c, blink);
        for (i, part) in digit.iter().enumerate() {
            rows[i].push_str(part);
            rows[i].push(' '); // 字符间距
        }
    }
    rows.into_iter()
        .map(|row| {
            Line::from(Span::styled(
                row,
                Style::default()
                    .fg(color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

mod banners;
mod detail;
mod help;
mod input;
mod popups;
