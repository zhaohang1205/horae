use super::*;
use crate::tui::app::Mode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use horae_core::db::migrate;
use horae_core::repo::tasks::ListFilter;
use horae_core::repo::tasks::{self, CaptureInput};
use ratatui::backend::TestBackend;
use std::io::Write;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}
fn kc(k: KeyCode) -> KeyEvent {
    KeyEvent::new(k, KeyModifiers::empty())
}

fn seed(conn: &Connection) {
    // 既有测试假设 Normal 模式起步：显式关掉"启动即快速录入"（生产默认开启）。
    horae_core::repo::settings::set(conn, "start_capture", "0").unwrap();
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
    mk("Write homepage copy", task::Status::Inbox, &["work"]);
    mk("Buy groceries", task::Status::Inbox, &["home", "errands"]);
    mk("Read Rust book", task::Status::Next, &["learning"]);
    mk("Pay taxes", task::Status::Waiting, &["work"]);
    mk("Plan vacation", task::Status::Someday, &["home"]);
    mk("Finish report", task::Status::Done, &[]);
}

/// 构造 Normal 模式起步的测试用 App：显式关闭“启动即快速录入”
/// （生产环境该开关默认开启）。
fn app_normal(conn: &Connection) -> App<'_> {
    horae_core::repo::settings::set(conn, "start_capture", "0").unwrap();
    App::new(conn).unwrap()
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
        horae_core::time::format_local(st.scheduled_start_at),
        horae_core::time::format_local(Some(
            horae_core::time::parse_time("tomorrow 15:30").unwrap()
        )),
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);
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
        horae_core::time::parse_time("today 12:00").unwrap(),
    );
    mk(
        "due-tomorrow",
        task::Status::Scheduled,
        horae_core::time::parse_time("tomorrow 12:00").unwrap(),
    );
    mk(
        "overdue",
        task::Status::Next,
        horae_core::time::now_ms() - 2 * day_ms,
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
        horae_core::time::parse_time("today 09:00").unwrap(),
        None,
        Some("FREQ=DAILY".into()),
    )
    .unwrap();

    let mut app = app_normal(&conn);

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
    assert!(
        !t.iter().any(|s| s == "overdue"),
        "明日视图不含逾期任务（逾期只归今日）"
    );
    assert!(
        !t.iter().any(|s| s == "due-today"),
        "明日视图不含今天到期但未完成的任务（不再结转）"
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
fn checked_in_habit_moves_to_its_next_day() {
    horae_core::repo::state::set_test_override();
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
        horae_core::time::parse_time("today 09:00").unwrap(),
        None,
        Some("FREQ=DAILY".into()),
    )
    .unwrap();

    // 今日打卡 → 锚点推进到 now 之后的下一次发生（明日 09:00）
    tasks::transition(&conn, &rec.id, task::Status::Done).unwrap();

    let mut app = app_normal(&conn);
    app.popup = None;
    app.handle_key(key('J')).unwrap(); // 今日视图
    assert!(
        !app.items.iter().any(|r| r.title == "daily-habit"),
        "今日已打卡、下次发生在明日的习惯不再占着今日视图"
    );

    app.handle_key(key('K')).unwrap(); // 明日视图
    let row = app
        .items
        .iter()
        .find(|r| r.title == "daily-habit")
        .expect("明日视图含下一次发生");
    assert!(row.checked_in_today, "标记为今日已打卡");
    let next = row.due.expect("有下一次执行时间");
    let (t1s, t1e) = horae_core::time::local_day_bounds(1);
    assert!(next >= t1s && next <= t1e, "下一次发生落在明日窗口内");

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
fn overdue_recurring_shows_in_today_view() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 每周循环、锚点 3 天前：今天和明天都不是发生日，但上一次已错过 → 应算逾期
    tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "weekly-missed".into(),
            status: task::Status::Scheduled,
            due_at: Some(horae_core::time::now_ms() - 3 * 24 * 3600 * 1000),
            tag_names: vec![],
            rrule: Some("FREQ=WEEKLY".into()),
            ..Default::default()
        },
    )
    .unwrap();

    let mut app = app_normal(&conn);
    app.popup = None;
    app.handle_key(key('J')).unwrap(); // 今日视图
    let row = app
        .items
        .iter()
        .find(|r| r.title == "weekly-missed")
        .expect("逾期循环任务出现在今日视图");
    let due = row.due.expect("展示的是最近一次已错过的发生点");
    assert!(due < horae_core::time::now_ms(), "该发生点已过期");
    assert!(
        horae_core::time::is_overdue(Some(due)),
        "今日视图把它标为逾期"
    );

    app.handle_key(key('K')).unwrap(); // 明日视图
    assert!(
        !app.items.iter().any(|r| r.title == "weekly-missed"),
        "明日没有发生 → 明日视图不含它"
    );
}

#[test]
fn stale_daily_habit_still_shows_in_today_view() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 锚点停在 400 天前（超过 HORIZON=366）：展开视野不能从锚点起算，否则任务消失
    tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "daily-stale".into(),
            status: task::Status::Scheduled,
            due_at: Some(horae_core::time::now_ms() - 400 * 24 * 3600 * 1000),
            tag_names: vec![],
            rrule: Some("FREQ=DAILY".into()),
            ..Default::default()
        },
    )
    .unwrap();

    let mut app = app_normal(&conn);
    app.popup = None;
    app.handle_key(key('J')).unwrap(); // 今日视图
    assert!(
        app.items.iter().any(|r| r.title == "daily-stale"),
        "停摆很久的每日习惯仍出现在今日视图"
    );
    app.handle_key(key('K')).unwrap(); // 明日视图
    assert!(
        app.items.iter().any(|r| r.title == "daily-stale"),
        "也出现在明日视图"
    );
}

#[test]
fn startup_opens_no_popup() {
    use horae_core::repo::tasks::CaptureInput;
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 今日到期 + 逾期各一条：启动不应弹任何弹层（弹层会吞掉第一次按键）
    for (title, due) in [
        (
            "today",
            horae_core::time::parse_time("today 12:00").unwrap(),
        ),
        ("overdue", horae_core::time::now_ms() - 2 * 24 * 3600 * 1000),
    ] {
        tasks::create_capture(
            &conn,
            &CaptureInput {
                title: title.into(),
                status: task::Status::Next,
                due_at: Some(due),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
    }

    let app = app_normal(&conn);
    assert!(app.popup.is_none(), "启动不弹今日概览");
}

#[test]
fn day_views_only_include_actionable_statuses() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 同一到期时间，7 种状态各一条（今日 / 明日各一组）
    let statuses = [
        task::Status::Inbox,
        task::Status::Next,
        task::Status::Waiting,
        task::Status::Scheduled,
        task::Status::Someday,
        task::Status::Reference,
        task::Status::Done,
    ];
    for when in ["today", "tomorrow"] {
        for status in statuses.iter().copied() {
            tasks::create_capture(
                &conn,
                &CaptureInput {
                    title: format!("{}-{}", status, when),
                    status,
                    due_at: Some(horae_core::time::parse_time(&format!("{} 12:00", when)).unwrap()),
                    tag_names: vec![],
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }

    // 日视图只收「今天能动手的」状态：下一步 / 已排程
    let mut app = app_normal(&conn);
    let collect =
        |app: &App| -> Vec<String> { app.items.iter().map(|r| r.title.clone()).collect() };

    app.handle_key(key('J')).unwrap(); // 今日视图
    assert_eq!(collect(&app), vec!["next-today", "scheduled-today"]);
    assert_eq!(app.context_count(View::Today), 2, "侧栏今日计数同步");

    app.handle_key(key('K')).unwrap(); // 明日视图
    assert_eq!(collect(&app), vec!["next-tomorrow", "scheduled-tomorrow"]);
    assert_eq!(app.context_count(View::Tomorrow), 2, "侧栏明日计数同步");

    // 被过滤掉的状态仍照常待在各自的状态视图
    app.handle_key(key('1')).unwrap(); // Inbox
    assert_eq!(collect(&app), vec!["inbox-today", "inbox-tomorrow"]);
    app.handle_key(key('3')).unwrap(); // Waiting
    assert_eq!(collect(&app), vec!["waiting-today", "waiting-tomorrow"]);
}

#[test]
fn enter_opens_organize_on_scheduled() {
    horae_core::repo::state::set_test_override();
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
        horae_core::time::parse_time("tomorrow 10:00").unwrap(),
        None,
        None,
    )
    .unwrap();

    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
    let tags = horae_core::repo::tags::get_task_tags(&conn, &rec.id).unwrap();
    let names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, vec!["work"], "标签被替换");
}

#[test]
fn organize_edit_sets_time_tags_rrule() {
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
        horae_core::time::format_local(t.scheduled_start_at),
        horae_core::time::format_local(Some(horae_core::time::parse_time("tomorrow").unwrap())),
        "排程起点已设置"
    );
    let tags = horae_core::repo::tags::get_task_tags(&conn, &rec.id).unwrap();
    let names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, vec!["home"], "标签被替换为 @home");
}

