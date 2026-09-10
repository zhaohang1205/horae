//! 开屏（splash）：ASCII 艺术字、GTD 标语展示与按键等待。

use horae_core::i18n::Lang;

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

use unicode_width::UnicodeWidthStr;

/// 水平居中所需的起始列。
fn center_x(cols: u16, w: u16) -> u16 {
    if cols > w {
        (cols - w) / 2
    } else {
        0
    }
}

/// Catppuccin 点缀色（RGB），与 `theme.rs` 的 Mocha 保持一致。
type Rgb = (u8, u8, u8);
const ROSEWATER: Rgb = (245, 224, 220);
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

    // 从 settings 表恢复语言（与应用一致：en → 英文，否则中文）。
    let mut lang = match horae_core::repo::settings::get(conn, "lang")
        .ok()
        .flatten()
        .as_deref()
    {
        Some("en") => Lang::En,
        _ => Lang::Zh,
    };

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

/// 纵向布局常量（单位：终端行）。
const TOP_MARGIN: u16 = 2;
const BOTTOM_MARGIN: u16 = 2; // 版本行上方留白（版本行占最后一行）
const SUBTITLE_H: u16 = 1; // 品牌副标题占一行
const PROMPT_H: u16 = 1; // 提示语
const GAP_WORD_SUBTITLE: u16 = 3;
const BANNER_PAD_TOP: u16 = 2;
const BANNER_PAD_BOT: u16 = 2; // 字标与副标题之间
const GAP_SUBTITLE_PROMPT: u16 = 2; // 副标题区与提示语之间

/// 一帧开屏的纵向布局（各元素起始行）。
#[derive(Debug, PartialEq, Eq)]
struct SplashLayout {
    logo_y: u16,
    subtitle_y: u16,
}

/// 纵向布局推演（纯函数）：将文本组合块在可用区内垂直居中。
fn splash_layout(rows: u16, logo_h: u16) -> SplashLayout {
    // 组合块高 = 字标 + 副标题，在可用区内取中。
    let comp_h = BANNER_PAD_TOP + logo_h + BANNER_PAD_BOT.max(GAP_WORD_SUBTITLE) + SUBTITLE_H;
    let prompt_y = rows.saturating_sub(BOTTOM_MARGIN + PROMPT_H);
    let avail = prompt_y.saturating_sub(GAP_SUBTITLE_PROMPT + TOP_MARGIN);
    let logo_y = TOP_MARGIN + avail.saturating_sub(comp_h) / 2 + BANNER_PAD_TOP;

    let subtitle_y = logo_y + logo_h + GAP_WORD_SUBTITLE;
    SplashLayout { logo_y, subtitle_y }
}

/// 清屏并绘制一帧纯文本开屏内容。
fn draw_frame_with<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    rows: u16,
    lang: Lang,
    boot_ms: Option<u128>,
) -> anyhow::Result<()> {
    use crossterm::{cursor, ExecutableCommand};

    let logo_h = HORAE_LOGO.len() as u16;
    let logo_w = HORAE_LOGO
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(0);
    let lay = splash_layout(rows, logo_h);

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

    // 绘制横幅背景与内容
    // 顶部留白
    for i in 0..BANNER_PAD_TOP {
        draw_banner_line(out, lay.logo_y - BANNER_PAD_TOP + i, "", "", None)?;
    }

    // Logo (只有Logo带有横幅背景色)
    for (i, line) in HORAE_LOGO.iter().enumerate() {
        draw_banner_line(out, lay.logo_y + i as u16, line, "\x1b[1m", Some(logo_w))?;
    }

    // 底部留白
    for i in 0..BANNER_PAD_BOT {
        draw_banner_line(out, lay.logo_y + logo_h + i, "", "", None)?;
    }

    // 副标题 (移除横幅背景，使用系统透明底色)
    write_centered(out, cols, lay.subtitle_y, "", BRAND_SUBTITLE, ROSEWATER)?;

    // 3. 提示语（闪烁暗色）；版本、作者与启动用时（底部居中、暗）
    let prompt = prompts(lang);
    let prompt_y = rows.saturating_sub(BOTTOM_MARGIN + PROMPT_H);
    write_centered(out, cols, prompt_y, "\x1b[5m", prompt, OVERLAY0)?;
    let mut version = format!("v{} · by zhaohang1205", env!("CARGO_PKG_VERSION"));
    if let Some(ms) = boot_ms {
        version.push_str(&tr!(lang, " · 启动用时 {}ms", " · started in {}ms", ms));
    }
    write_centered(out, cols, rows.saturating_sub(1), "", &version, OVERLAY0)?;

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
        let lay = splash_layout(24, 9);
        assert_eq!(lay.logo_y, 5);
        assert_eq!(lay.subtitle_y, 17);
    }

    #[test]
    fn draw_frame_renders_clean_text_splash() {
        let mut buf = std::io::Cursor::new(Vec::new());
        draw_frame_with(&mut buf, 80, 24, Lang::Zh, None).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("█"), "应绘制 HORAE 艺术字");
        assert!(s.contains(BRAND_SUBTITLE), "应绘制品牌副标题");
        assert!(!s.contains("启动用时"), "未打点时不应出现启动用时段");
        assert!(!s.contains("\x1b_G"), "不应包含任何 Kitty 图形协议控制指令");
    }

    #[test]
    fn draw_frame_shows_boot_ms_bilingual() {
        // 打点后底部版本行应携带启动用时；中英文措辞随语言切换。
        for (lang, needle) in [(Lang::Zh, "启动用时 42ms"), (Lang::En, "started in 42ms")] {
            let mut buf = std::io::Cursor::new(Vec::new());
            draw_frame_with(&mut buf, 80, 24, lang, Some(42)).unwrap();
            let s = String::from_utf8(buf.into_inner()).unwrap();
            assert!(s.contains("by zhaohang1205"), "版本行应存在");
            assert!(s.contains(needle), "{lang:?} 应显示「{needle}」");
        }
    }
}
