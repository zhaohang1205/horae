use super::AppRender;
use crate::tui::app::{App, Mode, Pane, View};
use horae_core::parser::{
    parse_quick_add, parse_rrule_shorthand, priority_value, tokenize_quick_add, QuickAddKind,
};
use horae_core::time;
use ratatui::symbols::border;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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
                    let val = horae_core::parser::strip_token_prefix(&tok.text);
                    let p = priority_value(val).unwrap_or("");
                    Style::default()
                        .fg(crate::tui::ui::priority_color(p).unwrap_or(self.theme.hl_fg))
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

    /// 输入/编辑模式的弹层：标题 + 文本行（快速录入带实时解析预览）。
    pub(super) fn render_input_overlay(&mut self, f: &mut Frame, size: Rect) {
        let title = match self.mode {
            Mode::Search => {
                tr!(
                    self.lang,
                    " 搜索任务 (标题 / 备注，日期用4位 MMDD，如0829) ",
                    " Search Tasks (Title / Notes, date: 4-digit MMDD, e.g. 0829) "
                )
            }
            Mode::Capturing => {
                if self.organizing_id.is_some() {
                    tr!(
                        self.lang,
                        " 组织: 编辑 标题 @标签 ~时间 *周期 (空/Esc 跳过) ",
                        " Organize: edit title @tags ~time *rrule (empty/Esc to skip) "
                    )
                } else if self.view == View::Quotes && self.quotes.enabled {
                    tr!(
                        self.lang,
                        " 快速录入金句 (自动 @quote 入库, 支持 @标签 及 Tab 补全) ",
                        " Quick capture quote (auto @quote, @tag and Tab to complete) "
                    )
                } else {
                    tr!(
                        self.lang,
                        " 快速录入 (支持 @标签 及 Tab 补全: home, work, errands, quick, focus...) ",
                        " Quick capture (@tag, Tab to complete: home, work, errands, quick, focus...) "
                    )
                }
            }
            Mode::Tagging => tr!(
                self.lang,
                " 添加标签 [支持 Tab 补全] (预设: home, work, errands, quick, focus...) ",
                " Add tags [Tab to complete] (presets: home, work, errands, quick, focus...) "
            ),
            Mode::WaitingWho => {
                tr!(self.lang, " 等待谁/什么? ", " Waiting for who/what? ")
            }
            Mode::WaitingWhen => tr!(
                self.lang,
                " 提醒时间? (如 +1d, tomorrow 10:00) ",
                " Reminder time? (e.g. +1d, tomorrow 10:00) "
            ),
            Mode::ChecklistAdding => {
                tr!(self.lang, " 新增检查单 ", " Add checklist item ")
            }
            Mode::FilteringTag => {
                tr!(self.lang, " 过滤标签 (情境) ", " Filter by tag (Context) ")
            }
            Mode::CreatingTag => tr!(
                self.lang,
                " 新增自定义标签 (输入标签名称，按 Enter 保存) ",
                " Create custom tag (enter name, Enter to save) "
            ),
            Mode::ConfiguringPomo => tr!(
                self.lang,
                " 自定义番茄钟时长 (格式: 工作;短休;长休[;长休周期], 如 25;5;15;4) ",
                " Custom pomodoro lengths (format: work;short;long[;interval], e.g. 25;5;15;4) "
            ),
            Mode::CreatingProfile => tr!(
                self.lang,
                " 新建 profile (输入名称，如 work / personal / prod1) ",
                " New profile (enter name, e.g. work / personal / prod1) "
            ),
            Mode::RenamingProfile => tr!(
                self.lang,
                " 重命名 profile (输入新名称) ",
                " Rename profile (enter new name) "
            ),
            Mode::RenamingChecklist => {
                tr!(
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

    /// 补全候选下拉层：紧贴输入框下方渲染。
    /// - 语法参考模式 (Reference)：提供丰富语义解释与语法范式 (Cheat-Sheet)。
    /// - 极速补全模式 (Speed)：紧凑单列快速匹配。
    fn render_completion_dropdown(&mut self, f: &mut Frame, input_area: Rect) {
        use crate::tui::app::completion::{completion_meta, CompletionStyle};
        let prefix = self.completion_prefix;
        let candidates = self.completion_candidates.clone();
        let idx = self.completion_index;
        let is_reference = self.completion_style == CompletionStyle::Reference;

        let mut lines: Vec<Line> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            let label = format!(" {}{} ", prefix, c);
            let is_sel = i == idx;
            let key_style = if is_sel {
                Style::default()
                    .fg(self.theme.bg)
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.fg)
            };

            if is_reference {
                let (desc, pattern) = completion_meta(prefix, c, self.lang);
                let label_text = if is_sel {
                    format!("❯{:<14}", label)
                } else {
                    format!(" {:<14}", label)
                };
                let mut spans = vec![
                    Span::styled(label_text, key_style),
                    Span::styled(
                        format!(" {:<20} ", desc),
                        if is_sel {
                            Style::default()
                                .fg(self.theme.hl_fg)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(self.theme.fg)
                        },
                    ),
                ];
                if !pattern.is_empty() {
                    spans.push(Span::styled(
                        format!(" {}", pattern),
                        Style::default().fg(self.theme.text_dim),
                    ));
                }
                lines.push(Line::from(spans));
            } else {
                let desc = match prefix {
                    '~' => time::parse_time(c)
                        .ok()
                        .map(|ms| time::format_local(Some(ms)))
                        .unwrap_or_default(),
                    '*' => horae_core::parser::parse_rrule_shorthand(c),
                    '!' => crate::tui::ui::priority_label(c)
                        .map(|(zh, en)| self.lang.tr(zh, en).to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
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
        }

        let width = if is_reference {
            input_area
                .width
                .max(64)
                .min(f.area().width.saturating_sub(4))
        } else {
            input_area
                .width
                .max(32)
                .min(f.area().width.saturating_sub(4))
        };
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
        let block_title = if is_reference {
            tr!(
                self.lang,
                " 语法速查 (↑↓ 浏览 · Tab 采纳 · Esc 关闭) ",
                " Syntax Guide (↑↓ browse · Tab apply · Esc close) "
            )
        } else {
            tr!(
                self.lang,
                " 极速补全 (↑↓ 切换 · Tab 补全 · Esc 取消) ",
                " Quick Complete (↑↓ pick · Tab complete · Esc cancel) "
            )
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(Style::default().fg(self.theme.accent))
            .title(block_title);
        f.render_widget(Paragraph::new(lines).block(block), dd);
    }

    /// 快速录入实时解析预览（标题 / 标签 / 时间 / 循环 / 优先级）。
    fn capture_preview_lines(&self) -> Vec<Line<'static>> {
        let parsed = parse_quick_add(&self.input);
        let tokens = tokenize_quick_add(&self.input);
        let mut text_lines: Vec<Line> = Vec::new();
        if !parsed.title.is_empty() {
            text_lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, " 标题: ", " Title: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::raw(parsed.title.clone()),
            ]));
        }
        if !parsed.tags.is_empty() {
            let mut spans = vec![Span::styled(
                tr!(self.lang, " 标签: ", " Tags: "),
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
                    tr!(self.lang, "[无效]", "[invalid]").to_string(),
                    Style::default().fg(self.theme.text_urgent),
                ),
            };
            text_lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, " 时间: ", " Time: "),
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
            .map(|t| horae_core::parser::strip_token_prefix(&t.text).to_string())
        {
            let valid = horae_core::parser::rrule_valid(&raw);
            let resolved = parse_rrule_shorthand(&raw);
            let (resolved_text, resolved_style) = if valid {
                (resolved, Style::default().fg(self.theme.text_dim))
            } else {
                (
                    tr!(self.lang, "[无效循环]", "[invalid rrule]").to_string(),
                    Style::default().fg(self.theme.text_urgent),
                )
            };
            text_lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, " 循环: ", " Rrule: "),
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
            let (zh, en) = crate::tui::ui::priority_label(p).unwrap_or(("", ""));
            let color = crate::tui::ui::priority_color(p).unwrap_or(self.theme.hl_fg);
            text_lines.push(Line::from(vec![
                Span::styled(
                    tr!(self.lang, " 优先级: ", " Priority: "),
                    Style::default().fg(self.theme.text_dim),
                ),
                Span::styled(
                    format!("!{}", self.lang.tr(zh, en)),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(" → "),
                Span::styled(format!("!{}", self.lang.tr(zh, en)), Style::default().fg(color)),
            ]));
        }
        text_lines
    }

    /// 快速录入语法提示行（常驻于输入/编辑弹层底部）。
    fn capture_syntax_hint_line(&self) -> Line<'static> {
        Line::from(Span::styled(
            tr!(
                self.lang,
                " [语法] @标签 (如 @work)  |  ~时间 (如 ~tomorrow, ~+3d, ~18:00)  |  *循环 (如 *2w[1,3], *m[1,2,-2,-1], *y[jan,jul])  |  !优先级 (如 !high)  |  日期搜索: MMDD (如 0829)",
                " [syntax] @tag (@work)  |  ~time (~tomorrow, ~+3d, ~18:00)  |  *rrule (*2w[1,3], *m[1,2,-2,-1], *y[jan,jul])  |  !priority (!high)  |  date search: MMDD (e.g. 0829)"
            ),
            Style::default().fg(self.theme.text_dim),
        ))
    }
}