#[test]
fn missed_habit_shows_overdue_in_today_view() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 今天的 slot 取凌晨后 1 分钟（几乎必然已过），未打卡 → 今日视图显示逾期
    let slot = horae_core::time::local_day_bounds(0).0 + 60_000;
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

    let mut app = app_normal(&conn);
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
        horae_core::time::is_overdue(Some(due)),
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 锚点在 3 天前（已错过），FREQ=DAILY → 最近计划执行 = 今天的同一时刻
    let anchor = horae_core::time::local_day_bounds(-3).0 + 60_000;
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

    let mut app = app_normal(&conn);
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
    let next = horae_core::schedule::effective_due(&task).unwrap();
    let next_str = norm(&horae_core::time::format_local(Some(next)));
    let anchor_str = norm(&horae_core::time::format_local(Some(anchor)));
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
    horae_core::repo::state::set_test_override();
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
        horae_core::time::parse_time("today 09:00").unwrap(),
        None,
        Some("FREQ=DAILY".into()),
    )
    .unwrap();
    tasks::transition(&conn, &rec.id, task::Status::Done).unwrap();

    let mut app = app_normal(&conn);
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
    let now = horae_core::time::now_ms();
    let h = 3600 * 1000i64;
    let d = 24 * 3600 * 1000i64;
    let zh = horae_core::i18n::Lang::Zh;

    // 未来
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now + 5 * h)).as_deref(),
        Some("5小时后")
    );
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now + 30 * 60 * 1000)).as_deref(),
        Some("30分钟后")
    );
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now + 2 * d)).as_deref(),
        Some("2天后")
    );

    // 过去（统一逾期措辞）
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now - 5 * h)).as_deref(),
        Some("逾期5小时")
    );
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now - 40 * 60 * 1000)).as_deref(),
        Some("逾期40分钟")
    );
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now - 2 * d)).as_deref(),
        Some("逾期2天")
    );
    assert_eq!(
        horae_core::time::relative_due(zh, Some(now - d)).as_deref(),
        Some("逾期1天")
    );

    // 完成时间展示
    assert_eq!(
        horae_core::time::relative_past(zh, Some(now - 3 * h)).as_deref(),
        Some("3小时前")
    );
    assert_eq!(
        horae_core::time::relative_past(zh, Some(now - 2 * d)).as_deref(),
        Some("2天前")
    );
    assert_eq!(horae_core::time::relative_past(zh, None), None);
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
            due_at: Some(horae_core::time::parse_time("today 09:00").unwrap()),
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
    let (tom_start, tom_end) = horae_core::time::local_day_bounds(1);
    assert!(
        next >= tom_start && next <= tom_end,
        "下一次发生落在明日窗口内"
    );

    // 明日视图应仍包含它
    let mut app = app_normal(&conn);
    app.popup = None;
    app.handle_key(key('K')).unwrap();
    assert!(app.items.iter().any(|r| r.title == "standup"));
}

#[test]
fn next_view_cycles_full_ring() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);
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
        View::Workflow,
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
    app.popup = None;
    // 默认关闭
    assert!(!app.quotes.enabled, "金句功能默认关闭");
    // 功能未启用：0 键与 " 键均无效
    app.handle_key(key('0')).unwrap();
    assert_eq!(app.view, View::Inbox, "未启用时 0 不切视图");
    app.handle_key(key('"')).unwrap();
    let tags = horae_core::repo::tags::get_task_tags(&conn, &t.id).unwrap();
    assert!(tags.is_empty(), "未启用时 \" 不生效");

    // F7 启用 + 持久化
    app.handle_key(kc(KeyCode::F(7))).unwrap(); // open popup
    for _ in 0..5 {
        app.handle_key(kc(KeyCode::Down)).unwrap(); // move from 0 to 5 (Quotes)
    }
    app.handle_key(key(' ')).unwrap(); // space toggles 5 (Quotes)
    app.handle_key(kc(KeyCode::Esc)).unwrap(); // close popup
    assert!(app.quotes.enabled, "F7+Space 启用金句功能");
    assert_eq!(
        horae_core::repo::settings::get(&conn, "quotes")
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
    let tags = horae_core::repo::tags::get_task_tags(&conn, &t.id).unwrap();
    assert!(
        tags.iter()
            .any(|g| g.name == horae_core::repo::tasks::QUOTE_TAG),
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
    let tags = horae_core::repo::tags::get_task_tags(&conn, &t.id).unwrap();
    assert!(
        tags.iter()
            .all(|g| g.name != horae_core::repo::tasks::QUOTE_TAG),
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
    let mut app = app_normal(&conn);
    app.popup = None;
    assert!(app.quotes.enabled, "重启后恢复启用");

    // F7 停用：若在金句视图则跳回收件箱
    app.handle_key(key('0')).unwrap();
    app.handle_key(kc(KeyCode::F(7))).unwrap();
    for _ in 0..5 {
        app.handle_key(kc(KeyCode::Down)).unwrap(); // move to index 5 (Quotes)
    }
    app.handle_key(key(' ')).unwrap();
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert!(!app.quotes.enabled);
    assert_eq!(app.view, View::Inbox, "停用时离开金句视图");
    assert_eq!(
        horae_core::repo::settings::get(&conn, "quotes")
            .unwrap()
            .as_deref(),
        Some("0"),
        "停用状态写入 settings"
    );
}

#[test]
fn settings_view_manages_profiles() {
    use horae_core::config::Config;

    // 隔离配置目录并持锁，避免与其它依赖 HORAE_CONFIG_DIR 的测试竞争
    horae_core::testutil::with_test_config_dir(|| {
        let mut conn = Connection::open(":memory:").unwrap();
        migrate::run(&mut conn).unwrap();
        let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let q = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "普通引用".into(),
            status: task::Status::Reference,
            tag_names: vec![horae_core::repo::tasks::QUOTE_TAG.to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let mut app = app_normal(&conn);
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
    use crate::tui::keys::{
        help_rows, status_strip, strip_keys, Ctx, KeyGroup, GROUP_ORDER, KEY_TABLE, NON_TASK_VIEWS,
    };
    use horae_core::i18n::Lang;

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

    // 检查单管理模式 → 专用模式键（不走 KEY_TABLE，无 j/k 等条目则说明回归）
    let chk_mode = Ctx {
        mode: Mode::ChecklistFocus,
        ..ctx(View::Inbox, true)
    };
    let chk_strip = strip_keys(&chk_mode, Lang::Zh);
    for k in ["j/k", "Space", "d", "J/K", "e", "Tab/Esc"] {
        assert!(
            chk_strip.iter().any(|(key, _)| *key == k),
            "检查单管理模式动态条应含 {}",
            k
        );
    }

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

    // 状态栏快捷键条：含 hjkl, a, x, s, /, w, f, F1, P, F5, F6, F7, Ctrl+P
    let strip = status_strip(Lang::Zh);
    assert_eq!(
        strip,
        vec![
            ("hjkl", "方向键"),
            ("a", "捕获"),
            ("x", "完成"),
            ("s", "将来"),
            ("/", "搜索"),
            ("w", "等待"),
            ("f", "过滤"),
            ("F1", "帮助"),
            ("P", "专注/续杯"),
            ("F5", "主题"),
            ("F6", "语言"),
            ("F7", "功能开关"),
            ("Ctrl+P", "语法"),
        ]
    );
    let strip_en = status_strip(Lang::En);
    assert_eq!(
        strip_en,
        vec![
            ("hjkl", "arrows"),
            ("a", "capture"),
            ("x", "done"),
            ("s", "someday"),
            ("/", "search"),
            ("w", "waiting"),
            ("f", "filter"),
            ("F1", "help"),
            ("P", "focus/continue"),
            ("F5", "theme"),
            ("F6", "lang"),
            ("F7", "toggles"),
            ("Ctrl+P", "syntax"),
        ]
    );
    assert!(strip.iter().any(|(k, _)| *k == "hjkl"), "状态栏含 hjkl");
    assert!(!strip.iter().any(|(k, _)| *k == "q"), "状态栏不含 q");
    assert!(!strip.iter().any(|(k, _)| *k == "g/G"), "状态栏不含 g/G");
    assert!(!strip.iter().any(|(k, _)| *k == "0-9"), "状态栏不含视图键");
    // 状态栏快捷键条按热度降序
    assert!(
        heats(&strip).windows(2).all(|w| w[0] >= w[1]),
        "状态栏按热度降序: {:?}",
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
            due_at: Some(horae_core::time::now_ms() - 3 * 24 * 3600 * 1000i64),
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let t = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "finishedlongago".into(),
            status: task::Status::Next,
            due_at: Some(horae_core::time::now_ms() - 3 * 24 * 3600 * 1000i64),
            tag_names: vec![],
            ..Default::default()
        },
    )
    .unwrap();
    tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let t = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "completed-then-archived".into(),
            status: task::Status::Next,
            due_at: Some(horae_core::time::now_ms() - 3 * 24 * 3600 * 1000i64),
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
            due_at: Some(horae_core::time::now_ms() - 5 * 24 * 3600 * 1000i64),
            tag_names: vec![],
            ..Default::default()
        },
    )
    .unwrap();
    let arch = tasks::archive(&conn, &del.id).unwrap();
    assert_eq!(arch.archive_reason.as_deref(), Some("deleted"));

    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
    let mut app2 = app_normal(&conn2);
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
    horae_core::repo::state::set_test_override();
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

    let mut app = app_normal(&conn);
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
    let q = horae_core::parser::parse_quick_add("上体育课 *2w[1,3] ~2026-08-12 09:00");
    assert_eq!(
        q.rrule.as_deref(),
        Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE")
    );
    let due = horae_core::time::parse_time("2026-08-12 09:00").unwrap(); // 周三

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

    // 完成后被重新排程到 now 之后的下一次发生（隔周的周一/周三），而非结束。
    // 期望值从展开结果里算，不写死日期：锚点早已过期，只前进一步会停在过去。
    let done = tasks::transition(&conn, &t.id, task::Status::Done).unwrap();
    assert_eq!(done.status, task::Status::Scheduled, "循环任务重新排程");
    assert_eq!(done.completed_at, None);
    let next = done.due_at.expect("有下一次发生");
    let now = horae_core::time::now_ms();
    assert!(next > now, "下一次发生在未来，而不是停在过去: {:?}", next);
    let occ = horae_core::schedule::occurrences("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE", due).unwrap();
    let expect = occ.into_iter().find(|m| *m > now).unwrap();
    assert_eq!(next, expect, "跳到 now 之后的第一次发生");
}

#[test]
fn lang_and_theme_toggle_persist_to_settings() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 默认中文 + 深色主题
    let mut app = app_normal(&conn);
    app.popup = None;
    assert_eq!(app.lang, horae_core::i18n::Lang::Zh);
    assert!(app.theme.is_dark, "默认 Catppuccin Mocha 深色");
    assert_eq!(
        horae_core::repo::settings::get(&conn, "lang").unwrap(),
        None
    );
    assert_eq!(
        horae_core::repo::settings::get(&conn, "theme").unwrap(),
        None
    );

    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s = norm(&snap(&term));
    assert!(s.contains("收件箱"), "中文默认显示收件箱");

    // F6 切英文 → 写入 settings 表，界面文案切换
    app.handle_key(kc(KeyCode::F(6))).unwrap();
    assert_eq!(app.lang, horae_core::i18n::Lang::En);
    assert_eq!(
        horae_core::repo::settings::get(&conn, "lang")
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
        horae_core::repo::settings::get(&conn, "theme")
            .unwrap()
            .as_deref(),
        Some("latte")
    );

    // 模拟重启：从 DB 恢复语言与主题
    drop(app);
    let mut app = app_normal(&conn);
    app.popup = None;
    assert_eq!(app.lang, horae_core::i18n::Lang::En, "重启后恢复英文");
    assert!(!app.theme.is_dark, "重启后恢复亮色主题");
}

#[test]
fn shift_c_enters_checklist_adding_and_pomo_config_moved_to_bracket() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

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
fn paste_inserts_into_capture_and_normalizes_newlines() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    // 普通模式粘贴：自动进入快速录入并填入文本。
    app.handle_paste("买牛奶 @home ~明天".to_string());
    assert_eq!(app.mode, Mode::Capturing);
    assert_eq!(app.input, "买牛奶 @home ~明天");
    assert_eq!(app.input_cursor, app.input.len());

    // 多行粘贴：换行/制表符归一为空格（快速录入为单行）。
    app.input_clear();
    app.handle_paste("第一行\r\n第二行\t末尾".to_string());
    assert_eq!(app.input, "第一行 第二行 末尾");

    // 已在输入模式下粘贴：光标处插入。
    app.input_clear();
    app.handle_paste("ab".to_string());
    app.handle_key(kc(KeyCode::Home)).unwrap();
    app.handle_paste("XY".to_string());
    assert_eq!(app.input, "XYab");
}

