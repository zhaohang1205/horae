//! 任务备注极简 Markdown 转换器。
//!
//! 在零重型外部依赖的前提下，将 GTD 任务备注中常用的 Markdown 语法流式解析并转换为
//! 贴合 horae 动态主题系统与终端字宽（中英文混排、长单词安全折断、悬挂缩进）的富文本 `Line<'static>`。

use crate::tui::icons::IconStyle;
use crate::tui::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// GitHub / Obsidian 风格的 Callout / Alert 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    pub fn parse(s: &str) -> Option<AlertKind> {
        let trimmed = s.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("[!NOTE]") {
            Some(AlertKind::Note)
        } else if upper.starts_with("[!TIP]") {
            Some(AlertKind::Tip)
        } else if upper.starts_with("[!IMPORTANT]") {
            Some(AlertKind::Important)
        } else if upper.starts_with("[!WARNING]") {
            Some(AlertKind::Warning)
        } else if upper.starts_with("[!CAUTION]") {
            Some(AlertKind::Caution)
        } else {
            None
        }
    }

    pub fn title_badge(&self, icon_style: IconStyle) -> (&'static str, &'static str) {
        match self {
            AlertKind::Tip => {
                if icon_style == IconStyle::Ascii {
                    ("*", "TIP")
                } else {
                    ("💡", "TIP")
                }
            }
            AlertKind::Note => {
                if icon_style == IconStyle::Ascii {
                    ("i", "NOTE")
                } else {
                    ("ℹ", "NOTE")
                }
            }
            AlertKind::Important => {
                if icon_style == IconStyle::Ascii {
                    ("!", "IMPORTANT")
                } else {
                    ("★", "IMPORTANT")
                }
            }
            AlertKind::Warning => {
                if icon_style == IconStyle::Ascii {
                    ("/!\\", "WARNING")
                } else {
                    ("⚠", "WARNING")
                }
            }
            AlertKind::Caution => {
                if icon_style == IconStyle::Ascii {
                    ("[X]", "CAUTION")
                } else {
                    ("🛑", "CAUTION")
                }
            }
        }
    }

    pub fn color(&self, theme: &Theme) -> ratatui::style::Color {
        match self {
            AlertKind::Tip => theme.text_success,
            AlertKind::Note => theme.accent,
            AlertKind::Important => theme.hl_fg,
            AlertKind::Warning => theme.rrule_fg,
            AlertKind::Caution => theme.text_urgent,
        }
    }
}

