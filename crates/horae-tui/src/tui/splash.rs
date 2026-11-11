//! 开屏（splash）：ASCII 艺术字、GTD 标语展示与按键等待。
//! 支持动态自适应当前系统主题（包含 24 款 Omarchy 主题与亮/深色模式切换），
//! 严格遵循 WCAG 2.1 高对比度色彩排版规范。

use horae_core::i18n::Lang;
use unicode_width::UnicodeWidthStr;

use crate::tui::icons::IconStyle;
use crate::tui::theme::Theme;

/// figlet 字体 "Delta Corps Priest1" 渲染的 HORAE 艺术字（视觉主体）。
const HORAE_LOGO: &[&str] = &[
    "   ▄█    █▄     ▄██████▄     ▄████████    ▄████████    ▄████████",
    "  ███    ███   ███    ███   ███    ███   ███    ███   ███    ███",
    "  ███    ███   ███    ███   ███    ███   ███    ███   ███    █▀",
    " ▄███▄▄▄▄███▄▄ ███    ███  ▄███▄▄▄▄██▀   ███    ███  ▄███▄▄▄",
    "▀▀███▀▀▀▀███▀  ███    ███ ▀▀███▀▀▀▀▀   ▀███████████ ▀▀███▀▀▀",
    "  ███    ███   ███    ███ ▀███████████   ███    ███   ███    █▄",
    "  ███    ███   ███    ███   ███    ███   ███    ███   ███    ███",
    "  ███    █▀     ▀██████▀    ███    ███   ███    █▀    ██████████",
    "                            ███    ███",
];

/// 水平居中所需的起始列。
fn center_x(cols: u16, w: u16) -> u16 {
    if cols > w {
        (cols - w) / 2
    } else {
        0
    }
}

/// 文本按显示宽度居中填充到指定列宽。
fn center_pad(text: &str, width: usize) -> String {
    let w = text.width();
    if w >= width {
        return text.to_string();
    }
    let pad = width - w;
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

/// RGB 颜色三元组。
pub(crate) type Rgb = (u8, u8, u8);

/// 将 [`ratatui::style::Color`] 转换为 RGB 三元组。
pub(crate) fn color_to_rgb(color: ratatui::style::Color) -> Rgb {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
        ratatui::style::Color::Black => (0, 0, 0),
        ratatui::style::Color::White => (255, 255, 255),
        _ => (128, 128, 128),
    }
}