#[test]
fn input_cursor_edits_mid_string_for_full_edit() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

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

    // 多候选：*w 有 w/weekday/weekend，用 ↑↓ 与 Ctrl+n/p 循环候选索引，Tab 采纳，Esc 取消。
    app.input.clear();
    app.input_cursor = 0;
    for c in "*w".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active(), "*w 实时激活候选");
    assert_eq!(app.input, "*w", "首候选 w 以 ghost 显示，输入保持 *w");
    assert_eq!(app.completion_index, 0);
    app.handle_key(kc(KeyCode::Down)).unwrap();
    assert_eq!(app.completion_index, 1, "Down 切到 index 1 (weekday)");
    assert_eq!(app.input, "*w", "输入仍然保持 *w，未被提前覆盖");
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.completion_index, 2, "Ctrl+n 切到 index 2 (weekend)");
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.completion_index, 1, "Ctrl+p 切回 index 1 (weekday)");
    app.handle_key(kc(KeyCode::Up)).unwrap();
    assert_eq!(app.completion_index, 0, "Up 回到首候选 (w)");
    app.handle_key(kc(KeyCode::Down)).unwrap();
    assert_eq!(app.completion_index, 1, "Down 再切到 weekday");
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert!(!app.completion_active(), "Esc 取消候选");
    assert_eq!(app.input, "*w", "取消保留用户原始输入 *w");
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
fn cursor_left_editing_after_completion_does_not_interfere() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    app.handle_key(key('a')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);

    // 1. 用户输入 buy @ho 并按 Tab 补全 -> 变为 "buy @home "
    for c in "buy @ho".chars() {
        app.handle_key(key(c)).unwrap();
    }
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.input, "buy @home ");
    assert!(!app.completion_active(), "补全后已关闭候选");

    // 2. 向左移动光标想要修改 @home 为 @work
    // 连续按 5 次 Left：移到 '@' 之后 ('h' 处，索引 5)
    for _ in 0..5 {
        app.handle_key(kc(KeyCode::Left)).unwrap();
        assert!(
            !app.completion_active(),
            "光标左移导航过程中不应强行弹出补全干扰"
        );
    }
    // 光标现在在 '@' 后面 ('h' 处，索引 5)
    assert_eq!(app.input_cursor, 5);

    // 3. 删除 "home" (4次 Delete) 并输入 "work"
    for _ in 0..4 {
        app.handle_key(kc(KeyCode::Delete)).unwrap();
    }
    assert_eq!(app.input, "buy @ ");
    for c in "work".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert_eq!(app.input, "buy @work ");

    // 4. 用户可以在任何位置自由移动光标
    app.handle_key(kc(KeyCode::Home)).unwrap();
    assert_eq!(app.input_cursor, 0);
    assert!(!app.completion_active());
    app.handle_key(kc(KeyCode::End)).unwrap();
    assert_eq!(app.input_cursor, app.input.len());
    assert!(!app.completion_active());
}

#[test]
fn completion_extends_to_time_and_rrule() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

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
    let filter = horae_core::repo::tasks::ListFilter {
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    let reloaded = horae_core::repo::modules::ModuleVisibility::load(app.conn);
    assert!(!reloaded.splash);

    // j 向下循环导航（0 → 1），k 反向且在 0 处回绕到 11（最后一项：纯净录入）
    app.handle_key(key('j')).unwrap();
    assert_eq!(app.popup, Some(crate::tui::app::Popup::ModuleToggles(1)));
    app.popup = Some(crate::tui::app::Popup::ModuleToggles(0));
    app.handle_key(key('k')).unwrap();
    assert_eq!(app.popup, Some(crate::tui::app::Popup::ModuleToggles(11)));

    // Esc 关闭弹层
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert!(app.popup.is_none());
}

#[test]
fn disabled_module_blocks_digit_view_and_quotes_toggle_off_returns_to_inbox() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

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
fn icons_toggle_persists_and_switches_style() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    use crate::tui::icons::IconStyle;

    // 打开弹层并跳到第 9 项（idx 8：图标开关）
    app.handle_key(kc(KeyCode::F(7))).unwrap();
    for _ in 0..8 {
        app.handle_key(key('j')).unwrap();
    }
    assert_eq!(
        app.popup,
        Some(crate::tui::app::Popup::ModuleToggles(8)),
        "j*8 应停在图标项"
    );

    let before = app.icon_style;
    app.handle_key(key(' ')).unwrap();

    let expected = match before {
        IconStyle::Nerd => IconStyle::Ascii,
        IconStyle::Ascii => IconStyle::Nerd,
    };
    assert_eq!(app.icon_style, expected, "space 应翻转图标风格");
    assert_eq!(
        horae_core::repo::settings::get(app.conn, "icons")
            .unwrap()
            .as_deref(),
        Some(expected.key()),
        "图标风格应持久化到 settings 表"
    );
    assert_eq!(
        IconStyle::load(app.conn),
        expected,
        "重新加载应保持翻转后的风格"
    );
}