/// 将包含常用 Markdown 语法的文本解析并渲染为 Ratatui 的多行富文本。
pub(crate) fn render_markdown(
    raw: &str,
    theme: &Theme,
    inner_width: usize,
    icon_style: IconStyle,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let inner_width = inner_width.max(10);
    let mut in_code_block = false;
    let mut in_alert: Option<AlertKind> = None;
    let mut prev_was_empty = false;

    for raw_line in raw.lines() {
        let trimmed = raw_line.trim();

        // 1. 代码块围栏 (``` 或 ~~~ 或 ''')
        let is_code_fence = trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || (trimmed.starts_with("'''") && trimmed.len() >= 3);

        if is_code_fence {
            if in_code_block {
                // 退出代码块
                in_code_block = false;
                let close_border = if icon_style == IconStyle::Ascii {
                    "  +--"
                } else {
                    "  └─"
                };
                lines.push(Line::from(Span::styled(
                    close_border,
                    Style::default().fg(theme.border_inactive),
                )));
            } else {
                // 进入代码块前若有未闭合的 Alert 先关闭
                if let Some(ak) = in_alert.take() {
                    let close_border = if icon_style == IconStyle::Ascii {
                        "  +--"
                    } else {
                        "  └─"
                    };
                    lines.push(Line::from(Span::styled(
                        close_border,
                        Style::default().fg(ak.color(theme)),
                    )));
                }
                in_code_block = true;
                let fence_char = if trimmed.starts_with("```") {
                    '`'
                } else if trimmed.starts_with("~~~") {
                    '~'
                } else {
                    '\''
                };
                let lang = trimmed.trim_start_matches(fence_char).trim();
                let lang_display = if lang.is_empty() { "code" } else { lang };
                let open_border = if icon_style == IconStyle::Ascii {
                    format!("  +-- [{}]", lang_display)
                } else {
                    format!("  ┌─ [{}]", lang_display)
                };
                lines.push(Line::from(Span::styled(
                    open_border,
                    Style::default().fg(theme.border_inactive),
                )));
            }
            prev_was_empty = false;
            continue;
        }

        // 2. 代码块内部行：保持原始字符，不解析 Markdown 语法，使用代码专属高亮与左侧边框
        if in_code_block {
            let prefix_str = if icon_style == IconStyle::Ascii {
                "  | "
            } else {
                "  │ "
            };
            let prefix = vec![Span::styled(
                prefix_str,
                Style::default().fg(theme.border_inactive),
            )];
            let prefix_w = UnicodeWidthStr::width(prefix_str);
            let code_style = Style::default().fg(theme.rrule_fg);

            let tokens = tokenize(raw_line, code_style);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix.clone(),
                prefix,
                prefix_w,
                prefix_w,
                inner_width,
                true,
            );
            lines.extend(wrapped);
            prev_was_empty = false;
            continue;
        }

        // 3. 空白行（压缩连续空行，避免过多留白挤占折叠阈值与空间）
        if trimmed.is_empty() {
            if let Some(ak) = in_alert.take() {
                let close_border = if icon_style == IconStyle::Ascii {
                    "  +--"
                } else {
                    "  └─"
                };
                lines.push(Line::from(Span::styled(
                    close_border,
                    Style::default().fg(ak.color(theme)),
                )));
            }
            if !prev_was_empty {
                lines.push(Line::from(""));
                prev_was_empty = true;
            }
            continue;
        }
        prev_was_empty = false;

        // 非引用行时，若之前处于 Alert 模式，先闭合卡片
        if !trimmed.starts_with('>') {
            if let Some(ak) = in_alert.take() {
                let close_border = if icon_style == IconStyle::Ascii {
                    "  +--"
                } else {
                    "  └─"
                };
                lines.push(Line::from(Span::styled(
                    close_border,
                    Style::default().fg(ak.color(theme)),
                )));
            }
        }

        // 4. 水平分割线 (---, ***, ___)
        if (trimmed.starts_with("---") && trimmed.chars().all(|c| c == '-'))
            || (trimmed.starts_with("***") && trimmed.chars().all(|c| c == '*'))
            || (trimmed.starts_with("___") && trimmed.chars().all(|c| c == '_'))
        {
            let hr_char = if icon_style == IconStyle::Ascii {
                "-"
            } else {
                "─"
            };
            let count = inner_width.saturating_sub(4).max(1);
            lines.push(Line::from(Span::styled(
                format!("  {}", hr_char.repeat(count)),
                Style::default().fg(theme.border_inactive),
            )));
            continue;
        }

        // 5. 标题 (#, ##, ###, ...)
        if let Some((h_level, title_text)) = parse_heading(trimmed) {
            let (h_style, h_prefix) = match h_level {
                1 => (
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                    "# ",
                ),
                2 => (
                    Style::default()
                        .fg(theme.hl_fg)
                        .add_modifier(Modifier::BOLD),
                    "## ",
                ),
                3 => (
                    Style::default()
                        .fg(theme.rrule_fg)
                        .add_modifier(Modifier::BOLD),
                    "### ",
                ),
                _ => (
                    Style::default()
                        .fg(theme.text_dim)
                        .add_modifier(Modifier::BOLD),
                    "#### ",
                ),
            };
            let prefix_spans = vec![Span::styled(h_prefix, h_style)];
            let prefix_w = UnicodeWidthStr::width(h_prefix);
            let cont_prefix_str = " ".repeat(prefix_w);
            let cont_spans = vec![Span::raw(cont_prefix_str)];

            let inline_spans = parse_inline(title_text, h_style, theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans,
                cont_spans,
                prefix_w,
                prefix_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
            continue;
        }

        // 6. 引用块 (> 文本) 与 GitHub-style Alert/Callout 卡片
        if let Some(stripped) = trimmed.strip_prefix('>') {
            let quote_text = stripped.trim_start();

            // 检查是否是 Alert 头部，例如 `[!TIP]`、`[!IMPORTANT]`
            if let Some(alert_kind) = AlertKind::parse(quote_text) {
                if let Some(old_ak) = in_alert.take() {
                    let close_border = if icon_style == IconStyle::Ascii {
                        "  +--"
                    } else {
                        "  └─"
                    };
                    lines.push(Line::from(Span::styled(
                        close_border,
                        Style::default().fg(old_ak.color(theme)),
                    )));
                }
                in_alert = Some(alert_kind);
                let (glyph, label) = alert_kind.title_badge(icon_style);
                let alert_color = alert_kind.color(theme);

                let extra_title = quote_text
                    .find(']')
                    .map(|idx| quote_text[idx + 1..].trim())
                    .unwrap_or("");
                let open_line = if extra_title.is_empty() {
                    if icon_style == IconStyle::Ascii {
                        format!("  +-- [{}] {}", glyph, label)
                    } else {
                        format!("  ┌─ {} {}", glyph, label)
                    }
                } else if icon_style == IconStyle::Ascii {
                    format!("  +-- [{}] {} · {}", glyph, label, extra_title)
                } else {
                    format!("  ┌─ {} {} · {}", glyph, label, extra_title)
                };
                lines.push(Line::from(Span::styled(
                    open_line,
                    Style::default()
                        .fg(alert_color)
                        .add_modifier(Modifier::BOLD),
                )));
                continue;
            }

            // 若正处于 Alert 卡片内：延续对应主题色的边框
            if let Some(alert_kind) = in_alert {
                let alert_color = alert_kind.color(theme);
                let prefix_str = if icon_style == IconStyle::Ascii {
                    "  | "
                } else {
                    "  │ "
                };
                let prefix_spans = vec![Span::styled(prefix_str, Style::default().fg(alert_color))];
                let prefix_w = UnicodeWidthStr::width(prefix_str);
                let content_style = Style::default().fg(alert_color);
                let inline_spans = parse_inline(quote_text, content_style, theme);
                let tokens = tokenize_spans(inline_spans);
                let wrapped = wrap_styled_tokens(
                    tokens,
                    prefix_spans.clone(),
                    prefix_spans,
                    prefix_w,
                    prefix_w,
                    inner_width,
                    false,
                );
                lines.extend(wrapped);
                continue;
            }

            // 普通引用块
            let quote_border = if icon_style == IconStyle::Ascii {
                "  | "
            } else {
                "  │ "
            };
            let prefix_spans = vec![Span::styled(
                quote_border,
                Style::default().fg(theme.border_inactive),
            )];
            let prefix_w = UnicodeWidthStr::width(quote_border);
            let quote_style = Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::ITALIC);

            let inline_spans = parse_inline(quote_text, quote_style, theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans.clone(),
                prefix_spans,
                prefix_w,
                prefix_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
            continue;
        }

        // 7. 任务列表复选框 (- [ ] 或 - [x] 或 + [ ] 或 * [ ])
        if let Some((is_checked, item_text)) = parse_checklist_item(trimmed) {
            let (check_str, check_style, text_style) = if is_checked {
                (
                    "  [x] ",
                    Style::default()
                        .fg(theme.text_success)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.text_dim)
                        .add_modifier(Modifier::CROSSED_OUT),
                )
            } else {
                (
                    "  [ ] ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(theme.fg),
                )
            };
            let prefix_spans = vec![Span::styled(check_str, check_style)];
            let prefix_w = UnicodeWidthStr::width(check_str);
            let cont_prefix_str = " ".repeat(prefix_w);
            let cont_spans = vec![Span::raw(cont_prefix_str)];

            let inline_spans = parse_inline(item_text, text_style, theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans,
                cont_spans,
                prefix_w,
                prefix_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
            continue;
        }

        // 8. 无序列表 (- , * , + )
        if let Some((indent_level, item_text)) = parse_bullet_list(raw_line) {
            let (bullet_str, cont_str) = if indent_level == 0 {
                let b = if icon_style == IconStyle::Ascii {
                    "  - "
                } else {
                    "  • "
                };
                (b, "    ")
            } else {
                let b = if icon_style == IconStyle::Ascii {
                    "    * "
                } else {
                    "    ◦ "
                };
                (b, "      ")
            };
            let prefix_spans = vec![Span::styled(bullet_str, Style::default().fg(theme.accent))];
            let prefix_w = UnicodeWidthStr::width(bullet_str);
            let cont_spans = vec![Span::raw(cont_str)];
            let cont_w = UnicodeWidthStr::width(cont_str);

            let inline_spans = parse_inline(item_text, Style::default().fg(theme.fg), theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans,
                cont_spans,
                prefix_w,
                cont_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
            continue;
        }

        // 9. 有序列表 (1. , 2. )
        if let Some((num_str, item_text)) = parse_ordered_list(trimmed) {
            let prefix_str = format!("  {}. ", num_str);
            let prefix_w = UnicodeWidthStr::width(prefix_str.as_str());
            let prefix_spans = vec![Span::styled(prefix_str, Style::default().fg(theme.accent))];
            let cont_prefix_str = " ".repeat(prefix_w);
            let cont_spans = vec![Span::raw(cont_prefix_str)];

            let inline_spans = parse_inline(item_text, Style::default().fg(theme.fg), theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans,
                cont_spans,
                prefix_w,
                prefix_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
            continue;
        }

        // 10. 普通段落 (保留首行两格缩进 "  ")
        {
            let prefix_spans = vec![Span::raw("  ")];
            let cont_spans = vec![Span::raw("  ")];
            let prefix_w = 2;

            let inline_spans = parse_inline(trimmed, Style::default().fg(theme.fg), theme);
            let tokens = tokenize_spans(inline_spans);
            let wrapped = wrap_styled_tokens(
                tokens,
                prefix_spans,
                cont_spans,
                prefix_w,
                prefix_w,
                inner_width,
                false,
            );
            lines.extend(wrapped);
        }
    }

    if let Some(ak) = in_alert {
        let close_border = if icon_style == IconStyle::Ascii {
            "  +--"
        } else {
            "  └─"
        };
        lines.push(Line::from(Span::styled(
            close_border,
            Style::default().fg(ak.color(theme)),
        )));
    } else if in_code_block {
        let close_border = if icon_style == IconStyle::Ascii {
            "  +--"
        } else {
            "  └─"
        };
        lines.push(Line::from(Span::styled(
            close_border,
            Style::default().fg(theme.border_inactive),
        )));
    }

    lines
}

fn parse_heading(s: &str) -> Option<(usize, &str)> {
    if !s.starts_with('#') {
        return None;
    }
    let mut count = 0;
    for c in s.chars() {
        if c == '#' {
            count += 1;
        } else {
            break;
        }
    }
    if (1..=6).contains(&count) && s[count..].starts_with(' ') {
        Some((count, s[count..].trim_start()))
    } else {
        None
    }
}

fn parse_checklist_item(s: &str) -> Option<(bool, &str)> {
    let lower = s.to_lowercase();
    for marker in &["- [ ]", "* [ ]", "+ [ ]"] {
        if lower.starts_with(marker) {
            return Some((false, s[marker.len()..].trim_start()));
        }
    }
    for marker in &["- [x]", "* [x]", "+ [x]"] {
        if lower.starts_with(marker) {
            return Some((true, s[marker.len()..].trim_start()));
        }
    }
    None
}

fn parse_bullet_list(s: &str) -> Option<(usize, &str)> {
    let leading_spaces = s.chars().take_while(|c| *c == ' ').count();
    let rest = s[leading_spaces..].trim_start();
    if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        if rest.starts_with("- [") || rest.starts_with("* [") || rest.starts_with("+ [") {
            return None;
        }
        let indent_level = if leading_spaces >= 2 { 1 } else { 0 };
        Some((indent_level, rest[2..].trim_start()))
    } else {
        None
    }
}

fn parse_ordered_list(s: &str) -> Option<(&str, &str)> {
    let dot_pos = s.find('.')?;
    if dot_pos == 0 || dot_pos > 4 {
        return None;
    }
    let num_part = &s[..dot_pos];
    if num_part.chars().all(|c| c.is_ascii_digit()) {
        let after_dot = &s[dot_pos + 1..];
        if after_dot.starts_with(' ') {
            return Some((num_part, after_dot.trim_start()));
        }
    }
    None
}

/// 解析单行文本内的 Markdown 行内富文本元素（代码、粗体、斜体、删除线、超链接）。
pub(crate) fn parse_inline(text: &str, base_style: Style, theme: &Theme) -> Vec<(String, Style)> {
    let mut result = Vec::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut cur_buf = String::new();

    let flush_buf = |buf: &mut String, res: &mut Vec<(String, Style)>| {
        if !buf.is_empty() {
            res.push((std::mem::take(buf), base_style));
        }
    };

    while i < len {
        // 1. 行内代码 `code`
        if bytes[i] == b'`' {
            if let Some(end) = text[i + 1..].find('`') {
                let code_content = &text[i + 1..i + 1 + end];
                flush_buf(&mut cur_buf, &mut result);
                let code_style = base_style
                    .fg(theme.accent)
                    .remove_modifier(Modifier::ITALIC);
                result.push((code_content.to_string(), code_style));
                i = i + 1 + end + 1;
                continue;
            }
        }

        // 2. 加粗 **bold** 或 __bold__
        if (text[i..].starts_with("**") && i + 2 < len && !bytes[i + 2].is_ascii_whitespace())
            || (text[i..].starts_with("__") && i + 2 < len && !bytes[i + 2].is_ascii_whitespace())
        {
            let delim = &text[i..i + 2];
            if let Some(end) = text[i + 2..].find(delim) {
                if end > 0 && !text[i + 2..i + 2 + end].ends_with(' ') {
                    let inner = &text[i + 2..i + 2 + end];
                    flush_buf(&mut cur_buf, &mut result);
                    let bold_style = base_style.add_modifier(Modifier::BOLD);
                    let mut nested = parse_inline(inner, bold_style, theme);
                    result.append(&mut nested);
                    i = i + 2 + end + 2;
                    continue;
                }
            }
        }

        // 3. 删除线 ~~deleted~~
        if text[i..].starts_with("~~") && i + 2 < len && !bytes[i + 2].is_ascii_whitespace() {
            if let Some(end) = text[i + 2..].find("~~") {
                if end > 0 && !text[i + 2..i + 2 + end].ends_with(' ') {
                    let inner = &text[i + 2..i + 2 + end];
                    flush_buf(&mut cur_buf, &mut result);
                    let strike_style = base_style.add_modifier(Modifier::CROSSED_OUT);
                    let mut nested = parse_inline(inner, strike_style, theme);
                    result.append(&mut nested);
                    i = i + 2 + end + 2;
                    continue;
                }
            }
        }

        // 4. 超链接 [label](url)
        if bytes[i] == b'[' {
            if let Some(close_bracket) = text[i + 1..].find(']') {
                let mid = i + 1 + close_bracket;
                if text[mid..].starts_with("](") {
                    if let Some(close_paren) = text[mid + 2..].find(')') {
                        let end = mid + 2 + close_paren;
                        let label = &text[i + 1..mid];
                        let url = &text[mid + 2..end];
                        flush_buf(&mut cur_buf, &mut result);

                        let link_style = base_style
                            .fg(theme.accent)
                            .add_modifier(Modifier::UNDERLINED);
                        result.push((label.to_string(), link_style));
                        result.push((format!(" ({})", url), Style::default().fg(theme.text_dim)));

                        i = end + 1;
                        continue;
                    }
                }
            }
        }

        // 5. 斜体 *italic* 或 _italic_
        if (bytes[i] == b'*' && !text[i..].starts_with("**"))
            || (bytes[i] == b'_' && !text[i..].starts_with("__"))
        {
            let delim = bytes[i] as char;
            let prev_is_alphanumeric = if i > 0 {
                text[..i]
                    .chars()
                    .last()
                    .map(|c| c.is_alphanumeric())
                    .unwrap_or(false)
            } else {
                false
            };
            let next_is_ws = i + 1 < len && bytes[i + 1].is_ascii_whitespace();

            if (!prev_is_alphanumeric || delim == '*') && !next_is_ws {
                if let Some(end) = text[i + 1..].find(delim) {
                    let inner = &text[i + 1..i + 1 + end];
                    if !inner.is_empty() && !inner.ends_with(' ') && !inner.contains(delim) {
                        flush_buf(&mut cur_buf, &mut result);
                        let italic_style = base_style.add_modifier(Modifier::ITALIC);
                        let mut nested = parse_inline(inner, italic_style, theme);
                        result.append(&mut nested);
                        i = i + 1 + end + 1;
                        continue;
                    }
                }
            }
        }

        // 6. 普通字符
        let ch = text[i..].chars().next().unwrap();
        cur_buf.push(ch);
        i += ch.len_utf8();
    }

    flush_buf(&mut cur_buf, &mut result);
    result
}

#[derive(Debug, Clone)]
struct StyledToken {
    text: String,
    width: usize,
    is_ws: bool,
    style: Style,
}

fn tokenize_spans(spans: Vec<(String, Style)>) -> Vec<StyledToken> {
    let mut tokens = Vec::new();
    for (text, style) in spans {
        tokens.extend(tokenize(&text, style));
    }
    tokens
}

fn tokenize(text: &str, style: Style) -> Vec<StyledToken> {
    let mut tokens = Vec::new();
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
            let piece = &text[idx..end];
            tokens.push(StyledToken {
                text: piece.to_string(),
                width: UnicodeWidthStr::width(piece),
                is_ws: true,
                style,
            });
        } else if UnicodeWidthChar::width(c).unwrap_or(0) >= 2 || !c.is_ascii() {
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            let piece = &text[idx..end];
            tokens.push(StyledToken {
                text: piece.to_string(),
                width: UnicodeWidthStr::width(piece),
                is_ws: false,
                style,
            });
        } else if c == ';' || c == ',' || c == ':' || c == '/' || c == '-' || c == '_' {
            let end = chars.peek().map(|&(i, _)| i).unwrap_or(text.len());
            let piece = &text[idx..end];
            tokens.push(StyledToken {
                text: piece.to_string(),
                width: UnicodeWidthStr::width(piece),
                is_ws: false,
                style,
            });
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
            let piece = &text[idx..end];
            tokens.push(StyledToken {
                text: piece.to_string(),
                width: UnicodeWidthStr::width(piece),
                is_ws: false,
                style,
            });
        }
    }

    tokens
}

