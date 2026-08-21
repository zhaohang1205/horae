use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::model::task::{Status, Task};
use crate::repo::tags;
use crate::repo::tasks;
use crate::schedule::effective_due;
use crate::time;

/// 默认轮询间隔（秒）。
pub const DEFAULT_INTERVAL_SECS: u64 = 5;

/// 提醒提前量：任务在截止前这么长时间内（含已逾期）落盘提醒文件。
const REMIND_LEAD_MS: i64 = 5 * 60 * 1000;

/// `.done` 回执保留行数上限（也即去重集合的上界）。
const DONE_KEEP: usize = 2000;

/// 提醒去重状态文件名（放在同步目录内，随 Syncthing 同步但无碍）。
const WATCH_STATE_FILE: &str = ".horae-watch.json";

const FILE_CAPTURE: &str = "capture";
const FILE_ACTIONS: &str = "actions";
const FILE_TODAY: &str = "today.md";
const DIR_REMINDERS: &str = "reminders";

pub struct WatchArgs {
    pub dir: PathBuf,
    pub interval_secs: u64,
    pub once: bool,
}

/// 单轮对账的结果，便于测试断言与日志。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessSummary {
    pub captures: usize,
    pub actions: usize,
    pub reminders: usize,
    pub today_written: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WatchState {
    #[serde(default)]
    reminded: Vec<String>,
}

/// 默认同步目录：`~/.config/horae/sync`（与 DB 同根，加入 Syncthing 即可）。
pub fn default_sync_dir() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("horae");
    p.push("sync");
    p
}

/// 常驻（或 `--once` 单轮）对账入口。
pub fn run(conn: &Connection, args: WatchArgs) -> Result<()> {
    fs::create_dir_all(&args.dir)?;
    fs::create_dir_all(args.dir.join(DIR_REMINDERS))?;
    if args.once {
        let s = process_once(conn, &args.dir)?;
        println!(
            "processed: {} captures, {} actions, {} reminders",
            s.captures, s.actions, s.reminders
        );
        return Ok(());
    }
    eprintln!(
        "horae watch running on {} (every {}s). Ctrl-C to stop.",
        args.dir.display(),
        args.interval_secs
    );
    loop {
        if let Err(e) = process_once(conn, &args.dir) {
            eprintln!("watch error: {e:#}");
        }
        std::thread::sleep(Duration::from_secs(args.interval_secs));
    }
}

/// 执行一轮对账：
/// 1. 采集 `capture.txt` 的新行（quick-add 语法）进收件箱；
/// 2. 执行 `actions.txt` 的新操作行；
/// 3. 到点/逾期的任务写 `reminders/*.md`；
/// 4. 重写 `today.md` 活动任务快照。
pub fn process_once(conn: &Connection, dir: &Path) -> Result<ProcessSummary> {
    fs::create_dir_all(dir)?;
    fs::create_dir_all(dir.join(DIR_REMINDERS))?;
    Ok(ProcessSummary {
        captures: ingest_queue(conn, dir, FILE_CAPTURE, do_capture)?,
        actions: ingest_queue(conn, dir, FILE_ACTIONS, do_action)?,
        reminders: write_reminders(conn, dir)?,
        today_written: write_today(conn, dir)?,
    })
}

/// 消费一个意图队列文件。协议（以 capture 为例）：
/// - 手机在 `capture.txt` 里写入/重写整份采集行；
/// - 本函数把 `capture.txt` 原子改名成 `capture.processing`（防手机写入竞态），
///   逐行执行，追加回执到 `capture.done`，最后删除 `.processing`；
/// - 崩溃恢复：若 `.processing` 已存在则直接消费它（不重复改名）；
/// - 去重：`capture.done` 里已有的行（崩溃中断/重写竞态）不再执行。
fn ingest_queue<F>(conn: &Connection, dir: &Path, name: &str, handler: F) -> Result<usize>
where
    F: Fn(&Connection, &str) -> Result<()>,
{
    let txt = dir.join(format!("{name}.txt"));
    let proc = dir.join(format!("{name}.processing"));
    let done = dir.join(format!("{name}.done"));

    if !proc.exists() && fs::rename(&txt, &proc).is_err() {
        return Ok(0);
    }

    let done_set = read_done_set(&done)?;
    let content = match fs::read_to_string(&proc) {
        Ok(c) => c,
        Err(_) => {
            let _ = fs::remove_file(&proc);
            return Ok(0);
        }
    };

    let mut out = String::new();
    let mut processed = 0usize;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || done_set.contains(line) {
            continue;
        }
        match handler(conn, line) {
            Ok(()) => writeln!(out, "[ok] {line}").unwrap(),
            Err(e) => {
                eprintln!("watch {name} failed for {line:?}: {e:#}");
                writeln!(out, "[fail] {line}").unwrap();
            }
        }
        processed += 1;
    }
    if !out.is_empty() {
        append_file(&done, &out)?;
    }
    let _ = fs::remove_file(&proc);
    prune_done(&done)?;
    Ok(processed)
}