#[test]
fn start_capture_default_on_enters_capturing() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    // 无 settings 记录 = 默认开启：启动直接进入快速录入，输入为空。
    let mut app = App::new(&conn).unwrap();
    assert!(app.start_in_capture, "缺省应视为开启");
    assert_eq!(app.mode, Mode::Capturing, "启动即快速录入");
    assert!(app.input.is_empty(), "空输入等待录入");

    // Esc 退出后回到 Normal 列表浏览。
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Normal);

    // 显式写 "0" 后启动保持 Normal（既有行为）。
    horae_core::repo::settings::set(&conn, "start_capture", "0").unwrap();
    let app = App::new(&conn).unwrap();
    assert!(!app.start_in_capture);
    assert_eq!(app.mode, Mode::Normal, "显式关闭后 Normal 起步");
}

#[test]
fn start_capture_toggle_via_module_popup_persists() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    // F7 打开弹层，j*9 到第 10 项（idx 9：启动即快速录入），space 打开。
    app.handle_key(kc(KeyCode::F(7))).unwrap();
    for _ in 0..9 {
        app.handle_key(key('j')).unwrap();
    }
    assert_eq!(
        app.popup,
        Some(crate::tui::app::Popup::ModuleToggles(9)),
        "j*9 应停在“启动即快速录入”项"
    );
    app.handle_key(key(' ')).unwrap();
    assert!(app.start_in_capture, "space 应翻转开关");
    assert_eq!(
        horae_core::repo::settings::get(app.conn, "start_capture")
            .unwrap()
            .as_deref(),
        Some("1"),
        "应持久化到 settings 表"
    );

    // 重新构造 App 应以 Capturing 起步；Esc 退出后再经弹层关掉则回到 Normal。
    drop(app.popup.take());
    let mut app2 = App::new(&conn).unwrap();
    assert_eq!(app2.mode, Mode::Capturing, "重启后保持开启状态");
    assert!(app2.start_in_capture);

    app2.handle_key(kc(KeyCode::Esc)).unwrap();
    assert_eq!(app2.mode, Mode::Normal);
    app2.handle_key(kc(KeyCode::F(7))).unwrap();
    for _ in 0..9 {
        app2.handle_key(key('j')).unwrap();
    }
    app2.handle_key(key(' ')).unwrap();
    assert!(!app2.start_in_capture, "再次 space 关闭");
    assert_eq!(
        horae_core::repo::settings::get(app2.conn, "start_capture")
            .unwrap()
            .as_deref(),
        Some("0"),
        "关闭也应持久化"
    );
}

#[test]
fn help_drawer_navigation_scrolls_and_closes() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

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
fn pomo_banner_shows_only_shortly_after_break_end() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();

    let mut draw_contains = |app: &mut App, needle: &str| -> bool {
        term.clear().unwrap();
        term.draw(|f| app.render(f)).unwrap();
        norm(&snap(&term)).contains(needle)
    };

    // 今日已积 1 个番茄，但不是刚结束休息（典型：当天重启 TUI）→ 不显示横幅
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    app.pomo.last_date = Some(today);
    app.pomo.today_count = 1;
    app.pomo.break_ended_at = None;
    assert!(
        !draw_contains(&mut app, "成就结清"),
        "启动时不应常驻“成就结清”横幅"
    );

    // 刚结束休息（窗口内）→ 显示“再接再厉”横幅
    app.hide_pomo_banner = false;
    app.pomo.break_ended_at = Some(horae_core::time::now_ms());
    assert!(draw_contains(&mut app, "成就结清"));

    // 休息结束已久（窗口外）→ 横幅消失
    app.pomo.break_ended_at = Some(
        horae_core::time::now_ms() - horae_core::model::pomodoro::BREAK_PROMPT_WINDOW_MS - 1_000,
    );
    assert!(
        !draw_contains(&mut app, "成就结清"),
        "过窗后不应再显示旧横幅"
    );
}

#[test]
fn active_pomodoro_exits_capture_mode_before_handling_stop_key() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    let pomo = horae_core::model::pomodoro::PomoState {
        phase: horae_core::model::pomodoro::Phase::Work,
        ..Default::default()
    };
    horae_core::repo::pomodoro::save_state(&pomo).unwrap();
    app.pomo = pomo;
    app.mode = Mode::Capturing;
    app.input = "draft".into();

    app.handle_key(key('S')).unwrap();

    assert_eq!(app.mode, Mode::Normal, "活动番茄钟应退出快速录入模式");
    assert!(app.input.is_empty(), "退出快速录入时应丢弃未提交草稿");
    assert_eq!(
        horae_core::repo::pomodoro::get_state().unwrap().phase,
        horae_core::model::pomodoro::Phase::Idle,
        "S 应停止番茄钟"
    );
}

#[test]
fn exclamation_pops_priority_completion_immediately() {
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
    app.mode = Mode::Capturing;
    // 仅输入 '!' 一个字符（光标在末尾）即应弹出优先级候选，无需再敲字符。
    app.input_insert_char('!');
    assert_eq!(app.input, "!");
    assert!(app.completion_active(), "输入 '!' 后补全候选应立即可用");
    assert_eq!(
        app.completion_candidates,
        vec!["high".to_string(), "medium".to_string(), "low".to_string()],
        "优先级候选应为 high/medium/low"
    );
    assert_eq!(app.completion_prefix, '!');

    // 空 '*' 同样应立刻弹出循环候选。
    app.input_clear();
    app.input_insert_char('*');
    assert!(app.completion_active(), "输入 '*' 后补全候选应立即可用");
    assert!(!app.completion_candidates.is_empty(), "循环候选不应为空");

    // 继续敲字符可过滤候选。
    app.input_insert_char('w');
    assert!(app.completion_candidates.iter().all(|c| c.starts_with('w')));
}

#[test]
fn archived_view_restore_single_and_batch() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

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
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
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

#[test]
fn checklist_manage_toggle_delete_reorder_rename() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let t = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "Pack bags".into(),
            status: task::Status::Inbox,
            ..Default::default()
        },
    )
    .unwrap();
    tasks::add_checklist_item(&conn, &t.id, "charger").unwrap();
    tasks::add_checklist_item(&conn, &t.id, "clothes").unwrap();
    tasks::add_checklist_item(&conn, &t.id, "toothbrush").unwrap();

    let mut app = app_normal(&conn);
    assert_eq!(app.selected, 0, "唯一任务应被选中");

    // Tab 进入检查单逐项管理
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.mode, Mode::ChecklistFocus);
    assert_eq!(app.checklist_cursor, Some(0));

    // 管理模式下应渲染操作提示，且不应弹出输入浮层（无 "Quick capture" 等输入框标题）
    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let frame = norm(&snap(&term));
    assert!(
        frame.contains("检查单管理") || frame.contains("managing:"),
        "检查单管理模式应显示操作提示，实际帧:\n{frame}"
    );
    assert!(
        !frame.contains("Quickcapture") && !frame.contains("Renameitem"),
        "检查单管理模式不应弹出输入浮层"
    );

    // 勾选第一项
    app.handle_key(key(' ')).unwrap();
    assert!(tasks::get(&conn, &t.id).unwrap().checklist[0].done);

    // 下移并排序（J 下移）
    app.handle_key(key('j')).unwrap();
    assert_eq!(app.checklist_cursor, Some(1));
    app.handle_key(key('J')).unwrap();
    let after_move = tasks::get(&conn, &t.id).unwrap().checklist;
    assert_eq!(after_move[2].title, "clothes", "clothes 应被下移到底");

    // 删除当前项（clothes）
    let before = tasks::get(&conn, &t.id).unwrap().checklist.len();
    app.handle_key(key('d')).unwrap();
    assert_eq!(
        tasks::get(&conn, &t.id).unwrap().checklist.len(),
        before - 1
    );

    // 改名：进入 rename（预填当前标题），清空后输入新标题，回车
    app.handle_key(key('e')).unwrap();
    assert_eq!(app.mode, Mode::RenamingChecklist);
    for _ in 0..16 {
        app.handle_key(kc(KeyCode::Backspace)).unwrap();
    }
    for c in "adapter".chars() {
        app.handle_key(key(c)).unwrap();
    }
    app.handle_key(kc(KeyCode::Enter)).unwrap();
    assert_eq!(app.mode, Mode::ChecklistFocus);
    let titles: Vec<String> = tasks::get(&conn, &t.id)
        .unwrap()
        .checklist
        .iter()
        .map(|i| i.title.clone())
        .collect();
    assert!(
        titles.iter().any(|t| t == "adapter"),
        "应出现改名后的项: {titles:?}"
    );

    // Esc 退出管理
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn checklist_all_done_shows_complete_hint() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let t = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "One step".into(),
            status: task::Status::Inbox,
            ..Default::default()
        },
    )
    .unwrap();
    let a = tasks::add_checklist_item(&conn, &t.id, "only")
        .unwrap()
        .unwrap();
    tasks::toggle_checklist_item(&conn, &t.id, &a).unwrap();

    let mut app = app_normal(&conn);
    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let frame = norm(&snap(&term));
    assert!(
        frame.contains("所有步骤已完成") || frame.contains("allstepsdone"),
        "全勾选应在详情区显示完成提示"
    );
}

