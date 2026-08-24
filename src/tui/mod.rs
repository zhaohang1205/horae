pub mod app;
pub mod handlers;
pub mod keys;
pub mod render;
pub mod theme;
pub mod ui;

pub(crate) use app::{App, Pane, View};
pub(crate) use handlers::AppHandlers;
pub(crate) use render::AppRender;

use crate::model::task::{self, Task};
use crate::repo::tags;
use anyhow::Result;
use app::Row;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rusqlite::Connection;
use std::io::{self, Stdout};
use std::time::Duration;

/// 状态的中文含义，用于引导栏的“状态地图”。按当前语言返回。
pub(crate) fn status_cn(lang: crate::i18n::Lang, s: task::Status) -> &'static str {
    match s {
        task::Status::Inbox => crate::tr!(lang, "收件箱", "Inbox"),
        task::Status::Next => crate::tr!(lang, "下一步", "Next"),
        task::Status::Waiting => crate::tr!(lang, "等待中", "Waiting"),
        task::Status::Scheduled => crate::tr!(lang, "已排程", "Scheduled"),
        task::Status::Someday => crate::tr!(lang, "将来/也许", "Someday"),
        task::Status::Reference => crate::tr!(lang, "参考资料", "Reference"),
        task::Status::Done => crate::tr!(lang, "已完成", "Done"),
    }
}

/// 引导栏里各视图的中文/英文名（含日视图与归档箱等无状态视图）。
pub(crate) fn view_label(lang: crate::i18n::Lang, v: View) -> &'static str {
    match v {
        View::Inbox => crate::tr!(lang, "收件箱", "Inbox"),
        View::Today => crate::tr!(lang, "今日", "Today"),
        View::Tomorrow => crate::tr!(lang, "明日", "Tomorrow"),
        View::Next => crate::tr!(lang, "下一步", "Next"),
        View::Waiting => crate::tr!(lang, "等待中", "Waiting"),
        View::Scheduled => crate::tr!(lang, "已排程", "Scheduled"),
        View::Someday => crate::tr!(lang, "将来/也许", "Someday"),
        View::Reference => crate::tr!(lang, "参考资料", "Reference"),
        View::Done => crate::tr!(lang, "已完成", "Done"),
        View::Review => crate::tr!(lang, "周回顾", "Review"),
        View::Archived => crate::tr!(lang, "归档箱", "Archived"),
        View::Tags => crate::tr!(lang, "标签库", "Tags"),
        View::Quotes => crate::tr!(lang, "金句", "Quotes"),
        View::Settings => crate::tr!(lang, "设置", "Settings"),
    }
}

pub(crate) fn row_from(t: &Task, indent: usize, conn: &Connection) -> Result<Row> {
    let tags = tags::get_task_tags(conn, &t.id)?
        .iter()
        .map(|x| x.name.clone())
        .collect();
    Ok(row_from_tags(t, indent, tags))
}

/// 用已取好的标签名构建行，避免每行一次 DB 查询。
/// 循环任务会展开一次 rrule 来算展示用到期时间；批量刷新请改用
/// [`row_from_tags_with_due`] 传入预计算值。
pub(crate) fn row_from_tags(t: &Task, indent: usize, tags: Vec<String>) -> Row {
    let due = crate::schedule::display_due(t, None);
    row_from_tags_with_due(t, indent, tags, due)
}

/// 用已取好的标签名与预计算好的展示用到期时间构建行。
pub(crate) fn row_from_tags_with_due(
    t: &Task,
    indent: usize,
    tags: Vec<String>,
    due: Option<i64>,
) -> Row {
    // 完成进度：行动按检查单完成数。
    let (done, total) = if !t.checklist.is_empty() {
        let total = t.checklist.len();
        let done = t.checklist.iter().filter(|i| i.done).count();
        (Some(done), Some(total))
    } else {
        (None, None)
    };

    Row {
        id: t.id.clone(),
        title: t.title.clone(),
        status: t.status.to_string(),
        due,
        tags,
        indent,
        done,
        total,
        archive_reason: t.archive_reason.clone(),
        checked_in_today: false,
    }
}

/// 启动交互式 TUI。
/// 内置默认开屏图；允许用户用 `~/.config/horae/splash.png` 覆盖。
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