/// 读取 `.done` 回执里的原始行集合（去掉 `[ok] `/`[fail] ` 前缀），用于去重。
fn read_done_set(path: &Path) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(set),
    };
    for line in content.lines() {
        let line = line.trim();
        let raw = line
            .strip_prefix("[ok] ")
            .or_else(|| line.strip_prefix("[fail] "))
            .unwrap_or(line);
        set.insert(raw.to_string());
    }
    Ok(set)
}

/// 采集一行：复用 `capture` 命令的 quick-add 解析与落库逻辑。
fn do_capture(conn: &Connection, line: &str) -> Result<()> {
    crate::commands::capture::run(
        conn,
        crate::commands::capture::CaptureArgs {
            title: line.to_string(),
            tags: Vec::new(),
            p1: false,
            p2: false,
            p3: false,
            due: None,
            status: None,
            json: false,
        },
    )
}

/// 执行一行操作：`done <ref>` | `set <ref> status <s>` | `set <ref> due <time>`。
fn do_action(conn: &Connection, line: &str) -> Result<()> {
    let mut words = line.split_whitespace();
    let verb = words.next().unwrap_or("");
    match verb {
        "done" => {
            let r = words
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: done <id|title>"))?;
            let id = resolve_ref(conn, r)?;
            tasks::transition(conn, &id, Status::Done)?;
            Ok(())
        }
        "set" => {
            let r = words
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: set <id|title> <field> <value>"))?;
            let field = words
                .next()
                .ok_or_else(|| anyhow::anyhow!("usage: set <id|title> <field> <value>"))?;
            let value = words.collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                anyhow::bail!("missing value for set {field}");
            }
            match field {
                "status" => {
                    let st: Status = value.parse().map_err(|e: String| anyhow::anyhow!(e))?;
                    let id = resolve_ref(conn, r)?;
                    tasks::transition(conn, &id, st)?;
                }
                "due" => {
                    let ms = time::parse_time(&value)?;
                    let id = resolve_ref(conn, r)?;
                    tasks::set_due(conn, &id, Some(ms))?;
                }
                _ => anyhow::bail!("unknown set field: {field}"),
            }
            Ok(())
        }
        _ => anyhow::bail!("unknown action: {verb} (expected done|set)"),
    }
}

/// 解析任务引用：id 精确 / 唯一前缀，退回精确标题匹配。
fn resolve_ref(conn: &Connection, key: &str) -> Result<String> {
    if let Ok(id) = tasks::resolve_id(conn, key) {
        return Ok(id);
    }
    let all = tasks::list(conn, &all_filter())?;
    let mut found: Vec<String> = all
        .into_iter()
        .filter(|t| t.title == key)
        .map(|t| t.id)
        .collect();
    found.sort();
    found.dedup();
    match found.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(crate::error::Error::TaskNotFound(key.to_string()).into()),
        _ => anyhow::bail!("ambiguous title: {key}"),
    }
}

fn all_filter() -> tasks::ListFilter {
    tasks::ListFilter {
        status: None,
        tags: Vec::new(),
        query: None,
        review_stale: false,
    }
}