/// 计算 sRGB 颜色的相对感知亮度（WCAG 2.1 规范）。
pub(crate) fn relative_luminance((r, g, b): Rgb) -> f32 {
    let srgb = |c: u8| -> f32 {
        let v = (c as f32) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * srgb(r) + 0.7152 * srgb(g) + 0.0722 * srgb(b)
}

/// 计算两种颜色的 WCAG 对比度比率（1.0 ~ 21.0）。
pub(crate) fn contrast_ratio(c1: Rgb, c2: Rgb) -> f32 {
    let l1 = relative_luminance(c1);
    let l2 = relative_luminance(c2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// 从候选颜色集合中，挑选与指定背景具有最高 WCAG 对比度的颜色。
pub(crate) fn best_contrast(bg: Rgb, candidates: &[Rgb]) -> Rgb {
    candidates
        .iter()
        .copied()
        .max_by(|&a, &b| {
            contrast_ratio(bg, a)
                .partial_cmp(&contrast_ratio(bg, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((255, 255, 255))
}

/// 开屏页自适应调色板（派生自当前激活的 [`Theme`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashPalette {
    /// 终端基底色
    pub bg: Rgb,
    /// 正文主体文字色
    pub fg: Rgb,
    /// 横幅底色（取自主题的主强调色 accent）
    pub banner_bg: Rgb,
    /// 横幅字标文字色（WCAG 最高对比度计算）
    pub banner_fg: Rgb,
    /// 装饰分割线与卡片边框色
    pub border: Rgb,
    /// 弱化辅助文字（标签、页脚、慢闪提示）
    pub text_dim: Rgb,
    /// 图标与关键视觉点缀色
    pub accent: Rgb,
}

impl SplashPalette {
    /// 从主题构建开屏调色板，确保无论浅色/深色主题，各元素均具备清晰的设计层级。
    pub fn from_theme(theme: &Theme) -> Self {
        let bg = color_to_rgb(theme.bg);
        let fg = color_to_rgb(theme.fg);
        let accent = color_to_rgb(theme.accent);
        let border = color_to_rgb(theme.border_inactive);
        let text_dim = color_to_rgb(theme.text_dim);

        let banner_bg = accent;
        // 在主题底色、主体前景色、纯黑、纯白以及深炭黑中挑选对比度最高的前景字标色
        let banner_fg = best_contrast(
            banner_bg,
            &[
                bg,
                fg,
                (17, 17, 27),    // 深炭黑（Crust）
                (255, 255, 255), // 纯白
                (0, 0, 0),       // 纯黑
            ],
        );

        Self {
            bg,
            fg,
            banner_bg,
            banner_fg,
            border,
            text_dim,
            accent,
        }
    }
}

/// 前景色转义序列。
fn fg(color: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2)
}

/// 背景色转义序列。
fn bg(color: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", color.0, color.1, color.2)
}

/// 在第 `y` 行水平居中写出文本；`sgr` 为附加样式前缀
/// （如 `\x1b[1m` 加粗、`\x1b[5m` 闪烁），无样式传空串。
fn write_centered<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    y: u16,
    sgr: &str,
    text: &str,
    color: Rgb,
) -> std::io::Result<()> {
    use crossterm::{cursor::MoveTo, ExecutableCommand};
    out.execute(MoveTo(center_x(cols, text.width() as u16), y))?;
    write!(out, "{sgr}{}{text}\x1b[0m", fg(color))
}

/// 绘制带左右横线的副标题 `───── Goddess of Time ─────`。
fn draw_subtitle<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    y: u16,
    brand: &str,
    palette: &SplashPalette,
) -> std::io::Result<()> {
    use crossterm::{cursor::MoveTo, ExecutableCommand};
    let dash = "─────";
    let full_text = format!("{dash} {brand} {dash}");
    let start_x = center_x(cols, full_text.width() as u16);

    out.execute(MoveTo(start_x, y))?;
    write!(out, "{}{dash} \x1b[0m", fg(palette.border))?;
    write!(out, "\x1b[1m{}{brand}\x1b[0m", fg(palette.accent))?;
    write!(out, "{} {dash}\x1b[0m", fg(palette.border))?;
    Ok(())
}

/// 绘制卡片网格中的单行（4列，用竖线 `│` 分隔）。
#[allow(clippy::too_many_arguments)]
fn draw_card_row<W: std::io::Write>(
    out: &mut W,
    start_x: u16,
    y: u16,
    items: [&str; 4],
    card_w: usize,
    text_sgr: &str,
    text_color: Rgb,
    sep_color: Rgb,
) -> std::io::Result<()> {
    use crossterm::{cursor::MoveTo, ExecutableCommand};
    out.execute(MoveTo(start_x, y))?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            write!(out, "{}│\x1b[0m", fg(sep_color))?;
        }
        let padded = center_pad(item, card_w);
        write!(out, "{text_sgr}{}{padded}\x1b[0m", fg(text_color))?;
    }
    Ok(())
}

/// 品牌副标题：呼应「时间女神」。
const BRAND_SUBTITLE: &str = "Goddess of Time";

/// horae 开源仓库地址。
const REPO_URL: &str = "https://github.com/zhaohang1205/horae";

/// 作者个人主页。
const AUTHOR_URL: &str = "https://github.com/zhaohang1205";

/// 底部提示：主提示语（自适应宽度展示快捷键引导）。
fn prompts(lang: Lang, cols: u16) -> &'static str {
    if cols >= 62 {
        match lang {
            Lang::Zh => "按任意键开启 · F5 切换主题 · F6 切换语言",
            Lang::En => "Press any key · F5 Theme · F6 Language",
        }
    } else {
        match lang {
            Lang::Zh => "按任意键开始…",
            Lang::En => "Press any key to start…",
        }
    }
}

