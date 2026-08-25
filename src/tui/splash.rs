//! 开屏（splash）：PNG 加载、Kitty 图形协议输出与按键等待。
//! 用户可用 `~/.config/horae/splash.png` 覆盖内置图；`HORAE_FORCE_KITTY_SPLASH`
//! 强制启用图形协议，`HORAE_SPLASH_DEBUG` 写诊断日志。

use crate::i18n::Lang;

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

/// 从指定目录（或 `None` 表示走 `config_dir()`）加载开屏图，便于测试。
/// 优先用户覆盖文件，否则内置默认图；均非 PNG 则放弃显示。
fn load_splash_png_from(override_dir: Option<&std::path::Path>) -> Option<Vec<u8>> {
    let base = match override_dir {
        Some(d) => d.to_path_buf(),
        None => crate::config::config_dir(),
    };
    let override_path = base.join("splash.png");
    if let Ok(b) = std::fs::read(&override_path) {
        if looks_like_png(&b) {
            return Some(b);
        }
    }
    let bundled = include_bytes!("../../assets/horae.png");
    if looks_like_png(bundled) {
        Some(bundled.to_vec())
    } else {
        None
    }
}

fn looks_like_png(b: &[u8]) -> bool {
    b.len() >= 8 && &b[0..8] == b"\x89PNG\r\n\x1a\n"
}

/// 从 PNG IHDR 解析像素尺寸（无需完整解码，零依赖）。
fn png_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if !looks_like_png(b) || b.len() < 24 {
        return None;
    }
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

/// 开屏图占用的单元格尺寸：在可用区内保持宽高比取最大（字符高≈2×列宽，行数减半）。
fn png_cell_size(bytes: &[u8], avail_cols: u16, avail_rows: u16) -> Option<(u16, u16)> {
    let (iw, ih) = png_dimensions(bytes)?;
    if avail_cols == 0 || avail_rows == 0 {
        return None;
    }
    let row_per_col = f64::from(ih) / f64::from(iw) * 0.5;
    let mut w = f64::from(avail_cols.max(4));
    let mut h = w * row_per_col;
    if h > f64::from(avail_rows) {
        h = f64::from(avail_rows);
        w = (h / row_per_col).max(4.0);
    }
    let (w, h) = (w.round() as u16, h.round() as u16);
    (w > 0 && h > 0).then_some((w, h))
}

/// 当前终端是否支持 Kitty 图形协议（Ghostty / Kitty / WezTerm 等均支持）。
fn is_kitty_terminal() -> bool {
    // 调试/强制开关：设 `HORAE_FORCE_KITTY_SPLASH=1` 可跳过下面的探测。
    if std::env::var_os("HORAE_FORCE_KITTY_SPLASH").is_some() {
        return true;
    }
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let lower = |v: String| v.to_ascii_lowercase();
    let tp = std::env::var("TERM_PROGRAM").map(lower).unwrap_or_default();
    if tp.contains("kitty") || tp.contains("ghostty") || tp.contains("wezterm") {
        return true;
    }
    let term = std::env::var("TERM").map(lower).unwrap_or_default();
    term.contains("kitty") || term.contains("ghostty") || term.contains("wezterm")
}

/// 内联标准 base64 编码（含填充），避免引入额外依赖。
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 用 Kitty 图形协议传输 PNG（base64 分块）。`a=T,f=100,c=,r=` 必须在**首块**声明、
/// 末块以 `m=0` 收尾，否则 Ghostty 等实现不会触发显示。
fn write_kitty_image<W: std::io::Write>(
    out: &mut W,
    bytes: &[u8],
    w: u16,
    h: u16,
) -> std::io::Result<()> {
    let b64 = base64_encode(bytes);
    let chunk_size = 4096usize;
    let id = 1u32;
    let n_chunks = b64.len().div_ceil(chunk_size);
    splash_debug(format!("kitty n_chunks={n_chunks} b64_len={}", b64.len()));
    for (i, chunk) in b64.as_bytes().chunks(chunk_size).enumerate() {
        let first = i == 0;
        let last = i + 1 == n_chunks;
        let mut control = String::from("\x1b_G");
        if first {
            // 显示动作 + PNG 格式 + 单元格尺寸（列/行）都在首块声明。
            control.push_str(&format!("a=T,f=100,c={},r={},", w, h));
        }
        control.push_str(&format!("i={},", id));
        control.push_str(&format!("m={},", if last { 0 } else { 1 }));
        control.push_str("q=2;");
        if first {
            splash_debug(format!(
                "kitty first control={:?} first_data_preview={:?}",
                control,
                &chunk[..chunk.len().min(24)]
            ));
        }
        if last {
            splash_debug(format!("kitty last control={:?}", control));
        }
        write!(
            out,
            "{}{}\x1b\\",
            control,
            std::str::from_utf8(chunk).unwrap()
        )?;
    }
    Ok(())
}

