//! 开屏（splash）：ASCII 艺术字、GTD 标语展示与按键等待。

use horae_core::i18n::Lang;
use unicode_width::UnicodeWidthStr;

use crate::tui::icons::IconStyle;

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

/// Catppuccin 点缀色（RGB），与 `theme.rs` 的 Mocha 保持一致。
type Rgb = (u8, u8, u8);
const ROSEWATER: Rgb = (245, 224, 220);
const TEXT: Rgb = (205, 214, 244);
const OVERLAY0: Rgb = (108, 112, 134);
const CRUST: Rgb = (17, 17, 27);

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
) -> std::io::Result<()> {
    use crossterm::{cursor::MoveTo, ExecutableCommand};
    let dash = "─────";
    let full_text = format!("{dash} {brand} {dash}");
    let start_x = center_x(cols, full_text.width() as u16);

    out.execute(MoveTo(start_x, y))?;
    write!(out, "{}{dash} \x1b[0m", fg(OVERLAY0))?;
    write!(out, "{}{brand}\x1b[0m", fg(ROSEWATER))?;
    write!(out, "{} {dash}\x1b[0m", fg(OVERLAY0))?;
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

/// 底部提示：主提示语。
fn prompts(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "按任意键开始…",
        Lang::En => "Press any key to start…",
    }
}