pub(super) fn show_splash(
    conn: &rusqlite::Connection,
    mut lang: Lang,
    mut theme: Theme,
    mut omarchy_auto: bool,
    icon_style: IconStyle,
) -> anyhow::Result<(Lang, Theme, bool)> {
    use crossterm::{
        cursor,
        event::{self, Event, KeyCode},
        terminal,
    };
    use std::io::Write;

    let mut stdout = std::io::stdout();

    // 先进入 raw mode 再画首帧，这样等待期间能收到 Resize / F5 / F6 事件并重绘。
    crossterm::terminal::enable_raw_mode()?;
    let result = (|| -> anyhow::Result<(Lang, Theme, bool)> {
        // 开屏绘制会多次移动输出位置，隐藏硬件光标避免用户看到跳动。
        crossterm::execute!(stdout, cursor::Hide)?;
        let (mut cols, mut rows) = terminal::size()?;
        // 首帧绘制前获得耗时快照并冻结（包含全部初始化与终端就绪开销）
        let boot_duration = horae_core::time::boot_elapsed();
        loop {
            draw_frame_with(
                &mut stdout,
                cols,
                rows,
                lang,
                &theme,
                omarchy_auto,
                boot_duration,
                icon_style,
            )?;
            stdout.flush()?;
            let mut redraw = false;
            while !redraw {
                if !event::poll(std::time::Duration::from_millis(100))? {
                    continue;
                }
                match event::read()? {
                    // F6 切换语言（与应用一致），写回 settings 并重绘。
                    Event::Key(key) if key.code == KeyCode::F(6) => {
                        lang = lang.toggle();
                        let _ = horae_core::repo::settings::set(
                            conn,
                            "lang",
                            if lang.is_zh() { "zh" } else { "en" },
                        );
                        redraw = true;
                    }
                    // F5 切换主题（与应用一致），写回 settings 并重绘。
                    Event::Key(key) if key.code == KeyCode::F(5) => {
                        let (next_theme, next_auto, key_str) =
                            crate::tui::theme::cycle_theme(&theme, omarchy_auto);
                        let _ = horae_core::repo::settings::set(conn, "theme", key_str);
                        theme = next_theme;
                        omarchy_auto = next_auto;
                        redraw = true;
                    }
                    Event::Key(_) => return Ok((lang, theme, omarchy_auto)),
                    Event::Resize(c, r) => {
                        cols = c;
                        rows = r;
                        redraw = true;
                    }
                    _ => {}
                }
            }
        }
    })();
    let _ = crossterm::execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        cursor::MoveTo(0, 0),
        cursor::Show
    );
    let _ = write!(stdout, "\x1b[0m");
    let _ = stdout.flush();
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

/// 一帧开屏的纵向布局（各元素起始行与卡片列宽）。
#[derive(Debug, PartialEq, Eq)]
struct SplashLayout {
    banner_y: u16,
    banner_pad_top: u16,
    banner_pad_bot: u16,
    subtitle_y: u16,
    slogan_y: u16,
    cards_y: Option<u16>,
    prompt_y: Option<u16>,
    footer_y: u16,
    card_width: u16,
}

