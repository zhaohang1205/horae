//! 手机提醒推送（ntfy）。
//!
//! 设计很轻：桌面 `watch` 守护进程在任务到点前（默认 10 分钟）向 ntfy 主题
//! POST 一条消息，手机上订阅该主题的 ntfy App 即收原生推送。不建日历、不做双向
//! 录入——只解决「移动端收不到提醒」这一痛点。
//!
//! 网络部分抽象在 [`NtfyTransport`] trait 后，便于用 [`FakeTransport`] 做无网测试。
//! 真实实现 [`UreqTransport`] 基于 `ureq`（blocking，契合全库无 async 的现状）。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::NtfyConfig;
use crate::model::task::{Status, Task};
use crate::repo::tasks;
use crate::schedule::effective_due;
use crate::time;

/// 一条 ntfy 推送请求（纯数据，便于测试断言）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfyRequest {
    pub url: String,
    pub title: String,
    pub body: String,
    pub priority: u8,
    pub tags: Option<String>,
    pub token: Option<String>,
}

/// 网络发送抽象。真实环境用 [`UreqTransport`]，测试用 [`FakeTransport`]。
pub trait NtfyTransport {
    fn send(&self, req: &NtfyRequest) -> Result<()>;
}

/// 真实发送：基于 ureq 的一次阻塞 POST。
pub struct UreqTransport;

impl NtfyTransport for UreqTransport {
    fn send(&self, req: &NtfyRequest) -> Result<()> {
        // 注意：ntfy 的 `Title` 头若含非 ASCII（中文标题）会被 ureq 的 header
        // 校验拒绝。ntfy 在缺省 `Title` 头时会把正文首行当作标题，因此这里把
        // 标题并入正文首行，既保留中文又绕开该限制。
        let body = if req.title.is_empty() {
            req.body.clone()
        } else {
            format!("{}\n{}", req.title, req.body)
        };
        let mut builder = ureq::post(&req.url).set("Priority", &req.priority.to_string());
        if let Some(tags) = &req.tags {
            builder = builder.set("Tags", tags);
        }
        if let Some(token) = &req.token {
            builder = builder.set("Authorization", &format!("Bearer {token}"));
        }
        builder
            .send_string(&body)
            .map_err(|e| anyhow::anyhow!("ntfy 推送失败: {e}"))?;
        Ok(())
    }
}

/// 测试用「发送器」：不触网，记录所有发出的请求供断言。
#[cfg(test)]
pub struct FakeTransport {
    pub sent: std::cell::RefCell<Vec<NtfyRequest>>,
}

#[cfg(test)]
impl FakeTransport {
    pub fn new() -> Self {
        Self {
            sent: std::cell::RefCell::new(Vec::new()),
        }
    }
    pub fn count(&self) -> usize {
        self.sent.borrow().len()
    }
}

#[cfg(test)]
impl Default for FakeTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl NtfyTransport for FakeTransport {
    fn send(&self, req: &NtfyRequest) -> Result<()> {
        self.sent.borrow_mut().push(req.clone());
        Ok(())
    }
}

/// 根据配置 + 任务构造一条推送请求。
///
/// 仅当任务有「有效到期时间」（`effective_due`，即带排程起点/截止或循环下一次
/// 发生）时才返回 `Some`——无时间的纯收件箱任务不推送。已归档/完成任务由调用方
/// 的列表过滤保证（不在 `tasks::list` 结果里）。
pub fn build_request(cfg: &NtfyConfig, task: &Task, tag_names: &[String]) -> Option<NtfyRequest> {
    let due = effective_due(task)?;
    let when = time::format_local(Some(due));
    let id8 = &task.id[..task.id.len().min(8)];
    let tags_line = if tag_names.is_empty() {
        String::new()
    } else {
        format!("\n🏷 {}", tag_names.join(" "))
    };
    let body = format!("{}{}\n⏰ {}\n#{}", task.title, tags_line, when, id8);
    let token = cfg.token_env.as_ref().and_then(|e| std::env::var(e).ok());
    Some(NtfyRequest {
        url: format!("{}/{}", cfg.url.trim_end_matches('/'), cfg.topic),
        title: "⏰ 任务提醒".to_string(),
        body,
        priority: cfg.priority,
        tags: cfg.tags.clone(),
        token,
    })
}