/// 开屏图诊断日志（仅当 `HORAE_SPLASH_DEBUG` 设置时写入临时文件）。
fn splash_debug(msg: impl std::fmt::Display) {
    if std::env::var_os("HORAE_SPLASH_DEBUG").is_some() {
        use std::io::Write;
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("horae_splash.log"))
            .and_then(|mut f| writeln!(f, "{}", msg));
    }
}

/// 字符是否为全角（CJK 等占 2 列；方块字/emoji/ASCII 计 1，figlet 与 📥 才不会翻倍）。
fn is_wide(c: char) -> bool {
    matches!(
        c as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
    )
}

/// 显示宽度（列数）：CJK 计 2，其余计 1。
fn disp_width(s: &str) -> u16 {
    s.chars().map(|c| if is_wide(c) { 2 } else { 1 }).sum()
}

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
const MAUVE: Rgb = (203, 166, 247);
const PEACH: Rgb = (250, 179, 135);
const PINK: Rgb = (245, 194, 231);
const OVERLAY0: Rgb = (108, 112, 134);

/// 前景色转义序列。
fn fg(color: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2)
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
    out.execute(MoveTo(center_x(cols, disp_width(text)), y))?;
    write!(out, "{sgr}{}{text}\x1b[0m", fg(color))
}

/// 标语：固定为 David Allen 名言（中英文状态都显示英文）。
const TAGLINE: &str = "Your mind is for having ideas, not holding them.";
/// 名言作者署名（名言恒为英文，署名随之保持英文）。
const QUOTE_AUTHOR: &str = "—— David Allen";
/// 品牌副标题：仅在无图回退时显示，填补 hero 空白、呼应「时间女神」。
const BRAND_SUBTITLE: &str = "Goddess of Time";

/// 底部提示：（主提示语，F6 语言提示）。
fn prompts(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::Zh => ("按任意键开始…", "F6 切换语言"),
        Lang::En => ("Press any key to start…", "F6 toggle language"),
    }
}

pub(super) fn show_splash(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode},
        terminal,
    };
    use std::io::Write;

    let mut stdout = std::io::stdout();
    let splash_png = load_splash_png_from(None);

    // 从 settings 表恢复语言（与应用一致：en → 英文，否则中文）。
    let mut lang = match crate::repo::settings::get(conn, "lang")
        .ok()
        .flatten()
        .as_deref()
    {
        Some("en") => Lang::En,
        _ => Lang::Zh,
    };

    splash_debug(format!(
        "start lang={:?} is_kitty={} force={:?} TERM={:?} TERM_PROGRAM={:?} KITTY_WINDOW_ID={:?}",
        lang,
        is_kitty_terminal(),
        std::env::var_os("HORAE_FORCE_KITTY_SPLASH").is_some(),
        std::env::var("TERM").ok(),
        std::env::var("TERM_PROGRAM").ok(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
    ));

    // 先进入 raw mode 再画首帧，这样等待期间能收到 Resize / F6 事件并重绘。
    crossterm::terminal::enable_raw_mode()?;
    let result = (|| -> anyhow::Result<()> {
        let (mut cols, mut rows) = terminal::size()?;
        loop {
            let drew_image = draw_frame_with(
                &mut stdout,
                cols,
                rows,
                splash_png.as_deref(),
                lang,
                is_kitty_terminal(),
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
                        let _ = crate::repo::settings::set(
                            conn,
                            "lang",
                            if lang.is_zh() { "zh" } else { "en" },
                        );
                        redraw = true;
                    }
                    Event::Key(_) => return Ok(()),
                    Event::Resize(c, r) => {
                        splash_debug(format!("resize {cols}x{rows} -> {c}x{r}, redraw"));
                        cols = c;
                        rows = r;
                        redraw = true;
                    }
                    _ => {}
                }
            }
            // Kitty 图片不受 Clear 影响，重绘前必须显式删除以避免重影。
            if drew_image {
                let _ = delete_kitty_image(&mut stdout);
            }
        }
    })();
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