/// 纵向布局推演（纯函数）：根据终端行列自适应布局。
fn splash_layout(cols: u16, rows: u16, logo_h: u16) -> SplashLayout {
    let show_cards = rows >= 20 && cols >= 45;
    let show_prompt = rows >= 18;

    let banner_pad_top = if rows >= 32 {
        2
    } else if rows >= 22 {
        1
    } else {
        0
    };
    let banner_pad_bot = if rows >= 32 {
        2
    } else if rows >= 22 {
        1
    } else {
        0
    };
    let banner_h = banner_pad_top + logo_h + banner_pad_bot;

    let footer_y = rows.saturating_sub(1);

    let card_width = if cols >= 67 {
        16
    } else if cols >= 55 {
        13
    } else {
        10
    };

    let cards_h = if show_cards { 3 } else { 0 };
    let gap_banner_sub = 1;
    let gap_sub_slogan = if rows >= 26 { 1 } else { 0 };
    let gap_slogan_cards = if show_cards {
        if rows >= 26 {
            2
        } else {
            1
        }
    } else {
        0
    };

    let content_h = banner_h + gap_banner_sub + 1 + gap_sub_slogan + 1 + gap_slogan_cards + cards_h;

    let avail = footer_y.saturating_sub(content_h + if show_prompt { 2 } else { 0 });
    let top_margin = if rows >= 24 {
        (avail * 2 / 5).max(1) + 1
    } else {
        avail / 2
    };

    let banner_y = top_margin;
    let subtitle_y = banner_y + banner_h + gap_banner_sub;
    let slogan_y = subtitle_y + 1 + gap_sub_slogan;
    let cards_y = if show_cards {
        Some(slogan_y + 1 + gap_slogan_cards)
    } else {
        None
    };

    let prompt_y = if show_prompt {
        if let Some(cy) = cards_y {
            let space = footer_y.saturating_sub(cy + cards_h);
            if space >= 2 {
                Some(cy + cards_h + space / 2)
            } else {
                None
            }
        } else {
            let space = footer_y.saturating_sub(slogan_y + 1);
            if space >= 2 {
                Some(slogan_y + 1 + space / 2)
            } else {
                None
            }
        }
    } else {
        None
    };

    SplashLayout {
        banner_y,
        banner_pad_top,
        banner_pad_bot,
        subtitle_y,
        slogan_y,
        cards_y,
        prompt_y,
        footer_y,
        card_width,
    }
}