/// 发送一条样例推送，供 `horae ntfy test` 验证手机是否收到。
pub fn send_test(cfg: &NtfyConfig, transport: &dyn NtfyTransport) -> Result<()> {
    let token = cfg.token_env.as_ref().and_then(|e| std::env::var(e).ok());
    let req = NtfyRequest {
        url: format!("{}/{}", cfg.url.trim_end_matches('/'), cfg.topic),
        title: "🔔 horae 测试".to_string(),
        body: "如果你在手机上看到这条，说明 ntfy 手机提醒已打通。".to_string(),
        priority: cfg.priority,
        tags: cfg.tags.clone().or_else(|| Some("bell".to_string())),
        token,
    };
    transport.send(&req)
}

/// ntfy 去重状态文件（放在同步目录内，随 Syncthing 同步无碍）。
const NTFY_STATE_FILE: &str = ".horae-ntfy.json";

/// 已推送集合（按 `task_id:due_ms` 去重），过期条目自动清理。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NtfyState {
    #[serde(default)]
    pushed: Vec<String>,
}

fn load_state(path: &Path) -> NtfyState {
    match fs::read_to_string(path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => NtfyState::default(),
    }
}

fn key_fresh(key: &str, keep_ms: i64) -> bool {
    key.rsplit_once(':')
        .is_some_and(|(_, m)| m.parse::<i64>().is_ok_and(|ms| ms >= keep_ms))
}