pub(super) fn show_splash(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use crossterm::{
        cursor,
        event::{self, Event, KeyCode},
        terminal,
    };
    use std::io::Write;

    let mut stdout = std::io::stdout();

    // 从 settings 表恢复语言与图标风格。
    let mut lang = match horae_core::repo::settings::get(conn, "lang")
        .ok()
        .flatten()
        .as_deref()
    {
        Some("en") => Lang::En,
        _ => Lang::Zh,
    };
    let icon_style = IconStyle::load(conn);

    // 先进入 raw mode 再画首帧，这样等待期间能收到 Resize / F6 事件并重绘。
    crossterm::terminal::enable_raw_mode()?;
    let result = (|| -> anyhow::Result<()> {
        // 开屏绘制会多次移动输出位置，隐藏硬件光标避免用户看到跳动。
        crossterm::execute!(stdout, cursor::Hide)?;
        let (mut cols, mut rows) = terminal::size()?;
        loop {
            draw_frame_with(
                &mut stdout,
                cols,
                rows,
                lang,
                horae_core::time::boot_elapsed_ms(),
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
                    Event::Key(_) => return Ok(()),
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

/// 清屏并绘制一帧纯文本开屏内容。
fn draw_frame_with<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    rows: u16,
    lang: Lang,
    boot_ms: Option<u128>,
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

    out.execute(crossterm::terminal::Clear(
        crossterm::terminal::ClearType::All,
    ))?;

    let banner_bg = ROSEWATER;
    let banner_fg = CRUST;

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

    // 1. 粉色通栏横幅与 HORAE 像素字标
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
    draw_subtitle(out, cols, lay.subtitle_y, BRAND_SUBTITLE)?;

    // 3. 核心哲学标语 `Ideas in. Time accounted.`
    let slogan = tr!(lang, "心念所至 · 岁月有迹", "Ideas in. Time accounted.");
    write_centered(out, cols, lay.slogan_y, "", slogan, TEXT)?;

    // 4. 四列特性指标卡片（极速启动、快速捕获、时间资产、GTD心流）
    if let Some(cy) = lay.cards_y {
        let (icon_bolt, icon_pencil, icon_clock, icon_workflow) = match icon_style {
            IconStyle::Nerd => ("\u{f0e7}", "\u{f040}", "\u{f017}", "\u{f0e8}"),
            IconStyle::Ascii => ("*", "+", "@", "&"),
        };

        let boot_str = boot_ms
            .map(|ms| format!("{ms}ms"))
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
            ROSEWATER,
            OVERLAY0,
        )?;
        // 行 1：指标值
        draw_card_row(
            out,
            start_x,
            cy + 1,
            [&boot_str, "<10s", val_time, val_gtd],
            card_w,
            "\x1b[1m",
            TEXT,
            OVERLAY0,
        )?;
        // 行 2：副标签
        draw_card_row(
            out,
            start_x,
            cy + 2,
            [lbl_startup, lbl_capture, lbl_asset, lbl_workflow],
            card_w,
            "",
            OVERLAY0,
            OVERLAY0,
        )?;
    }

    // 5. 交互提示（暗色闪烁）
    if let Some(py) = lay.prompt_y {
        let prompt = prompts(lang);
        write_centered(out, cols, py, "\x1b[5m", prompt, OVERLAY0)?;
    }

    // 6. 底部信息行：版本（沙漏图标）、作者（GitHub 图标 + 点击跳转隐藏链接）、RUST BUILT（Rust 图标）
    let (icon_hourglass, icon_github, icon_rust) = match icon_style {
        IconStyle::Nerd => ("\u{f252} ", "\u{f09b} ", "\u{e7a8} "),
        IconStyle::Ascii => ("", "", ""),
    };

    let version = format!("{icon_hourglass}horae v{}", env!("CARGO_PKG_VERSION"));
    let author_plain = format!("{icon_github}by zhaohang1205");
    let rust_built = format!("{icon_rust}RUST BUILT");

    let author_link =
        format!("\x1b]8;;https://github.com/zhaohang1205\x1b\\{author_plain}\x1b]8;;\x1b\\");

    let (visible_footer, full_footer) = if cols >= 54 {
        (
            format!("{version}  ·  {author_plain}  ·  {rust_built}"),
            format!("{version}  ·  {author_link}  ·  {rust_built}"),
        )
    } else {
        let rust_short = format!("{icon_rust}RUST");
        (
            format!("{version} · {author_plain} · {rust_short}"),
            format!("{version} · {author_link} · {rust_short}"),
        )
    };

    let start_x = center_x(cols, visible_footer.width() as u16);
    out.execute(cursor::MoveTo(start_x, lay.footer_y))?;
    write!(out, "{}{full_footer}\x1b[0m", fg(OVERLAY0))?;

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
    fn draw_frame_renders_clean_text_splash() {
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(&mut buf, 80, 24, Lang::Zh, None, IconStyle::Nerd).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("█"), "应绘制 HORAE 艺术字");
        assert!(s.contains(BRAND_SUBTITLE), "应绘制品牌副标题");
        assert!(s.contains("horae v"), "应包含 horae 及其版本号");
        assert!(s.contains("RUST BUILT"), "应包含 RUST BUILT 标识");
        assert!(s.contains("by zhaohang1205"), "应包含作者信息");
        assert!(
            s.contains("https://github.com/zhaohang1205"),
            "应包含 github 跳转链接"
        );
        assert!(s.contains("\u{f252}"), "应包含沙漏图标");
        assert!(s.contains("\u{f09b}"), "应包含 GitHub 图标");
        assert!(s.contains("\u{e7a8}"), "应包含 Rust 图标");
        assert!(s.contains("<10s"), "应绘制特性卡片");
        assert!(!s.contains("\x1b_G"), "不应包含任何 Kitty 图形协议控制指令");
    }

    #[test]
    fn draw_frame_shows_boot_ms_bilingual() {
        // 打点后卡片应携带启动用时
        for lang in [Lang::Zh, Lang::En] {
            let mut buf = std::io::Cursor::new(Vec::new());
            draw_frame_with(&mut buf, 80, 24, lang, Some(42), IconStyle::Nerd).unwrap();
            let s = String::from_utf8(buf.into_inner()).unwrap();
            assert!(s.contains("by zhaohang1205"), "版本行应存在");
            assert!(s.contains("42ms"), "{lang:?} 卡片应显示「42ms」");
            assert!(s.contains("RUST BUILT"), "应包含 RUST BUILT");
        }
    }

    #[test]
    fn draw_frame_supports_ascii_icon_style() {
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(&mut buf, 80, 24, Lang::En, Some(3), IconStyle::Ascii).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("startup"), "英文卡片应显示 startup");
        assert!(s.contains("Ideas in. Time accounted."));
        assert!(s.contains("│"), "卡片应有竖线分隔符");
        assert!(
            !s.contains("\u{f252}"),
            "ASCII 模式不应含 Nerd Font 沙漏图标"
        );
    }
}