#[test]
fn fullwidth_symbols_completion_works() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    let mut app = app_normal(&conn);
    app.handle_key(key('a')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);

    // 测试 ＠work 补全
    for c in "买牛奶 ＠wo".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.input, "买牛奶 @work ");

    // 测试 ～to 补全
    for c in "～to".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.input, "买牛奶 @work ~today ");
}

#[test]
fn workflow_view_content_scrolling_and_bilingual() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();

    // 1. 切换到 Workflow 视图
    app.handle_key(key('W')).unwrap();
    assert_eq!(app.view, View::Workflow);

    // 2. 中文模式渲染验证
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let frame_zh = norm(&snap(&term));
    assert!(
        frame_zh.contains("GTD工作流") && frame_zh.contains("决策树与五步闭环"),
        "中心面板应包含 GTD 决策树标题: {}",
        frame_zh
    );
    assert!(
        frame_zh.contains("是否可行动") && frame_zh.contains("两分钟原则"),
        "中心面板应包含决策树判定分支: {}",
        frame_zh
    );
    assert!(
        frame_zh.contains("DavidAllen") && frame_zh.contains("心如止水"),
        "右侧面板应包含 David Allen 与心如止水: {}",
        frame_zh
    );

    // 3. 滚动测试：中心面板 (Pane::Center)
    app.pane = Pane::Center;
    app.handle_key(key('j')).unwrap();
    assert_eq!(app.workflow_scroll, 1);
    app.handle_key(kc(KeyCode::PageDown)).unwrap();
    assert_eq!(app.workflow_scroll, 11);
    app.handle_key(kc(KeyCode::PageUp)).unwrap();
    assert_eq!(app.workflow_scroll, 1);
    app.handle_key(key('G')).unwrap();
    assert!(app.workflow_scroll >= 10000);
    app.handle_key(key('g')).unwrap();
    assert_eq!(app.workflow_scroll, 0);

    // 4. 滚动测试：右侧面板 (Pane::Right)
    app.pane = Pane::Right;
    app.handle_key(key('j')).unwrap();
    assert_eq!(app.workflow_side_scroll, 1);
    app.handle_key(kc(KeyCode::PageDown)).unwrap();
    assert_eq!(app.workflow_side_scroll, 11);
    app.handle_key(kc(KeyCode::PageUp)).unwrap();
    assert_eq!(app.workflow_side_scroll, 1);
    app.handle_key(key('G')).unwrap();
    assert!(app.workflow_side_scroll >= 10000);
    app.handle_key(key('g')).unwrap();
    assert_eq!(app.workflow_side_scroll, 0);

    // 5. 英文模式渲染验证
    app.lang = horae_core::i18n::Lang::En;
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let frame_en = norm(&snap(&term));
    assert!(
        frame_en.contains("GTDWorkflow") && frame_en.contains("Philosophy"),
        "英文模式应包含英文标题: {}",
        frame_en
    );
    assert!(
        frame_en.contains("DavidAllen") && frame_en.contains("MindLikeWater"),
        "英文模式应包含 David Allen 与 Mind Like Water: {}",
        frame_en
    );
    assert!(
        frame_en.contains("Isitactionable?")
            && frame_en.contains("No→")
            && frame_en.contains("Yes→"),
        "英文模式决策树应为纯英文: {}",
        frame_en
    );
    assert!(
        frame_en.contains("Delegate→")
            && frame_en.contains("Defer/Do:")
            && frame_en.contains("ASAP→"),
        "英文模式决策树分支应为纯英文: {}",
        frame_en
    );

    // 6. 切换视图重置滚动偏移
    app.workflow_scroll = 5;
    app.workflow_side_scroll = 5;
    app.set_view(View::Inbox);
    assert_eq!(app.workflow_scroll, 0);
    assert_eq!(app.workflow_side_scroll, 0);
}

#[test]
fn completion_style_toggle_and_rendering() {
    use crate::tui::app::completion::CompletionStyle;
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    // 默认是 Reference (语法参考模式)
    assert_eq!(app.completion_style, CompletionStyle::Reference);

    // 渲染测试：Reference 模式下输入 * 弹出语法速查卡
    app.handle_key(key('a')).unwrap();
    app.handle_key(key('*')).unwrap();
    assert!(app.completion_active());

    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s = norm(&snap(&term));
    assert!(
        s.contains("语法速查"),
        "Reference 模式标题包含语法速查: {}",
        s
    );
    assert!(s.contains("范式:"), "Reference 模式包含范式提示: {}", s);

    // 通过 F7 弹层切换到 Speed 模式
    app.handle_key(kc(KeyCode::Esc)).unwrap(); // 取消补全
    app.handle_key(kc(KeyCode::Esc)).unwrap(); // 退出 capture
    app.handle_key(kc(KeyCode::F(7))).unwrap(); // 打开 F7
    for _ in 0..10 {
        app.handle_key(key('j')).unwrap(); // 导航到第 10 项 (补全风格)
    }
    app.handle_key(key(' ')).unwrap(); // 切换
    app.handle_key(kc(KeyCode::Esc)).unwrap(); // 关闭 F7
    assert_eq!(app.completion_style, CompletionStyle::Speed);
    assert_eq!(
        horae_core::repo::settings::get(&conn, "completion_style")
            .unwrap()
            .as_deref(),
        Some("speed")
    );

    // 渲染测试：Speed 模式下输入 * 弹出极速补全
    app.handle_key(key('a')).unwrap();
    app.handle_key(key('*')).unwrap();
    assert!(app.completion_active());

    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s2 = norm(&snap(&term));
    assert!(
        s2.contains("极速补全"),
        "Speed 模式标题包含极速补全: {}",
        s2
    );
}

#[test]
fn bilingual_completion_candidates_and_parsing() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    // 1. 中文模式下：~ 首屏覆盖自然天词（今天/明天/后天）、常用英文词（today/tomorrow）与中文星期
    app.lang = horae_core::i18n::Lang::Zh;
    app.mode = Mode::Capturing;
    app.input_insert_char('~');
    assert!(app.completion_active());
    assert!(
        app.completion_candidates.contains(&"today".to_string()),
        "中文模式保留 'today'"
    );
    assert!(
        app.completion_candidates.contains(&"tomorrow".to_string()),
        "中文模式保留 'tomorrow'"
    );
    assert!(
        app.completion_candidates.contains(&"周五".to_string()),
        "中文模式包含 '周五'"
    );
    assert!(
        app.completion_candidates.contains(&"今天".to_string()),
        "中文模式包含 '今天' 候选"
    );
    assert!(
        app.completion_candidates.contains(&"明天".to_string()),
        "中文模式包含 '明天' 候选"
    );
    assert!(
        app.completion_candidates.contains(&"后天".to_string()),
        "中文模式包含 '后天' 候选"
    );

    // 2. 解析端仍完整支持 今天/明天/后天
    assert!(horae_core::time::parse_time("今天 15:00").is_ok());
    assert!(horae_core::time::parse_time("明天 10:00").is_ok());
    assert!(horae_core::time::parse_time("后天 09:00").is_ok());

    // 3. 英文模式下：~ 绝不包含汉字，只包含英文词汇 (mon, tue, fri, today 等)
    app.input_clear();
    app.lang = horae_core::i18n::Lang::En;
    app.input_insert_char('~');
    assert!(app.completion_active());
    for cand in &app.completion_candidates {
        assert!(
            cand.is_ascii(),
            "英文模式下的时间候选必须全部为纯 ASCII 英文词汇: {}",
            cand
        );
    }
    assert!(
        app.completion_candidates.contains(&"mon".to_string()),
        "英文模式应包含 'mon'"
    );
    assert!(
        app.completion_candidates.contains(&"fri".to_string()),
        "英文模式应包含 'fri'"
    );
    assert!(
        app.completion_candidates.contains(&"tomorrow".to_string()),
        "英文模式应包含 'tomorrow'"
    );

    // 4. 英文模式下输入 ~fri 10:00 可被正常解析
    let ms = horae_core::time::parse_time("fri 10:00").unwrap();
    assert!(ms > 0);
}

