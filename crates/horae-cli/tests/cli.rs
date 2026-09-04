//! 端到端 CLI 集成测试：跑真实二进制，隔离在独立的 `HORAE_CONFIG_DIR`，
//! 锁住 CLI 外部行为契约（capture → 流转 → done、循环任务推进、归档/清除
//! 门禁、id 前缀/标题解析、export/import 往返）。
//!
//! 每个测试用独立的 TempDir 作为配置目录，子进程环境变量互不干扰，可并行。

use assert_cmd::Command;
use chrono::{Datelike, Local};
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// 一个测试专属的隔离配置目录。`_dir` 保活到测试结束。
struct Env {
    _dir: TempDir,
}

fn env() -> Env {
    Env {
        _dir: tempfile::tempdir().unwrap(),
    }
}

impl Env {
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("horae").unwrap();
        c.env("HORAE_CONFIG_DIR", self._dir.path());
        c
    }
}

/// capture 并以 `--json` 输出解析出任务 id。
fn capture(env: &Env, args: &[&str]) -> String {
    let out = env
        .cmd()
        .args(["capture"])
        .args(args)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "capture 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["id"].as_str().unwrap().to_string()
}

/// `show --json` 解析为 Value（含 task/tags/events）。
fn show_json(env: &Env, id: &str) -> Value {
    let out = env.cmd().args(["show", id, "--json"]).output().unwrap();
    assert!(
        out.status.success(),
        "show 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn list_json(env: &Env, extra: &[&str]) -> Vec<Value> {
    let out = env
        .cmd()
        .args(["list", "--json"])
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "list 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

// ---------------------------------------------------------------- lifecycle

#[test]
fn capture_transition_done_full_lifecycle() {
    let env = env();
    let id = capture(&env, &["写周报", "--tag", "work"]);

    // 出现在收件箱列表
    let rows = list_json(&env, &["--status", "inbox"]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"].as_str().unwrap(), id);
    assert_eq!(rows[0]["status"].as_str().unwrap(), "Inbox");

    // inbox -> next -> done，stdout 报告流转结果
    env.cmd()
        .args(["next", &id])
        .assert()
        .success()
        .stdout(contains("-> next"));
    env.cmd()
        .args(["done", &id])
        .assert()
        .success()
        .stdout(contains("-> done"));

    // 详情：状态落库 + completed_at + 事件时间线完整
    let shown = show_json(&env, &id);
    assert_eq!(shown["task"]["status"].as_str().unwrap(), "Done");
    assert!(shown["task"]["completed_at"].is_i64());
    let types: Vec<&str> = shown["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    for expected in ["captured", "status_changed"] {
        assert!(types.contains(&expected), "时间线缺 {expected}: {types:?}");
    }

    // done 后不再出现在 next 列表
    assert!(list_json(&env, &["--status", "next"]).is_empty());
}

#[test]
fn list_filters_tasks_by_mmdd_date() {
    let env = env();
    let today = Local::now().date_naive();
    let target = format!(
        "{:04}-{:02}-{:02} 15:00",
        today.year(),
        today.month(),
        today.day()
    );
    capture(&env, &["打球", "--due", &target]);
    capture(&env, &["其他事情"]);

    let date = format!("{:02}{:02}", today.month(), today.day());
    let rows = list_json(&env, &["--date", &date]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "打球");

    env.cmd()
        .args(["list", "--date", "0230"])
        .assert()
        .failure()
        .stderr(contains("invalid date"));
}

#[test]
fn resolves_by_prefix_and_exact_title() {
    let env = env();
    let a = capture(&env, &["买牛奶"]);
    let b = capture(&env, ["写周报", "--tag", "work"].as_slice());

    // 8 位前缀解析
    let prefix = &a[..8];
    env.cmd()
        .args(["done", prefix])
        .assert()
        .success()
        .stdout(contains("-> done"));

    // 精确标题解析
    let title = show_json(&env, &b)["task"]["title"]
        .as_str()
        .unwrap()
        .to_string();
    env.cmd()
        .args(["next", &title])
        .assert()
        .success()
        .stdout(contains("-> next"));
    assert_eq!(
        show_json(&env, &b)["task"]["status"].as_str().unwrap(),
        "Next"
    );
}

// ----------------------------------------------------------- recurring task

#[test]
fn recurring_task_reschedules_on_done() {
    let env = env();
    let id = capture(&env, &["晨跑"]);

    env.cmd()
        .args(["schedule", &id, "--start", "+1d", "--rrule", "FREQ=DAILY"])
        .assert()
        .success();

    let before = show_json(&env, &id);
    let anchor = before["task"]["scheduled_start_at"].as_i64().unwrap();

    // Done 不终结循环任务：推进锚点重新排程到下一次发生
    env.cmd()
        .args(["done", &id])
        .assert()
        .success()
        .stdout(contains("-> scheduled"));

    let after = show_json(&env, &id);
    assert_eq!(after["task"]["status"].as_str().unwrap(), "Scheduled");
    assert_eq!(after["task"]["rrule"].as_str().unwrap(), "FREQ=DAILY");
    let next_anchor = after["task"]["scheduled_start_at"].as_i64().unwrap();
    assert!(
        next_anchor > anchor,
        "锚点应推进到下一次发生: {next_anchor} > {anchor}"
    );

    // 时间线记录 habit_completed 而非 completed
    let types: Vec<String> = after["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap().to_string())
        .collect();
    assert!(types.contains(&"habit_completed".to_string()), "{types:?}");
}

// ------------------------------------------------------------ archive/purge

#[test]
fn purge_requires_archive_then_deletes_permanently() {
    let env = env();
    let id = capture(&env, &["要清除的"]);

    // 未归档直接 purge 必须被拒绝
    env.cmd()
        .args(["purge", &id])
        .assert()
        .failure()
        .stderr(contains("not archived"));

    // archive -> restore 往返
    env.cmd()
        .args(["archive", &id])
        .assert()
        .success()
        .stdout(contains("archived"));
    assert_eq!(
        show_json(&env, &id)["task"]["archive_reason"]
            .as_str()
            .unwrap(),
        "deleted"
    );
    env.cmd()
        .args(["restore", &id])
        .assert()
        .success()
        .stdout(contains("restored"));
    assert!(show_json(&env, &id)["task"]["archived_at"].is_null());

    // archive -> purge 后永久消失
    env.cmd().args(["archive", &id]).assert().success();
    env.cmd()
        .args(["purge", &id])
        .assert()
        .success()
        .stdout(contains("purged"));
    env.cmd().args(["show", &id]).assert().failure();
    assert!(list_json(&env, &[]).is_empty());
}

#[test]
fn archived_task_hidden_from_actionable_lists() {
    let env = env();
    let id = capture(&env, &["被归档的"]);
    env.cmd().args(["next", &id]).assert().success();
    env.cmd().args(["archive", &id]).assert().success();

    assert!(list_json(&env, &[]).is_empty(), "默认列表不含归档任务");
    assert!(list_json(&env, &["--status", "next"]).is_empty());
}

// ------------------------------------------------------------ export/import

#[test]
fn export_import_roundtrip_across_config_dirs() {
    let shared = TempDir::new().unwrap();
    let backup = shared.path().join("horae-backup.json");

    // 库 A：两个任务 + 标签 + 完成事件
    let src = env();
    let live = capture(&src, &["买牛奶", "@home"]);
    let done = capture(&src, &["写周报"]);
    src.cmd().args(["done", &done]).assert().success();
    src.cmd()
        .args(["export", "--file"])
        .arg(&backup)
        .assert()
        .success()
        .stdout(contains("exported")); // 计数含系统 __journal__ 任务，不断言具体值

    // 库 B（全新配置目录 = 模拟换机）：导入后数据完整还原
    let dst = env();
    dst.cmd()
        .args(["import"])
        .arg(&backup)
        .assert()
        .success()
        .stdout(contains("tasks created"));

    let titles: Vec<String> = list_json(&dst, &[])
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_string())
        .collect();
    assert!(titles.contains(&"买牛奶".to_string()), "{titles:?}");

    let restored = show_json(&dst, &live);
    let tag_names: Vec<&str> = restored["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(tag_names.contains(&"home"), "{tag_names:?}");
    let types: Vec<&str> = restored["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"captured"), "{types:?}");

    // 已完成的任务还原后仍是 Done 且带 completed 原因归档信息可查
    let restored_done = dst.cmd().args(["import"]).arg(&backup).output().unwrap();
    assert!(restored_done.status.success());
    let stdout = String::from_utf8_lossy(&restored_done.stdout);
    assert!(
        stdout.contains("0 tasks created") && stdout.contains("skipped"),
        "重复合并应全部跳过: {stdout}"
    );
}

// ---------------------------------------------------------------- journal

#[test]
fn log_appends_then_lists_recent_entries() {
    let env = env();
    env.cmd().args(["log", "喝了三杯水"]).assert().success();
    env.cmd()
        .args(["log"])
        .assert()
        .success()
        .stdout(contains("喝了三杯水"));
}

// ---------------------------------------------------------------- pomodoro

#[test]
fn pomo_start_accepts_id_prefix() {
    let env = env();
    let id = capture(&env, &["写周报", "--tag", "work"]);
    let prefix = &id[..8];

    // 与其他命令一致的 git 式解析：前缀即可启动番茄
    env.cmd().args(["pomo", "start", prefix]).assert().success();

    // daemon 状态已写入：waybar 负载携带 work 相位与完整任务 id
    let out = env.cmd().args(["pomo", "waybar"]).output().unwrap();
    assert!(out.status.success());
    let payload: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(payload["class"], "work");
    assert_eq!(payload["id"], id.as_str());

    // 清理后台 daemon，避免影响后续测试与进程残留
    env.cmd().args(["pomo", "stop"]).assert().success();
}

// ---------------------------------------------------------------- ntfy

#[test]
fn ntfy_test_errors_when_unconfigured() {
    let env = env();
    // 全新配置目录、未写入 ntfy 块 → 应给出清晰的配置缺失提示而非崩溃
    env.cmd()
        .args(["ntfy", "test"])
        .assert()
        .failure()
        .stderr(contains("未配置 ntfy"));
}

#[test]
fn ntfy_rejects_unknown_action() {
    let env = env();
    env.cmd()
        .args(["ntfy", "bogus"])
        .assert()
        .failure()
        .stderr(contains("unknown ntfy action"));
}

#[test]
fn help_defaults_to_english() {
    let env = env();
    env.cmd()
        .args(["--help"])
        .assert()
        .success()
        .stdout(contains("GTD terminal task manager"));
}

#[test]
fn help_switches_to_chinese_via_env() {
    let env = env();
    env.cmd()
        .env("HORAE_LANG", "zh")
        .args(["--help"])
        .assert()
        .success()
        .stdout(contains("GTD 终端任务管理器"));
}

#[test]
fn help_switches_to_chinese_via_flag() {
    let env = env();
    env.cmd()
        .args(["--lang", "zh", "--help"])
        .assert()
        .success()
        .stdout(contains("GTD 终端任务管理器"));
}

// ---------------------------------------------------------------- modify

#[test]
fn modify_quick_add_and_explicit_flags() {
    let env = env();
    let id = capture(&env, &["原始任务", "--tag", "old"]);

    // 1. 使用 quick-add 语法更新标题、标签、时间与优先级
    env.cmd()
        .args([
            "modify",
            &id,
            "买有机牛奶",
            "@groceries",
            "~tomorrow 10:00",
            "!high",
        ])
        .assert()
        .success()
        .stdout(contains("modified ["));

    let show = show_json(&env, &id);
    assert_eq!(show["task"]["title"], "买有机牛奶");
    assert_eq!(show["task"]["status"], "Scheduled");
    assert_eq!(show["task"]["priority"], "high");
    let tag_names: Vec<String> = show["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(tag_names.contains(&"groceries".to_string()));
    assert!(tag_names.contains(&"old".to_string()));

    // 2. 使用显式参数更新 notes、due，并通过 --untag 移除旧标签
    env.cmd()
        .args([
            "edit", // 测试 alias
            &id,
            "--notes",
            "必须是全脂鲜牛奶",
            "--due",
            "+2d 18:00",
            "--untag",
            "old",
            "--status",
            "next",
        ])
        .assert()
        .success();

    let show2 = show_json(&env, &id);
    assert_eq!(show2["task"]["notes"], "必须是全脂鲜牛奶");
    assert_eq!(show2["task"]["status"], "Next");
    assert!(show2["task"]["due_at"].as_i64().is_some());
    let tag_names2: Vec<String> = show2["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!tag_names2.contains(&"old".to_string()));

    // 3. 清除 due 和全部 tags
    env.cmd()
        .args(["m", &id, "--clear-due", "--clear-tags"])
        .assert()
        .success();

    let show3 = show_json(&env, &id);
    assert!(show3["task"]["due_at"].is_null());
    assert!(show3["tags"].as_array().unwrap().is_empty());
}

#[test]
fn calendar_command_short_and_json_outputs() {
    let env = env();

    // 1. 测试 --short 紧凑单行
    let out = env.cmd().args(["calendar", "--short"]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.trim().is_empty());
    assert!(s.contains("·"));

    // 2. 测试指定日期与 --json 结构化输出（以中秋节 2026-09-25 为例）
    let out = env
        .cmd()
        .args(["cal", "2026-09-25", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["date"], "2026-09-25");
    assert_eq!(v["lunar"]["month_name"], "八月");
    assert_eq!(v["lunar"]["day_name"], "十五");
    assert!(v["holidays"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["name"] == "中秋节"));

    // 3. 测试 3 天提前提醒（2026-09-22 为中秋节前 3 天）
    let out = env
        .cmd()
        .args(["calendar", "2026-09-22", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let warns = v["warnings"].as_array().unwrap();
    assert!(warns
        .iter()
        .any(|w| w["holiday_name"] == "中秋节" && w["days_left"] == 3));

    // 4. 测试默认终端卡片输出
    let out = env.cmd().args(["calendar"]).output().unwrap();
    assert!(out.status.success());
    let card = String::from_utf8_lossy(&out.stdout);
    assert!(card.contains("公历日期"));
    assert!(card.contains("农历干支"));
    assert!(card.contains("下一节气") || card.contains("今日节气"));
    assert!(card.contains("摘要一览"));

    // 5. 测试非法日期解析报错
    let out = env.cmd().args(["calendar", "not-a-date"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("无法解析日期"));

    // 6. 测试超出农历范围日期报错 (1800年)
    let out = env.cmd().args(["calendar", "1800-01-01"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("超出支持的农历年份范围"));
}