/// 清屏并绘制一帧自适应系统主题的开屏内容。
#[allow(clippy::too_many_arguments)]
fn draw_frame_with<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    rows: u16,
    lang: Lang,
    theme: &Theme,
    omarchy_auto: bool,
    boot_duration: Option<std::time::Duration>,
    icon_style: IconStyle,
) -> anyhow::Result<()> {
    use crossterm::{cursor, ExecutableCommand};

    let logo_h = HORAE_LOGO.len() as u16;
    let logo_w = HORAE_LOGO
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(0);
    let lay = splash_layout(cols, rows, logo_h);

    let palette = SplashPalette::from_theme(theme);

    out.execute(crossterm::terminal::Clear(
        crossterm::terminal::ClearType::All,
    ))?;

    let banner_bg = palette.banner_bg;
    let banner_fg = palette.banner_fg;

    let draw_banner_line =
        |out: &mut W, y: u16, text: &str, sgr: &str, block_w: Option<u16>| -> std::io::Result<()> {
            let text_w = text.width() as u16;
            let w = block_w.unwrap_or(text_w);
            let x = center_x(cols, w);
            let left_pad = " ".repeat(x as usize);
            let printed_w = x + text_w;
            let right_pad = " ".repeat(cols.saturating_sub(printed_w) as usize);

            out.execute(cursor::MoveTo(0, y))?;
            write!(
                out,
                "{}{}{}{}{}\x1b[0m{}{}{}\x1b[0m",
                bg(banner_bg),
                fg(banner_fg),
                left_pad,
                sgr,
                text,
                bg(banner_bg),
                fg(banner_fg),
                right_pad
            )
        };

    // 1. 主题自适应通栏横幅与 HORAE 像素字标
    for i in 0..lay.banner_pad_top {
        draw_banner_line(out, lay.banner_y + i, "", "", None)?;
    }
    for (i, line) in HORAE_LOGO.iter().enumerate() {
        draw_banner_line(
            out,
            lay.banner_y + lay.banner_pad_top + i as u16,
            line,
            "\x1b[1m",
            Some(logo_w),
        )?;
    }
    for i in 0..lay.banner_pad_bot {
        draw_banner_line(
            out,
            lay.banner_y + lay.banner_pad_top + logo_h + i,
            "",
            "",
            None,
        )?;
    }

    // 2. 带分割细线的品牌副标题 `───── Goddess of Time ─────`
    draw_subtitle(out, cols, lay.subtitle_y, BRAND_SUBTITLE, &palette)?;

    // 3. 核心哲学标语 `Ideas in. Time accounted.`
    let slogan = tr!(lang, "心念所至 · 岁月有迹", "Ideas in. Time accounted.");
    write_centered(out, cols, lay.slogan_y, "", slogan, palette.fg)?;

    // 4. 四列特性指标卡片（极速启动、快速捕获、时间资产、GTD心流）
    if let Some(cy) = lay.cards_y {
        let (icon_bolt, icon_pencil, icon_clock, icon_workflow) = match icon_style {
            IconStyle::Nerd => ("\u{f0e7}", "\u{f040}", "\u{f017}", "\u{f0e8}"),
            IconStyle::Ascii => ("*", "+", "@", "&"),
        };

        let boot_str = boot_duration
            .map(horae_core::time::format_boot_elapsed)
            .unwrap_or_else(|| "3ms".to_string());
        let val_time = tr!(lang, "time", "time");
        let val_gtd = tr!(lang, "gtd", "gtd");

        let lbl_startup = tr!(lang, "极速启动", "startup");
        let lbl_capture = tr!(lang, "闪电捕获", "capture");
        let lbl_asset = tr!(lang, "时间资产", "asset");
        let lbl_workflow = tr!(lang, "心流流程", "workflow");

        let total_w = lay.card_width * 4 + 3;
        let start_x = center_x(cols, total_w);
        let card_w = lay.card_width as usize;

        // 行 0：图标
        draw_card_row(
            out,
            start_x,
            cy,
            [icon_bolt, icon_pencil, icon_clock, icon_workflow],
            card_w,
            "",
            palette.accent,
            palette.border,
        )?;
        // 行 1：指标值
        draw_card_row(
            out,
            start_x,
            cy + 1,
            [&boot_str, "<10s", val_time, val_gtd],
            card_w,
            "\x1b[1m",
            palette.fg,
            palette.border,
        )?;
        // 行 2：副标签
        draw_card_row(
            out,
            start_x,
            cy + 2,
            [lbl_startup, lbl_capture, lbl_asset, lbl_workflow],
            card_w,
            "",
            palette.text_dim,
            palette.border,
        )?;
    }

    // 5. 交互提示（暗色慢闪，智能展示按键指引）
    if let Some(py) = lay.prompt_y {
        let prompt = prompts(lang, cols);
        write_centered(out, cols, py, "\x1b[5m", prompt, palette.text_dim)?;
    }

    // 6. 底部信息行：版本（沙漏图标）、作者（GitHub 图标 + 点击跳转隐藏链接）、Star（星标图标 + 仓库跳转隐藏链接）、主题徽章
    let (icon_hourglass, icon_github, icon_star, icon_palette) = match icon_style {
        IconStyle::Nerd => ("\u{f252} ", "\u{f09b} ", "\u{f005} ", "\u{f042} "),
        IconStyle::Ascii => ("", "", "* ", ""),
    };

    let version = format!("{icon_hourglass}horae v{}", env!("CARGO_PKG_VERSION"));
    let author_plain = format!("{icon_github}by zhaohang1205");
    let star_plain = format!("{icon_star}Star on GitHub");

    let author_link = format!("\x1b]8;;{AUTHOR_URL}\x1b\\{author_plain}\x1b]8;;\x1b\\");
    let star_link = format!("\x1b]8;;{REPO_URL}\x1b\\{star_plain}\x1b]8;;\x1b\\");

    let theme_badge = if omarchy_auto {
        format!("{icon_palette}{}", tr!(lang, "系统自适应", "Auto"))
    } else {
        format!("{icon_palette}{}", theme.id.display_name(lang))
    };

    let star_short = format!("{icon_star}Star");
    let star_short_link = format!("\x1b]8;;{REPO_URL}\x1b\\{star_short}\x1b]8;;\x1b\\");

    let (visible_footer, full_footer) = if cols >= 80 {
        (
            format!("{version}  ·  {author_plain}  ·  {star_plain}  ·  {theme_badge}"),
            format!("{version}  ·  {author_link}  ·  {star_link}  ·  {theme_badge}"),
        )
    } else if cols >= 60 {
        (
            format!("{version}  ·  {author_plain}  ·  {star_plain}"),
            format!("{version}  ·  {author_link}  ·  {star_link}"),
        )
    } else if cols >= 45 {
        (
            format!("{version} · {author_plain} · {star_short}"),
            format!("{version} · {author_link} · {star_short_link}"),
        )
    } else {
        (
            format!("{version} · {star_short}"),
            format!("{version} · {star_short_link}"),
        )
    };

    let start_x = center_x(cols, visible_footer.width() as u16);
    out.execute(cursor::MoveTo(start_x, lay.footer_y))?;
    write!(out, "{}{full_footer}\x1b[0m", fg(palette.text_dim))?;

    Ok(())
}