/// 重写 `today.md`：仅活动任务（next/scheduled/waiting + 今日/逾期的 inbox），
/// 按 effective_due 排序，逾期置顶。内容未变化时跳过写入（避免 Syncthing 空通知）。
fn write_today(conn: &Connection, dir: &Path) -> Result<bool> {
    let all = tasks::list(conn, &all_filter())?;
    let ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
    let tag_map = tags::get_tags_for_tasks(conn, &ids)?;

    let (day_start, day_end) = time::local_day_bounds(0);
    let mut overdue: Vec<&Task> = Vec::new();
    let mut next: Vec<&Task> = Vec::new();
    let mut scheduled: Vec<&Task> = Vec::new();
    let mut waiting: Vec<&Task> = Vec::new();
    let mut inbox_today: Vec<&Task> = Vec::new();

    for t in &all {
        if t.status == Status::Done {
            continue;
        }
        let eff = effective_due(t);
        let is_overdue = eff.is_some_and(|d| d < day_start);
        match t.status {
            Status::Next => bucket(&mut overdue, &mut next, t, is_overdue),
            Status::Scheduled => bucket(&mut overdue, &mut scheduled, t, is_overdue),
            Status::Waiting => bucket(&mut overdue, &mut waiting, t, is_overdue),
            Status::Inbox => {
                if let Some(d) = eff {
                    if d < day_start {
                        overdue.push(t);
                    } else if d <= day_end {
                        inbox_today.push(t);
                    }
                }
            }
            Status::Someday | Status::Reference | Status::Done => {}
        }
    }

    let sort = |v: &mut Vec<&Task>| {
        v.sort_by_key(|t| (effective_due(t).unwrap_or(i64::MAX), t.created_at));
    };
    for v in [
        &mut overdue,
        &mut next,
        &mut scheduled,
        &mut waiting,
        &mut inbox_today,
    ] {
        sort(v);
    }

    let mut md = String::new();
    writeln!(
        md,
        "# 今日待办 · {}\n> horae watch 自动生成 · 采集写 capture.txt · 操作写 actions.txt\n",
        chrono::Local::now().format("%Y-%m-%d")
    )
    .unwrap();

    let mut any = false;
    any |= render_section(&mut md, "已逾期", &overdue, &tag_map);
    any |= render_section(&mut md, "Next", &next, &tag_map);
    any |= render_section(&mut md, "Scheduled", &scheduled, &tag_map);
    any |= render_section(&mut md, "Waiting", &waiting, &tag_map);
    any |= render_section(&mut md, "今日收件箱", &inbox_today, &tag_map);
    if !any {
        writeln!(md, "暂无活动任务").unwrap();
    }

    let path = dir.join(FILE_TODAY);
    if fs::read_to_string(&path).ok().as_deref() == Some(md.as_str()) {
        return Ok(false);
    }
    fs::write(&path, md)?;
    Ok(true)
}

fn bucket<'a>(
    overdue: &mut Vec<&'a Task>,
    normal: &mut Vec<&'a Task>,
    t: &'a Task,
    is_overdue: bool,
) {
    if is_overdue {
        overdue.push(t);
    } else {
        normal.push(t);
    }
}

fn render_section(
    md: &mut String,
    title: &str,
    items: &[&Task],
    tag_map: &HashMap<String, Vec<String>>,
) -> bool {
    if items.is_empty() {
        return false;
    }
    writeln!(md, "## {title}").unwrap();
    for t in items {
        let due = time::format_local(effective_due(t));
        let due_s = if due.is_empty() {
            String::new()
        } else {
            format!(" · {due}")
        };
        let tag_names = tag_map.get(&t.id).map(|v| v.join(",")).unwrap_or_default();
        let tags_s = if tag_names.is_empty() {
            String::new()
        } else {
            format!(" · @{tag_names}")
        };
        let id8 = &t.id[..t.id.len().min(8)];
        writeln!(md, "- [ ] `{id8}` {}{}{}", t.title, due_s, tags_s).unwrap();
    }
    writeln!(md).unwrap();
    true
}