/// 一轮推送：把当前到点且未推送过的定时任务发到 ntfy。
///
/// 健壮性：单条推送失败不标记去重、不阻断其余任务，下一轮重试；返回本轮实际
/// 成功推送数。网络全挂时返回 0 且不写状态文件，避免污染去重集。
pub fn push_due(
    conn: &Connection,
    dir: &Path,
    cfg: &NtfyConfig,
    transport: &dyn NtfyTransport,
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

    let state_path = dir.join(NTFY_STATE_FILE);
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
            let tag_names = tag_map.get(&t.id).cloned().unwrap_or_default();
            match build_request(cfg, t, &tag_names) {
                Some(req) => {
                    if let Err(e) = transport.send(&req) {
                        eprintln!("ntfy 推送失败（{}）：{e:#}", t.id);
                        continue; // 不标记 → 下一轮重试
                    }
                    state.pushed.push(key);
                    pushed += 1;
                }
                None => continue,
            }
        }
    }

    let keep = now - 7 * 24 * 3600 * 1000;
    let pre = state.pushed.len();
    state.pushed.retain(|k| key_fresh(k, keep));
    if pushed > 0 || state.pushed.len() != pre {
        fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
    }
    Ok(pushed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tasks::CaptureInput;
    use crate::testutil::test_conn;

    fn untimed_task() -> Task {
        Task {
            id: "abc123def".into(),
            title: "无时间任务".into(),
            notes: String::new(),
            status: Status::Next,
            rrule: None,
            priority: None,
            created_at: 0,
            clarified_at: None,
            due_at: None,
            scheduled_start_at: None,
            scheduled_end_at: None,
            completed_at: None,
            archived_at: None,
            archive_reason: None,
            updated_at: 0,
            delegated_to: None,
            checklist: Vec::new(),
        }
    }

    #[test]
    fn build_request_omits_untimed_task() {
        let cfg = NtfyConfig {
            url: "https://ntfy.sh".into(),
            topic: "horae-x".into(),
            token_env: None,
            priority: 5,
            lead_minutes: 10,
            tags: None,
        };
        assert!(
            build_request(&cfg, &untimed_task(), &[]).is_none(),
            "无有效到期时间的任务不推送"
        );
    }

    #[test]
    fn build_request_includes_timed_task_fields() {
        let cfg = NtfyConfig {
            url: "https://ntfy.sh/".into(), // 末尾斜杠应被规整
            topic: "horae-x".into(),
            token_env: Some("HORAE_NTFY_TEST_TOKEN".into()),
            priority: 4,
            lead_minutes: 10,
            tags: Some("alarm".into()),
        };
        std::env::set_var("HORAE_NTFY_TEST_TOKEN", "sekret");
        let mut t = untimed_task();
        t.due_at = Some(1_700_000_000_000);
        let req = build_request(&cfg, &t, &["home".into()]).unwrap();
        assert_eq!(req.url, "https://ntfy.sh/horae-x", "url 规整尾斜杠");
        assert_eq!(req.priority, 4);
        assert_eq!(req.tags.as_deref(), Some("alarm"));
        assert_eq!(req.token.as_deref(), Some("sekret"));
        assert!(req.body.contains("无时间任务"), "标题在正文");
        assert!(req.body.contains("home"), "标签在正文");
        assert!(req.body.contains("#abc123de"), "短 id 在正文");
        assert_eq!(req.title, "⏰ 任务提醒");
    }

    #[test]
    fn push_due_sends_only_due_tasks_and_dedups() {
        let (root, conn) = test_conn();
        let dir = root.path().join("sync");
        fs::create_dir_all(&dir).unwrap();

        let due_soon = time::now_ms() - 1000; // 已到点
        let due_later = time::now_ms() + 3600 * 1000; // 远未到点
        let _a = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "到点任务".into(),
                status: Status::Next,
                due_at: Some(due_soon),
                tag_names: vec!["home".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let _b = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "未来任务".into(),
                status: Status::Next,
                due_at: Some(due_later),
                ..Default::default()
            },
        )
        .unwrap();

        let cfg = NtfyConfig {
            url: "https://ntfy.sh".into(),
            topic: "horae-x".into(),
            token_env: None,
            priority: 5,
            lead_minutes: 10,
            tags: None,
        };
        let fake = FakeTransport::new();
        let n = push_due(&conn, &dir, &cfg, &fake).unwrap();
        assert_eq!(n, 1, "只推送已到点任务");
        assert_eq!(fake.count(), 1, "FakeTransport 记录一次发送");
        assert!(fake.sent.borrow()[0].body.contains("到点任务"));

        // 第二轮不应重复推送
        let n2 = push_due(&conn, &dir, &cfg, &fake).unwrap();
        assert_eq!(n2, 0, "已推送任务不重复发");
        assert_eq!(fake.count(), 1);
    }

    #[test]
    fn push_due_retries_only_failed_tasks() {
        let (root, conn) = test_conn();
        let dir = root.path().join("sync");
        fs::create_dir_all(&dir).unwrap();

        let due = time::now_ms() - 1000;
        tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "会失败".into(),
                status: Status::Next,
                due_at: Some(due),
                ..Default::default()
            },
        )
        .unwrap();

        let cfg = NtfyConfig {
            url: "https://ntfy.sh".into(),
            topic: "horae-x".into(),
            token_env: None,
            priority: 5,
            lead_minutes: 10,
            tags: None,
        };

        // 第一次用永远失败的 transport：成功数 0，不入去重集
        let n = push_due(&conn, &dir, &cfg, &FailTransport).unwrap();
        assert_eq!(n, 0, "失败不入去重集");

        // 第二次换回正常 transport：应成功推送
        let fake = FakeTransport::new();
        let n2 = push_due(&conn, &dir, &cfg, &fake).unwrap();
        assert_eq!(n2, 1, "失败后下一轮重试成功");
    }

    /// 永远失败的 transport，用于验证重试语义。
    struct FailTransport;
    impl NtfyTransport for FailTransport {
        fn send(&self, _req: &NtfyRequest) -> Result<()> {
            anyhow::bail!("simulated network failure")
        }
    }
}