#[test]
fn universal_zero_config_completion_and_editing_shortcuts() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    // 1. Vim 风格快捷键：Ctrl+N/P 切换, Ctrl+Y 采纳, Ctrl+E 取消
    app.handle_key(key('a')).unwrap();
    app.handle_key(key('@')).unwrap();
    app.handle_key(key('h')).unwrap();
    assert!(app.completion_active());
    assert_eq!(app.completion_candidates[0], "home");

    // Ctrl+N / Ctrl+P
    app.handle_key(ctrl('n')).unwrap();
    assert_eq!(app.completion_index, 0); // home 是唯一以 h 开头的默认标签，index 循环为 0
                                         // Ctrl+Y 采纳
    app.handle_key(ctrl('y')).unwrap();
    assert!(!app.completion_active());
    assert_eq!(app.input, "@home ");

    // 输入 @ 激活，按 Ctrl+E 取消
    app.handle_key(key('@')).unwrap();
    assert!(app.completion_active());
    app.handle_key(ctrl('e')).unwrap();
    assert!(!app.completion_active(), "Ctrl+E 取消补全候选");
    assert_eq!(app.input, "@home @");

    // 2. Ctrl+J / Ctrl+K 切换补全候选项
    app.input_clear();
    app.input_insert_char('*');
    app.input_insert_char('w');
    assert!(app.completion_active());
    assert_eq!(app.completion_index, 0); // "w"
    app.handle_key(ctrl('j')).unwrap(); // Ctrl+J 向下
    assert_eq!(app.completion_index, 1); // "weekday"
    app.handle_key(ctrl('k')).unwrap(); // Ctrl+K 向上
    assert_eq!(app.completion_index, 0); // "w"
    app.handle_key(kc(KeyCode::Tab)).unwrap(); // Tab 采纳
    assert!(!app.completion_active());
    assert_eq!(app.input, "*w ");

    // 3. 词与行编辑快捷键：Ctrl+W / Ctrl+U / Ctrl+K / Ctrl+A / Ctrl+E
    // Ctrl+W 删词
    app.handle_key(ctrl('w')).unwrap();
    assert_eq!(app.input, "");

    // 插入文本并用 Ctrl+U / Ctrl+K / Ctrl+A / Ctrl+E 编辑
    app.input_insert_str("hello world test");
    app.handle_key(ctrl('a')).unwrap(); // Home
    assert_eq!(app.input_cursor, 0);
    app.handle_key(ctrl('e')).unwrap(); // End (非补全状态下)
    assert_eq!(app.input_cursor, 16);
    app.handle_key(ctrl('w')).unwrap(); // 删词
    assert_eq!(app.input, "hello world ");
    app.handle_key(ctrl('u')).unwrap(); // 删到行首
    assert_eq!(app.input, "");
    app.input_insert_str("keep this delete after");
    app.input_cursor = 9; // 光标在 "keep this" 之后
    app.handle_key(ctrl('k')).unwrap(); // 非补全时 Ctrl+K 删到行尾
    assert_eq!(app.input, "keep this");

    // 4. VSCode 模式快捷键：Alt+[ / Alt+] 切换
    app.input_clear();
    app.input_insert_char('~');
    app.input_insert_char('t');
    assert!(app.completion_active());
    assert_eq!(app.completion_index, 0);
    app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.completion_index, 1);
    app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.completion_index, 0);
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert!(!app.completion_active());
    assert_eq!(app.input, "~today ");

    // 5. Emacs 快捷键：Alt+N / Alt+P 切换, Ctrl+G 取消
    app.input_clear();
    app.input_insert_char('*');
    app.input_insert_char('w');
    assert!(app.completion_active());
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.completion_index, 1);
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT))
        .unwrap();
    assert_eq!(app.completion_index, 0);

    // Ctrl+G 取消补全
    app.handle_key(ctrl('g')).unwrap(); // Ctrl+G 取消候选
    assert!(!app.completion_active());
    assert_eq!(app.input, "*w");
    app.handle_key(ctrl('g')).unwrap(); // 再次 Ctrl+G 退出输入模式
    assert_eq!(app.mode, Mode::Normal);

    // 6. BackTab (Shift+Tab) 倒序切换
    app.handle_key(key('a')).unwrap();
    app.input_insert_char('~');
    assert!(app.completion_active());
    app.handle_key(kc(KeyCode::Down)).unwrap();
    assert_eq!(app.completion_index, 1);
    app.handle_key(kc(KeyCode::BackTab)).unwrap();
    assert_eq!(app.completion_index, 0);
}

#[test]
fn smart_autocomplete_matching_and_quick_pick() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    // 1. 中文星期拼音首字母匹配：~zy -> 周一, ~ze -> 周二
    app.handle_key(key('a')).unwrap();
    for c in "~zy".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    assert_eq!(app.completion_candidates[0], "周一");
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.input, "~周一 ");

    // 2. 英文星期与别名匹配：~mon -> 周一 (在中文模式)
    app.input_clear();
    for c in "~mon".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    assert_eq!(app.completion_candidates[0], "周一");

    // 3. 大小写不敏感匹配：!H -> high, !M -> medium
    app.input_clear();
    for c in "!H".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    assert_eq!(app.completion_candidates[0], "high");
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert_eq!(app.input, "!high ");

    // 4. 候选导航与采纳（已取消 Alt+1..9 快捷直选）
    app.input_clear();
    app.handle_key(key('!')).unwrap();
    assert!(app.completion_active());
    assert!(app.completion_candidates.len() >= 3);
    // 按 Down 切换至第 2 项 ("medium") 并 Tab 采纳
    app.handle_key(kc(KeyCode::Down)).unwrap();
    assert_eq!(app.completion_index, 1);
    app.handle_key(kc(KeyCode::Tab)).unwrap();
    assert!(!app.completion_active());
    assert_eq!(app.input, "!medium ");

    // 5. 动态循环简写推导：*2 -> 2d, 2w, 2m, 2y
    app.input_clear();
    for c in "*2".chars() {
        app.handle_key(key(c)).unwrap();
    }
    assert!(app.completion_active());
    assert!(app.completion_candidates.contains(&"2d".to_string()));
    assert!(app.completion_candidates.contains(&"2w".to_string()));
    assert!(app.completion_candidates.contains(&"2m".to_string()));

    // 6. 标签按频次排序
    let t1 = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task 1".to_string(),
            tag_names: vec!["custom-rare".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let _t2 = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task 2".to_string(),
            tag_names: vec!["custom-hot".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let _t3 = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task 3".to_string(),
            tag_names: vec!["custom-hot".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let _ = t1;

    app.input_clear();
    app.handle_key(key('@')).unwrap();
    assert!(app.completion_active());
    let hot_pos = app
        .completion_candidates
        .iter()
        .position(|c| c == "custom-hot")
        .unwrap();
    let rare_pos = app
        .completion_candidates
        .iter()
        .position(|c| c == "custom-rare")
        .unwrap();
    assert!(hot_pos < rare_pos, "高频使用的标签应排在低频标签前面");
}

#[test]
fn undo_and_redo_status_transitions() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    let t = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task for Undo".to_string(),
            status: task::Status::Next,
            ..Default::default()
        },
    )
    .unwrap();

    app.set_view(View::Next);
    app.selected = 0;

    // 1. 标记完成 (x)
    app.handle_key(key('x')).unwrap();
    let task_after_x = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert_eq!(task_after_x.status, task::Status::Done);
    assert!(app.status_message.contains("✓ 已完成"));

    // 2. 撤销 (u)
    app.handle_key(key('u')).unwrap();
    let task_after_u = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert_eq!(task_after_u.status, task::Status::Next);
    assert!(app.status_message.contains("已撤销"));

    // 3. 重做 (Ctrl+r)
    let mut ctrl_r = key('r');
    ctrl_r.modifiers = crossterm::event::KeyModifiers::CONTROL;
    app.handle_key(ctrl_r).unwrap();
    let task_after_redo = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert_eq!(task_after_redo.status, task::Status::Done);
    assert!(app.status_message.contains("已重做"));
}

#[test]
fn pomodoro_in_focus_checklist_space_toggle_and_undo() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    let t = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Project Alpha".to_string(),
            status: task::Status::Next,
            ..Default::default()
        },
    )
    .unwrap();

    let c1 = horae_core::repo::tasks::add_checklist_item(&conn, &t.id, "Step 1")
        .unwrap()
        .unwrap();
    let _c2 = horae_core::repo::tasks::add_checklist_item(&conn, &t.id, "Step 2")
        .unwrap()
        .unwrap();

    // 开启番茄钟
    let pomo = horae_core::model::pomodoro::PomoState {
        phase: horae_core::model::pomodoro::Phase::Work,
        task_id: Some(t.id.clone()),
        task_title: Some(t.title.clone()),
        ..Default::default()
    };
    horae_core::repo::pomodoro::save_state(&pomo).unwrap();
    app.pomo = pomo;

    // 在专注态按 Space 勾选子项
    app.handle_key(key(' ')).unwrap();
    let task = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert!(task.checklist.iter().find(|i| i.id == c1).unwrap().done);
    assert!(app.status_message.contains("打卡子项"));

    // 渲染帧验证进度条
    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s = snap(&term);
    assert!(s.contains("Step 2") || s.contains("Project Alpha"));

    // 撤销子项打卡
    app.handle_key(key('u')).unwrap();
    let task_reverted = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert!(
        !task_reverted
            .checklist
            .iter()
            .find(|i| i.id == c1)
            .unwrap()
            .done
    );
}