/// 到点/逾期的活动任务写 `reminders/*.md`（每个 occurrence 至多一次，去重状态
/// 落在 `.horae-watch.json`）。电脑关机期间到期的任务，开机后会补写（catch-up）。
fn write_reminders(conn: &Connection, dir: &Path) -> Result<usize> {
    let state_path = dir.join(WATCH_STATE_FILE);
    let mut state: WatchState = match fs::read_to_string(&state_path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => WatchState::default(),
    };

    let all = tasks::list(conn, &all_filter())?;
    let now = time::now_ms();
    let mut wrote = 0usize;
    for t in &all {
        if t.status == Status::Done {
            continue;
        }
        if let Some(d) = effective_due(t) {
            let key = format!("{}:{}", t.id, d);
            if now >= d - REMIND_LEAD_MS && !state.reminded.contains(&key) {
                write_reminder_file(dir, t, d)?;
                state.reminded.push(key);
                wrote += 1;
            }
        }
    }

    let keep = now - 7 * 24 * 3600 * 1000;
    let pre = state.reminded.len();
    state.reminded.retain(|k| key_fresh(k, keep));
    if wrote > 0 || state.reminded.len() != pre {
        fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
    }
    Ok(wrote)
}

fn key_fresh(key: &str, keep_ms: i64) -> bool {
    key.rsplit_once(':')
        .is_some_and(|(_, m)| m.parse::<i64>().is_ok_and(|ms| ms >= keep_ms))
}

fn write_reminder_file(dir: &Path, t: &Task, due_ms: i64) -> Result<()> {
    let dir = dir.join(DIR_REMINDERS);
    fs::create_dir_all(&dir)?;
    let id8 = &t.id[..t.id.len().min(8)];
    let stamp = time::format_local(Some(due_ms)).replace(' ', "_");
    let safe_title: String = t
        .title
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .take(30)
        .collect();
    let name = format!("{stamp}_{id8}_{safe_title}.md");
    let path = dir.join(name);
    if path.exists() {
        return Ok(());
    }
    let content = format!(
        "# ⏰ 任务提醒\n\n任务：{}\n截止：{}\n状态：{}\nid：{}\n",
        t.title,
        time::format_local(Some(due_ms)),
        t.status,
        t.id
    );
    fs::write(path, content)?;
    Ok(())
}

fn append_file(path: &Path, content: &str) -> Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