fn show_splash(_conn: &rusqlite::Connection) -> anyhow::Result<()> {
    use crossterm::{cursor, terminal, ExecutableCommand};
    use std::io::Write;

    let mut stdout = std::io::stdout();
    let (cols, rows) = terminal::size()?;
    splash_debug(format!(
        "start cols={cols} rows={rows} is_kitty={} force={:?} TERM={:?} TERM_PROGRAM={:?} KITTY_WINDOW_ID={:?}",
        is_kitty_terminal(),
        std::env::var_os("HORAE_FORCE_KITTY_SPLASH").is_some(),
        std::env::var("TERM").ok(),
        std::env::var("TERM_PROGRAM").ok(),
        std::env::var_os("KITTY_WINDOW_ID").is_some(),
    ));

    let splash_png = load_splash_png();

    let mut img_w = 0;
    let mut img_h = 0;
    let mut use_kitty = false;

    let title_str = "HORAE";
    let title_lines: Vec<&str> = title_str.lines().collect();
    let title_h = title_lines.len() as u16;
    let title_w = 5u16;

    let quote1_lines = ["物有本末，事有终始，", "知所先后，则近道矣", "--《大学》"];
    let quote2_lines = [
        "your mind is for having ideas,",
        "not holding them",
        "--David Allen",
    ];
    let slogan_url = "https://gettingthingsdone.com/";

    let prompt = "Press any key to start...";
    let author_str = format!("v{} - by zhaohang1205", env!("CARGO_PKG_VERSION"));

    let text_h = title_h + 10; // title + blank + q1(3) + blank + q2(3) + blank + prompt

    if let Some(bytes) = &splash_png {
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

    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    if render_img_w > 0 && use_kitty {
        stdout.execute(cursor::MoveTo(start_x, img_y))?;
        write_kitty_image(&mut stdout, splash_png.as_ref().unwrap(), img_w, img_h)?;
        splash_debug(format!(
            "image written at left={start_x} top={img_y} cell=({img_w},{img_h})"
        ));
    }

    let text_x = start_x + render_img_w + gap;

    // Title (Catppuccin Blue, Bold)
    for (i, line) in title_lines.iter().enumerate() {
        stdout.execute(cursor::MoveTo(text_x, text_y + i as u16))?;
        write!(stdout, "\x1b[1m\x1b[38;2;137;180;250m{}\x1b[0m", line)?;
    }

    // Quote 1 (Catppuccin Blue, Italic, source right-aligned)
    let quote1_y = text_y + title_h + 1;
    for (i, line) in quote1_lines.iter().enumerate() {
        let pad = if i == quote1_lines.len() - 1 {
            max_q1_w.saturating_sub(str_w(line))
        } else {
            0
        };
        stdout.execute(cursor::MoveTo(text_x + pad, quote1_y + i as u16))?;
        write!(stdout, "\x1b[38;2;137;180;250m\x1b[3m{}\x1b[0m", line)?;
    }

    // Quote 2 (Catppuccin Blue, Italic + OSC 8 Hyperlink, source right-aligned)
    let quote2_y = quote1_y + quote1_lines.len() as u16 + 1;
    for (i, line) in quote2_lines.iter().enumerate() {
        let pad = if i == quote2_lines.len() - 1 {
            max_q2_w.saturating_sub(str_w(line))
        } else {
            0
        };
        stdout.execute(cursor::MoveTo(text_x + pad, quote2_y + i as u16))?;
        write!(
            stdout,
            "\x1b[38;2;137;180;250m\x1b[3m\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\\x1b[0m",
            slogan_url, line
        )?;
    }

    // Prompt
    let prompt_y = quote2_y + quote2_lines.len() as u16 + 1;
    stdout.execute(cursor::MoveTo(text_x, prompt_y))?;
    write!(stdout, "\x1b[90m\x1b[5m{}\x1b[0m", prompt)?;

    // Author at edge area (bottom right)
    let author_x = if cols > author_w {
        cols - author_w - 1
    } else {
        0
    };
    let author_y = rows.saturating_sub(1);
    stdout.execute(cursor::MoveTo(author_x, author_y))?;
    write!(stdout, "\x1b[90m{}\x1b[0m", author_str)?;

    stdout.flush()?;

    crossterm::terminal::enable_raw_mode()?;
    loop {
        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let crossterm::event::Event::Key(_) = crossterm::event::read()? {
                break;
            }
        }
    }
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

pub fn run(conn: &Connection, profile: Option<&str>) -> Result<()> {
    let mods = crate::repo::modules::ModuleVisibility::load(conn);
    if mods.splash {
        let _ = show_splash(conn);
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, conn, profile);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    conn: &Connection,
    profile: Option<&str>,
) -> Result<()> {
    let mut app = App::new(conn)?;
    app.profile_name = profile
        .map(|s| s.to_string())
        .or_else(|| {
            crate::config::Config::load()
                .ok()
                .map(|c| c.default_profile)
        })
        .unwrap_or_default();
    loop {
        if app.needs_clear {
            terminal.clear()?;
            app.needs_clear = false;
        }
        app.check_notifications();
        app.refresh_pomo();
        terminal.draw(|f| app.render(f))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    app.handle_key(key)?;
                }
                Event::Mouse(m) => {
                    let left_width = terminal.size()?.width * 22 / 100;
                    let is_left_panel = m.column < left_width;
                    match m.kind {
                        crossterm::event::MouseEventKind::ScrollDown => {
                            if app.show_help {
                                app.help_scroll = app.help_scroll.saturating_add(1);
                            } else {
                                app.move_sel(1);
                            }
                        }
                        crossterm::event::MouseEventKind::ScrollUp => {
                            if app.show_help {
                                app.help_scroll = app.help_scroll.saturating_sub(1);
                            } else {
                                app.move_sel(-1);
                            }
                        }
                        crossterm::event::MouseEventKind::Down(
                            crossterm::event::MouseButton::Left,
                        ) => {
                            if m.column > terminal.size()?.width / 2 {
                                app.pane = Pane::Right;
                            } else if is_left_panel {
                                app.pane = Pane::Left;
                            } else {
                                app.pane = Pane::Center;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use crate::repo::tasks::ListFilter;
    use crate::repo::tasks::{self, CaptureInput};
    use crate::tui::app::Mode;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use std::io::Write;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }
    fn kc(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::empty())
    }

    fn seed(conn: &Connection) {
        let mk = |title: &str, status: task::Status, tags: &[&str]| {
            tasks::create_capture(
                conn,
                &CaptureInput {
                    title: title.into(),
                    status,
                    due_at: None,
                    tag_names: tags.iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                },
            )
            .unwrap();
        };
        mk("Write homepage copy", task::Status::Inbox, &["work", "p1"]);
        mk("Buy groceries", task::Status::Inbox, &["home", "errands"]);
        mk("Read Rust book", task::Status::Next, &["learning"]);
        mk("Pay taxes", task::Status::Waiting, &["work", "p2"]);
        mk("Plan vacation", task::Status::Someday, &["home"]);
        mk("Finish report", task::Status::Done, &[]);
    }

    fn snap(term: &Terminal<TestBackend>) -> String {
        let buf = term.backend().buffer();
        let w = buf.area().width as usize;
        let h = buf.area().height as usize;
        let content = buf.content();
        let mut s = String::with_capacity(w * h * 2);
        for y in 0..h {
            for x in 0..w {
                s.push_str(content[y * w + x].symbol());
            }
            s.push('\n');
        }
        s
    }

    /// 去掉所有空格，规避无头快照里 CJK 字符被逐字加空格的渲染产物，
    /// 便于对中文文本做 contains 断言（真实终端无此问题）。
    fn norm(s: &str) -> String {
        s.chars().filter(|c| *c != ' ').collect()
    }

    #[test]
    fn drive_tui() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        let mut out = std::fs::File::create("/tmp/horae_tui_frames.txt").unwrap();
        let frame = |label: &str,
                     term: &mut Terminal<TestBackend>,
                     app: &mut App,
                     out: &mut std::fs::File|
         -> String {
            term.clear().unwrap();
            term.draw(|f| app.render(f)).unwrap();
            let s = snap(term);
            writeln!(out, "===== {label} =====").unwrap();
            out.write_all(s.as_bytes()).unwrap();
            s
        };

        // 1) 三栏布局：引导栏 + 列表 + 详情
        let s = norm(&frame("1-initial-inbox", &mut term, &mut app, &mut out));
        assert!(s.contains("Active"), "引导栏应显示分组");
        assert!(s.contains("收件箱"), "引导栏含中文含义");
        assert!(s.contains("任务·收件箱"), "中栏列表标题");
        assert!(s.contains("任务详情"), "右侧详情栏");
        assert!(s.contains("Buygroceries"), "inbox 列出已灌入的任务");
        assert!(
            s.contains("等待中") && s.contains("将来/也许"),
            "上下文分组已列出"
        );

        // 2) vim 导航：下、上
        app.handle_key(key('j')).unwrap();
        frame("2-nav-down", &mut term, &mut app, &mut out);
        app.handle_key(key('k')).unwrap();
        frame("3-nav-up", &mut term, &mut app, &mut out);

        // 3) h/l 把焦点在 Left, Center, Right 之间切换
        app.pane = Pane::Center;
        app.handle_key(key('l')).unwrap();
        frame("4-pane-right", &mut term, &mut app, &mut out);
        assert!(app.pane == Pane::Right, "l 把焦点移到右栏");
        app.handle_key(key('h')).unwrap();
        frame("5-pane-center", &mut term, &mut app, &mut out);
        assert!(app.pane == Pane::Center, "h 把焦点移回中栏");
        app.handle_key(key('h')).unwrap();
        assert!(app.pane == Pane::Left, "h 把焦点移到左栏");
        app.handle_key(key('l')).unwrap();
        assert!(app.pane == Pane::Center, "l 把焦点移回中栏");

        // 4) 收集后自动跳回 Inbox（a 在非 Inbox 选择上 = 新建捕获）
        app.handle_key(key('2')).unwrap(); // Next 视图
        app.handle_key(key('a')).unwrap();
        let s = norm(&frame("6-capture-mode", &mut term, &mut app, &mut out));
        assert!(s.contains("快速录入"), "收集提示");
        for c in "Buy milk".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        let s = norm(&frame("7-after-capture", &mut term, &mut app, &mut out));
        assert!(s.contains("Buymilk"), "新收集的任务出现");
        assert!(s.contains("·收件箱"), "收集后跳到 Inbox");

        // 5) 回车 -> 组织/编辑模式：一句话补全 @标签 ~时间 *周期
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        let s = norm(&frame("8-organize", &mut term, &mut app, &mut out));
        assert!(s.contains("组织"), "回车进入组织/编辑模式");
        assert!(s.contains("[语法]"), "编辑模式下语法提示常驻不消失");
        // 预填了当前标题，追加单个 token 时间后确认
        for c in " ~tomorrow".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        frame("9-after-organize", &mut term, &mut app, &mut out);
        assert!(app.mode == Mode::Normal, "组织完成回到正常模式");
        assert!(
            app.status_message.contains("已组织"),
            "显示组织状态消息，实际: {}",
            app.status_message
        );
        let in_scheduled = tasks::list(
            &conn,
            &ListFilter {
                status: Some(task::Status::Scheduled),
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap()
        .iter()
        .any(|t| t.title == "Write homepage copy");
        assert!(in_scheduled, "补上时间后按逻辑自动归类为 Scheduled");

        // 6) 用数字键切换视图
        for (d, lbl, expect) in [
            ('3', "11-waiting", "等待中"),
            ('4', "12-scheduled", "已排程"),
            ('5', "13-someday", "将来/也许"),
            ('6', "14-reference", "参考资料"),
            ('7', "15-done", "已完成"),
            ('1', "16-back-inbox", "收件箱"),
        ] {
            app.handle_key(key(d)).unwrap();
            let s = norm(&frame(lbl, &mut term, &mut app, &mut out));
            assert!(s.contains(expect), "视图 {lbl} 应显示 {expect}");
        }

        // 7) 周回顾向导
        app.handle_key(key('r')).unwrap();
        let s = norm(&frame("17-review", &mut term, &mut app, &mut out));
        assert!(s.contains("每周回顾"), "回顾向导");
        app.handle_key(kc(KeyCode::Esc)).unwrap(); // Cancel wizard

        // 8) 在非 inbox 视图收集后自动跳回 Inbox
        app.handle_key(key('3')).unwrap();
        app.handle_key(key('a')).unwrap();
        for c in "Captured from waiting".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        let s = norm(&frame("19-capture-jump", &mut term, &mut app, &mut out));
        assert!(s.contains("·收件箱"), "从 waiting 视图收集后跳到 Inbox");
        assert!(s.contains("Capturedfromwaiting"));

        // 9) 一句话编辑：加标签 + 排程
        app.handle_key(kc(KeyCode::Enter)).unwrap(); // 组织编辑器（= e 全量编辑）
        let s = norm(&frame("20-organize", &mut term, &mut app, &mut out));
        assert!(s.contains("组织"), "回车进入一句话编辑器");
        for c in " @urgent ~tomorrow 15:30".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        let sched_id = tasks::list(
            &conn,
            &ListFilter {
                status: None,
                tags: vec!["urgent".to_string()],
                query: None,
                review_stale: false,
            },
        )
        .unwrap()
        .into_iter()
        .next()
        .map(|t| t.id)
        .expect("urgent 任务已打标签");
        let s = norm(&frame("21-after-schedule", &mut term, &mut app, &mut out));
        assert!(
            s.contains(&format!("已组织{}", &sched_id[..8])),
            "显示组织状态消息"
        );
        let st = tasks::get(&conn, &sched_id).unwrap();
        assert_eq!(
            st.status,
            task::Status::Scheduled,
            "一句话排程后进入 Scheduled"
        );
        assert_eq!(
            crate::time::format_local(st.scheduled_start_at),
            crate::time::format_local(Some(crate::time::parse_time("tomorrow 15:30").unwrap())),
            "排程起点 = 明天 15:30"
        );

        // 10) 归档(需确认) + 帮助切换 + 退出
        app.handle_key(key('4')).unwrap();
        app.handle_key(key('A')).unwrap();
        frame("22-archive-confirm", &mut term, &mut app, &mut out);
        // 归档需要 y 确认，确认后回到 Normal 才能打开帮助
        app.handle_key(key('y')).unwrap();
        frame("22-after-archive", &mut term, &mut app, &mut out);
        app.handle_key(key('?')).unwrap();
        let s = norm(&frame("23-help", &mut term, &mut app, &mut out));
        assert!(s.contains("快捷键"), "help text");
        app.handle_key(key('?')).unwrap();
        frame("24-help-off", &mut term, &mut app, &mut out);
        app.handle_key(key('q')).unwrap();
        assert!(app.should_quit, "q quits");

        // --- NEW FEATURES TESTS ---

        // Visual Mode
        app.should_quit = false;
        app.handle_key(key('1')).unwrap(); // Switch to Inbox
        app.handle_key(key('v')).unwrap();
        assert!(app.mode == Mode::Visual, "进入可视模式");
        app.handle_key(key('j')).unwrap(); // Move down to select two items
        assert!(!app.selected_ids.is_empty(), "选中了多个任务");
        // Tag them in bulk
        app.handle_key(key('T')).unwrap();
        assert!(app.mode == Mode::Tagging);
        for c in "bulk_tag".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        let s = norm(&frame("8-bulk-tagged", &mut term, &mut app, &mut out));
        assert!(s.contains("bulk_tag"), "批量打标签成功");

        // Context Filter
        app.handle_key(key('f')).unwrap();
        assert!(app.mode == Mode::FilteringTag);
        for c in "bulk_tag".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.tag_filter.as_deref(), Some("bulk_tag"));
        let s = norm(&frame("9-context-filter", &mut term, &mut app, &mut out));
        assert!(s.contains("bulk_tag"), "过滤成功");
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert_eq!(app.tag_filter, None);

        // Weekly Review Wizard
        app.handle_key(key('r')).unwrap();
        assert!(app.is_reviewing);
        assert_eq!(app.review_step, 1);
        assert_eq!(app.view, View::Inbox);
        let s = norm(&frame("10-review-step1", &mut term, &mut app, &mut out));
        assert!(s.contains("每周回顾"));

        app.handle_key(key('R')).unwrap(); // Step 2
        assert_eq!(app.review_step, 2);
        assert_eq!(app.view, View::Waiting);

        app.handle_key(key('R')).unwrap(); // Step 3
        assert_eq!(app.review_step, 3);
        assert_eq!(app.view, View::Someday);

        app.handle_key(key('R')).unwrap(); // Step 4 (view=Done)
        assert_eq!(app.view, View::Done);
        let s = norm(&frame("11-review-done", &mut term, &mut app, &mut out));
        assert!(s.contains("已完成"), "周回顾第4步显示已完成视图");

        app.handle_key(key('R')).unwrap(); // Finish
        assert!(!app.is_reviewing);
        assert_eq!(app.view, View::Next);
    }

    #[test]
    fn empty_db_shows_guide() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let raw = snap(&term);
        let mut out = std::fs::File::create("/tmp/horae_empty_guide.txt").unwrap();
        out.write_all(raw.as_bytes()).unwrap();
        let s = norm(&raw);
        assert!(
            s.contains("欢迎使用horae"),
            "empty db should show welcome guide"
        );
        assert!(s.contains("Active"), "guide shows groups");
    }

    #[test]
    fn today_tomorrow_views() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let mk = |title: &str, status: task::Status, due_at: i64| {
            tasks::create_capture(
                &conn,
                &CaptureInput {
                    title: title.into(),
                    status,
                    due_at: Some(due_at),
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
        };
        let day_ms = 24 * 3600 * 1000i64;
        mk(
            "due-today",
            task::Status::Next,
            crate::time::parse_time("today 12:00").unwrap(),
        );
        mk(
            "due-tomorrow",
            task::Status::Scheduled,
            crate::time::parse_time("tomorrow 12:00").unwrap(),
        );
        mk(
            "overdue",
            task::Status::Next,
            crate::time::now_ms() - 2 * day_ms,
        );

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "daily-habit".into(),
                status: task::Status::Scheduled,
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(
            &conn,
            &rec.id,
            crate::time::parse_time("today 09:00").unwrap(),
            None,
            Some("FREQ=DAILY".into()),
        )
        .unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None; // 关闭启动时的今日任务弹窗，避免吞掉按键

        let collect =
            |app: &App| -> Vec<String> { app.items.iter().map(|r| r.title.clone()).collect() };

        app.handle_key(key('J')).unwrap();
        assert_eq!(app.view, View::Today, "Shift+J 切换到今日视图");
        let t = collect(&app);
        assert!(t.iter().any(|s| s == "due-today"), "今日视图含今天到期任务");
        assert!(t.iter().any(|s| s == "overdue"), "今日视图含逾期任务");
        assert!(
            t.iter().any(|s| s == "daily-habit"),
            "今日视图含今日循环发生"
        );

        app.handle_key(key('K')).unwrap();
        assert_eq!(app.view, View::Tomorrow, "Shift+K 切换到明日视图");
        let t = collect(&app);
        assert!(
            t.iter().any(|s| s == "due-tomorrow"),
            "明日视图含明天到期任务"
        );
        assert!(
            t.iter().any(|s| s == "daily-habit"),
            "明日视图含明日循环发生"
        );
        assert!(t.iter().any(|s| s == "overdue"), "明日视图含逾期任务");
        assert!(
            t.iter().any(|s| s == "due-today"),
            "明日视图含今天到期但未完成的任务（结转）"
        );

        // 循环独立性：把今天这次循环标记完成后，明天的发生仍应显示在明日视图
        tasks::transition(&conn, &rec.id, task::Status::Done).unwrap();
        app.handle_key(key('K')).unwrap();
        let t = collect(&app);
        assert!(
            t.iter().any(|s| s == "daily-habit"),
            "今日执行不影响明日循环显示"
        );
    }

    #[test]
    fn checked_in_habit_stays_in_today_with_next_time() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "daily-habit".into(),
                status: task::Status::Scheduled,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(
            &conn,
            &rec.id,
            crate::time::parse_time("today 09:00").unwrap(),
            None,
            Some("FREQ=DAILY".into()),
        )
        .unwrap();

        // 今日打卡 → 锚点推进到下次 occurrence
        tasks::transition(&conn, &rec.id, task::Status::Done).unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('J')).unwrap(); // 今日视图
        let row = app
            .items
            .iter()
            .find(|r| r.title == "daily-habit")
            .expect("已打卡习惯仍保留在今日视图");
        assert!(row.checked_in_today, "标记为已打卡");
        let next = row.due.expect("有下一次执行时间");
        assert!(next > crate::time::now_ms(), "展示的是未来的下次时间");

        // Scheduled 视图同样标记已打卡
        app.handle_key(key('4')).unwrap();
        let row = app
            .items
            .iter()
            .find(|r| r.title == "daily-habit")
            .expect("Scheduled 视图含该习惯");
        assert!(row.checked_in_today, "Scheduled 视图也标记已打卡");
    }

    #[test]
    fn enter_opens_organize_on_scheduled() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "scheduled-task".into(),
                status: task::Status::Inbox,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(
            &conn,
            &rec.id,
            crate::time::parse_time("tomorrow 10:00").unwrap(),
            None,
            None,
        )
        .unwrap();

        let mut app = App::new(&conn).unwrap();
        app.handle_key(key('4')).unwrap(); // Scheduled 视图
        let row = app
            .items
            .iter()
            .find(|r| r.title == "scheduled-task")
            .expect("Scheduled 视图含该任务");
        assert_eq!(row.status, task::Status::Scheduled.to_string());

        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(
            app.mode,
            Mode::Capturing,
            "Enter 进入与 capture 同源的一句话编辑器"
        );
        assert_eq!(
            app.organizing_id.as_deref(),
            Some(rec.id.as_str()),
            "记录待编辑任务"
        );
        assert!(
            app.input.contains("scheduled-task"),
            "预填当前标题: {}",
            app.input
        );
        let t = tasks::get(&conn, &rec.id).unwrap();
        assert_eq!(t.status, task::Status::Scheduled, "编辑不改变状态");
        assert_eq!(t.due_at, None, "排程任务无 due，序列化不出现 ~时间");

        // Esc 取消编辑，状态不变
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Normal, "Esc 退出编辑模式");
        assert_eq!(app.organizing_id, None);
        let t = tasks::get(&conn, &rec.id).unwrap();
        assert_eq!(t.status, task::Status::Scheduled);
    }

    #[test]
    fn a_always_captures_and_e_edits_selected_task() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "滞留任务".into(),
                status: task::Status::Inbox,
                tag_names: vec!["health".to_string()],
                ..Default::default()
            },
        )
        .unwrap();

        let mut app = App::new(&conn).unwrap();
        // Inbox 视图选中该任务，按 a = 一律新建捕获，绝不修改选中任务
        app.handle_key(key('a')).unwrap();
        assert_eq!(app.mode, Mode::Capturing, "a 打开快速录入");
        assert_eq!(app.organizing_id, None, "a 不进入编辑模式");
        assert!(app.input.is_empty(), "a 不预填选中任务: {}", app.input);
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Normal);

        // e = 全量编辑选中任务：与 capture 同源的一句话编辑器，预填标题与标签
        app.handle_key(key('e')).unwrap();
        assert_eq!(app.mode, Mode::Capturing, "e 打开一句话编辑器");
        assert_eq!(app.organizing_id.as_deref(), Some(rec.id.as_str()));
        assert!(
            app.input.contains("滞留任务") && app.input.contains("@health"),
            "预填标题与标签: {}",
            app.input
        );

        app.input.clear();
        app.input_cursor = 0;
        for c in "滞留任务 @work ~tomorrow *d".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.organizing_id, None, "编辑后清除标记");

        let t = tasks::get(&conn, &rec.id).unwrap();
        assert_eq!(t.status, task::Status::Scheduled, "Inbox 补时间后自动归类");
        assert_eq!(t.rrule.as_deref(), Some("FREQ=DAILY"));
        let tags = crate::repo::tags::get_task_tags(&conn, &rec.id).unwrap();
        let names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names, vec!["work"], "标签被替换");
    }

    #[test]
    fn organize_edit_sets_time_tags_rrule() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "跑3公里".into(),
                status: task::Status::Inbox,
                tag_names: vec!["health".to_string()],
                ..Default::default()
            },
        )
        .unwrap();

        let mut app = App::new(&conn).unwrap();
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Capturing);
        assert!(app.input.contains("@health"), "预填已有标签: {}", app.input);

        // 直接替换为完整一句话：标题 + @标签 + ~时间 + *周期
        app.input.clear();
        app.input_cursor = 0;
        for c in "跑5公里 @home ~tomorrow *d".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Normal);

        let t = tasks::get(&conn, &rec.id).unwrap();
        assert_eq!(t.title, "跑5公里", "标题已更新");
        assert_eq!(t.status, task::Status::Scheduled, "Inbox 补时间后自动归类");
        assert_eq!(t.rrule.as_deref(), Some("FREQ=DAILY"), "周期已设置");
        assert_eq!(
            crate::time::format_local(t.scheduled_start_at),
            crate::time::format_local(Some(crate::time::parse_time("tomorrow").unwrap())),
            "排程起点已设置"
        );
        let tags = crate::repo::tags::get_task_tags(&conn, &rec.id).unwrap();
        let names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names, vec!["home"], "标签被替换为 @home");
    }

    #[test]
    fn missed_habit_shows_overdue_in_today_view() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        // 今天的 slot 取凌晨后 1 分钟（几乎必然已过），未打卡 → 今日视图显示逾期
        let slot = crate::time::local_day_bounds(0).0 + 60_000;
        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "missed-habit".into(),
                status: task::Status::Scheduled,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(&conn, &rec.id, slot, None, Some("FREQ=DAILY".into())).unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('J')).unwrap();
        let row = app
            .items
            .iter()
            .find(|r| r.title == "missed-habit")
            .expect("今日视图含该习惯");
        assert!(!row.checked_in_today, "未打卡");
        let due = row.due.expect("有 due");
        assert!(
            crate::time::is_overdue(Some(due)),
            "错过的 slot 显示为逾期: {:?}",
            due
        );

        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("逾期"), "列表行显示逾期措辞");
    }

    #[test]
    fn detail_planned_shows_next_occurrence_for_habit() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        // 锚点在 3 天前（已错过），FREQ=DAILY → 最近计划执行 = 今天的同一时刻
        let anchor = crate::time::local_day_bounds(-3).0 + 60_000;
        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "habit-detail".into(),
                status: task::Status::Scheduled,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(&conn, &rec.id, anchor, None, Some("FREQ=DAILY".into())).unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('4')).unwrap(); // Scheduled 视图
        let idx = app
            .items
            .iter()
            .position(|r| r.title == "habit-detail")
            .expect("Scheduled 视图含该习惯");
        app.selected = idx;
        app.load_detail();

        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));

        let task = tasks::get(&conn, &rec.id).unwrap();
        let next = crate::schedule::effective_due(&task).unwrap();
        let next_str = norm(&crate::time::format_local(Some(next)));
        let anchor_str = norm(&crate::time::format_local(Some(anchor)));
        assert!(
            next > anchor,
            "最近计划执行应晚于原始锚点: anchor={anchor_str}, next={next_str}"
        );
        assert!(
            s.contains(&next_str),
            "详情「计划」应显示最近计划执行日期 {next_str}，实际帧:\n{s}"
        );
        assert!(
            !s.contains(&anchor_str),
            "详情「计划」不应显示过期的原始锚点 {anchor_str}"
        );
    }

    #[test]
    fn x_does_not_double_check_in_habit() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "daily-habit".into(),
                status: task::Status::Scheduled,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::schedule(
            &conn,
            &rec.id,
            crate::time::parse_time("today 09:00").unwrap(),
            None,
            Some("FREQ=DAILY".into()),
        )
        .unwrap();
        tasks::transition(&conn, &rec.id, task::Status::Done).unwrap();

        let mut app = App::new(&conn).unwrap();
        app.handle_key(key('4')).unwrap(); // Scheduled 视图
        let row = app
            .items
            .iter()
            .find(|r| r.title == "daily-habit")
            .expect("Scheduled 视图含该习惯");
        assert!(row.checked_in_today, "今日已打卡");

        app.handle_key(key('x')).unwrap();
        let t = tasks::get(&conn, &rec.id).unwrap();
        assert_eq!(t.status, task::Status::Scheduled, "同日二次 x 不应改变状态");
        assert!(
            app.status_message.contains("今日已打卡"),
            "应提示已打卡，实际: {}",
            app.status_message
        );
        assert!(!app.should_quit, "TUI 不因拒绝打卡而退出");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn relative_due_direction_and_precision() {
        let now = crate::time::now_ms();
        let h = 3600 * 1000i64;
        let d = 24 * 3600 * 1000i64;
        let zh = crate::i18n::Lang::Zh;

        // 未来
        assert_eq!(
            crate::time::relative_due(zh, Some(now + 5 * h)).as_deref(),
            Some("5小时后")
        );
        assert_eq!(
            crate::time::relative_due(zh, Some(now + 30 * 60 * 1000)).as_deref(),
            Some("30分钟后")
        );
        assert_eq!(
            crate::time::relative_due(zh, Some(now + 2 * d)).as_deref(),
            Some("2天后")
        );

        // 过去（统一逾期措辞）
        assert_eq!(
            crate::time::relative_due(zh, Some(now - 5 * h)).as_deref(),
            Some("逾期5小时")
        );
        assert_eq!(
            crate::time::relative_due(zh, Some(now - 40 * 60 * 1000)).as_deref(),
            Some("逾期40分钟")
        );
        assert_eq!(
            crate::time::relative_due(zh, Some(now - 2 * d)).as_deref(),
            Some("逾期2天")
        );
        assert_eq!(
            crate::time::relative_due(zh, Some(now - d)).as_deref(),
            Some("逾期1天")
        );

        // 完成时间展示
        assert_eq!(
            crate::time::relative_past(zh, Some(now - 3 * h)).as_deref(),
            Some("3小时前")
        );
        assert_eq!(
            crate::time::relative_past(zh, Some(now - 2 * d)).as_deref(),
            Some("2天前")
        );
        assert_eq!(crate::time::relative_past(zh, None), None);
    }

    #[test]
    fn recurring_with_due_only_reschedules_on_done() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "standup".into(),
                status: task::Status::Scheduled,
                due_at: Some(crate::time::parse_time("today 09:00").unwrap()),
                tag_names: vec![],
                rrule: Some("FREQ=DAILY".into()),
                ..Default::default()
            },
        )
        .unwrap();

        // 只有 due_at + rrule（快速录入场景），完成后应重新排程而非结束
        let done = tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
        assert_eq!(
            done.status,
            task::Status::Scheduled,
            "循环任务完成后被重新排程"
        );
        assert_eq!(done.completed_at, None, "循环任务不进入已完成");
        let next = done.due_at.unwrap();
        let (tom_start, tom_end) = crate::time::local_day_bounds(1);
        assert!(
            next >= tom_start && next <= tom_end,
            "下一次发生落在明日窗口内"
        );

        // 明日视图应仍包含它
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('K')).unwrap();
        assert!(app.items.iter().any(|r| r.title == "standup"));
    }

    #[test]
    fn next_view_cycles_full_ring() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        let ring = [
            View::Today,
            View::Tomorrow,
            View::Inbox,
            View::Next,
            View::Waiting,
            View::Scheduled,
            View::Someday,
            View::Reference,
            View::Done,
            View::Archived,
            View::Tags,
            View::Review,
            View::Settings,
        ];
        for (i, v) in ring.iter().enumerate() {
            app.view = *v;
            app.next_view(1);
            assert_eq!(
                app.view,
                ring[(i + 1) % ring.len()],
                "正向：{:?} 的下一个",
                v
            );
        }
        for (i, v) in ring.iter().enumerate() {
            app.view = *v;
            app.next_view(-1);
            assert_eq!(
                app.view,
                ring[(i + ring.len() - 1) % ring.len()],
                "反向：{:?} 的上一个",
                v
            );
        }
    }

    #[test]
    fn quotes_feature_toggle_and_shortcut() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "好的句子".into(),
                status: task::Status::Inbox,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        // 默认关闭
        assert!(!app.quotes.enabled, "金句功能默认关闭");
        // 功能未启用：0 键与 " 键均无效
        app.handle_key(key('0')).unwrap();
        assert_eq!(app.view, View::Inbox, "未启用时 0 不切视图");
        app.handle_key(key('"')).unwrap();
        let tags = crate::repo::tags::get_task_tags(&conn, &t.id).unwrap();
        assert!(tags.is_empty(), "未启用时 \" 不生效");

        // F7 启用 + 持久化
        app.handle_key(kc(KeyCode::F(7))).unwrap(); // open popup
        for _ in 0..3 {
            app.handle_key(kc(KeyCode::Up)).unwrap(); // move from 0 to 5 (0->7->6->5)
        }
        app.handle_key(key(' ')).unwrap(); // space toggles 5 (Quotes)
        app.handle_key(kc(KeyCode::Esc)).unwrap(); // close popup
        assert!(app.quotes.enabled, "F7+Space 启用金句功能");
        assert_eq!(
            crate::repo::settings::get(&conn, "quotes")
                .unwrap()
                .as_deref(),
            Some("1"),
            "启用状态写入 settings"
        );
        // 侧栏出现 [Library] 金句分组
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("金句"), "启用后侧栏显示金句分组");

        // 0 → 金句视图（空）
        app.handle_key(key('0')).unwrap();
        assert_eq!(app.view, View::Quotes, "0 切换到金句视图");
        assert!(app.items.is_empty(), "金句视图初始为空");

        // 收件箱 → 金句：加 @quote + 流转 reference，离开收件箱，留在当前视图
        app.handle_key(key('1')).unwrap();
        app.selected = 0;
        app.load_detail();
        app.handle_key(key('"')).unwrap();
        assert_eq!(app.view, View::Inbox, "快捷键留在当前视图");
        let st = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(
            st.status,
            task::Status::Reference,
            "移入金句后流转为 reference"
        );
        let tags = crate::repo::tags::get_task_tags(&conn, &t.id).unwrap();
        assert!(
            tags.iter().any(|g| g.name == crate::repo::tasks::QUOTE_TAG),
            "移入金句后带有 @quote"
        );
        assert!(
            app.items.iter().all(|r| r.id != t.id),
            "移入金句后收件箱不再显示该任务"
        );

        // 金句仅在金句视图：Reference 视图与徽标排除 @quote
        app.handle_key(key('6')).unwrap();
        assert!(
            app.items.iter().all(|r| r.id != t.id),
            "Reference 视图不含金句"
        );
        assert_eq!(
            app.context_count(View::Reference),
            0,
            "Reference 徽标排除金句"
        );

        // 金句视图内看到它，且按 " 移出（摘除标签）
        app.handle_key(key('0')).unwrap();
        assert!(
            app.items.iter().any(|r| r.id == t.id),
            "金句视图显示移入的金句"
        );
        app.handle_key(key('"')).unwrap();
        let tags = crate::repo::tags::get_task_tags(&conn, &t.id).unwrap();
        assert!(
            tags.iter().all(|g| g.name != crate::repo::tasks::QUOTE_TAG),
            "金句视图按 \" 移出金句"
        );
        assert!(app.items.is_empty(), "移出后金句视图为空");

        // 金句视图内 a + @quote 直接入库，自动进入金句视图
        app.handle_key(key('a')).unwrap();
        assert_eq!(app.mode, Mode::Capturing);
        for c in "灵感: 知行合一 @quote".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.view, View::Quotes, "捕获 @quote 后自动进入金句视图");
        let q = tasks::list_quotes(&conn, None, None).unwrap();
        assert_eq!(q.len(), 1, "仅一条金句");
        assert_eq!(q[0].status, task::Status::Reference);
        assert_eq!(q[0].title, "灵感: 知行合一");

        // 启用后视图环尾依次接 Quotes、Review、Settings
        app.view = View::Tags;
        app.next_view(1);
        assert_eq!(app.view, View::Quotes, "Tags 之后是 Quotes");
        app.next_view(1);
        assert_eq!(app.view, View::Review, "Quotes 之后是 Review");
        app.next_view(1);
        assert_eq!(app.view, View::Settings, "Review 之后是 Settings");

        // 重启恢复启用状态
        drop(app);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        assert!(app.quotes.enabled, "重启后恢复启用");

        // F7 停用：若在金句视图则跳回收件箱
        app.handle_key(key('0')).unwrap();
        app.handle_key(kc(KeyCode::F(7))).unwrap();
        for _ in 0..3 {
            app.handle_key(kc(KeyCode::Up)).unwrap(); // move to index 5
        }
        app.handle_key(key(' ')).unwrap();
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(!app.quotes.enabled);
        assert_eq!(app.view, View::Inbox, "停用时离开金句视图");
        assert_eq!(
            crate::repo::settings::get(&conn, "quotes")
                .unwrap()
                .as_deref(),
            Some("0"),
            "停用状态写入 settings"
        );
    }

    #[test]
    fn settings_view_manages_profiles() {
        use crate::config::Config;

        // 隔离配置目录并持锁，避免与其它依赖 HORAE_CONFIG_DIR 的测试竞争
        crate::testutil::with_test_config_dir(|| {
            let mut conn = Connection::open(":memory:").unwrap();
            migrate::run(&mut conn).unwrap();
            let mut app = App::new(&conn).unwrap();
            app.popup = None;

            // 进入设置视图：默认 profile 应在列表中
            app.handle_key(key('M')).unwrap();
            assert_eq!(app.view, View::Settings);
            assert!(
                app.items.iter().any(|r| r.id == "default"),
                "默认 profile 在设置页"
            );
            assert!(app
                .items
                .iter()
                .any(|r| r.tags.iter().any(|t| t == "horae.db")));

            // n → 新建 work
            app.handle_key(key('n')).unwrap();
            assert_eq!(app.mode, Mode::CreatingProfile);
            for c in "work".chars() {
                app.handle_key(key(c)).unwrap();
            }
            app.handle_key(kc(KeyCode::Enter)).unwrap();
            assert_eq!(app.mode, Mode::Normal);
            assert!(
                app.items.iter().any(|r| r.id == "work"),
                "新建的 work profile 出现在列表"
            );
            let cfg = Config::load().unwrap();
            assert_eq!(
                cfg.profile("work").unwrap().db,
                "profiles/work.db",
                "work 使用默认 db 路径"
            );

            // 选中 work 并设为默认
            let idx = app.items.iter().position(|r| r.id == "work").unwrap();
            app.selected = idx;
            app.handle_key(key('s')).unwrap();
            let cfg = Config::load().unwrap();
            assert_eq!(cfg.default_profile, "work", "work 成为默认");

            // r → 重命名为 work2
            app.handle_key(key('r')).unwrap();
            assert_eq!(app.mode, Mode::RenamingProfile);
            assert_eq!(app.input, "work", "重命名预填当前名称");
            for _ in 0..4 {
                app.handle_key(kc(KeyCode::Backspace)).unwrap();
            }
            for c in "work2".chars() {
                app.handle_key(key(c)).unwrap();
            }
            app.handle_key(kc(KeyCode::Enter)).unwrap();
            let cfg = Config::load().unwrap();
            assert!(cfg.profile("work2").is_some(), "重命名后 work2 存在");
            assert!(cfg.profile("work").is_none(), "work 已不存在");
            assert_eq!(cfg.default_profile, "work2", "默认跟随重命名");

            // d + y → 删除 work2
            app.handle_key(key('d')).unwrap();
            assert_eq!(app.mode, Mode::ConfirmProfileDelete);
            app.handle_key(key('y')).unwrap();
            let cfg = Config::load().unwrap();
            assert!(cfg.profile("work2").is_none(), "work2 已删除");
            assert!(
                cfg.profile("default").is_some(),
                "删除默认后保留剩余 default"
            );
        });
    }

    #[test]
    fn quote_tag_is_plain_when_feature_off() {
        // 回归：功能关闭时 @quote 只是普通标签，参考资料视图照常显示。
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let q = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "普通引用".into(),
                status: task::Status::Reference,
                tag_names: vec![crate::repo::tasks::QUOTE_TAG.to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('6')).unwrap(); // Reference 视图
        assert!(
            app.items.iter().any(|r| r.id == q.id),
            "功能关闭时 Reference 视图显示 @quote 任务"
        );
        assert_eq!(app.context_count(View::Reference), 1, "徽标计入");
    }

    #[test]
    fn key_table_respects_view_selection_and_mode() {
        use crate::i18n::Lang;
        use crate::tui::keys::{
            help_rows, status_strip, strip_keys, Ctx, KeyGroup, GROUP_ORDER, KEY_TABLE,
            NON_TASK_VIEWS,
        };

        let ctx = |v: View, sel: bool| Ctx {
            view: v,
            mode: Mode::Normal,
            has_selection: sel,
            is_reviewing: false,
            pomo_active: false,
            task_status: Some(task::Status::Inbox),
            has_checklist: false,
            quotes_enabled: false,
        };
        let keys = |v: View, sel: bool| strip_keys(&ctx(v, sel), Lang::Zh);

        // 任务操作键仅在选中时出现，且全局键不进动态条
        let inbox_sel = keys(View::Inbox, true);
        assert!(
            inbox_sel.iter().any(|(k, _)| *k == "Enter"),
            "有选中→含 Enter"
        );
        assert!(
            !inbox_sel.iter().any(|(k, _)| *k == "hjkl"),
            "全局导航键不进动态条"
        );
        assert!(
            !inbox_sel.iter().any(|(k, _)| *k == "q"),
            "全局退出键不进动态条"
        );
        // 空 Inbox 无任务操作 → 动态条只含捕获键 a（渲染层仍显示该条）
        let inbox_empty = keys(View::Inbox, false);
        assert!(
            !inbox_empty.iter().any(|(k, _)| *k == "Enter"),
            "无选中→不含任务操作键"
        );
        assert!(
            inbox_empty.iter().any(|(k, _)| *k == "a"),
            "无选中也提示捕获 a"
        );
        // 捕获键 a 在所有视图（含 Tags/Archived/Review）常驻提示
        for v in [
            View::Inbox,
            View::Next,
            View::Today,
            View::Archived,
            View::Tags,
            View::Review,
        ] {
            assert!(
                keys(v, true).iter().any(|(k, _)| *k == "a"),
                "{:?} 视图含捕获 a",
                v
            );
        }

        // 任务状态驱动：x/w/s 对已完成任务不提示，w 对等待中不提示，s 对将来不提示
        let done_ctx = Ctx {
            task_status: Some(task::Status::Done),
            ..ctx(View::Inbox, true)
        };
        let done_keys = strip_keys(&done_ctx, Lang::Zh);
        assert!(!done_keys.iter().any(|(k, _)| *k == "x"), "Done→不含 x");
        assert!(!done_keys.iter().any(|(k, _)| *k == "w"), "Done→不含 w");
        assert!(!done_keys.iter().any(|(k, _)| *k == "s"), "Done→不含 s");
        let waiting_keys = strip_keys(
            &Ctx {
                task_status: Some(task::Status::Waiting),
                ..ctx(View::Inbox, true)
            },
            Lang::Zh,
        );
        assert!(
            waiting_keys.iter().any(|(k, _)| *k == "x"),
            "Waiting→仍含 x"
        );
        assert!(
            !waiting_keys.iter().any(|(k, _)| *k == "w"),
            "Waiting→不含 w"
        );
        let someday_keys = strip_keys(
            &Ctx {
                task_status: Some(task::Status::Someday),
                ..ctx(View::Inbox, true)
            },
            Lang::Zh,
        );
        assert!(
            !someday_keys.iter().any(|(k, _)| *k == "s"),
            "Someday→不含 s"
        );

        // 检查单：仅当活动任务含检查单才提示 =
        assert!(!inbox_sel.iter().any(|(k, _)| *k == "="), "无检查单→不含 =");
        let chk_keys = strip_keys(
            &Ctx {
                has_checklist: true,
                ..ctx(View::Inbox, true)
            },
            Lang::Zh,
        );
        assert!(chk_keys.iter().any(|(k, _)| *k == "="), "含检查单→含 =");

        // 引导栏键按热度降序
        let heats = |vs: &[(&'static str, &'static str)]| {
            vs.iter()
                .map(|(k, d)| {
                    KEY_TABLE
                        .iter()
                        .find(|def| def.keys == *k && def.zh == *d)
                        .map(|def| def.heat)
                        .unwrap_or_else(|| panic!("找不到 {} ({})", k, d))
                })
                .collect::<Vec<_>>()
        };
        let h = heats(&inbox_sel);
        assert!(
            h.windows(2).all(|w| w[0] >= w[1]),
            "引导栏按热度降序: {:?}",
            inbox_sel
        );

        // 归档箱：u/D 需要选中；v（多选）是全局键，空选也可提示
        let arch_no_sel = keys(View::Archived, false);
        assert!(
            arch_no_sel.iter().any(|(k, _)| *k == "v"),
            "空选也可提示 v 多选"
        );
        assert!(
            !arch_no_sel.iter().any(|(k, _)| *k == "u" || *k == "D"),
            "u/D 需选中"
        );
        let arch_sel = keys(View::Archived, true);
        assert!(arch_sel.iter().any(|(k, _)| *k == "u"));
        assert!(arch_sel.iter().any(|(k, _)| *k == "D"));
        assert!(arch_sel.iter().any(|(k, _)| *k == "v"));

        // 周回顾视图（未进行中）：提示 r 开启回顾，不提示失效的 R
        let review = keys(View::Review, true);
        assert!(
            review.iter().any(|(k, _)| *k == "r"),
            "Review 提示 r 开启回顾"
        );
        assert!(
            !review.iter().any(|(k, _)| *k == "R"),
            "Review 未进行中不提示 R"
        );

        // 非任务视图：任务操作键不出现（即使有选中行）
        let tags_sel = keys(View::Tags, true);
        assert!(!tags_sel.iter().any(|(k, _)| *k == "Enter"));
        assert!(tags_sel.iter().any(|(k, _)| *k == "c"), "Tags 有新增标签");
        assert!(tags_sel.iter().any(|(k, _)| *k == "D"), "Tags 有删除标签");

        // 周回顾进行中才出现 R
        let mut reviewing = ctx(View::Inbox, true);
        reviewing.is_reviewing = true;
        assert!(
            strip_keys(&reviewing, Lang::Zh)
                .iter()
                .any(|(k, _)| *k == "R"),
            "周回顾中→含 R"
        );
        assert!(!keys(View::Inbox, true).iter().any(|(k, _)| *k == "R"));

        // 输入/确认模式 → 模式键
        let confirm = Ctx {
            mode: Mode::ConfirmArchive,
            ..ctx(View::Inbox, true)
        };
        assert!(
            strip_keys(&confirm, Lang::Zh)
                .iter()
                .any(|(k, _)| *k == "y/Enter"),
            "确认模式→含 y/Enter"
        );

        // 金句：默认关闭 → " 不出现；启用 + 选中任务 → " 加入金句；
        // 金句视图选中 → " 变为"移出金句"
        let quotes_off = keys(View::Inbox, true);
        assert!(
            !quotes_off.iter().any(|(k, _)| *k == "\""),
            "关闭时动态条不含金句键"
        );
        let quotes_on = strip_keys(
            &Ctx {
                quotes_enabled: true,
                ..ctx(View::Inbox, true)
            },
            Lang::Zh,
        );
        let add_q = quotes_on.iter().find(|(k, _)| *k == "\"").unwrap();
        assert_eq!(add_q.1, "加入金句");
        let quotes_view = strip_keys(
            &Ctx {
                quotes_enabled: true,
                ..ctx(View::Quotes, true)
            },
            Lang::Zh,
        );
        let rm_q = quotes_view.iter().find(|(k, _)| *k == "\"").unwrap();
        assert_eq!(rm_q.1, "移出金句");

        // 状态栏全局条：含压缩后的 hjkl 与捕获/退出，不含低频键 g/G 与视图键
        let strip = status_strip(Lang::Zh);
        assert!(strip.iter().any(|(k, _)| *k == "hjkl"), "全局条含 hjkl");
        assert!(strip.iter().any(|(k, _)| *k == "q"), "全局条含 q");
        assert!(!strip.iter().any(|(k, _)| *k == "g/G"), "全局条不含 g/G");
        assert!(!strip.iter().any(|(k, _)| *k == "0-9"), "全局条不含视图键");
        assert!(
            strip.iter().any(|(k, _)| *k == "F7"),
            "全局条含 F7 功能开关"
        );
        // 状态栏全局条按热度降序
        assert!(
            heats(&strip).windows(2).all(|w| w[0] >= w[1]),
            "全局条按热度降序: {:?}",
            strip
        );

        // F1 面板：分组顺序全局→任务，组内按热度降序
        let rows = help_rows(&ctx(View::Inbox, true), Lang::Zh);
        let groups: Vec<KeyGroup> = rows.iter().map(|(g, _, _, _)| *g).collect();
        let mut seen_task = false;
        for g in &groups {
            assert!(
                !(seen_task && *g == KeyGroup::Global),
                "全局组必须在任务组之前"
            );
            if *g == KeyGroup::Task {
                seen_task = true;
            }
        }
        for g in GROUP_ORDER {
            let sub: Vec<(&'static str, &'static str)> = rows
                .iter()
                .filter(|(rg, _, _, _)| *rg == g)
                .map(|(_, k, d, _)| (*k, *d))
                .collect();
            let gh = heats(&sub);
            assert!(
                gh.windows(2).all(|w| w[0] >= w[1]),
                "F1 组内按热度降序: {:?}",
                sub
            );
        }

        // 表不变量：每条都有键与双语描述与热度；NON_TASK_VIEWS 恰好是 Tags/Archived/Settings
        assert!(!KEY_TABLE.is_empty());
        for k in KEY_TABLE {
            assert!(!k.keys.is_empty());
            assert!(!k.zh.is_empty());
            assert!(!k.en.is_empty());
            assert!(k.heat > 0, "{} 必须有热度", k.keys);
        }
        assert_eq!(
            NON_TASK_VIEWS,
            &[View::Tags, View::Archived, View::Settings]
        );
    }

    #[test]
    fn done_row_shows_completion_time() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "finished-thing".into(),
                status: task::Status::Next,
                due_at: Some(crate::time::now_ms() - 3 * 24 * 3600 * 1000i64),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        let done = tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
        let row = row_from(&done, 0, &conn).unwrap();
        assert_eq!(row.status, "done");
        assert_eq!(
            row.due, done.completed_at,
            "已完成行显示完成时间而非截止时间"
        );
        assert!(row.due.is_some());
    }

    #[test]
    fn done_view_shows_completion_not_overdue() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "finishedlongago".into(),
                status: task::Status::Next,
                due_at: Some(crate::time::now_ms() - 3 * 24 * 3600 * 1000i64),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('7')).unwrap(); // Done 视图
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("finishedlongago"), "已完成任务显示在 Done 视图");
        assert!(!s.contains("逾期"), "已完成任务不应显示逾期");
    }

    #[test]
    fn archived_view_shows_reason_not_status_or_overdue() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "completed-then-archived".into(),
                status: task::Status::Next,
                due_at: Some(crate::time::now_ms() - 3 * 24 * 3600 * 1000i64),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
        let arch = tasks::archive(&conn, &t.id).unwrap();
        assert_eq!(arch.archive_reason.as_deref(), Some("completed"));

        let del = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "deleted-straight-away".into(),
                status: task::Status::Inbox,
                due_at: Some(crate::time::now_ms() - 5 * 24 * 3600 * 1000i64),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        let arch = tasks::archive(&conn, &del.id).unwrap();
        assert_eq!(arch.archive_reason.as_deref(), Some("deleted"));

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('8')).unwrap(); // 归档箱视图
        assert_eq!(app.view, View::Archived);
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(
            s.contains("completed-then-archived"),
            "已完成并归档的任务在归档箱"
        );
        assert!(
            s.contains("deleted-straight-away"),
            "直接删除的任务在归档箱"
        );
        assert!(s.contains("完成"), "显示归档原因：完成");
        assert!(s.contains("删除"), "显示归档原因：删除");
        assert!(!s.contains("逾期"), "归档箱不再显示逾期");
    }

    #[test]
    fn archived_view_can_purge_task() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "purge-from-archive".into(),
                status: task::Status::Inbox,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::archive(&conn, &t.id).unwrap();

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('8')).unwrap(); // 归档箱视图
        assert_eq!(app.view, View::Archived);
        app.handle_key(key('D')).unwrap(); // 触发永久删除确认
        assert_eq!(app.mode, Mode::ConfirmPurge, "进入永久删除确认");
        app.handle_key(key('y')).unwrap(); // 确认
        assert!(tasks::get(&conn, &t.id).is_err(), "任务已被永久删除");
        assert!(app.items.is_empty(), "归档箱列表已刷新为空");
        assert_eq!(app.view, View::Archived, "仍停留在归档箱视图");

        // 取消路径：再归档一条，按 D 后按 n 应保留任务
        let t2 = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "keep-me".into(),
                status: task::Status::Inbox,
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        tasks::archive(&conn, &t2.id).unwrap();
        app.handle_key(key('8')).unwrap();
        app.handle_key(key('D')).unwrap();
        assert_eq!(app.mode, Mode::ConfirmPurge);
        app.handle_key(key('n')).unwrap();
        assert_eq!(app.mode, Mode::Normal, "取消后回到 Normal");
        assert!(tasks::get(&conn, &t2.id).is_ok(), "取消删除后任务仍在");
    }

    #[test]
    fn archived_view_can_purge_multiple_in_visual_mode() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        for i in 0..3 {
            let t = tasks::create_capture(
                &conn,
                &CaptureInput {
                    title: format!("bulk-purge-{i}"),
                    status: task::Status::Inbox,
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
            tasks::archive(&conn, &t.id).unwrap();
        }

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('8')).unwrap(); // 归档箱视图
                                           // 归档按 archived_at DESC 排序，但快速连建三条可能落同一毫秒导致并列序不定，
                                           // 故按实际列表顺序取前两行，而非依赖插入顺序。
        let top_two: Vec<String> = tasks::list_archived(&conn)
            .unwrap()
            .into_iter()
            .take(2)
            .map(|t| t.id)
            .collect();
        assert_eq!(top_two.len(), 2, "归档箱至少两条用于批量测试");
        app.handle_key(key('v')).unwrap(); // 进入可视模式
        app.handle_key(key('j')).unwrap(); // 选中前两项
        app.handle_key(key('D')).unwrap(); // 触发批量永久删除确认
        assert_eq!(app.mode, Mode::ConfirmPurge, "进入批量永久删除确认");
        assert_eq!(app.pending_purge_ids.len(), 2, "可视模式选中了 2 项");
        app.handle_key(key('y')).unwrap(); // 确认

        for id in &top_two {
            assert!(tasks::get(&conn, id).is_err(), "选中项已删除: {}", id);
        }
        let remaining: Vec<_> = tasks::list_archived(&conn).unwrap();
        assert_eq!(remaining.len(), 1, "归档箱只剩未被选中的任务");
        assert!(
            remaining[0].id != top_two[0] && remaining[0].id != top_two[1],
            "剩余项未被选中"
        );
        assert_eq!(app.mode, Mode::Normal, "删除后退出可视模式");
        assert!(app.selected_ids.is_empty(), "选择集已清空");
    }

    #[test]
    fn normal_mode_space_batch_ops_use_all_selected() {
        // 回归：普通模式下用 Space 点选多项后批量操作，应作用于全部选中项，
        // 而非仅当前光标行（此前归档/删除/状态变更被 Mode::Visual 门控误伤）。
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        for i in 0..3 {
            let t = tasks::create_capture(
                &conn,
                &CaptureInput {
                    title: format!("normal-batch-{i}"),
                    status: task::Status::Inbox,
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
            tasks::archive(&conn, &t.id).unwrap();
        }

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('8')).unwrap(); // 归档箱视图
        app.handle_key(key('l')).unwrap(); // 焦点移到列表栏（否则 j/k 是切视图）
        let top_two: Vec<String> = tasks::list_archived(&conn)
            .unwrap()
            .into_iter()
            .take(2)
            .map(|t| t.id)
            .collect();
        assert_eq!(top_two.len(), 2, "归档箱至少两条用于批量测试");

        // 普通模式：Space 点选第一行，j 移动，Space 点选第二行（不进入可视模式）。
        app.handle_key(key(' ')).unwrap();
        app.handle_key(key('j')).unwrap();
        app.handle_key(key(' ')).unwrap();
        assert_eq!(app.mode, Mode::Normal, "Space 点选不进入可视模式");
        assert_eq!(app.selected_ids.len(), 2, "普通模式选中 2 项");

        // D 触发批量永久删除确认：应收集全部 2 项。
        app.handle_key(key('D')).unwrap();
        assert_eq!(app.mode, Mode::ConfirmPurge);
        assert_eq!(app.pending_purge_ids.len(), 2, "普通模式批量删除收集 2 项");
        app.handle_key(key('y')).unwrap();
        for id in &top_two {
            assert!(tasks::get(&conn, id).is_err(), "选中项已删除: {}", id);
        }
        let remaining: Vec<_> = tasks::list_archived(&conn).unwrap();
        assert_eq!(remaining.len(), 1, "只剩未被选中的任务");
        assert!(app.selected_ids.is_empty(), "批量操作后选择集已清空");

        // 状态批量变更（x = Done）同样应作用于全部选中项，且操作后清空选择集。
        let mut conn2 = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn2).unwrap();
        for i in 0..2 {
            tasks::create_capture(
                &conn2,
                &CaptureInput {
                    title: format!("status-batch-{i}"),
                    status: task::Status::Next,
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut app2 = App::new(&conn2).unwrap();
        app2.popup = None;
        app2.handle_key(key('2')).unwrap(); // Next 视图
        app2.handle_key(key('l')).unwrap(); // 焦点移到列表栏
        app2.handle_key(key(' ')).unwrap();
        app2.handle_key(key('j')).unwrap();
        app2.handle_key(key(' ')).unwrap();
        assert_eq!(app2.selected_ids.len(), 2, "普通模式选中 2 项");
        app2.handle_key(key('x')).unwrap(); // 批量完成
        let nexts = tasks::list(
            &conn2,
            &ListFilter {
                status: Some(task::Status::Next),
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(nexts.len(), 0, "批量状态变更作用于全部 2 项");
        assert!(app2.selected_ids.is_empty(), "批量状态变更后清空选择集");
    }

    #[test]
    fn archived_view_can_restore_multiple_selected() {
        // 回归：普通模式 Space 多选后按 u（恢复）应恢复全部选中项，而非仅当前行。
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        for i in 0..3 {
            let t = tasks::create_capture(
                &conn,
                &CaptureInput {
                    title: format!("restore-batch-{i}"),
                    status: task::Status::Inbox,
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
            tasks::archive(&conn, &t.id).unwrap();
        }

        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        app.handle_key(key('8')).unwrap(); // 归档箱视图
        app.handle_key(key('l')).unwrap(); // 焦点移到列表栏
        let top_two: Vec<String> = tasks::list_archived(&conn)
            .unwrap()
            .into_iter()
            .take(2)
            .map(|t| t.id)
            .collect();
        assert_eq!(top_two.len(), 2, "归档箱至少两条用于批量恢复测试");

        // 普通模式：Space 点选前两行。
        app.handle_key(key(' ')).unwrap();
        app.handle_key(key('j')).unwrap();
        app.handle_key(key(' ')).unwrap();
        assert_eq!(app.selected_ids.len(), 2, "普通模式选中 2 项");

        app.handle_key(key('u')).unwrap(); // 批量恢复
        for id in &top_two {
            let t = tasks::get(&conn, id).unwrap();
            assert!(t.archived_at.is_none(), "选中项已恢复: {}", id);
        }
        assert!(app.selected_ids.is_empty(), "批量恢复后清空选择集");

        // 只剩 1 条仍归档。
        let remaining = tasks::list_archived(&conn).unwrap();
        assert_eq!(remaining.len(), 1, "只剩未被选中的任务仍归档");
    }

    #[test]
    fn biweekly_shorthand_reschedules_after_done() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        // 一句话录入：*2w[1,3] → 每两周周一/周三
        let q = crate::parser::parse_quick_add("上体育课 *2w[1,3] ~2026-08-12 09:00");
        assert_eq!(
            q.rrule.as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE")
        );
        let due = crate::time::parse_time("2026-08-12 09:00").unwrap(); // 周三

        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: q.title,
                status: task::Status::Scheduled,
                due_at: Some(due),
                tag_names: q.tags,
                rrule: q.rrule,
                ..Default::default()
            },
        )
        .unwrap();

        // 完成后被重新排程到隔周的周一 (08-24), 而非结束
        let done = tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
        assert_eq!(done.status, task::Status::Scheduled, "循环任务重新排程");
        assert_eq!(done.completed_at, None);
        assert_eq!(
            crate::time::format_local(done.due_at),
            "2026-08-24 09:00",
            "下一次发生 = 2 周后的周一"
        );
    }

    #[test]
    fn lang_and_theme_toggle_persist_to_settings() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();

        // 默认中文 + 深色主题
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        assert_eq!(app.lang, crate::i18n::Lang::Zh);
        assert!(app.theme.is_dark, "默认 Catppuccin Mocha 深色");
        assert_eq!(crate::repo::settings::get(&conn, "lang").unwrap(), None);
        assert_eq!(crate::repo::settings::get(&conn, "theme").unwrap(), None);

        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("收件箱"), "中文默认显示收件箱");

        // F6 切英文 → 写入 settings 表，界面文案切换
        app.handle_key(kc(KeyCode::F(6))).unwrap();
        assert_eq!(app.lang, crate::i18n::Lang::En);
        assert_eq!(
            crate::repo::settings::get(&conn, "lang")
                .unwrap()
                .as_deref(),
            Some("en")
        );
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("Inbox"), "英文侧边栏显示 Inbox");

        // F5 切亮色主题 → 写入 settings 表
        app.handle_key(kc(KeyCode::F(5))).unwrap();
        assert!(!app.theme.is_dark, "F5 切到 Latte 亮色");
        assert_eq!(
            crate::repo::settings::get(&conn, "theme")
                .unwrap()
                .as_deref(),
            Some("latte")
        );

        // 模拟重启：从 DB 恢复语言与主题
        drop(app);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;
        assert_eq!(app.lang, crate::i18n::Lang::En, "重启后恢复英文");
        assert!(!app.theme.is_dark, "重启后恢复亮色主题");
    }

    #[test]
    fn shift_c_enters_checklist_adding_and_pomo_config_moved_to_bracket() {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        // Shift+C（Char('C')）→ 新增检查单（不再被番茄钟配置遮蔽）
        app.handle_key(key('C')).unwrap();
        assert_eq!(app.mode, Mode::ChecklistAdding, "Shift+C 进入新增检查单");
        app.handle_key(kc(KeyCode::Esc)).unwrap();

        // '[' → 自定义番茄钟时长
        app.handle_key(key('[')).unwrap();
        assert_eq!(app.mode, Mode::ConfiguringPomo, "'[' 进入番茄钟配置");
        app.handle_key(kc(KeyCode::Esc)).unwrap();
    }

    #[test]
    fn input_cursor_edits_insert_delete_and_move() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();

        // 打开快速录入：光标默认在末尾。
        app.handle_key(key('a')).unwrap();
        assert_eq!(app.mode, Mode::Capturing);
        assert_eq!(app.input_cursor, 0);

        for c in "ab".chars() {
            app.handle_key(key(c)).unwrap();
        }
        assert_eq!(app.input, "ab");
        assert_eq!(app.input_cursor, 2);

        // 光标移到中间，插入字符。
        app.handle_key(kc(KeyCode::Left)).unwrap();
        assert_eq!(app.input_cursor, 1);
        app.handle_key(key('X')).unwrap();
        assert_eq!(app.input, "aXb");
        assert_eq!(app.input_cursor, 2);

        // 光标处插入多字节（中文），光标保持在字符边界。
        app.handle_key(key('测')).unwrap();
        assert_eq!(app.input, "aX测b");
        assert_eq!(&app.input[..app.input_cursor], "aX测");

        // Delete 删除光标后一个字符。
        app.handle_key(kc(KeyCode::Delete)).unwrap();
        assert_eq!(app.input, "aX测");

        // Backspace 删除光标前一个字符。
        app.handle_key(kc(KeyCode::Backspace)).unwrap();
        assert_eq!(app.input, "aX");

        // Home / End。
        app.handle_key(kc(KeyCode::Home)).unwrap();
        assert_eq!(app.input_cursor, 0);
        app.handle_key(key('S')).unwrap();
        assert_eq!(app.input, "SaX");
        app.handle_key(kc(KeyCode::End)).unwrap();
        assert_eq!(app.input_cursor, 3);

        // 光标始终是字符边界，Backspace 到开头不越界。
        app.handle_key(kc(KeyCode::Home)).unwrap();
        app.handle_key(kc(KeyCode::Backspace)).unwrap();
        assert_eq!(app.input, "SaX");
        assert_eq!(app.input_cursor, 0);

        // Esc 退出并清空。
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn input_cursor_edits_mid_string_for_full_edit() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        // 进入全量编辑（e = 一句话编辑器），预填当前任务，光标在末尾。
        app.handle_key(key('e')).unwrap();
        assert_eq!(app.mode, Mode::Capturing);
        assert!(app.organizing_id.is_some(), "e 记录待编辑任务");
        let input = app.input.clone();
        assert_eq!(app.input_cursor, input.len());

        // 光标移到开头，删除再插入，验证可修改预填内容。
        app.handle_key(kc(KeyCode::Home)).unwrap();
        app.handle_key(kc(KeyCode::Delete)).unwrap();
        app.handle_key(key('P')).unwrap();
        assert!(app.input.starts_with('P'));
        assert_eq!(app.input_cursor, 1);
    }

    #[test]
    fn tab_completion_works_at_cursor() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();

        app.handle_key(key('a')).unwrap();
        assert_eq!(app.mode, Mode::Capturing);

        // 实时弹出：输入 @ho 无需按 Tab 即出现候选（唯一 home），输入保持 @ho，ghost=me。
        for c in "buy @ho".chars() {
            app.handle_key(key(c)).unwrap();
        }
        assert!(app.completion_active(), "输入 @ho 即激活候选");
        assert_eq!(app.input, "buy @ho", "ghost 模式不改输入");
        let ghost = app.completion_ghost().unwrap();
        assert_eq!(ghost.0, '@');
        assert_eq!(ghost.1, "ho", "typed=ho");
        assert_eq!(ghost.2, "me", "ghost=me");

        // Tab 接受候选：补齐完整 token + 追加空格。
        app.handle_key(kc(KeyCode::Tab)).unwrap();
        assert_eq!(
            app.input, "buy @home ",
            "接受候选补齐并加空格: {}",
            app.input
        );
        assert_eq!(app.mode, Mode::Capturing, "接受不退出编辑");
        assert!(!app.completion_active(), "接受后关闭候选");

        // 多候选：*w 有 w/weekday/weekend，用 ↑↓ 与 Ctrl+n/p 循环，Esc 取消。
        app.input.clear();
        app.input_cursor = 0;
        for c in "*w".chars() {
            app.handle_key(key(c)).unwrap();
        }
        assert!(app.completion_active(), "*w 实时激活候选");
        assert_eq!(app.input, "*w", "首候选 w 以 ghost 显示");
        app.handle_key(kc(KeyCode::Down)).unwrap();
        assert_eq!(app.input, "*weekday", "Down 切到 weekday: {}", app.input);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.input, "*weekend", "Ctrl+n 切到 weekend: {}", app.input);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.input, "*weekday", "Ctrl+p 切回 weekday: {}", app.input);
        app.handle_key(kc(KeyCode::Up)).unwrap();
        assert_eq!(app.input, "*w", "Up 回到首候选: {}", app.input);
        app.handle_key(kc(KeyCode::Down)).unwrap();
        assert_eq!(app.input, "*weekday", "Down 再切: {}", app.input);
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(!app.completion_active(), "Esc 取消候选");
        assert_eq!(app.input, "*weekday", "取消保留当前输入");
        assert_eq!(app.mode, Mode::Capturing, "取消不退出编辑");

        // 光标在中间时：Tab 补全光标所在词。
        app.input.clear();
        app.input_cursor = 0;
        for c in "buy @ho milk".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Home)).unwrap();
        for _ in "buy ".chars() {
            app.handle_key(kc(KeyCode::Right)).unwrap();
        }
        for _ in "@ho".chars() {
            app.handle_key(kc(KeyCode::Right)).unwrap();
        }
        app.handle_key(kc(KeyCode::Tab)).unwrap();
        assert!(app.input.contains("@home"), "光标所在词补全: {}", app.input);
    }

    #[test]
    fn completion_extends_to_time_and_rrule() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();

        app.handle_key(key('a')).unwrap();

        // ~t → ghost today；Tab 采纳。
        for c in "~t".chars() {
            app.handle_key(key(c)).unwrap();
        }
        assert!(app.completion_active(), "~t 实时激活候选");
        let ghost = app.completion_ghost().unwrap();
        assert_eq!(ghost.0, '~');
        assert_eq!(ghost.2, "oday", "ghost=oday");
        app.handle_key(kc(KeyCode::Tab)).unwrap();
        assert!(
            app.input.contains("~today"),
            "~t 采纳为 ~today: {}",
            app.input
        );

        // *we → ghost weekday
        app.input.clear();
        app.input_cursor = 0;
        for c in "*we".chars() {
            app.handle_key(key(c)).unwrap();
        }
        assert!(app.completion_active(), "*we 实时激活候选");
        let ghost = app.completion_ghost().unwrap();
        assert_eq!(ghost.2, "ekday", "ghost=ekday");
    }

    #[test]
    fn capture_rejects_invalid_rrule_and_preserves_input() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = App::new(&conn).unwrap();

        app.handle_key(key('a')).unwrap();
        assert_eq!(app.mode, Mode::Capturing);

        // 无效循环：提交被拦截，输入保留、仍处编辑模式。
        for c in "买牛奶 *xx".chars() {
            app.handle_key(key(c)).unwrap();
        }
        let kept = app.input.clone();
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Capturing, "无效循环不退出编辑");
        assert_eq!(app.input, kept, "无效循环保留输入");
        assert!(
            app.status_message.contains("循环无效"),
            "提示循环无效: {}",
            app.status_message
        );

        // 无任务被创建。
        let filter = crate::repo::tasks::ListFilter {
            status: None,
            tags: vec![],
            query: None,
            review_stale: false,
        };
        assert_eq!(tasks::list(&conn, &filter).unwrap().len(), 0);

        // 修正为有效循环后可提交。
        app.input.clear();
        app.input_cursor = 0;
        for c in "买牛奶 *d".chars() {
            app.handle_key(key(c)).unwrap();
        }
        app.handle_key(kc(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Normal, "有效循环提交成功");
        assert_eq!(tasks::list(&conn, &filter).unwrap().len(), 1);
    }

    #[test]
    fn visual_mode_does_not_render_input_overlay() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();

        // 进入可视模式：不应额外叠加输入弹层。三栏布局各有一个圆角边框，
        // 若误渲染空输入框则会多出第 4 个圆角框。
        app.handle_key(key('v')).unwrap();
        assert_eq!(app.mode, Mode::Visual);
        term.clear().unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let visual_snap = snap(&term);
        assert_eq!(
            visual_snap.matches('╭').count(),
            3,
            "可视模式不应渲染第 4 个输入弹层框"
        );
    }

    #[test]
    fn space_toggles_and_ctrl_selects_non_contiguous() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        // 普通模式下 Space 切换当前行加入选择集。
        app.handle_key(key(' ')).unwrap();
        assert_eq!(app.selected_ids.len(), 1, "Space 选中当前行");

        // 普通模式下选中行应有可见标记 [v]（不进入可视模式也能看出已点选）。
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.clear().unwrap();
        term.draw(|f| app.render(f)).unwrap();
        assert!(
            norm(&snap(&term)).contains("[v]"),
            "Space 选中后普通模式应显示 [v] 标记"
        );

        // j 移动后 Space 再选第二行（Inbox 视图两行），形成非连续/多点选择。
        app.handle_key(key('j')).unwrap();
        app.handle_key(key(' ')).unwrap();
        assert_eq!(app.selected_ids.len(), 2, "多选: 行0,1");

        // 再次 Space 切换当前行：从选择集移出。
        app.handle_key(key(' ')).unwrap();
        assert_eq!(app.selected_ids.len(), 1, "Space 再次切换移出当前行");

        // Ctrl+a 全选。
        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_a).unwrap();
        assert_eq!(
            app.selected_ids.len(),
            app.items.len(),
            "Ctrl+a 全选当前视图所有行"
        );

        // Ctrl+u 反选：全选后再反选应为空。
        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        app.handle_key(ctrl_u).unwrap();
        assert!(app.selected_ids.is_empty(), "Ctrl+u 反选后清空");

        // 再反选一次 = 全选。
        app.handle_key(ctrl_u).unwrap();
        assert_eq!(app.selected_ids.len(), app.items.len(), "再反选=全选");

        // Esc 清空选择集。
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(app.selected_ids.is_empty(), "Esc 清空选择集");
    }

    #[test]
    fn confirm_dialog_renders_centered_for_batch_ops() {
        use crate::tui::app::Mode as M;
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.popup = None;

        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();

        // 进入归档确认：应渲染居中确认弹层，而非底部仅文字提示。
        app.handle_key(kc(KeyCode::Enter)).unwrap(); // 选中第一个 Inbox 任务 → 组织编辑
        app.handle_key(kc(KeyCode::Esc)).unwrap(); // 取消编辑回 Normal
        assert_eq!(app.mode, Mode::Normal);
        app.handle_key(key('A')).unwrap();
        assert_eq!(app.mode, M::ConfirmArchive);
        term.clear().unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("确认归档"), "确认弹层标题可见");
        assert!(s.contains("y/Enter"), "确认键提示可见");
        assert!(s.contains("n/Esc"), "取消键提示可见");
        app.handle_key(key('y')).unwrap();

        // 永久删除确认（归档箱视图）。
        app.handle_key(key('8')).unwrap();
        app.handle_key(key('D')).unwrap();
        assert_eq!(app.mode, M::ConfirmPurge);
        term.clear().unwrap();
        term.draw(|f| app.render(f)).unwrap();
        let s = norm(&snap(&term));
        assert!(s.contains("确认永久删除"), "删除确认弹层标题可见");
        assert!(
            s.contains("y/Enter") && s.contains("n/Esc"),
            "确认/取消提示可见"
        );
    }
    // ---------- Phase 4：handlers/render 盲区补测 ----------

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn module_toggle_popup_navigates_and_persists() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        assert!(!app.show_help);

        // F7 打开模块开关弹层，初始 idx=0（splash）
        app.handle_key(kc(KeyCode::F(7))).unwrap();
        assert_eq!(
            app.popup,
            Some(crate::tui::app::Popup::ModuleToggles(0)),
            "F7 应打开模块开关弹层"
        );
        assert!(app.modules.splash);

        // space 关掉 splash 并持久化
        app.handle_key(key(' ')).unwrap();
        assert!(!app.modules.splash, "space 应翻转 splash");
        assert_eq!(
            app.popup,
            Some(crate::tui::app::Popup::ModuleToggles(0)),
            "切换后弹层保持打开"
        );
        // 重新加载后仍为关（已持久化到 settings 表）
        let reloaded = crate::repo::modules::ModuleVisibility::load(app.conn);
        assert!(!reloaded.splash);

        // j 向下循环导航（0 → 1），k 反向且在 0 处回绕到 7
        app.handle_key(key('j')).unwrap();
        assert_eq!(app.popup, Some(crate::tui::app::Popup::ModuleToggles(1)));
        app.popup = Some(crate::tui::app::Popup::ModuleToggles(0));
        app.handle_key(key('k')).unwrap();
        assert_eq!(app.popup, Some(crate::tui::app::Popup::ModuleToggles(7)));

        // Esc 关闭弹层
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(app.popup.is_none());
    }

    #[test]
    fn disabled_module_blocks_digit_view_and_quotes_toggle_off_returns_to_inbox() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();

        // 关闭 reference 模块后按 '6' 不应切视图
        app.handle_key(kc(KeyCode::F(7))).unwrap(); // idx 0 splash
        for _ in 0..1 {
            app.handle_key(key('j')).unwrap(); // idx 1 reference
        }
        app.handle_key(key(' ')).unwrap(); // off
        app.handle_key(kc(KeyCode::Esc)).unwrap();

        app.handle_key(key('6')).unwrap();
        assert_ne!(app.view, View::Reference, "禁用模块的数字键应被忽略");
        assert!(!app.modules.reference);

        // 在 Quotes 视图下关闭 quotes 模块应自动回到 Inbox
        app.view = View::Quotes;
        app.quotes.enabled = true;
        app.handle_key(kc(KeyCode::F(7))).unwrap();
        for _ in 0..5 {
            app.handle_key(key('j')).unwrap(); // idx 5 quotes
        }
        app.handle_key(key(' ')).unwrap();
        assert!(!app.quotes.enabled);
        assert_eq!(app.view, View::Inbox, "quotes 关闭时应离开 Quotes 视图");
    }

    #[test]
    fn help_drawer_navigation_scrolls_and_closes() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.handle_key(key('?')).unwrap();
        assert!(app.show_help);

        app.handle_key(key('j')).unwrap();
        assert_eq!(app.help_scroll, 1);
        app.handle_key(kc(KeyCode::PageDown)).unwrap();
        assert_eq!(app.help_scroll, 2);
        app.handle_key(key('G')).unwrap();
        assert_eq!(app.help_scroll, usize::MAX, "G 跳到底部");
        app.handle_key(key('g')).unwrap();
        assert_eq!(app.help_scroll, 0, "g 回顶部");
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(!app.show_help, "Esc 关闭帮助抽屉");
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn system_keys_toggle_bar_syntax_and_selection() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();

        // F2 快捷键条
        let before = app.show_shortcut_bar;
        app.handle_key(kc(KeyCode::F(2))).unwrap();
        assert_eq!(app.show_shortcut_bar, !before);

        // Ctrl+P 语法面板
        assert!(!app.show_syntax);
        app.handle_key(ctrl('p')).unwrap();
        assert!(app.show_syntax);

        // Ctrl+A 全选 / Ctrl+U 反选
        app.pane = Pane::Center;
        app.reload().unwrap();
        let total = app.items.len();
        app.handle_key(ctrl('a')).unwrap();
        assert_eq!(app.selected_ids.len(), total, "全选覆盖所有行");
        app.handle_key(ctrl('u')).unwrap();
        assert_eq!(app.selected_ids.len(), 0, "反选清空（原本全选）");
    }

    #[test]
    fn esc_cascades_through_visual_filter_and_selection() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();

        // 1) 可视模式 Esc → 回 Normal 并清空选择
        app.handle_key(key('v')).unwrap();
        assert_eq!(app.mode, Mode::Visual);
        app.selected_ids.insert("a".to_string());
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.selected_ids.is_empty());

        // 2) 有过滤时 Esc → 清除过滤
        app.tag_filter = Some("work".into());
        app.search_query.push_str("rust");
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(app.tag_filter.is_none() && app.search_query.is_empty());

        // 3) 有选择时 Esc → 清除选择
        app.selected_ids.insert("b".to_string());
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(app.selected_ids.is_empty(), "过滤已空时 Esc 应清选择");

        // 4) 无任何状态时 Esc → 隐藏番茄横幅
        assert!(!app.hide_pomo_banner);
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        assert!(app.hide_pomo_banner, "兜底分支应隐藏 pomo banner");
    }

    #[test]
    fn archived_view_restore_single_and_batch() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        app.pane = Pane::Center;

        // 归档两个任务后进入 Archived 视图（数字 8）
        let ids: Vec<String> = app.items.iter().take(2).map(|r| r.id.clone()).collect();
        for id in &ids {
            tasks::archive(app.conn, id).unwrap();
        }
        app.handle_key(key('8')).unwrap();
        assert_eq!(app.view, View::Archived);

        // 单个恢复：光标停在第一行，'u' 恢复
        app.selected_ids.clear();
        app.reload().unwrap();
        app.selected = 0;
        let first_archived = app.items[app.selected].id.clone();
        app.handle_key(key('u')).unwrap();
        assert!(
            tasks::get(app.conn, &first_archived)
                .unwrap()
                .archived_at
                .is_none(),
            "'u' 应恢复当前选中的归档任务"
        );

        // 批量恢复：选中剩余归档行后 'u'
        app.reload().unwrap();
        assert!(!app.items.is_empty(), "仍有归档任务可批量恢复");
        app.selected_ids.clear();
        for row in &app.items {
            app.selected_ids.insert(row.id.clone());
        }
        app.visual_start_idx = None;
        app.handle_key(key('u')).unwrap();
        for row in &tasks::list(
            app.conn,
            &ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap()
        {
            assert!(row.archived_at.is_none(), "{} 仍处于归档", row.id);
        }
        assert!(app.selected_ids.is_empty(), "批量恢复后清空选择集");
    }

    #[test]
    fn pane_keys_cycle_and_clamp_between_three_columns() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();

        // l 从 Left → Center → Right → 停在 Right（钳位）
        app.pane = Pane::Left;
        app.handle_key(key('l')).unwrap();
        assert_eq!(app.pane, Pane::Center);
        app.handle_key(key('l')).unwrap();
        assert_eq!(app.pane, Pane::Right);
        app.handle_key(key('l')).unwrap();
        assert_eq!(app.pane, Pane::Right, "右边界钳位");

        // h 反向：Right → Center → Left → 停在 Left（钳位）
        app.handle_key(key('h')).unwrap();
        assert_eq!(app.pane, Pane::Center);
        app.handle_key(key('h')).unwrap();
        assert_eq!(app.pane, Pane::Left);
        app.handle_key(key('h')).unwrap();
        assert_eq!(app.pane, Pane::Left, "左边界钳位");
    }

    #[test]
    fn render_views_smoke_cover_disabled_modules_and_popups() {
        crate::repo::state::set_test_override();
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        seed(&conn);
        let mut app = App::new(&conn).unwrap();
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();

        let mut draw_contains = |app: &mut App, needle: &str| -> bool {
            term.clear().unwrap();
            term.draw(|f| app.render(f)).unwrap();
            norm(&snap(&term)).contains(needle)
        };

        // 各视图渲染冒烟：标题出现、无 panic
        assert!(draw_contains(&mut app, "任务·收件箱"));
        for (digit, needle) in [
            ('2', "下一步"),
            ('3', "等待中"),
            ('5', "将来"),
            ('7', "已完成"),
            ('8', "归档箱"),
            ('9', "标签库"),
        ] {
            app.handle_key(key(digit)).unwrap();
            assert!(
                draw_contains(&mut app, needle),
                "视图 {digit} 应渲染 {needle}"
            );
        }

        // 帮助抽屉 + 模块弹层的渲染路径
        app.handle_key(key('?')).unwrap();
        assert!(draw_contains(&mut app, "快捷键"), "帮助抽屉渲染标题");
        app.handle_key(kc(KeyCode::Esc)).unwrap();
        app.handle_key(kc(KeyCode::F(7))).unwrap();
        assert!(draw_contains(&mut app, "模块"), "模块弹层应渲染");
    }
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
            "your mind is for having ideas,",
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
        assert!(osc8.contains("your mind is for having ideas,"));
    }
}
