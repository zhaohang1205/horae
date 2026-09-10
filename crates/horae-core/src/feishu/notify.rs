//! 飞书任务提醒调度与去重核心逻辑。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::FeishuConfig;
use crate::feishu::card::{build_due_card, build_summary_card, build_test_card};
use crate::feishu::client::{wrap_card_payload, FeishuTransport};
use crate::model::task::{Status, Task};
use crate::repo::tasks;
use crate::schedule::effective_due;
use crate::time;

/// 飞书提醒去重状态文件名（置于同步目录，多端协同无妨）。
const FEISHU_STATE_FILE: &str = ".horae-feishu.json";

/// 每日简报去重状态文件名。
const FEISHU_BRIEF_FILE: &str = ".horae-feishu-brief.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FeishuState {
    #[serde(default)]
    pushed: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FeishuBriefState {
    #[serde(default)]
    last_date: String,
}

fn load_state(path: &Path) -> FeishuState {
    match fs::read_to_string(path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => FeishuState::default(),
    }
}

fn load_brief_state(path: &Path) -> FeishuBriefState {
    match fs::read_to_string(path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => FeishuBriefState::default(),
    }
}

fn key_fresh(key: &str, keep_ms: i64) -> bool {
    key.rsplit_once(':')
        .is_some_and(|(_, m)| m.parse::<i64>().is_ok_and(|ms| ms >= keep_ms))
}

/// 执行一轮到期提醒检查与推送。
///
/// 健壮性契约：单条推送失败不标记去重，留待下一轮重试；返回本轮成功推送数。
pub fn push_due(
    conn: &Connection,
    dir: &Path,
    cfg: &FeishuConfig,
    transport: &dyn FeishuTransport,
) -> Result<usize> {
    fs::create_dir_all(dir)?;

    let all = tasks::list(
        conn,
        &tasks::ListFilter {
            status: None,
            tags: Vec::new(),
            query: None,
            review_stale: false,
        },
    )?;
    let ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
    let tag_map: HashMap<String, Vec<String>> = crate::repo::tags::get_tags_for_tasks(conn, &ids)?;

    let now = time::now_ms();
    let lead_ms = (cfg.lead_minutes as i64) * 60_000;
    let secret = cfg.resolve_secret();

    let state_path = dir.join(FEISHU_STATE_FILE);
    let mut state = load_state(&state_path);

    let mut pushed = 0usize;
    for t in &all {
        if t.status == Status::Done {
            continue;
        }
        let Some(due) = effective_due(t) else {
            continue;
        };

        let key = format!("{}:{}", t.id, due);
        if now >= due - lead_ms && !state.pushed.contains(&key) {
            let tags = tag_map.get(&t.id).cloned().unwrap_or_default();
            let is_overdue = now > due;
            let card = build_due_card(t, due, is_overdue, &tags);

            let payload = match wrap_card_payload(card, secret.as_deref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("飞书包装卡片失败（{}）: {e:#}", t.id);
                    continue;
                }
            };

            if let Err(e) = transport.send(&cfg.webhook_url, &payload) {
                eprintln!("飞书推送失败（{}）: {e:#}", t.id);
                continue; // 失败不入 state，下一轮继续重试
            }

            state.pushed.push(key);
            pushed += 1;
        }
    }

    // 清理超过 7 天的历史记录
    let keep = now - 7 * 24 * 3600 * 1000;
    let pre_len = state.pushed.len();
    state.pushed.retain(|k| key_fresh(k, keep));
    if pushed > 0 || state.pushed.len() != pre_len {
        fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
    }

    Ok(pushed)
}

/// 检查并按需推送今日晨报（Daily Briefing）。
pub fn check_daily_briefing(
    conn: &Connection,
    dir: &Path,
    cfg: &FeishuConfig,
    transport: &dyn FeishuTransport,
) -> Result<bool> {
    let Some(briefing_time) = &cfg.daily_briefing else {
        return Ok(false);
    };

    let now_dt = chrono::Local::now();
    let today_str = now_dt.format("%Y-%m-%d").to_string();
    let current_hm = now_dt.format("%H:%M").to_string();

    if current_hm.as_str() < briefing_time.as_str() {
        return Ok(false);
    }

    let brief_path = dir.join(FEISHU_BRIEF_FILE);
    let mut brief_state = load_brief_state(&brief_path);
    if brief_state.last_date == today_str {
        return Ok(false);
    }

    send_summary(conn, cfg, transport)?;

    brief_state.last_date = today_str;
    fs::write(&brief_path, serde_json::to_string_pretty(&brief_state)?)?;
    Ok(true)
}

/// 立即手动推送今日待办摘要卡片。
pub fn send_summary(
    conn: &Connection,
    cfg: &FeishuConfig,
    transport: &dyn FeishuTransport,
) -> Result<()> {
    let all = tasks::list(
        conn,
        &tasks::ListFilter {
            status: None,
            tags: Vec::new(),
            query: None,
            review_stale: false,
        },
    )?;

    let (_, today_end) = time::local_day_bounds(0);

    // 筛选出属于今日及已逾期的未完成行动任务 (Next / Scheduled)
    let today_tasks: Vec<Task> = all
        .into_iter()
        .filter(|t| {
            if t.status != Status::Next && t.status != Status::Scheduled {
                return false;
            }
            match effective_due(t) {
                Some(due) => due <= today_end,
                None => false,
            }
        })
        .collect();

    let card = build_summary_card(&today_tasks);
    let secret = cfg.resolve_secret();
    let payload = wrap_card_payload(card, secret.as_deref())?;

    transport.send(&cfg.webhook_url, &payload)
}

/// 发送一条测试卡片，供 `horae feishu test` 验证连接。
pub fn send_test(cfg: &FeishuConfig, transport: &dyn FeishuTransport) -> Result<()> {
    let card = build_test_card();
    let secret = cfg.resolve_secret();
    let payload = wrap_card_payload(card, secret.as_deref())?;
    transport.send(&cfg.webhook_url, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feishu::client::FakeTransport;
    use crate::repo::tasks::CaptureInput;
    use crate::testutil::test_conn;

    #[test]
    fn test_push_due_and_dedup() {
        let (_tmp, conn) = test_conn();
        let fake = FakeTransport::new();
        let state_dir = tempfile::tempdir().unwrap();

        let cfg = FeishuConfig {
            webhook_url: "https://open.feishu.cn/mock".into(),
            secret: None,
            secret_env: None,
            lead_minutes: 10,
            daily_briefing: None,
        };

        // 创建一个 5 分钟后到期的任务 (符合 10 分钟提前量)
        let now = time::now_ms();
        let due = now + 5 * 60 * 1000;
        tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "准备发版".into(),
                due_at: Some(due),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        // 第一轮检查：应成功推送 1 条
        let count = push_due(&conn, state_dir.path(), &cfg, &fake).unwrap();
        assert_eq!(count, 1);
        assert_eq!(fake.count(), 1);

        // 第二轮检查：由于已在去重状态中，不再推送
        let count2 = push_due(&conn, state_dir.path(), &cfg, &fake).unwrap();
        assert_eq!(count2, 0);
        assert_eq!(fake.count(), 1);
    }
}