/// 删除之前通过 Kitty 协议显示的图片（按 id）。
fn delete_kitty_image<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    write!(out, "\x1b_Ga=d,d=i,i=1,q=2\x1b\\")
}

/// 纵向布局常量（单位：终端行）。
const TOP_MARGIN: u16 = 2;
const BOTTOM_MARGIN: u16 = 2; // 版本行上方留白（版本行占最后一行）
const RULE_H: u16 = 1; // 分割线自身占一行
const SUBTITLE_H: u16 = 1; // 品牌副标题占一行（仅回退路径）
const TAGLINE_H: u16 = 2; // 名言 + 作者
const PROMPT_H: u16 = 2; // 提示语 + F6 提示
const GAP_BANNER_RULE: u16 = 1; // 横幅与分割线之间
const GAP_RULE_WORD: u16 = 1; // 分割线与字标之间
const GAP_WORD_SUBTITLE: u16 = 1; // 字标与副标题之间
const GAP_WORD_QUOTE: u16 = 1; // 字标/副标题区与名言之间
const GAP_TAGLINE_PROMPT: u16 = 2; // 名言区与提示语之间

/// 分割线宽度取屏幕的约 1/3，克制不抢戏。
fn rule_width(cols: u16) -> u16 {
    (cols / 3).clamp(12, 48)
}

/// 横幅可用最大高度：提示语之上需完整容纳「名言区 + 字标 + 分割线」。
fn banner_max_height(rows: u16, logo_h: u16) -> u16 {
    let prompt_y = rows.saturating_sub(BOTTOM_MARGIN + PROMPT_H);
    let stack_h = TAGLINE_H + GAP_WORD_QUOTE + logo_h + GAP_RULE_WORD + RULE_H + GAP_BANNER_RULE;
    prompt_y.saturating_sub(GAP_TAGLINE_PROMPT + stack_h + TOP_MARGIN)
}

/// 一帧开屏的纵向布局（各元素起始行；`None` = 该元素本路径不绘制）。
struct SplashLayout {
    rule_y: Option<u16>,     // 分割线
    subtitle_y: Option<u16>, // 品牌副标题
    logo_y: u16,             // 艺术字首行
    quote_y: u16,            // 名言行（下一行为作者署名）
}

/// 纵向布局推演（纯函数）：有图为自上而下锚定；回退将文本组合块在可用区内垂直居中。
fn splash_layout(rows: u16, logo_h: u16, use_kitty: bool, img_h: u16) -> SplashLayout {
    if use_kitty {
        let rule_y = TOP_MARGIN + img_h + GAP_BANNER_RULE;
        let logo_y = rule_y + RULE_H + GAP_RULE_WORD;
        SplashLayout {
            rule_y: Some(rule_y),
            subtitle_y: None,
            logo_y,
            quote_y: logo_y + logo_h + GAP_WORD_QUOTE,
        }
    } else {
        // 组合块高 = 字标 + 副标题 + 名言区及其间距，在可用区内取中。
        let comp_h = logo_h + GAP_WORD_SUBTITLE + SUBTITLE_H + GAP_WORD_QUOTE + TAGLINE_H;
        let prompt_y = rows.saturating_sub(BOTTOM_MARGIN + PROMPT_H);
        let avail = prompt_y.saturating_sub(GAP_TAGLINE_PROMPT + TOP_MARGIN);
        let logo_y = TOP_MARGIN + avail.saturating_sub(comp_h) / 2;
        let subtitle_y = logo_y + logo_h + GAP_WORD_SUBTITLE;
        SplashLayout {
            rule_y: None,
            subtitle_y: Some(subtitle_y),
            logo_y,
            quote_y: subtitle_y + 1 + GAP_WORD_QUOTE,
        }
    }
}

