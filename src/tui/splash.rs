//! 开屏（splash）：PNG 加载、Kitty 图形协议输出与按键等待。
//! 用户可用 `~/.config/horae/splash.png` 覆盖内置图；`HORAE_FORCE_KITTY_SPLASH`
//! 强制启用图形协议，`HORAE_SPLASH_DEBUG` 写诊断日志。

fn load_splash_png() -> Option<Vec<u8>> {
    load_splash_png_from(None)
}

/// 从指定目录（或 `None` 表示走 `config_dir()`）加载开屏图，便于测试。
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

/// 估算开屏图在终端里占用的单元格尺寸，保持宽高比并在剩余空间内缩放。
fn png_cell_size(bytes: &[u8], cols: u16, rows: u16, text_h: u16) -> Option<(u16, u16)> {
    let (iw, ih) = png_dimensions(bytes)?;
    let max_w = (cols.saturating_sub(4)).clamp(10, 36);
    let mut w = max_w;
    // Terminal characters are typically ~1:2 aspect ratio, so multiply height in cells by 0.5
    let mut h = ((w as f64) * (ih as f64 / iw as f64) * 0.5).round() as u16;
    let max_h = rows.saturating_sub(text_h + 3);
    if max_h == 0 {
        return None;
    }
    if h > max_h {
        let ratio = max_h as f64 / h as f64;
        h = max_h;
        w = ((w as f64) * ratio).round() as u16;
        if w < 4 {
            w = 4;
        }
    }
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
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
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 用 Kitty 图形协议把 PNG 原样（base64）传给终端解码显示，分块以适配转义长度上限。
/// 关键：参照 kitty 官方示例，`a=T,f=100` 与单元格尺寸 `c/r` 必须放在**首块**，
/// 末块仅用 `m=0` 收尾，否则 Ghostty 等实现不会触发显示。
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

pub(super) fn show_splash(_conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use crossterm::{
        event::{self, Event},
        terminal,
    };
    use std::io::Write;

    let mut stdout = std::io::stdout();
    let splash_png = load_splash_png();
    splash_debug(format!(
        "start is_kitty={} force={:?} TERM={:?} TERM_PROGRAM={:?} KITTY_WINDOW_ID={:?}",
        is_kitty_terminal(),
        std::env::var_os("HORAE_FORCE_KITTY_SPLASH").is_some(),
        std::env::var("TERM").ok(),
        std::env::var("TERM_PROGRAM").ok(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
    ));

    // 先进入 raw mode 再画首帧，这样等待期间能收到 Resize 事件并重绘。
    crossterm::terminal::enable_raw_mode()?;
    let result = (|| -> anyhow::Result<()> {
        let (mut cols, mut rows) = terminal::size()?;
        loop {
            let drew_image = draw_splash_frame(&mut stdout, cols, rows, splash_png.as_deref())?;
            stdout.flush()?;
            loop {
                if !event::poll(std::time::Duration::from_millis(100))? {
                    continue;
                }
                match event::read()? {
                    Event::Key(_) => return Ok(()),
                    Event::Resize(c, r) => {
                        // Kitty 图片不受 Clear 影响，重绘前必须显式删除。
                        if drew_image {
                            delete_kitty_image(&mut stdout)?;
                        }
                        splash_debug(format!("resize {cols}x{rows} -> {c}x{r}, redraw"));
                        cols = c;
                        rows = r;
                        break;
                    }
                    _ => {}
                }
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

/// 清屏并按当前终端尺寸绘制一帧开屏内容；返回本帧是否显示了 Kitty 图。
fn draw_splash_frame<W: std::io::Write>(
    out: &mut W,
    cols: u16,
    rows: u16,
    png: Option<&[u8]>,
) -> anyhow::Result<bool> {
    use crossterm::{cursor, ExecutableCommand};

    let mut img_w = 0;
    let mut img_h = 0;
    let mut use_kitty = false;

    let title_str = "HORAE";
    let title_lines: Vec<&str> = title_str.lines().collect();
    let title_h = title_lines.len() as u16;
    let title_w = 5u16;

    let quote1_lines = ["物有本末，事有终始，", "知所先后，则近道矣", "--《大学》"];
    let quote2_lines = [
        "Your mind is for having ideas,",
        "not holding them",
        "--David Allen",
    ];
    let slogan_url = "https://gettingthingsdone.com/";

    let prompt = "Press any key to start...";
    let author_str = format!("v{} - by zhaohang1205", env!("CARGO_PKG_VERSION"));

    let text_h = title_h + 10; // title + blank + q1(3) + blank + q2(3) + blank + prompt

    if let Some(bytes) = png {
        if is_kitty_terminal() {
            if let Some((w, h)) = png_cell_size(bytes, cols, rows, text_h) {
                img_w = w;
                img_h = h;
                use_kitty = true;
            }
        }
    }

    let render_img_h = if use_kitty { img_h } else { 0 };
    let render_img_w = if use_kitty { img_w } else { 0 };

    let str_w = |s: &str| -> u16 { s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum() };

    let max_q1_w = quote1_lines.iter().map(|l| str_w(l)).max().unwrap_or(0);
    let max_q2_w = quote2_lines.iter().map(|l| str_w(l)).max().unwrap_or(0);
    let prompt_w = str_w(prompt);
    let author_w = str_w(&author_str);

    let text_w = max_q1_w.max(max_q2_w).max(prompt_w).max(title_w);
    let gap = if render_img_w > 0 { 4u16 } else { 0 };
    let total_w = render_img_w + gap + text_w;
    let total_h = render_img_h.max(text_h);

    let start_x = if cols > total_w {
        (cols - total_w) / 2
    } else {
        0
    };
    let start_y = if rows > total_h {
        (rows - total_h) / 2
    } else {
        0
    };

    let img_y = if render_img_h < total_h {
        start_y + (total_h - render_img_h) / 2
    } else {
        start_y
    };

    let text_y = if text_h < total_h {
        start_y + (total_h - text_h) / 2
    } else {
        start_y
    };

    out.execute(crossterm::terminal::Clear(
        crossterm::terminal::ClearType::All,
    ))?;

    if render_img_w > 0 && use_kitty {
        out.execute(cursor::MoveTo(start_x, img_y))?;
        write_kitty_image(out, png.unwrap(), img_w, img_h)?;
        splash_debug(format!(
            "image written at left={start_x} top={img_y} cell=({img_w},{img_h})"
        ));
    }

    let text_x = start_x + render_img_w + gap;

    // Title (Catppuccin Blue, Bold)
    for (i, line) in title_lines.iter().enumerate() {
        out.execute(cursor::MoveTo(text_x, text_y + i as u16))?;
        write!(out, "\x1b[1m\x1b[38;2;137;180;250m{}\x1b[0m", line)?;
    }

    // Quote 1 (Catppuccin Blue, Italic, source right-aligned)
    let quote1_y = text_y + title_h + 1;
    for (i, line) in quote1_lines.iter().enumerate() {
        let pad = if i == quote1_lines.len() - 1 {
            max_q1_w.saturating_sub(str_w(line))
        } else {
            0
        };
        out.execute(cursor::MoveTo(text_x + pad, quote1_y + i as u16))?;
        write!(out, "\x1b[38;2;137;180;250m\x1b[3m{}\x1b[0m", line)?;
    }

    // Quote 2 (Catppuccin Blue, Italic + OSC 8 Hyperlink, source right-aligned)
    let quote2_y = quote1_y + quote1_lines.len() as u16 + 1;
    for (i, line) in quote2_lines.iter().enumerate() {
        let pad = if i == quote2_lines.len() - 1 {
            max_q2_w.saturating_sub(str_w(line))
        } else {
            0
        };
        out.execute(cursor::MoveTo(text_x + pad, quote2_y + i as u16))?;
        write!(
            out,
            "\x1b[38;2;137;180;250m\x1b[3m\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\\x1b[0m",
            slogan_url, line
        )?;
    }

    // Prompt
    let prompt_y = quote2_y + quote2_lines.len() as u16 + 1;
    out.execute(cursor::MoveTo(text_x, prompt_y))?;
    write!(out, "\x1b[90m\x1b[5m{}\x1b[0m", prompt)?;

    // Author at edge area (bottom right)
    let author_x = if cols > author_w {
        cols - author_w - 1
    } else {
        0
    };
    let author_y = rows.saturating_sub(1);
    out.execute(cursor::MoveTo(author_x, author_y))?;
    write!(out, "\x1b[90m{}\x1b[0m", author_str)?;

    Ok(use_kitty)
}

#[cfg(test)]
mod splash_tests {
    use super::*;

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
        let got = load_splash_png_from(Some(tmp.path()));
        assert_eq!(got, Some(fake), "存在覆盖文件时应优先返回它");
    }

    #[test]
    fn png_dimensions_parses_ihdr() {
        let mut b = Vec::new();
        b.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&481u32.to_be_bytes());
        b.extend_from_slice(&594u32.to_be_bytes());
        assert_eq!(png_dimensions(&b), Some((481, 594)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }

    #[test]
    fn base64_encode_known_vectors() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn delete_kitty_image_emits_delete_control() {
        let mut buf = std::io::Cursor::new(Vec::new());
        delete_kitty_image(&mut buf).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();
        assert_eq!(s, "\x1b_Ga=d,d=i,i=1,q=2\x1b\\");
    }

    #[test]
    fn write_kitty_image_emits_kitty_protocol() {
        let data = vec![0u8; 9000]; // 多块：触发分块（chunk_size=4096）
        let mut buf = std::io::Cursor::new(Vec::new());
        write_kitty_image(&mut buf, &data, 40, 20).unwrap();
        let s = String::from_utf8(buf.into_inner()).unwrap();

        // 首块必须带 a=T,f=100 以及单元格尺寸 c=/r=（kitty 官方示例如此）。
        let first = s.split("\x1b\\").next().unwrap();
        assert!(
            first.contains("\x1b_Ga=T,f=100,c=40,r=20,"),
            "首块应声明 a=T,f=100,c=40,r=20: {}",
            first
        );

        // 末块仅以 m=0 收尾，不应再带 a=T。
        let last = s.rsplit("\x1b_G").next().unwrap();
        assert!(last.contains("m=0,"), "末块应为 m=0 收尾: {}", last);
        assert!(!last.contains("a=T"), "末块不应再带 a=T");

        let chunks = s.matches("\x1b_G").count();
        assert!(chunks >= 2, "大数据应被分块传输，实际块数 {}", chunks);
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
    fn splash_quotes_and_link() {
        let q1 = ["物有本末，事有终始，", "知所先后，则近道矣", "--《大学》"];
        let q2 = [
            "Your mind is for having ideas,",
            "not holding them",
            "--David Allen",
        ];
        let url = "https://gettingthingsdone.com/";
        assert_eq!(q1.len(), 3);
        assert_eq!(q2.len(), 3);
        assert_eq!(q1[2], "--《大学》");
        assert_eq!(q2[2], "--David Allen");

        let osc8 = format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, q2[0]);
        assert!(osc8.contains("https://gettingthingsdone.com/"));
        assert!(osc8.contains("Your mind is for having ideas,"));
    }
}