#[cfg(test)]
mod splash_tests {
    use super::*;

    #[test]
    fn horae_logo_has_expected_rows() {
        assert_eq!(HORAE_LOGO.len(), 9, "艺术字应为 9 行");
        for l in HORAE_LOGO {
            assert!(!l.trim().is_empty(), "艺术字每行不应为空");
        }
        let w = HORAE_LOGO
            .iter()
            .map(|l| l.width() as u16)
            .max()
            .unwrap_or(0);
        assert!(w > 0 && w < 200, "艺术字宽度应在合理范围，实际 {w}");
    }

    #[test]
    fn unicode_width_counts_cjk_double() {
        assert_eq!("你".width(), 2);
        assert_eq!("H".width(), 1);
        assert_eq!("█".width(), 1, "方块字应计 1 宽");
        assert_eq!("📥".width(), 2, "emoji 应计 2 宽");
        assert_eq!("HORAE".width(), 5);
    }

    #[test]
    fn splash_layout_centers_block() {
        let lay = splash_layout(80, 24, 9);
        assert_eq!(lay.banner_y, 2);
        assert_eq!(lay.subtitle_y, 14);
        assert_eq!(lay.slogan_y, 15);
        assert_eq!(lay.cards_y, Some(17));
        assert_eq!(lay.prompt_y, Some(21));
        assert_eq!(lay.footer_y, 23);
        assert_eq!(lay.card_width, 16);
    }

    #[test]
    fn splash_palette_meets_wcag_contrast_across_themes() {
        // 测试深色主题（Mocha）与亮色主题（Latte / FlexokiLight）
        let themes = [
            Theme::catppuccin_mocha(),
            Theme::catppuccin_latte(),
            Theme::flexoki_light(),
            Theme::gruvbox(),
            Theme::nord(),
        ];

        for t in &themes {
            let palette = SplashPalette::from_theme(t);
            let contrast = contrast_ratio(palette.banner_bg, palette.banner_fg);
            assert!(
                contrast >= 4.5,
                "主题 {:?} 的横幅字标对比度应满足 WCAG AA 规范 (>= 4.5)，实际为 {contrast:.2}",
                t.id
            );
        }
    }