fn append_str_to_spans(spans: &mut Vec<Span<'static>>, s: &str, style: Style) {
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            let mut cur = last.content.to_string();
            cur.push_str(s);
            last.content = cur.into();
            return;
        }
    }
    spans.push(Span::styled(s.to_string(), style));
}

fn wrap_styled_tokens(
    tokens: Vec<StyledToken>,
    prefix: Vec<Span<'static>>,
    cont_prefix: Vec<Span<'static>>,
    prefix_w: usize,
    cont_w: usize,
    inner_width: usize,
    preserve_leading_ws: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut cur_spans: Vec<Span<'static>> = Vec::new();
    let mut cur_w = 0;
    let mut is_first = true;
    let mut cap = inner_width.saturating_sub(prefix_w).max(1);

    for token in tokens {
        if cur_w == 0 && token.is_ws && (!preserve_leading_ws || !is_first) {
            continue;
        }

        if cur_w + token.width <= cap {
            append_str_to_spans(&mut cur_spans, &token.text, token.style);
            cur_w += token.width;
        } else if cur_w > 0 {
            // 提交当前行
            let mut full_line = if is_first {
                prefix.clone()
            } else {
                cont_prefix.clone()
            };
            full_line.append(&mut cur_spans);
            lines.push(Line::from(full_line));

            cur_spans = Vec::new();
            cur_w = 0;
            is_first = false;
            cap = inner_width.saturating_sub(cont_w).max(1);

            if !token.is_ws {
                if token.width <= cap {
                    append_str_to_spans(&mut cur_spans, &token.text, token.style);
                    cur_w = token.width;
                } else {
                    for c in token.text.chars() {
                        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                        if cur_w + cw > cap && cur_w > 0 {
                            let mut full_line = cont_prefix.clone();
                            full_line.append(&mut cur_spans);
                            lines.push(Line::from(full_line));

                            cur_spans = Vec::new();
                            cur_w = 0;
                            cap = inner_width.saturating_sub(cont_w).max(1);
                        }
                        append_str_to_spans(&mut cur_spans, &c.to_string(), token.style);
                        cur_w += cw;
                    }
                }
            }
        } else {
            // cur_w == 0 但单个 token 超过 cap
            for c in token.text.chars() {
                let cw = UnicodeWidthChar::width(c).unwrap_or(0);
                if cur_w + cw > cap && cur_w > 0 {
                    let mut full_line = if is_first {
                        prefix.clone()
                    } else {
                        cont_prefix.clone()
                    };
                    full_line.append(&mut cur_spans);
                    lines.push(Line::from(full_line));

                    cur_spans = Vec::new();
                    cur_w = 0;
                    is_first = false;
                    cap = inner_width.saturating_sub(cont_w).max(1);
                }
                append_str_to_spans(&mut cur_spans, &c.to_string(), token.style);
                cur_w += cw;
            }
        }
    }

    if !cur_spans.is_empty() || lines.is_empty() {
        let mut full_line = if is_first { prefix } else { cont_prefix };
        full_line.append(&mut cur_spans);
        lines.push(Line::from(full_line));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_to_string(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn test_render_markdown_plain_text() {
        let theme = Theme::default();
        let lines = render_markdown(
            "这是普通的任务备注第一行\n这是第二行",
            &theme,
            40,
            IconStyle::Nerd,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(line_to_string(&lines[0]), "  这是普通的任务备注第一行");
        assert_eq!(line_to_string(&lines[1]), "  这是第二行");
    }

    #[test]
    fn test_render_markdown_headings() {
        let theme = Theme::default();
        let lines = render_markdown(
            "# 一级标题\n## 二级标题\n### 三级标题",
            &theme,
            40,
            IconStyle::Nerd,
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(line_to_string(&lines[0]), "# 一级标题");
        assert!(lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(line_to_string(&lines[1]), "## 二级标题");
        assert_eq!(line_to_string(&lines[2]), "### 三级标题");
    }

    #[test]
    fn test_render_markdown_lists() {
        let theme = Theme::default();
        let lines = render_markdown(
            "- 列表项A\n* 列表项B\n  - 子列表项",
            &theme,
            40,
            IconStyle::Nerd,
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(line_to_string(&lines[0]), "  • 列表项A");
        assert_eq!(line_to_string(&lines[1]), "  • 列表项B");
        assert_eq!(line_to_string(&lines[2]), "    ◦ 子列表项");
    }

    #[test]
    fn test_render_markdown_ordered_list() {
        let theme = Theme::default();
        let lines = render_markdown("1. 步骤一\n2. 步骤二", &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_to_string(&lines[0]), "  1. 步骤一");
        assert_eq!(line_to_string(&lines[1]), "  2. 步骤二");
    }

    #[test]
    fn test_render_markdown_checklist() {
        let theme = Theme::default();
        let lines = render_markdown(
            "- [ ] 未完成事项\n- [x] 已完成事项",
            &theme,
            40,
            IconStyle::Nerd,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(line_to_string(&lines[0]), "  [ ] 未完成事项");
        assert_eq!(line_to_string(&lines[1]), "  [x] 已完成事项");
        assert!(lines[1].spans[1]
            .style
            .add_modifier
            .contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn test_render_markdown_blockquote() {
        let theme = Theme::default();
        let lines = render_markdown("> 这是一个重要的引用说明", &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_to_string(&lines[0]), "  │ 这是一个重要的引用说明");
        assert!(lines[0].spans[1]
            .style
            .add_modifier
            .contains(Modifier::ITALIC));
    }

    #[test]
    fn test_render_markdown_code_block() {
        let theme = Theme::default();
        let raw = "```rust\nfn test() {\n    let a = 1;\n}\n```";
        let lines = render_markdown(raw, &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 5);
        assert_eq!(line_to_string(&lines[0]), "  ┌─ [rust]");
        assert_eq!(line_to_string(&lines[1]), "  │ fn test() {");
        assert_eq!(line_to_string(&lines[2]), "  │     let a = 1;");
        assert_eq!(line_to_string(&lines[3]), "  │ }");
        assert_eq!(line_to_string(&lines[4]), "  └─");
    }

    #[test]
    fn test_render_markdown_inline_formatting() {
        let theme = Theme::default();
        let raw = "测试 **加粗** 与 *斜体* 与 `代码` 与 ~~删除~~ 与 [官网](https://horae.dev)";
        let lines = render_markdown(raw, &theme, 80, IconStyle::Nerd);
        assert_eq!(lines.len(), 1);
        let s = line_to_string(&lines[0]);
        assert!(s.contains("加粗"));
        assert!(s.contains("斜体"));
        assert!(s.contains("代码"));
        assert!(s.contains("删除"));
        assert!(s.contains("官网 (https://horae.dev)"));
    }

    #[test]
    fn test_render_markdown_horizontal_rule() {
        let theme = Theme::default();
        let lines = render_markdown("段落1\n---\n段落2", &theme, 20, IconStyle::Nerd);
        assert_eq!(lines.len(), 3);
        assert_eq!(line_to_string(&lines[0]), "  段落1");
        assert!(line_to_string(&lines[1]).starts_with("  ────"));
        assert_eq!(line_to_string(&lines[2]), "  段落2");
    }

    #[test]
    fn test_render_markdown_wrapping_preserves_indent() {
        let theme = Theme::default();
        // 宽度很窄（20），多行折叠
        let lines = render_markdown(
            "- 这是一个非常非常非常非常长的无序列表项用于测试悬挂对齐折行",
            &theme,
            20,
            IconStyle::Nerd,
        );
        assert!(lines.len() >= 2);
        assert!(line_to_string(&lines[0]).starts_with("  • "));
        assert!(line_to_string(&lines[1]).starts_with("    "));
    }

    #[test]
    fn test_render_markdown_ascii_mode() {
        let theme = Theme::default();
        let lines = render_markdown(
            "- 列表\n> 引用\n---\n```\ncode\n```",
            &theme,
            20,
            IconStyle::Ascii,
        );
        assert_eq!(line_to_string(&lines[0]), "  - 列表");
        assert_eq!(line_to_string(&lines[1]), "  | 引用");
        assert!(line_to_string(&lines[2]).starts_with("  ----"));
        assert_eq!(line_to_string(&lines[3]), "  +-- [code]");
        assert_eq!(line_to_string(&lines[4]), "  | code");
        assert_eq!(line_to_string(&lines[5]), "  +--");
    }

    #[test]
    fn test_render_markdown_alert_callouts() {
        let theme = Theme::default();
        let raw = "> [!TIP]\n> it is important\n\n> [!IMPORTANT]\n> hello";
        let lines = render_markdown(raw, &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 7);
        assert_eq!(line_to_string(&lines[0]), "  ┌─ 💡 TIP");
        assert_eq!(line_to_string(&lines[1]), "  │ it is important");
        assert_eq!(line_to_string(&lines[2]), "  └─");
        assert_eq!(line_to_string(&lines[3]), "");
        assert_eq!(line_to_string(&lines[4]), "  ┌─ ★ IMPORTANT");
        assert_eq!(line_to_string(&lines[5]), "  │ hello");
        assert_eq!(line_to_string(&lines[6]), "  └─");
    }

    #[test]
    fn test_render_markdown_python_single_quotes_code_block() {
        let theme = Theme::default();
        let raw = "'''python\nfrom langchain import chatopenai\n'''";
        let lines = render_markdown(raw, &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 3);
        assert_eq!(line_to_string(&lines[0]), "  ┌─ [python]");
        assert_eq!(
            line_to_string(&lines[1]),
            "  │ from langchain import chatopenai"
        );
        assert_eq!(line_to_string(&lines[2]), "  └─");
    }

    #[test]
    fn test_render_markdown_squeezes_consecutive_blank_lines() {
        let theme = Theme::default();
        let raw = "段落1\n\n\n\n段落2";
        let lines = render_markdown(raw, &theme, 40, IconStyle::Nerd);
        assert_eq!(lines.len(), 3);
        assert_eq!(line_to_string(&lines[0]), "  段落1");
        assert_eq!(line_to_string(&lines[1]), "");
        assert_eq!(line_to_string(&lines[2]), "  段落2");
    }
}