/// 清屏并绘制一帧开屏内容，返回是否显示了 Kitty 图。
/// 布局双路径见 [`splash_layout`]：有图为「横幅→分割线→字标→名言」，
/// 回退（无图/图损坏）为「字标+副标题+名言区」垂直居中、不画悬空分割线。
fn draw_frame_with<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    rows: u16,
    png: Option<&[u8]>,
    lang: Lang,
    kitty: bool,
) -> anyhow::Result<bool> {
    use crossterm::{cursor, ExecutableCommand};

    let logo_h = HORAE_LOGO.len() as u16;
    let logo_w = HORAE_LOGO.iter().map(|l| disp_width(l)).max().unwrap_or(0);

    // 横幅尺寸：仅当 Kitty 可用且 PNG 有效时启用，占满文本栈之外的全部空间。
    let mut img_w = 0;
    let mut img_h = 0;
    let mut use_kitty = false;
    if let Some(bytes) = png {
        if kitty {
            if let Some((w, h)) = png_cell_size(
                bytes,
                cols.saturating_sub(2),
                banner_max_height(rows, logo_h),
            ) {
                img_w = w;
                img_h = h;
                use_kitty = true;
            }
        }
    }

    let lay = splash_layout(rows, logo_h, use_kitty, img_h);

    out.execute(crossterm::terminal::Clear(
        crossterm::terminal::ClearType::All,
    ))?;

    // 1. 女神图全宽横幅（hero，仅有图路径）
    if use_kitty {
        let banner_x = center_x(cols, img_w);
        out.execute(cursor::MoveTo(banner_x, TOP_MARGIN))?;
        write_kitty_image(out, png.unwrap(), img_w, img_h)?;
        splash_debug(format!(
            "banner written at left={banner_x} top={TOP_MARGIN} cell=({img_w},{img_h})"
        ));
    }

    // 2. 分割线（暗；仅回退时不画，避免悬空）
    if let Some(rule_y) = lay.rule_y {
        let rule = "─".repeat(rule_width(cols) as usize);
        write_centered(out, cols, rule_y, "", &rule, OVERLAY0)?;
    }

    // 3. HORAE 艺术字（单色 Mauve，加粗签名）
    let logo_x = center_x(cols, logo_w);
    for (i, line) in HORAE_LOGO.iter().enumerate() {
        out.execute(cursor::MoveTo(logo_x, lay.logo_y + i as u16))?;
        write!(out, "\x1b[1m{}{line}\x1b[0m", fg(MAUVE))?;
    }

    // 4. 副标题（Pink，仅回退路径）+ 名言（Peach 加粗）+ 作者署名（暗）
    if let Some(subtitle_y) = lay.subtitle_y {
        write_centered(out, cols, subtitle_y, "", BRAND_SUBTITLE, PINK)?;
    }
    write_centered(out, cols, lay.quote_y, "\x1b[1m", TAGLINE, PEACH)?;
    write_centered(out, cols, lay.quote_y + 1, "", QUOTE_AUTHOR, OVERLAY0)?;

    // 5. 提示语（闪烁暗色）+ F6 提示；版本与作者（底部居中、暗）
    let (prompt, hint) = prompts(lang);
    let prompt_y = rows.saturating_sub(BOTTOM_MARGIN + PROMPT_H);
    write_centered(out, cols, prompt_y, "\x1b[5m", prompt, OVERLAY0)?;
    write_centered(out, cols, prompt_y + 1, "", hint, OVERLAY0)?;
    let version = format!("v{} · by zhaohang1205", env!("CARGO_PKG_VERSION"));
    write_centered(out, cols, rows.saturating_sub(1), "", &version, OVERLAY0)?;

    Ok(use_kitty)
}

#[cfg(test)]
mod splash_tests {
    use super::*;