#[test]
fn micro_progress_bar_renders_in_list_items() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    let t = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task with checklist".to_string(),
            status: task::Status::Next,
            ..Default::default()
        },
    )
    .unwrap();

    horae_core::repo::tasks::add_checklist_item(&conn, &t.id, "Sub 1").unwrap();
    let c2 = horae_core::repo::tasks::add_checklist_item(&conn, &t.id, "Sub 2")
        .unwrap()
        .unwrap();
    horae_core::repo::tasks::toggle_checklist_item(&conn, &t.id, &c2).unwrap();

    app.set_view(View::Next);
    app.refresh().unwrap();

    let list_items = crate::tui::ui::build_list_items(&app);
    assert_eq!(list_items.len(), 1);
    // 检查清单进度包含 1/2
    let item_str = format!("{:?}", list_items[0]);
    assert!(item_str.contains("1/2"));
}

#[test]
fn pomodoro_x_completes_focused_task_and_stops_pomo() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    let t = horae_core::repo::tasks::create_capture(
        &conn,
        &horae_core::repo::tasks::CaptureInput {
            title: "Task in focus".to_string(),
            status: task::Status::Next,
            ..Default::default()
        },
    )
    .unwrap();

    // 启动专注番茄钟
    let pomo = horae_core::model::pomodoro::PomoState {
        phase: horae_core::model::pomodoro::Phase::Work,
        task_id: Some(t.id.clone()),
        task_title: Some(t.title.clone()),
        ..Default::default()
    };
    horae_core::repo::pomodoro::save_state(&pomo).unwrap();
    app.pomo = pomo;
    assert_eq!(app.pomo.phase, horae_core::model::pomodoro::Phase::Work);
    assert_eq!(app.pomo.task_id.as_deref(), Some(t.id.as_str()));

    // 在专注模式下按 x 完成当前任务
    app.handle_key(key('x')).unwrap();

    // 验证任务已完成
    let task_done = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert_eq!(task_done.status, task::Status::Done);

    // 验证番茄钟已停止重置为 Idle
    assert_eq!(app.pomo.phase, horae_core::model::pomodoro::Phase::Idle);
    assert!(app.status_message.contains("🎉 专注达成"));

    // 验证支持按 u 撤销
    app.handle_key(key('u')).unwrap();
    let task_reverted = horae_core::repo::tasks::get(&conn, &t.id).unwrap();
    assert_eq!(task_reverted.status, task::Status::Next);
}

#[test]
fn zen_capture_hides_background_panels() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);
    app.popup = None;

    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();

    // 默认 zen_capture 为 true，按 a 进入快速录入
    app.handle_key(key('a')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);
    assert!(app.is_zen_capturing());

    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let zen_snap = snap(&term);
    assert_eq!(
        zen_snap.matches('╭').count(),
        1,
        "纯净录入模式下应隐藏三栏背景，仅渲染输入卡片自身 1 个圆角框"
    );
    assert!(
        norm(&zen_snap).contains("快速录入") || zen_snap.contains("QuickCapture"),
        "录入标题应显示"
    );

    // 关闭 zen_capture 时，应渲染背景三栏 + 输入卡片共 4 个框
    app.zen_capture = false;
    assert!(!app.is_zen_capturing());
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let classic_snap = snap(&term);
    assert_eq!(
        classic_snap.matches('╭').count(),
        4,
        "关闭纯净录入后应展现底层三栏 + 输入卡片共 4 个框"
    );
}

#[test]
fn zen_capture_preserves_slot_hints_with_custom_order() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let mut app = app_normal(&conn);

    app.handle_key(key('a')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);
    assert!(app.is_zen_capturing());

    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();

    // 输入标题并追加空格，检查槽位提示顺序：周期、时间、优先级、标签
    for c in "买牛奶 ".chars() {
        app.handle_key(key(c)).unwrap();
    }
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s = norm(&snap(&term));
    assert!(
        s.contains("[*周期][~时间][!优先级][@标签]"),
        "槽位提示顺序必须为：周期、时间、优先级、标签，实际渲染:\n{s}"
    );
    // 验证纯净模式下不包含冗长 200 字符语法常驻说明行
    assert!(
        !s.contains("日期搜索:MMDD"),
        "纯净模式下不应显示底部常驻静态语法行"
    );

    // 追加周期后，槽位提示应只剩时间、优先级、标签
    for c in "*d ".chars() {
        app.handle_key(key(c)).unwrap();
    }
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s2 = norm(&snap(&term));
    assert!(
        s2.contains("[~时间][!优先级][@标签]"),
        "已输入周期后，槽位提示应顺次展示时间、优先级、标签，实际:\n{s2}"
    );
    assert!(s2.contains("循环:*d"), "实时解析行应清晰展现周期规则");
}

#[test]
fn zen_capture_toggle_via_f7_persists() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    assert!(app.zen_capture, "缺省开启纯净录入");

    // 打开 F7 弹层并跳到第 12 项（下标 11：纯净录入开关）
    app.handle_key(kc(KeyCode::F(7))).unwrap();
    for _ in 0..11 {
        app.handle_key(key('j')).unwrap();
    }
    assert_eq!(
        app.popup,
        Some(crate::tui::app::Popup::ModuleToggles(11)),
        "应导航到纯净录入选项"
    );

    // 空格切换为关闭
    app.handle_key(key(' ')).unwrap();
    assert!(!app.zen_capture);
    assert_eq!(
        horae_core::repo::settings::get(&conn, "zen_capture")
            .unwrap()
            .as_deref(),
        Some("0")
    );

    // 重新构造 App 验证持久化生效
    let app2 = App::new(&conn).unwrap();
    assert!(!app2.zen_capture, "重新加载后依然保持关闭");

    // 再次空格切换为开启
    app.handle_key(key(' ')).unwrap();
    assert!(app.zen_capture);
    assert_eq!(
        horae_core::repo::settings::get(&conn, "zen_capture")
            .unwrap()
            .as_deref(),
        Some("1")
    );
}