/// 限制 `.done` 回执文件行数，避免去重集合与磁盘无限增长。
fn prune_done(path: &Path) -> Result<()> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= DONE_KEEP {
        return Ok(());
    }
    let tail: String = lines[lines.len() - DONE_KEEP..].join("\n") + "\n";
    fs::write(path, tail)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::test_conn;

    fn sync_dir(root: &Path) -> PathBuf {
        let d = root.join("sync");
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn all_tasks(conn: &Connection) -> Vec<Task> {
        tasks::list(conn, &all_filter()).unwrap()
    }

    #[test]
    fn ingest_captures_quick_add_lines() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        fs::write(
            dir.join("capture.txt"),
            "买牛奶 @home\n写周报 ~明天 09:00\n",
        )
        .unwrap();

        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.captures, 2);
        assert!(!dir.join("capture.txt").exists(), "capture.txt 已被消费");

        let done = fs::read_to_string(dir.join("capture.done")).unwrap();
        assert!(done.contains("[ok] 买牛奶 @home"), "回执写入: {done}");

        let all = all_tasks(&conn);
        assert_eq!(all.len(), 2);
        let inbox = all.iter().find(|t| t.title == "买牛奶").unwrap();
        assert_eq!(inbox.status, Status::Inbox, "无时间词留在收件箱");
        let scheduled = all.iter().find(|t| t.title == "写周报").unwrap();
        assert_eq!(scheduled.status, Status::Scheduled, "~time → 排程起点");
        assert!(
            crate::repo::tags::get_tag_by_name(&conn, "home")
                .unwrap()
                .is_some(),
            "@home 标签自动创建"
        );
    }

    #[test]
    fn duplicate_line_not_reprocessed() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        fs::write(dir.join("capture.txt"), "买牛奶\n").unwrap();
        process_once(&conn, &dir).unwrap();

        // 模拟手机在处理期间重写同一内容（崩溃恢复/竞态）→ 去重跳过
        fs::write(dir.join("capture.txt"), "买牛奶\n").unwrap();
        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.captures, 0, "重复行跳过");
        assert_eq!(all_tasks(&conn).len(), 1);
    }

    #[test]
    fn leftover_processing_recovers() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        fs::write(dir.join("capture.processing"), "恢复我\n").unwrap();
        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.captures, 1);
        assert!(all_tasks(&conn).iter().any(|t| t.title == "恢复我"));
        assert!(!dir.join("capture.processing").exists(), "暂存已清理");
    }

    #[test]
    fn actions_done_status_due() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        let t = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "写周报".into(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        fs::write(dir.join("actions.txt"), format!("done {}\n", &t.id[..8])).unwrap();
        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.actions, 1);
        assert_eq!(tasks::get(&conn, &t.id).unwrap().status, Status::Done);

        let t2 = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "另一个".into(),
                ..Default::default()
            },
        )
        .unwrap();
        fs::write(dir.join("actions.txt"), "set 另一个 status next\n").unwrap();
        process_once(&conn, &dir).unwrap();
        assert_eq!(tasks::get(&conn, &t2.id).unwrap().status, Status::Next);

        fs::write(
            dir.join("actions.txt"),
            format!("set {} due tomorrow\n", &t2.id[..8]),
        )
        .unwrap();
        process_once(&conn, &dir).unwrap();
        assert!(
            tasks::get(&conn, &t2.id).unwrap().due_at.is_some(),
            "due 已设置"
        );

        let done = fs::read_to_string(dir.join("actions.done")).unwrap();
        assert!(done.contains("[ok] done "));
        assert!(done.contains("[ok] set 另一个 status next"));
    }

    #[test]
    fn bad_action_recorded_as_fail() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        fs::write(dir.join("actions.txt"), "done 不存在的任务\n").unwrap();
        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.actions, 1, "失败行也算被消费（回执记录）");
        let done = fs::read_to_string(dir.join("actions.done")).unwrap();
        assert!(done.contains("[fail] done 不存在的任务"), "回执: {done}");
    }

    #[test]
    fn today_md_lists_active_only() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        let mk = |title: &str, status: Status| {
            tasks::create_capture(
                &conn,
                &tasks::CaptureInput {
                    title: title.into(),
                    status,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        mk("活跃任务", Status::Next);
        mk("等待中", Status::Waiting);
        mk("以后再说", Status::Someday);
        mk("已完成", Status::Done);

        let s = process_once(&conn, &dir).unwrap();
        assert!(s.today_written, "today.md 首次写入");
        let md = fs::read_to_string(dir.join(FILE_TODAY)).unwrap();
        assert!(md.contains("活跃任务"), "{md}");
        assert!(md.contains("等待中"), "{md}");
        assert!(!md.contains("以后再说"), "someday 不进活动视图: {md}");
        assert!(!md.contains("已完成"), "done 不进活动视图: {md}");

        // 内容未变 → 不重复写
        let s2 = process_once(&conn, &dir).unwrap();
        assert!(!s2.today_written, "无变化不重写");
    }

    #[test]
    fn reminder_written_once_and_catch_up() {
        let (root, conn) = test_conn();
        let dir = sync_dir(root.path());
        let due = time::now_ms() - 1000; // 电脑关机期间已到期 → 开机补提醒
        tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "到期任务".into(),
                status: Status::Next,
                due_at: Some(due),
                ..Default::default()
            },
        )
        .unwrap();

        let s = process_once(&conn, &dir).unwrap();
        assert_eq!(s.reminders, 1);
        let files: Vec<_> = fs::read_dir(dir.join(DIR_REMINDERS))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
        let content = fs::read_to_string(files[0].path()).unwrap();
        assert!(content.contains("到期任务"), "{content}");

        let s2 = process_once(&conn, &dir).unwrap();
        assert_eq!(s2.reminders, 0, "同一 occurrence 不重复提醒");
        assert_eq!(
            fs::read_dir(dir.join(DIR_REMINDERS)).unwrap().count(),
            1,
            "不产生重复文件"
        );
    }

    #[test]
    fn resolve_ref_prefers_id_then_title() {
        let (_root, conn) = test_conn();
        let t = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "唯一标题".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(resolve_ref(&conn, &t.id).unwrap(), t.id, "全 id");
        assert_eq!(resolve_ref(&conn, &t.id[..8]).unwrap(), t.id, "唯一前缀");
        assert_eq!(resolve_ref(&conn, "唯一标题").unwrap(), t.id, "精确标题");
        assert!(resolve_ref(&conn, "查无此任务").is_err(), "不存在报错");
    }
}
