pub mod app;
pub mod handlers;
pub mod icons;
pub mod keys;
pub mod render;
pub mod theme;
pub mod ui;

pub(crate) use app::{App, Pane, View};
pub(crate) use handlers::AppHandlers;
pub(crate) use render::AppRender;

use anyhow::Result;
use app::Row;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use horae_core::model::task::{self, Task};
use horae_core::repo::tags;
use ratatui::{backend::CrosstermBackend, Terminal};
use rusqlite::Connection;
use std::io::{self, Stdout};
use std::time::Duration;

/// 状态的中文含义，用于引导栏的“状态地图”。按当前语言返回。
pub(crate) fn status_cn(lang: horae_core::i18n::Lang, s: task::Status) -> &'static str {
    match s {
        task::Status::Inbox => tr!(lang, "收件箱", "Inbox"),
        task::Status::Next => tr!(lang, "下一步", "Next"),
        task::Status::Waiting => tr!(lang, "等待中", "Waiting"),
        task::Status::Scheduled => tr!(lang, "已排程", "Scheduled"),
        task::Status::Someday => tr!(lang, "将来/也许", "Someday"),
        task::Status::Reference => tr!(lang, "参考资料", "Reference"),
        task::Status::Done => tr!(lang, "已完成", "Done"),
    }
}

/// 引导栏里各视图的中文/英文名（含日视图与归档箱等无状态视图）。
pub(crate) fn view_label(lang: horae_core::i18n::Lang, v: View) -> &'static str {
    match v {
        View::Inbox => tr!(lang, "收件箱", "Inbox"),
        View::Today => tr!(lang, "今日", "Today"),
        View::Tomorrow => tr!(lang, "明日", "Tomorrow"),
        View::Next => tr!(lang, "下一步", "Next"),
        View::Waiting => tr!(lang, "等待中", "Waiting"),
        View::Scheduled => tr!(lang, "已排程", "Scheduled"),
        View::Someday => tr!(lang, "将来/也许", "Someday"),
        View::Reference => tr!(lang, "参考资料", "Reference"),
        View::Done => tr!(lang, "已完成", "Done"),
        View::Review => tr!(lang, "周回顾", "Review"),
        View::Archived => tr!(lang, "归档箱", "Archived"),
        View::Tags => tr!(lang, "标签库", "Tags"),
        View::Quotes => tr!(lang, "金句", "Quotes"),
        View::Settings => tr!(lang, "设置", "Settings"),
        View::Workflow => tr!(lang, "GTD 工作流", "GTD Workflow"),
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
    let due = horae_core::schedule::display_due(t, None);
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
pub mod splash;

/// 启动交互式 TUI。
/// 内置默认开屏图；允许用户用 `~/.config/horae/splash.png` 覆盖。
pub fn run(conn: &Connection, profile: Option<&str>) -> Result<()> {
    let mods = horae_core::repo::modules::ModuleVisibility::load(conn);
    // 先把真正耗时的初始化（加载数据、构建 App）做完——这一刻才是「启动完成」的时点。
    // 在此冻结启动用时，开屏读到的才是包含全部初始化成本的准确值，而非开屏前的 0ms。
    let mut app = App::new(conn)?;
    app.profile_name = profile
        .map(|s| s.to_string())
        .or_else(|| {
            horae_core::config::Config::load()
                .ok()
                .map(|c| c.default_profile)
        })
        .unwrap_or_default();
    let _ = horae_core::time::boot_elapsed_ms();
    if mods.splash {
        let _ = splash::show_splash(conn);
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app, profile);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste
    )?;
    terminal.show_cursor()?;
    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    _profile: Option<&str>,
) -> Result<()> {
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
                Event::Paste(text) => {
                    app.handle_paste(text);
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
mod tests;