    /// 构造仅含 IHDR 尺寸头的最小 PNG（够 png_dimensions 解析即可）。
    fn fake_png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b
    }

    #[test]
    fn load_splash_png_falls_back_to_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let got = load_splash_png_from(Some(tmp.path()));
        assert!(got.is_some(), "无覆盖文件时应回退到内置默认图");
        assert!(looks_like_png(&got.unwrap()), "默认图应为 PNG");
    }

    #[test]
    fn load_splash_png_uses_override() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = b"\x89PNG\r\n\x1a\nfake".to_vec();
        std::fs::write(tmp.path().join("splash.png"), &fake).unwrap();
        assert_eq!(
            load_splash_png_from(Some(tmp.path())),
            Some(fake),
            "存在覆盖文件时应优先返回它"
        );
    }

    #[test]
    fn png_dimensions_and_cell_size() {
        let portrait = fake_png(481, 594);
        assert_eq!(png_dimensions(&portrait), Some((481, 594)));
        assert_eq!(png_dimensions(b"not a png"), None);

        let square = fake_png(100, 100);
        // 宽度受限：方形图行数 = 宽度的一半；高度受限则按比例缩回不越界。
        assert_eq!(png_cell_size(&square, 40, 60), Some((40, 20)));
        assert_eq!(png_cell_size(&square, 30, 10), Some((20, 10)));
        // 零可用空间直接放弃绘制。
        assert_eq!(png_cell_size(&square, 0, 10), None);
        assert_eq!(png_cell_size(&square, 30, 0), None);
    }

    #[test]
    fn base64_encode_known_vectors() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn kitty_controls_roundtrip() {
        // 删除控制序列应精确匹配。
        let mut buf = std::io::Cursor::new(Vec::new());
        delete_kitty_image(&mut buf).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert_eq!(s, "\x1b_Ga=d,d=i,i=1,q=2\x1b\\");

        // 显示传输：多块数据触发分块，首块带 a=T,f=100 与 c=/r=，末块 m=0 收尾。
        let data = vec![0u8; 9000]; // chunk_size=4096，必分多块
        let mut buf = std::io::Cursor::new(Vec::new());
        write_kitty_image(&mut buf, &data, 40, 20).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        let first = s.split("\x1b\\").next().unwrap();
        assert!(
            first.contains("\x1b_Ga=T,f=100,c=40,r=20,"),
            "首块应声明 a=T,f=100,c=40,r=20: {first}"
        );
        let last = s.rsplit("\x1b_G").next().unwrap();
        assert!(last.contains("m=0,"), "末块应为 m=0 收尾: {last}");
        assert!(!last.contains("a=T"), "末块不应再带 a=T");
        assert!(s.matches("\x1b_G").count() >= 2, "大数据应被分块传输");
    }

    #[test]
    fn is_kitty_terminal_detects() {
        let prev_term = std::env::var("TERM").ok();
        let prev_tp = std::env::var("TERM_PROGRAM").ok();
        let had_kw = std::env::var_os("KITTY_WINDOW_ID").is_some();

        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::set_var("TERM_PROGRAM", "dumb");

        std::env::set_var("TERM", "xterm-kitty");
        assert!(is_kitty_terminal(), "TERM 含 kitty 应被识别");

        std::env::set_var("TERM", "xterm-256color");
        assert!(!is_kitty_terminal(), "普通终端不应被识别");

        match prev_term {
            Some(v) => std::env::set_var("TERM", v),
            None => std::env::remove_var("TERM"),
        }
        match prev_tp {
            Some(v) => std::env::set_var("TERM_PROGRAM", v),
            None => std::env::remove_var("TERM_PROGRAM"),
        }
        if had_kw {
            std::env::set_var("KITTY_WINDOW_ID", "1");
        } else {
            std::env::remove_var("KITTY_WINDOW_ID");
        }
    }

    #[test]
    fn horae_logo_has_expected_rows() {
        assert_eq!(HORAE_LOGO.len(), 9, "艺术字应为 9 行");
        for l in HORAE_LOGO {
            assert!(!l.trim().is_empty(), "艺术字每行不应为空");
        }
        let w = HORAE_LOGO.iter().map(|l| disp_width(l)).max().unwrap_or(0);
        assert!(w > 0 && w < 200, "艺术字宽度应在合理范围，实际 {w}");
    }

    #[test]
    fn disp_width_counts_cjk_double() {
        assert_eq!(disp_width("你"), 2);
        assert_eq!(disp_width("H"), 1);
        assert_eq!(disp_width("█"), 1, "方块字应计 1 宽");
        assert_eq!(disp_width("📥"), 1, "emoji 应计 1 宽");
        assert_eq!(disp_width("HORAE"), 5);
    }

    #[test]
    fn banner_max_height_leaves_room_for_text_stack() {
        // 30 行终端：提示语上方要装下名言区 + 9 行字标 + 分割线，剩给横幅 7 行。
        assert_eq!(banner_max_height(30, 9), 7);
        // 极矮终端不 panic，最多为 0（横幅放弃绘制）。
        assert_eq!(banner_max_height(10, 9), 0);
    }

    #[test]
    fn splash_layout_fallback_centers_block() {
        let lay = splash_layout(24, 9, false, 0);
        assert!(lay.rule_y.is_none(), "回退路径不应有分割线");
        let subtitle_y = lay.subtitle_y.expect("回退路径应有副标题");
        // 组合块高 = 9 + 1 + 1 + 1 + 2 = 14，可用区 = [2, prompt_y-2)，垂直居中。
        let prompt_y = 24 - BOTTOM_MARGIN - PROMPT_H;
        assert_eq!(
            lay.logo_y,
            TOP_MARGIN + (prompt_y - GAP_TAGLINE_PROMPT - TOP_MARGIN - 14) / 2
        );
        assert_eq!(subtitle_y, lay.logo_y + 9 + GAP_WORD_SUBTITLE);
        assert_eq!(lay.quote_y, subtitle_y + 1 + GAP_WORD_QUOTE);
        assert!(
            lay.quote_y + TAGLINE_H <= prompt_y,
            "名言区不得侵入底部提示语"
        );
    }

    #[test]
    fn splash_layout_banner_anchors_top() {
        let lay = splash_layout(30, 9, true, 7);
        let rule_y = lay.rule_y.expect("有图路径应含分割线");
        assert_eq!(
            rule_y,
            TOP_MARGIN + 7 + GAP_BANNER_RULE,
            "横幅贴顶、分割线紧随"
        );
        assert_eq!(lay.logo_y, rule_y + RULE_H + GAP_RULE_WORD);
        assert!(lay.subtitle_y.is_none(), "有图路径不应重复副标题");
        let prompt_y = 30 - BOTTOM_MARGIN - PROMPT_H;
        assert!(
            lay.quote_y + TAGLINE_H <= prompt_y,
            "名言区不得侵入底部提示语"
        );
    }

    #[test]
    fn draw_frame_fallback_no_kitty() {
        // png=None 时无论环境如何都不会启用 Kitty 图，应安全绘制整帧：
        // 字标 + 副标题 + 名言 + 作者，且不出现悬空分割线。
        let mut buf = std::io::Cursor::new(Vec::new());
        let drew = draw_frame_with(&mut buf, 80, 24, None, Lang::Zh, false).unwrap();
        assert!(!drew);
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("█"), "应绘制 HORAE 艺术字");
        assert!(s.contains(BRAND_SUBTITLE), "回退时应绘制品牌副标题");
        assert!(s.contains(TAGLINE), "应绘制名言");
        assert!(s.contains(QUOTE_AUTHOR), "应署名作者");
        assert!(!s.contains('─'), "无图回退不应出现悬空分割线");
    }

    #[test]
    fn draw_frame_kitty_banner() {
        // 注入合法 PNG 头并直接声明 kitty 能力，验证有图路径（不经环境变量，避免竞争）。
        let png = fake_png(2814, 1536);
        let mut buf = std::io::Cursor::new(Vec::new());
        let drew = draw_frame_with(&mut buf, 100, 30, Some(&png), Lang::Zh, true).unwrap();

        assert!(drew, "kitty 可用且 PNG 合法时应绘制横幅");
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert!(s.contains("\x1b_G"), "应输出 Kitty 图片控制序列");
        assert!(s.contains('─'), "有图路径应以分割线落地横幅");
        assert!(
            !s.contains(BRAND_SUBTITLE),
            "有图时 hero 已是图片，不应重复副标题"
        );
    }
}
