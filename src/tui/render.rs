use super::app::{pad_right, App, Mode, Pane, View};
use super::icons::Icon;
use super::keys::{ctx_of, help_rows, status_strip, strip_keys, KeyGroup};
use super::status_cn;
use super::ui;
use super::ui::build_list_items;
use crate::model::event;
use crate::parser::{
    parse_quick_add, parse_rrule_shorthand, priority_letter, tokenize_quick_add, QuickAddKind,
};
use crate::time;
use ratatui::symbols::border;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Points},
        Block, Borders, List, Paragraph,
    },
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
    fn render_detail(&mut self, f: &mut Frame, area: Rect);
}

impl<'a> AppRender for App<'a> {
    fn render(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // ── 番茄专注模式：全屏接管 ──
        {
            let is_active = self.pomo.phase != crate::model::pomodoro::Phase::Idle;
            if is_active {
                self.hide_pomo_banner = false; // reset so it shows up next time it becomes Idle
                self.render_focus_mode(f, size);
                return;
            }
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

        match self.mode {
            Mode::ConfirmArchive | Mode::ConfirmPurge | Mode::ConfirmProfileDelete => {
                self.render_confirm_overlay(f, size);
            }
            Mode::Normal | Mode::Visual | Mode::ChecklistFocus => {}
            _ => self.render_input_overlay(f, size),
        }

        if self.show_syntax {
            // 当 show_syntax 为 true 时，如果处于输入/编辑模式，则将语法面板放右半屏实现“左右双开”；否则居中
            let syntax_area = if self.mode.is_input() {
                Rect {
                    x: size.width * 50 / 100,
                    y: size.height / 10,
                    width: size.width * 46 / 100,
                    height: (size.height * 80 / 100).min(30),
                }
            } else {
                self.centered_rect(76, 30, size)
            };
            self.render_syntax_drawer(f, syntax_area);
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
        use crate::model::pomodoro::Phase;

        let pomo = &self.pomo;
        let now = crate::time::now_ms();

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
                crate::tr!(self.lang, "🍅 专注", "🍅 Focus"),
                Color::Rgb(243, 139, 168), // Red
                mix_toward(self.theme.bg, Color::Rgb(243, 139, 168), 0.55),
                self.theme.bg,
            ),
            Phase::ShortBreak => (
                crate::tr!(self.lang, "☕ 小休", "☕ Short break"),
                Color::Rgb(166, 227, 161), // Green
                mix_toward(self.theme.bg, Color::Rgb(166, 227, 161), 0.55),
                self.theme.bg,
            ),
            Phase::LongBreak => (
                crate::tr!(self.lang, "🌿 长休", "🌿 Long break"),
                Color::Rgb(148, 226, 213), // Teal
                mix_toward(self.theme.bg, Color::Rgb(148, 226, 213), 0.55),
                self.theme.bg,
            ),
            Phase::Idle => return,
        };

        // ── 全屏背景 ──
        f.render_widget(Block::default().style(Style::default().bg(bg_color)), area);

        // ── 布局（垂直分区）──
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // 顶部留白
                Constraint::Length(2), // 任务标题
                Constraint::Min(10),   // Canvas 番茄进度环
                Constraint::Length(7), // 大数字倒计时
                Constraint::Length(1), // 留白
                Constraint::Length(1), // 统计栏
                Constraint::Length(1), // 操作提示
            ])
            .split(area);

        // ── 1. 当前任务与状态 ──
        let task_title =
            pomo.task_title
                .as_deref()
                .unwrap_or(crate::tr!(self.lang, "无标题", "untitled"));
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

        // ── 4. 克制的统计信息 & 快捷键 ──
        let hints = if matches!(pomo.phase, Phase::ShortBreak | Phase::LongBreak) {
            crate::tr!(
                self.lang,
                "{} [Space/P] 下一轮  |  [S] 结束专注",
                "{} [Space/P] next round  |  [S] end focus",
                self.icon(Icon::Active)
            )
        } else {
            crate::tr!(
                self.lang,
                "{} [S] 停止番茄钟",
                "{} [S] stop pomodoro",
                self.icon(Icon::Active)
            )
        };

        let stats_line = Line::from(vec![
            Span::styled(
                crate::tr!(
                    self.lang,
                    " 🏆 今日完成: {} ",
                    " 🏆 Today: {} ",
                    pomo.today_count
                ),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::styled(" • ", Style::default().fg(self.theme.text_dim)),
            Span::styled(
                crate::tr!(self.lang, " 🔥 连击: {} ", " 🔥 Streak: {} ", pomo.streak),
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
            .title(crate::tr!(
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
        let syntax_block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(self.theme.accent))
            .title(crate::tr!(
                self.lang,
                " 语法说明指南 (Ctrl+P) ",
                " Syntax guide (Ctrl+P) "
            ));
        let syntax = self.syntax_lines();
        let para = Paragraph::new(syntax)
            .block(syntax_block)
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
                    .title(crate::tr!(self.lang, " 引导 ", " Guide ")),
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
        let title = crate::tr!(
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

    /// GTD 工作流说明视图（中心面板）。五步法 + 与当前 horae 视图的映射。
    fn render_workflow(&self, f: &mut ratatui::Frame, area: Rect) {
        let border_color = if self.pane == Pane::Center {
            self.theme.accent
        } else {
            self.theme.text_dim
        };
        let title = crate::tr!(self.lang, " GTD 工作流 ", " GTD Workflow ");
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(border_color))
            .title(title);
        let lines = self.workflow_lines();
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .scroll((0, 0)),
            area,
        );
    }

    /// GTD 工作流说明文本（中英文双语）。每个步骤给出含义与对应 horae 视图。
    fn workflow_lines(&self) -> Vec<Line<'static>> {
        use Icon::*;
        let s_title = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let s_step = Style::default()
            .fg(self.theme.text_success)
            .add_modifier(Modifier::BOLD);
        let s_dim = Style::default().fg(self.theme.text_dim);
        let s_map = Style::default().fg(self.theme.hl_fg);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "GTD (Getting Things Done) 五步法",
                    "GTD (Getting Things Done) — 5 steps"
                ),
                s_title,
            )),
            Line::from(""),
            Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "核心思想：把脑子里的杂事全部倒出，按情境分类，只在合适的时候做合适的行动。",
                    "Core idea: empty your head, sort by context, and do the right action at the right time."
                ),
                s_dim,
            )),
            Line::from(""),
        ];

        let steps: [(&str, &str, Icon, &str, &str); 5] = [
            (
                crate::tr!(self.lang, "1. 收集 Capture", "1. Capture"),
                crate::tr!(
                    self.lang,
                    "把所有涌上心头的待办、想法、承诺一股脑放进收件箱，先不判断。",
                    "Dump every todo, idea and commitment into the inbox without judging."
                ),
                Inbox,
                crate::tr!(self.lang, "收件箱", "Inbox"),
                "1",
            ),
            (
                crate::tr!(self.lang, "2. 理清 Clarify", "2. Clarify"),
                crate::tr!(
                    self.lang,
                    "逐个问：这是什么？是否可执行？不可执行的丢弃 / 留作参考 / 延后。",
                    "Ask each: what is it? actionable? If not, drop / keep as reference / defer."
                ),
                Next,
                crate::tr!(self.lang, "下一步", "Next"),
                "2",
            ),
            (
                crate::tr!(self.lang, "3. 组织 Organize", "3. Organize"),
                crate::tr!(
                    self.lang,
                    "把可执行事项按情境分类：下一步 / 等待中 / 已排程 / 将来也许 / 参考资料。",
                    "Sort actionable items by context: Next / Waiting / Scheduled / Someday / Reference."
                ),
                Scheduled,
                crate::tr!(self.lang, "等待/排程/将来/参考", "Waiting/Scheduled/Someday/Ref"),
                "3,4,5,6",
            ),
            (
                crate::tr!(self.lang, "4. 回顾 Reflect", "4. Reflect"),
                crate::tr!(
                    self.lang,
                    "每周回顾，清空收件箱、跟进等待事项、重估将来也许，保持系统可信。",
                    "Weekly review: clear inbox, follow up waiting, re-evaluate someday to keep trust."
                ),
                Review,
                crate::tr!(self.lang, "周回顾", "Review"),
                "r",
            ),
            (
                crate::tr!(self.lang, "5. 执行 Engage", "5. Engage"),
                crate::tr!(
                    self.lang,
                    "按情境与精力挑选行动，专注推进，完成后归档或标记完成。",
                    "Pick actions by context and energy; focus, then archive or mark done."
                ),
                Done,
                crate::tr!(self.lang, "已完成 / 归档箱", "Done / Archived"),
                "7,8",
            ),
        ];

        for (title, desc, icon, view_name, key) in steps {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", self.icon(icon)),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(title, s_step),
            ]));
            lines.push(Line::from(Span::styled(format!("   {}", desc), s_dim)));
            lines.push(Line::from(vec![
                Span::styled("   ", s_dim),
                Span::styled(
                    crate::tr!(self.lang, "→ horae 视图: ", "→ horae view: "),
                    s_dim,
                ),
                Span::styled(view_name.to_string(), s_map),
                Span::styled(crate::tr!(self.lang, "  (键 {})", "  (key {})", key), s_dim),
            ]));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            crate::tr!(
                self.lang,
                "提示：按 a 快速捕获，按 w 设为等待，按 x 标记完成，按 r 开始周回顾。",
                "Tip: press a to capture, w to wait, x to complete, r for weekly review."
            ),
            s_dim,
        )));
        lines
    }

    fn render_detail(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
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
            .title(crate::tr!(self.lang, " 任务详情 ", " Task Details "));

        match &self.detail {
            None => {
                let empty_para =
                    Paragraph::new(crate::tr!(self.lang, " 未选中任务", " No task selected"))
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

impl<'a> App<'a> {
    fn capture_input_line(&self) -> Line<'a> {
        let input = &self.input;
        let mut spans: Vec<Span<'a>> = Vec::new();
        spans.push(Span::raw(" "));
        let mut prev = 0usize;
        for tok in tokenize_quick_add(input) {
            if tok.start > prev {
                spans.push(Span::raw(input[prev..tok.start].to_string()));
            }
            let style = match tok.kind {
                QuickAddKind::Tag => Style::default()
                    .fg(self.theme.hl_fg)
                    .add_modifier(Modifier::BOLD),
                QuickAddKind::Time => Style::default().fg(self.theme.text_success),
                QuickAddKind::Rrule => Style::default().fg(self.theme.rrule_fg),
                QuickAddKind::Priority => {
                    let tag =
                        crate::parser::priority_tag(&input[tok.start + 1..tok.end]).unwrap_or("");
                    Style::default()
                        .fg(crate::tui::ui::priority_color(tag).unwrap_or(self.theme.hl_fg))
                        .add_modifier(Modifier::BOLD)
                }
                QuickAddKind::Title => Style::default(),
            };
            spans.push(Span::styled(input[tok.start..tok.end].to_string(), style));
            prev = tok.end;
        }
        if prev < input.len() {
            spans.push(Span::raw(input[prev..].to_string()));
        }
        Line::from(spans)
    }
}

impl<'a> App<'a> {
    /// 顶部横幅：每周回顾进度条，或番茄“成就结清”提示。返回剩余主区域。
    fn render_banners(&mut self, f: &mut Frame, size: Rect) -> Rect {
        let mut main_area = size;
        if self.is_reviewing {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(size);

            let step_names = [
                "",
                crate::tr!(self.lang, "清空收件箱", "Clear Inbox"),
                crate::tr!(self.lang, "追踪等待事项", "Follow up Waiting"),
                crate::tr!(self.lang, "重估将来/也许", "Re-evaluate Someday"),
                crate::tr!(self.lang, "检视已完成", "Review Done"),
            ];
            let step_name = step_names.get(self.review_step as usize).unwrap_or(&"");

            let banner = Paragraph::new(Line::from(Span::styled(
                crate::tr!(
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

            if today_active && pomo.today_count > 0 {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(0)])
                    .split(size);

                let last_title = pomo
                    .last_completed_task_title
                    .as_deref()
                    .unwrap_or(crate::tr!(self.lang, "上一任务", "last task"));
                let banner = Paragraph::new(Line::from(vec![
                    Span::styled(
                        crate::tr!(
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
                        crate::tr!(
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
    fn render_status_bar(&mut self, f: &mut Frame, area: Rect) {
        let mode_str = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Visual => " VISUAL ",
            Mode::ChecklistFocus => crate::tr!(self.lang, " 检查单 ", " CHECKLIST "),
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

        // 内容区三态：消息 > F2 关闭提示 > 全局键条。
        let mut content_spans: Vec<Span> = Vec::new();
        if !self.status_message.is_empty() {
            content_spans.push(Span::styled(
                format!(" {}", self.status_message),
                Style::default()
                    .fg(self.theme.status_fg)
                    .bg(self.theme.status_bg),
            ));
        } else if !self.show_shortcut_bar {
            content_spans.push(Span::styled(
                crate::tr!(
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
                    format!("{:<3} {}", k, d),
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

    /// 输入/编辑模式的弹层：标题 + 文本行（快速录入带实时解析预览）。
    fn render_input_overlay(&mut self, f: &mut Frame, size: Rect) {
        let title = match self.mode {
            Mode::Search => {
                crate::tr!(
                    self.lang,
                    " 搜索任务 (标题 / 备注) ",
                    " Search Tasks (Title / Notes) "
                )
            }
            Mode::Capturing => {
                if self.organizing_id.is_some() {
                    crate::tr!(
                        self.lang,
                        " 组织: 编辑 标题 @标签 ~时间 *周期 (空/Esc 跳过) ",
                        " Organize: edit title @tags ~time *rrule (empty/Esc to skip) "
                    )
                } else if self.view == View::Quotes && self.quotes.enabled {
                    crate::tr!(
                        self.lang,
                        " 快速录入金句 (自动 @quote 入库, 支持 @标签 及 Tab 补全) ",
                        " Quick capture quote (auto @quote, @tag and Tab to complete) "
                    )
                } else {
                    crate::tr!(
                        self.lang,
                        " 快速录入 (支持 @标签 及 Tab 补全: home, work, errands, quick, focus...) ",
                        " Quick capture (@tag, Tab to complete: home, work, errands, quick, focus...) "
                    )
                }
            }
            Mode::Tagging => crate::tr!(
                self.lang,
                " 添加标签 [支持 Tab 补全] (预设: home, work, errands, quick, focus...) ",
                " Add tags [Tab to complete] (presets: home, work, errands, quick, focus...) "
            ),
            Mode::WaitingWho => {
                crate::tr!(self.lang, " 等待谁/什么? ", " Waiting for who/what? ")
            }
            Mode::WaitingWhen => crate::tr!(
                self.lang,
                " 提醒时间? (如 +1d, tomorrow 10:00) ",
                " Reminder time? (e.g. +1d, tomorrow 10:00) "
            ),
            Mode::ChecklistAdding => {
                crate::tr!(self.lang, " 新增检查单 ", " Add checklist item ")
            }
            Mode::FilteringTag => {
                crate::tr!(self.lang, " 过滤标签 (情境) ", " Filter by tag (Context) ")
            }
            Mode::CreatingTag => crate::tr!(
                self.lang,
                " 新增自定义标签 (输入标签名称，按 Enter 保存) ",
                " Create custom tag (enter name, Enter to save) "
            ),
            Mode::ConfiguringPomo => crate::tr!(
                self.lang,
                " 自定义番茄钟时长 (格式: 工作分钟;短休分钟;长休分钟, 如 25;5;15) ",
                " Custom pomodoro lengths (format: work;short;long, e.g. 25;5;15) "
            ),
            Mode::CreatingProfile => crate::tr!(
                self.lang,
                " 新建 profile (输入名称，如 work / personal / prod1) ",
                " New profile (enter name, e.g. work / personal / prod1) "
            ),
            Mode::RenamingProfile => crate::tr!(
                self.lang,
                " 重命名 profile (输入新名称) ",
                " Rename profile (enter new name) "
            ),
            Mode::RenamingChecklist => {
                crate::tr!(
                    self.lang,
                    " 改名检查项 (Enter 保存) ",
                    " Rename item (Enter to save) "
                )
            }
            Mode::Normal
            | Mode::Visual
            | Mode::ChecklistFocus
            | Mode::ConfirmArchive
            | Mode::ConfirmPurge
            | Mode::ConfirmProfileDelete => "",
        };

        let mut text_lines: Vec<Line> = Vec::new();
        let width = if self.mode == Mode::Capturing { 70 } else { 50 };

        // 输入行（首行）：快速录入带语法高亮，其余为纯文本行。行首固定一个空格。
        let mut input_line_display = if self.mode == Mode::Capturing {
            self.capture_input_line()
        } else {
            Line::from(format!(" {}", self.input))
        };
        // ghost 建议：当前候选尚未输入的部分，以灰字追加在光标后。
        if self.completion_active() {
            if let Some((_, _, ghost)) = self.completion_ghost() {
                if !ghost.is_empty() {
                    input_line_display.spans.push(Span::styled(
                        ghost,
                        Style::default().fg(self.theme.text_dim),
                    ));
                }
            }
        }

        if self.mode == Mode::Capturing {
            text_lines.push(input_line_display);
            text_lines.push(Line::from(""));
            if self.input.trim().is_empty() {
                text_lines.push(self.capture_syntax_hint_line());
            } else {
                text_lines.extend(self.capture_preview_lines());
                // 语法提示常驻：输入/编辑过程中始终可见，便于快速学习语法。
                text_lines.push(Line::from(""));
                text_lines.push(self.capture_syntax_hint_line());
            }
        } else {
            text_lines.push(input_line_display);
        }

        let height = text_lines.len() as u16 + 2;

        // 输入框区域：show_syntax 时居左靠上，否则居中。
        let area = if self.show_syntax {
            Rect {
                x: size.width / 20,
                y: size.height / 10,
                width: (size.width * 42 / 100).min(65),
                height,
            }
        } else {
            self.centered_rect(width, height, size)
        };

        // 输入区可用宽度：框宽 - 左右边框 - 左右内边距。
        let inner_width = area.width.saturating_sub(4) as usize;
        // 光标所在列（含行首空格）的显示宽度。
        let cursor_col =
            1 + unicode_width::UnicodeWidthStr::width(&self.input[..self.input_cursor]);
        // 横向滚动：保证光标始终可见。
        let scroll_x = cursor_col.saturating_sub(inner_width) as u16;

        f.render_widget(ratatui::widgets::Clear, area);
        let block_style = if self.show_syntax {
            if self.pane == Pane::Right {
                self.theme.border_active
            } else {
                self.theme.border_inactive
            }
        } else {
            self.theme.accent
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(block_style));
        f.render_widget(
            Paragraph::new(text_lines)
                .block(block)
                .scroll((0, scroll_x)),
            area,
        );

        // 补全候选下拉层：独立渲染在输入框下方，不抬高输入框本身。
        if !self.completion_candidates.is_empty() {
            self.render_completion_dropdown(f, area);
        }

        // 真实终端插入光标：定位到输入行，框内第一列 = area.x + 1(边框) + 1(内边距)。
        let cursor_x = area.x + 2 + (cursor_col.saturating_sub(scroll_x as usize)) as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }

    /// 补全候选下拉层：紧贴输入框下方渲染，当前候选反色高亮。
    /// 时间候选附解析结果、循环候选附展开 RRULE 作说明。
    fn render_completion_dropdown(&mut self, f: &mut Frame, input_area: Rect) {
        let prefix = self.completion_prefix;
        let candidates = self.completion_candidates.clone();
        let idx = self.completion_index;

        let mut lines: Vec<Line> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            let label = format!(" {}{} ", prefix, c);
            let desc = match prefix {
                '~' => time::parse_time(c)
                    .ok()
                    .map(|ms| time::format_local(Some(ms)))
                    .unwrap_or_default(),
                '*' => crate::parser::parse_rrule_shorthand(c),
                _ => String::new(),
            };
            let is_sel = i == idx;
            let key_style = if is_sel {
                Style::default()
                    .fg(self.theme.bg)
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.fg)
            };
            let mut spans = vec![Span::styled(
                if is_sel {
                    format!("❯{} ", label)
                } else {
                    format!("  {} ", label)
                },
                key_style,
            )];
            if !desc.is_empty() {
                spans.push(Span::styled(desc, Style::default().fg(self.theme.text_dim)));
            }
            lines.push(Line::from(spans));
        }

        let width = input_area.width.max(30);
        let height = lines.len() as u16 + 2;
        let x = input_area.x;
        let y = input_area.y + input_area.height;
        let dd = Rect {
            x,
            y,
            width,
            height,
        };
        if dd.y + dd.height > f.area().height {
            return; // 底部空间不足则不渲染，避免越界
        }
        f.render_widget(ratatui::widgets::Clear, dd);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(self.theme.accent))
            .title(crate::tr!(
                self.lang,
                " 补全 (↑↓/Tab 选择 · Enter 选中 · Esc 取消) ",
                " Complete (↑↓/Tab pick · Enter select · Esc cancel) "
            ));
        f.render_widget(Paragraph::new(lines).block(block), dd);
    }

    /// 批量操作确认弹层：归档 / 永久删除的居中醒目确认框。
    fn render_confirm_overlay(&mut self, f: &mut Frame, size: Rect) {
        let (title, desc) = match self.mode {
            Mode::ConfirmPurge => (
                crate::tr!(
                    self.lang,
                    " ⚠ 确认永久删除 ",
                    " ⚠ Confirm permanent delete "
                ),
                crate::tr!(
                    self.lang,
                    "将永久删除 {} 项，不可恢复。",
                    "Permanently delete {} item(s). This cannot be undone.",
                    self.pending_purge_ids.len()
                ),
            ),
            Mode::ConfirmProfileDelete => (
                crate::tr!(
                    self.lang,
                    " ⚠ 确认删除 profile ",
                    " ⚠ Confirm delete profile "
                ),
                crate::tr!(
                    self.lang,
                    "从配置移除 profile `{}`（数据库文件保留）。",
                    "Remove profile `{}` from config (db file kept).",
                    self.pending_profile_delete.as_deref().unwrap_or("")
                ),
            ),
            _ => (
                crate::tr!(self.lang, " ⚠ 确认归档 ", " ⚠ Confirm archive "),
                crate::tr!(
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
                    crate::tr!(self.lang, "  [y/Enter] ", "  [y/Enter] "),
                    Style::default()
                        .fg(self.theme.text_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(self.lang, "确认  ", "Confirm  ")),
                Span::styled(
                    crate::tr!(self.lang, "[n/Esc] ", "[n/Esc] "),
                    Style::default()
                        .fg(self.theme.text_urgent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(self.lang, "取消", "Cancel")),
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

    /// 快速录入实时解析预览（标题 / 标签 / 时间 / 循环 / 优先级）。
    fn capture_preview_lines(&self) -> Vec<Line<'static>> {
        let parsed = parse_quick_add(&self.input);
        let tokens = tokenize_quick_add(&self.input);
        let mut text_lines: Vec<Line> = Vec::new();
        if !parsed.title.is_empty() {
            text_lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, " 标题: ", " Title: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(parsed.title.clone()),
            ]));
        }
        if !parsed.tags.is_empty() {
            let mut spans = vec![Span::styled(
                crate::tr!(self.lang, " 标签: ", " Tags: "),
                Style::default().fg(self.theme.text_dim),
            )];
            for (i, tag) in parsed.tags.iter().enumerate() {
                spans.push(Span::styled(
                    format!("@{}", tag),
                    Style::default()
                        .fg(self.theme.hl_fg)
                        .add_modifier(Modifier::BOLD),
                ));
                if i + 1 < parsed.tags.len() {
                    spans.push(Span::raw(" "));
                }
            }
            text_lines.push(Line::from(spans));
        }
        if let Some(ref ts) = parsed.time_str {
            let parsed_ms = time::parse_time(ts);
            let (resolved_text, resolved_style) = match &parsed_ms {
                Ok(ms) => (
                    time::format_local(Some(*ms)),
                    Style::default().fg(self.theme.text_dim),
                ),
                Err(_) => (
                    crate::tr!(self.lang, "[无效]", "[invalid]").to_string(),
                    Style::default().fg(self.theme.text_urgent),
                ),
            };
            text_lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, " 时间: ", " Time: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("~{}", ts),
                    if parsed_ms.is_ok() {
                        Style::default().fg(self.theme.text_success)
                    } else {
                        Style::default().fg(self.theme.text_urgent)
                    },
                ),
                Span::raw(" → "),
                Span::styled(resolved_text, resolved_style),
            ]));
        }
        if let Some(raw) = tokens
            .iter()
            .rev()
            .find(|t| t.kind == QuickAddKind::Rrule)
            .map(|t| t.text[1..].to_string())
        {
            let valid = crate::parser::rrule_valid(&raw);
            let resolved = parse_rrule_shorthand(&raw);
            let (resolved_text, resolved_style) = if valid {
                (resolved, Style::default().fg(self.theme.text_dim))
            } else {
                (
                    crate::tr!(self.lang, "[无效循环]", "[invalid rrule]").to_string(),
                    Style::default().fg(self.theme.text_urgent),
                )
            };
            text_lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, " 循环: ", " Rrule: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("*{}", raw),
                    if valid {
                        Style::default().fg(self.theme.rrule_fg)
                    } else {
                        Style::default().fg(self.theme.text_urgent)
                    },
                ),
                Span::raw(" → "),
                Span::styled(resolved_text, resolved_style),
            ]));
        }
        if let Some(p) = &parsed.priority {
            let letter = priority_letter(p).unwrap_or('?');
            let color = crate::tui::ui::priority_color(p).unwrap_or(self.theme.hl_fg);
            text_lines.push(Line::from(vec![
                Span::styled(
                    crate::tr!(self.lang, " 优先级: ", " Priority: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("!{}", letter),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(" → "),
                Span::styled(format!("@{}", p), Style::default().fg(color)),
            ]));
        }
        text_lines
    }

    /// 快速录入语法提示行（常驻于输入/编辑弹层底部）。
    fn capture_syntax_hint_line(&self) -> Line<'static> {
        Line::from(Span::styled(
            crate::tr!(
                self.lang,
                " [语法] @标签 (如 @work)  |  ~时间 (如 ~tomorrow, ~+3d, ~18:00)  |  *循环 (如 *2w[1,3], *m[1,15])  |  !优先级 (如 !a)",
                " [syntax] @tag (@work)  |  ~time (~tomorrow, ~+3d, ~18:00)  |  *rrule (*2w[1,3], *m[1,15])  |  !priority (!a)"
            ),
            Style::default().fg(self.theme.text_dim),
        ))
    }

    /// 弹出框：今日任务概览 / 任务到期提醒。
    fn render_popups(&mut self, f: &mut Frame, size: Rect) {
        let Some(ref popup) = self.popup else { return };
        match popup {
            crate::tui::app::Popup::ModuleToggles(idx) => {
                let area = self.centered_rect(40, 18, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(crate::tr!(
                        self.lang,
                        " 模块显示设置 ",
                        " Module Visibility "
                    ))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let mut items = vec![];
                let nerd = matches!(self.icon_style, crate::tui::icons::IconStyle::Nerd);
                let icons_label = if nerd {
                    crate::tr!(self.lang, "图标 (Nerd Font)", "Icons (Nerd Font)")
                } else {
                    crate::tr!(self.lang, "图标 (ASCII 回退)", "Icons (ASCII fallback)")
                };
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
                        crate::tr!(
                            self.lang,
                            "启动即快速录入 (Capture)",
                            "Start in capture mode"
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
            crate::tui::app::Popup::TodayTasks(tasks) => {
                let mut lines = vec![Line::from(crate::tr!(
                    self.lang,
                    "今日有以下任务需要完成:",
                    "Tasks to complete today:"
                ))];
                lines.push(Line::from(""));
                for t in tasks.iter().take(10) {
                    lines.push(Line::from(format!(" - {}", t)));
                }
                if tasks.len() > 10 {
                    lines.push(Line::from(crate::tr!(
                        self.lang,
                        "   ... 等 {} 个任务",
                        "   ... and {} more",
                        tasks.len()
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    crate::tr!(
                        self.lang,
                        " [按 Enter 或 Esc 键关闭] ",
                        " [Press Enter or Esc to close] "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                let area = self.centered_rect(50, 15, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(crate::tr!(
                        self.lang,
                        " 📅 今日任务概览 ",
                        " 📅 Today's Tasks "
                    ))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));
                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
            crate::tui::app::Popup::TaskDueNow(_, title) => {
                let mut lines = vec![Line::from(crate::tr!(
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
                    crate::tr!(
                        self.lang,
                        " [Enter] 一键进入番茄钟  |  [Esc] 忽略 ",
                        " [Enter] start pomodoro  |  [Esc] dismiss "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                let area = self.centered_rect(50, 10, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(crate::tr!(self.lang, " ⏰ 任务提醒! ", " ⏰ Task due! "))
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
    fn render_focus_ring(
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

    /// 语法说明面板的行内容。
    fn syntax_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "快速录入语法 (按 a 捕获)",
                    "Quick capture syntax (press a)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    crate::tr!(self.lang, "@标签", "@tag"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "    添加情境, 如 ",
                    "    add context, e.g. "
                )),
                Span::styled("@work", Style::default().fg(self.theme.accent)),
                Span::raw(crate::tr!(
                    self.lang,
                    " (支持 Tab 补全)",
                    " (Tab to complete)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    crate::tr!(self.lang, "!优先级", "!priority"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "    设置优先级: ",
                    "    set priority: "
                )),
                Span::styled("!a", Style::default().fg(self.theme.text_urgent)),
                Span::raw(crate::tr!(self.lang, "(高) / ", "(high) / ")),
                Span::styled("!b", Style::default().fg(Color::Rgb(249, 226, 175))),
                Span::raw(crate::tr!(self.lang, "(中) / ", "(medium) / ")),
                Span::styled("!c", Style::default().fg(Color::Rgb(137, 180, 250))),
                Span::raw(crate::tr!(self.lang, "(低)", "(low)")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    crate::tr!(self.lang, "~时间", "~time"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "    设置截止时间, 见下方时间语法",
                    "    set due time, see below"
                )),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(self.lang, "  例: ", "  examples: ")),
                Span::styled(
                    "a买牛奶 @home ~tomorrow",
                    Style::default().fg(self.theme.accent),
                ),
                Span::raw(" / "),
                Span::styled(
                    "a写周报 @work !a ~+3d",
                    Style::default().fg(self.theme.accent),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "时间语法 (~ 排程起点)",
                    "Time syntax (~ schedule start)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "now / +2h +30m +1d +1w",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "    相对时间偏移",
                    "    relative offsets"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "today / tomorrow [HH:MM]",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "  今天/明天指定时刻",
                    "  today/tomorrow at a time"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("HH:MM", Style::default().fg(self.theme.text_success)),
                Span::raw(crate::tr!(
                    self.lang,
                    "                     当天指定时刻, 如 18:00",
                    "                     same-day time, e.g. 18:00"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "YYYY-MM-DD [HH:MM]",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    "        绝对日期与时间",
                    "        absolute date & time"
                )),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                crate::tr!(
                    self.lang,
                    "周期 / 循环任务 (Habit / RRULE)",
                    "Recurring / habit tasks (RRULE)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(crate::tr!(
                    self.lang,
                    "  一句话排程: ",
                    "  one-line schedule: "
                )),
                Span::styled("~明天 15:30", Style::default().fg(self.theme.accent)),
                Span::raw(crate::tr!(
                    self.lang,
                    " 即可设排程起点, 循环任务再补 *rrule",
                    " sets the start time; append *rrule for habits"
                )),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(
                    self.lang,
                    "  快速录入简写: ",
                    "  quick shorthand: "
                )),
                Span::styled("*2w[1,3]", Style::default().fg(self.theme.rrule_fg)),
                Span::raw(crate::tr!(
                    self.lang,
                    " = 每2周周一、周三  (星期用 1-7, 0=周日; 也可写 *mo,we)",
                    " = every 2 weeks Mon,Wed  (days 1-7, 0=Sun; or *mo,we)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "FREQ=DAILY|WEEKLY|MONTHLY",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(self.lang, "   循环频率", "   frequency")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("INTERVAL=2", Style::default().fg(self.theme.text_success)),
                Span::raw(crate::tr!(
                    self.lang,
                    "                  循环间隔 (如每 2 周)",
                    "                  interval (e.g. every 2 weeks)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("BYDAY=SA,SU", Style::default().fg(self.theme.text_success)),
                Span::raw(crate::tr!(
                    self.lang,
                    "                 指定周几 (MO TU WE TH FR SA SU)",
                    "                 days of week (MO TU WE TH FR SA SU)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "COUNT=10 / UNTIL=YYYY-MM-DD",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(crate::tr!(self.lang, " 终止条件", " end conditions")),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(self.lang, "  例: ", "  examples: ")),
                Span::styled(
                    ";FREQ=WEEKLY;BYDAY=SA,SU",
                    Style::default().fg(self.theme.rrule_fg),
                ),
                Span::raw("    "),
                Span::styled(
                    ";FREQ=DAILY;COUNT=30",
                    Style::default().fg(self.theme.rrule_fg),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                crate::tr!(self.lang, "其他操作说明", "Other tips"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(crate::tr!(self.lang, "  等待 ", "  waiting ")),
                Span::styled(
                    "w",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    " 后可填写 [谁/何时], 如 ",
                    " then set [who/when], e.g. "
                )),
                Span::styled("w → Alice → +1d", Style::default().fg(self.theme.accent)),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(self.lang, "  检查单 ", "  checklist ")),
                Span::styled(
                    "C",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(self.lang, " 新增; ", " add; ")),
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    " 逐项管理: j/k 移动, Space 勾选, d 删除, J/K 排序, e 改名",
                    " manage: j/k move, Space tick, d delete, J/K reorder, e rename"
                )),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(
                    self.lang,
                    "  标签库 (视图9): 按 ",
                    "  Tags (view 9): press "
                )),
                Span::styled(
                    "a",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(self.lang, " 动态新增, 按 ", " to add, ")),
                Span::styled(
                    "D",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(self.lang, " 删除", " to delete")),
            ]),
            Line::from(vec![
                Span::raw(crate::tr!(self.lang, "  按 ", "  press ")),
                Span::styled(
                    "Ctrl+P",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(crate::tr!(
                    self.lang,
                    " 弹出/关闭本语法说明指南",
                    " to toggle this guide"
                )),
            ]),
        ]
    }

    /// 左侧引导栏内容：视图分组 + 动态快捷键（按剩余行数截断）。
    fn guide_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = Vec::new();

        if self.total_count() == 0 {
            lines.push(Line::from(Span::styled(
                crate::tr!(self.lang, " 欢迎使用 horae", "  Welcome to horae"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
        }

        let cur = self.view;
        let is_left_pane = self.pane == Pane::Left;
        // 矮终端（如 80×24）收紧组间空行，把行数让给下方的 [Keys] 动态键。
        let spacious = area.height >= 30;

        let mut add_group = |views: &[(char, View)], title: String| {
            lines.push(Line::from(Span::styled(
                title,
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            )));
            for (key, v) in views {
                let cnt = self.context_count(*v);
                let active = cur == *v;
                let (icon, label) = match v {
                    View::Inbox => (
                        self.icon(Icon::Inbox),
                        super::view_label(self.lang, View::Inbox),
                    ),
                    View::Today => (
                        self.icon(Icon::Today),
                        super::view_label(self.lang, View::Today),
                    ),
                    View::Tomorrow => (
                        self.icon(Icon::Tomorrow),
                        super::view_label(self.lang, View::Tomorrow),
                    ),
                    View::Next => (
                        self.icon(Icon::Next),
                        super::view_label(self.lang, View::Next),
                    ),
                    View::Waiting => (
                        self.icon(Icon::Waiting),
                        super::view_label(self.lang, View::Waiting),
                    ),
                    View::Scheduled => (
                        self.icon(Icon::Scheduled),
                        super::view_label(self.lang, View::Scheduled),
                    ),
                    View::Someday => (
                        self.icon(Icon::Someday),
                        super::view_label(self.lang, View::Someday),
                    ),
                    View::Reference => (
                        self.icon(Icon::Reference),
                        super::view_label(self.lang, View::Reference),
                    ),
                    View::Done => (
                        self.icon(Icon::Done),
                        super::view_label(self.lang, View::Done),
                    ),
                    View::Review => (
                        self.icon(Icon::Review),
                        super::view_label(self.lang, View::Review),
                    ),
                    View::Archived => (
                        self.icon(Icon::Archived),
                        super::view_label(self.lang, View::Archived),
                    ),
                    View::Tags => (
                        self.icon(Icon::Tags),
                        super::view_label(self.lang, View::Tags),
                    ),
                    View::Quotes => (
                        self.icon(Icon::Quotes),
                        super::view_label(self.lang, View::Quotes),
                    ),
                    View::Settings => (
                        self.icon(Icon::Settings),
                        super::view_label(self.lang, View::Settings),
                    ),
                    _ => ("", ""),
                };
                let padded_label = pad_right(label, 10);

                if active {
                    let mut style = Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD);
                    if is_left_pane {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    lines.push(Line::from(Span::styled(
                        format!(
                            " {} {} {} {} {:>3} ",
                            self.icon(Icon::Active),
                            key,
                            icon,
                            padded_label,
                            cnt
                        ),
                        style,
                    )));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("   {} ", key),
                            Style::default().fg(self.theme.text_dim),
                        ),
                        Span::styled(
                            format!("{} ", icon),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("{} {:>3} ", padded_label, cnt)),
                    ]));
                }
            }
            if spacious {
                lines.push(Line::from(""));
            }
        };

        add_group(
            &[('J', View::Today), ('K', View::Tomorrow)],
            format!(" {} [Day] ⇧+", self.icon(Icon::GroupDay)),
        );
        add_group(
            &[('1', View::Inbox), ('2', View::Next)],
            format!(" {} [Active]", self.icon(Icon::GroupActive)),
        );
        add_group(
            &[
                ('3', View::Waiting),
                ('4', View::Scheduled),
                ('5', View::Someday),
            ],
            format!(" {} [Waiting]", self.icon(Icon::GroupWaiting)),
        );
        let mut archive_group = vec![];
        if self.modules.reference {
            archive_group.push(('6', View::Reference));
        }
        if self.modules.done {
            archive_group.push(('7', View::Done));
        }
        if !archive_group.is_empty() {
            add_group(
                &archive_group,
                format!(" {} [Archive]", self.icon(Icon::GroupArchive)),
            );
        }

        lines.push(Line::from(Span::styled(
            format!(" {} [Modules]", self.icon(Icon::GroupModules)),
            Style::default()
                .fg(self.theme.text_dim)
                .add_modifier(Modifier::BOLD),
        )));
        let mut mod_group = vec![];
        if self.modules.archived {
            mod_group.push(("8", View::Archived));
        }
        if self.modules.tags {
            mod_group.push(("9", View::Tags));
        }
        if self.quotes.enabled {
            mod_group.push(("0", View::Quotes));
        }
        if self.modules.review {
            mod_group.push(("r", View::Review));
        }
        if self.modules.settings {
            mod_group.push(("M", View::Settings));
        }
        mod_group.push(("W", View::Workflow));

        for (key, v) in mod_group {
            let active = cur == v;
            let (icon, label) = match v {
                View::Review => (
                    self.icon(Icon::Review),
                    super::view_label(self.lang, View::Review),
                ),
                View::Archived => (
                    self.icon(Icon::Archived),
                    super::view_label(self.lang, View::Archived),
                ),
                View::Tags => (
                    self.icon(Icon::Tags),
                    super::view_label(self.lang, View::Tags),
                ),
                View::Settings => (
                    self.icon(Icon::Settings),
                    super::view_label(self.lang, View::Settings),
                ),
                View::Quotes => (
                    self.icon(Icon::Quotes),
                    super::view_label(self.lang, View::Quotes),
                ),
                View::Workflow => (
                    self.icon(Icon::Workflow),
                    super::view_label(self.lang, View::Workflow),
                ),
                _ => ("", ""),
            };
            let padded_label = pad_right(label, 10);

            let cnt_str = if v == View::Quotes {
                format!("{:>3} ", self.context_count(View::Quotes))
            } else {
                "    ".to_string()
            };

            if active {
                let mut style = Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD);
                if is_left_pane {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                lines.push(Line::from(Span::styled(
                    format!(
                        " {} {:>1} {} {} {}",
                        self.icon(Icon::Active),
                        key,
                        icon,
                        padded_label,
                        cnt_str
                    ),
                    style,
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {:>1} ", key),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(
                        format!("{} ", icon),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{} {}", padded_label, cnt_str)),
                ]));
            }
        }
        if spacious {
            lines.push(Line::from(""));
        }

        // 动态快捷键：按当前视图/选择态/模式过滤，并严格按剩余行数截断。
        let rows_used = lines.len() as isize;
        let avail = area.height as isize - 2 - rows_used;
        let ctx = ctx_of(self);
        let keys = strip_keys(&ctx, self.lang);
        if avail >= 1 && !keys.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(" {} [Keys]", self.icon(Icon::GroupKeys)),
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            )));
            let mut budget = avail - 1;
            let mut shown = 0;
            for (k, desc) in &keys {
                if budget <= 0 {
                    break;
                }
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {:>6} ", k),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(*desc, Style::default().fg(self.theme.fg)),
                ]));
                shown += 1;
                budget -= 1;
            }
            if shown < keys.len() && budget >= 1 {
                lines.push(Line::from(Span::styled(
                    format!("   … {} 更多 (F1)", keys.len() - shown),
                    Style::default().fg(self.theme.text_dim),
                )));
            }
        }

        lines
    }

    /// 详情面板内容行。
    fn detail_lines(&self, d: &super::app::DetailData, width: u16) -> Vec<Line<'static>> {
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