    #[test]
    fn draw_frame_renders_clean_text_splash() {
        let theme = Theme::catppuccin_mocha();
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf,
            80,
            24,
            Lang::Zh,
            &theme,
            false,
            None,
            IconStyle::Nerd,
        )
        .unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("█"), "应绘制 HORAE 艺术字");
        assert!(s.contains(BRAND_SUBTITLE), "应绘制品牌副标题");
        assert!(s.contains("horae v"), "应包含 horae 及其版本号");
        assert!(s.contains("Star on GitHub"), "应包含 Star 引导标识");
        assert!(s.contains("by zhaohang1205"), "应包含作者信息");
        assert!(s.contains(AUTHOR_URL), "应包含作者 github 跳转链接");
        assert!(s.contains(REPO_URL), "应包含 horae 仓库跳转链接");
        assert!(s.contains("\u{f252}"), "应包含沙漏图标");
        assert!(s.contains("\u{f09b}"), "应包含 GitHub 图标");
        assert!(s.contains("\u{f005}"), "应包含 Star 图标");
        assert!(s.contains("<10s"), "应绘制特性卡片");
        assert!(!s.contains("\x1b_G"), "不应包含任何 Kitty 图形协议控制指令");
        // 验证中间提示行不提示快捷键点赞
        assert!(
            !s.contains("S 点赞"),
            "开屏中间提示行不应包含快捷键点赞提示"
        );
        assert!(
            s.contains("按任意键开启"),
            "开屏中间提示行应包含常规按任意键开启提示"
        );
    }

    #[test]
    fn draw_frame_shows_boot_ms_bilingual() {
        use std::time::Duration;
        let theme = Theme::catppuccin_mocha();
        // 打点后卡片应携带启动用时
        for lang in [Lang::Zh, Lang::En] {
            let mut buf = std::io::Cursor::new(Vec::new());
            draw_frame_with(
                &mut buf,
                80,
                24,
                lang,
                &theme,
                false,
                Some(Duration::from_millis(42)),
                IconStyle::Nerd,
            )
            .unwrap();
            let s = String::from_utf8(buf.into_inner()).unwrap();
            assert!(s.contains("by zhaohang1205"), "版本行应存在");
            assert!(s.contains("42ms"), "{lang:?} 卡片应显示「42ms」");
            assert!(s.contains("Star on GitHub"), "应包含 Star on GitHub");
        }

        // 亚毫秒耗时应显示真实的浮点毫秒
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf,
            80,
            24,
            Lang::Zh,
            &theme,
            false,
            Some(Duration::from_micros(800)),
            IconStyle::Nerd,
        )
        .unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("0.8ms"), "亚毫秒耗时应显示「0.8ms」");
    }

    #[test]
    fn draw_frame_supports_ascii_icon_style() {
        use std::time::Duration;
        let theme = Theme::catppuccin_mocha();
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf,
            80,
            24,
            Lang::En,
            &theme,
            false,
            Some(Duration::from_millis(3)),
            IconStyle::Ascii,
        )
        .unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("startup"), "英文卡片应显示 startup");
        assert!(s.contains("Ideas in. Time accounted."));
        assert!(s.contains("│"), "卡片应有竖线分隔符");
        assert!(
            !s.contains("\u{f252}"),
            "ASCII 模式不应含 Nerd Font 沙漏图标"
        );
        assert!(
            s.contains("* Star on GitHub"),
            "ASCII 模式应以星号显示 Star 引导"
        );
    }

    #[test]
    fn draw_frame_adapts_to_theme_and_displays_badge() {
        let mocha = Theme::catppuccin_mocha();
        let latte = Theme::catppuccin_latte();

        // 验证深色 Mocha
        let mut buf_mocha = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf_mocha,
            88,
            24,
            Lang::Zh,
            &mocha,
            false,
            None,
            IconStyle::Nerd,
        )
        .unwrap();
        let s_mocha = String::from_utf8(buf_mocha.into_inner()).unwrap();
        assert!(s_mocha.contains("Catppuccin 摩卡"));

        // 验证亮色 Latte
        let mut buf_latte = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf_latte,
            88,
            24,
            Lang::Zh,
            &latte,
            false,
            None,
            IconStyle::Nerd,
        )
        .unwrap();
        let s_latte = String::from_utf8(buf_latte.into_inner()).unwrap();
        assert!(s_latte.contains("Catppuccin 拿铁"));

        // 验证系统自动跟随模式
        let mut buf_auto = std::io::Cursor::new(Vec::new());
        draw_frame_with(
            &mut buf_auto,
            88,
            24,
            Lang::Zh,
            &mocha,
            true,
            None,
            IconStyle::Nerd,
        )
        .unwrap();
        let s_auto = String::from_utf8(buf_auto.into_inner()).unwrap();
        assert!(s_auto.contains("系统自适应"));
    }
}