#[test]
fn organize_edit_does_not_expand_syntax_in_edit_area() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();

    let start_time = horae_core::time::parse_time("2026-09-10 15:30").unwrap();
    let rec = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "定期复盘".into(),
            status: task::Status::Scheduled,
            tag_names: vec!["work".to_string()],
            ..Default::default()
        },
    )
    .unwrap();

    // 设置复杂循环与高优先级
    tasks::schedule(
        &conn,
        &rec.id,
        start_time,
        None,
        Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE".into()),
    )
    .unwrap();
    tasks::set_priority(&conn, &rec.id, Some("high".into())).unwrap();

    let mut app = app_normal(&conn);
    app.handle_key(key('4')).unwrap(); // 切换到 Scheduled 视图

    // 按 e 进入编辑模式
    app.handle_key(key('e')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);
    assert_eq!(app.organizing_id.as_deref(), Some(rec.id.as_str()));

    // 严禁展开语法：编辑区必须保持精简友好的 *2w[1,3]，不能包含机器语法 FREQ=
    assert!(
        app.input.contains("*2w[1,3]"),
        "循环必须还原为紧凑简写 *2w[1,3]，实际为: {}",
        app.input
    );
    assert!(
        !app.input.contains("FREQ="),
        "编辑区绝对不得出现机器展开语法 FREQ=，实际为: {}",
        app.input
    );

    // 时间无机器字符 T
    assert!(
        app.input.contains("~2026-09-10 15:30"),
        "时间应为自然格式 ~2026-09-10 15:30，实际为: {}",
        app.input
    );
    assert!(
        !app.input.contains('T'),
        "时间不得包含机器分隔符 T，实际为: {}",
        app.input
    );

    // 优先级 !high 完整保留
    assert!(
        app.input.contains("!high"),
        "优先级 !high 必须保留在编辑区，实际为: {}",
        app.input
    );

    // 验证测试每日任务（纯日期 00:00）
    let midnight_time = horae_core::time::parse_time("2026-09-10").unwrap();
    let rec2 = tasks::create_capture(
        &conn,
        &CaptureInput {
            title: "每日晨跑".into(),
            status: task::Status::Scheduled,
            tag_names: vec!["health".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    tasks::schedule(
        &conn,
        &rec2.id,
        midnight_time,
        None,
        Some("FREQ=DAILY".into()),
    )
    .unwrap();

    app.handle_key(kc(KeyCode::Esc)).unwrap(); // 退出当前编辑
    app.reload().unwrap();

    // 选中每日晨跑任务
    let idx = app
        .items
        .iter()
        .position(|r| r.title == "每日晨跑")
        .expect("找到每日晨跑");
    app.selected = idx;
    app.handle_key(key('e')).unwrap();

    assert!(
        app.input.contains("*d"),
        "每日循环必须还原为 *d，实际为: {}",
        app.input
    );
    assert!(
        !app.input.contains("FREQ="),
        "不得包含 FREQ=，实际为: {}",
        app.input
    );
    assert!(
        app.input.contains("~2026-09-10"),
        "零点时间应简洁显示日期 ~2026-09-10，实际为: {}",
        app.input
    );
    assert!(
        !app.input.contains("00:00"),
        "纯日期排程不应携带冗余 00:00，实际为: {}",
        app.input
    );

    // 提交编辑，再次确认 round-trip 无损
    app.handle_key(kc(KeyCode::Enter)).unwrap();
    assert_eq!(app.mode, Mode::Normal);
    let t = tasks::get(&conn, &rec2.id).unwrap();
    assert_eq!(t.rrule.as_deref(), Some("FREQ=DAILY"));
}

#[test]
fn task_to_quick_add_comprehensive_matrix() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    let app = app_normal(&conn);

    let cases = vec![
        (
            "纯标题任务",
            vec![],
            None,
            None,
            None,
            vec!["纯标题任务"],
            vec!["@", "~", "*", "!", "FREQ="],
        ),
        (
            "多标签分类任务",
            vec!["work", "rust"],
            None,
            None,
            None,
            vec!["@work", "@rust"],
            vec!["~", "*", "!", "FREQ="],
        ),
        (
            "纯日期排程任务",
            vec!["life"],
            Some("2026-09-10"),
            None,
            None,
            vec!["~2026-09-10"],
            vec!["00:00", "T", "*", "!"],
        ),
        (
            "日期与时刻排程任务",
            vec!["meeting"],
            Some("2026-09-10 14:30"),
            None,
            None,
            vec!["~2026-09-10 14:30"],
            vec!["T", "*", "!"],
        ),
        (
            "每日习惯任务",
            vec!["habit"],
            Some("2026-09-10"),
            Some("FREQ=DAILY"),
            Some("high"),
            vec!["*d", "!high", "~2026-09-10"],
            vec!["FREQ=", "00:00", "T"],
        ),
        (
            "隔天循环任务",
            vec![],
            Some("2026-09-10 08:00"),
            Some("FREQ=DAILY;INTERVAL=2"),
            Some("medium"),
            vec!["*2d", "!medium", "~2026-09-10 08:00"],
            vec!["FREQ=", "T"],
        ),
        (
            "工作日例会",
            vec!["team"],
            None,
            Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
            Some("low"),
            vec!["*weekday", "!low"],
            vec!["FREQ=", "BYDAY="],
        ),
        (
            "周末大扫除",
            vec!["home"],
            None,
            Some("FREQ=WEEKLY;BYDAY=SA,SU"),
            None,
            vec!["*weekend"],
            vec!["FREQ=", "BYDAY="],
        ),
        (
            "每两周一三游泳",
            vec!["health"],
            Some("2026-09-10 19:00"),
            Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE"),
            Some("high"),
            vec!["*2w[1,3]", "!high"],
            vec!["FREQ=", "INTERVAL=", "BYDAY="],
        ),
        (
            "每月指定日期交租",
            vec!["finance"],
            None,
            Some("FREQ=MONTHLY;BYMONTHDAY=1,15"),
            None,
            vec!["*m[1,15]"],
            vec!["FREQ=", "BYMONTHDAY="],
        ),
        (
            "半年回顾",
            vec!["okr"],
            None,
            Some("FREQ=YEARLY;BYMONTH=1,7"),
            None,
            vec!["*y[1,7]"],
            vec!["FREQ=", "BYMONTH="],
        ),
    ];

    for (title, tags, time_str, rrule, priority, must_contains, must_nots) in cases {
        let start_ms = time_str.map(|ts| horae_core::time::parse_time(ts).unwrap());
        let rec = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: title.to_string(),
                status: if start_ms.is_some() {
                    task::Status::Scheduled
                } else {
                    task::Status::Inbox
                },
                tag_names: tags.iter().map(|s| s.to_string()).collect(),
                rrule: rrule.map(|s| s.to_string()),
                priority: priority.map(|s| s.to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        if let Some(ms) = start_ms {
            tasks::schedule(&conn, &rec.id, ms, None, rrule.map(|s| s.to_string())).unwrap();
        }

        let task = tasks::get(&conn, &rec.id).unwrap();
        let serialized = app.task_to_quick_add(&task);

        for needle in must_contains {
            assert!(
                serialized.contains(needle),
                "任务 [{title}] 序列化结果必须包含 '{needle}'，实际: '{serialized}'"
            );
        }
        for bad in must_nots {
            assert!(
                !serialized.contains(bad),
                "任务 [{title}] 序列化结果严禁包含机器符号 '{bad}'，实际: '{serialized}'"
            );
        }

        // 双向闭环校验：将序列化结果送回 parse_quick_add，必须能 100% 正确无损解析还原
        let reparsed = horae_core::parser::parse_quick_add(&serialized);
        assert_eq!(reparsed.title, title, "标题解析还原一致");
        assert_eq!(reparsed.priority.as_deref(), priority, "优先级解析还原一致");
        if let Some(expected_rrule) = rrule {
            assert_eq!(
                reparsed.rrule.as_deref(),
                Some(expected_rrule),
                "循环规则在逆向后再次解析，必须与数据库底层标准 RRULE 完全一致！"
            );
        }
        if let Some(ms) = start_ms {
            let reparsed_ms =
                horae_core::time::parse_time(reparsed.time_str.as_ref().unwrap()).unwrap();
            assert_eq!(
                reparsed_ms, ms,
                "排程时间戳经逆向后再次解析，必须毫秒级一致！"
            );
        }
    }
}

#[test]
fn syntax_guide_normal_mode_navigation_and_esc() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    let initial_selected = app.selected;

    // 1. Ctrl+P 打开语法速查
    assert!(!app.show_syntax);
    app.handle_key(ctrl('p')).unwrap();
    assert!(app.show_syntax);
    assert_eq!(app.syntax_scroll, 0);

    // 2. j / Down / PageDown 滚动语法指南，但不触发背景列表移动
    app.handle_key(key('j')).unwrap();
    assert_eq!(app.syntax_scroll, 2);
    assert_eq!(
        app.selected, initial_selected,
        "语法弹层打开时 j 不移动背景任务"
    );

    app.handle_key(kc(KeyCode::PageDown)).unwrap();
    assert_eq!(app.syntax_scroll, 6);
    assert_eq!(app.selected, initial_selected);

    // 3. k / Up / PageUp 反向滚动
    app.handle_key(key('k')).unwrap();
    assert_eq!(app.syntax_scroll, 4);

    // 4. g 滚到顶部
    app.handle_key(key('g')).unwrap();
    assert_eq!(app.syntax_scroll, 0);

    // 5. Esc 关闭语法速查并复位滚动位置
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert!(!app.show_syntax);
    assert_eq!(app.syntax_scroll, 0);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn dual_open_syntax_and_completion_rendering() {
    horae_core::repo::state::set_test_override();
    let mut conn = Connection::open(":memory:").unwrap();
    migrate::run(&mut conn).unwrap();
    seed(&conn);
    let mut app = app_normal(&conn);

    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();

    app.lang = horae_core::i18n::Lang::Zh;

    // 1. 录入模式下按 Ctrl+P 展开语法速查
    app.handle_key(key('a')).unwrap();
    assert_eq!(app.mode, Mode::Capturing);
    app.handle_key(ctrl('p')).unwrap();
    assert!(app.show_syntax);

    // 此时未键入补全前缀，语法指南完整呈现
    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s_syntax = norm(&snap(&term));
    assert!(
        s_syntax.contains("语法说明指南") || s_syntax.contains("Syntax guide"),
        "未激活补全时展示语法说明指南"
    );

    // 2. 输入 ~ 激活补全：候选下拉浮层（语法参考）浮在最上层
    app.input_insert_char('~');
    assert!(app.completion_active());

    term.clear().unwrap();
    term.draw(|f| app.render(f)).unwrap();
    let s_comp = norm(&snap(&term));
    assert!(
        s_comp.contains("today") || s_comp.contains("今天"),
        "补全激活时浮层展示时间补全候选"
    );
    assert!(
        s_comp.contains("表达多样性启发") || s_comp.contains("语法参考"),
        "补全激活时展示补全参考卡片"
    );

    // 3. 两段式 Esc：第一个 Esc 取消补全浮层，保留录入模式并重新显露语法说明指南
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert!(!app.completion_active(), "首个 Esc 关闭补全候选");
    assert_eq!(app.mode, Mode::Capturing, "仍处于录入模式");
    assert!(app.show_syntax, "语法指南重新完全展现");

    // 4. 第二个 Esc 退出录入模式，重置语法指南
    app.handle_key(kc(KeyCode::Esc)).unwrap();
    assert_eq!(app.mode, Mode::Normal, "退出到 Normal 模式");
    assert!(!app.show_syntax, "语法指南随录入退出而关闭");
    assert_eq!(app.syntax_scroll, 0);
}
