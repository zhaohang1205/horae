//! 飞书原生任务 (Tasks v2 API) 双向/单向同步器。
//!
//! 支持：
//! 1. Push：将本地新建/更新的 GTD 任务推送至飞书官方任务列表；本地完成时调用远程标记完成。
//! 2. Pull：拉取飞书官方任务列表中用户新建或在飞书侧勾选完成的任务，同步更新本地 SQLite。
//! 3. 映射表 `task_feishu_links` 跟踪两端绑定关系与数据哈希。

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::Arc;

use super::auth::FeishuAuth;
use crate::model::task::{Status, Task};
use crate::repo::tasks;
use crate::schedule::effective_due;
use crate::time;

const FEISHU_API_BASE: &str = "https://open.feishu.cn";

/// 同步对账统计指标
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncStats {
    pub created: usize,
    pub updated: usize,
    pub completed: usize,
    pub pulled: usize,
}

/// 数据库映射记录
#[derive(Debug, Clone)]
pub struct FeishuTaskLink {
    pub task_id: String,
    pub feishu_guid: String,
    pub last_synced_at: i64,
    pub sync_hash: String,
}

/// 计算任务的轻量同步签名，用于识别任务是否有本地变更
fn compute_task_hash(task: &Task) -> String {
    format!(
        "{}:{}:{}:{:?}:{:?}",
        task.title, task.notes, task.status, task.due_at, task.scheduled_start_at
    )
}

/// 获取指定本地任务的飞书关联信息
pub fn get_task_link(conn: &Connection, task_id: &str) -> Result<Option<FeishuTaskLink>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, feishu_guid, last_synced_at, sync_hash \
         FROM task_feishu_links WHERE task_id = ?1",
    )?;
    let mut rows = stmt.query([task_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(FeishuTaskLink {
            task_id: row.get(0)?,
            feishu_guid: row.get(1)?,
            last_synced_at: row.get(2)?,
            sync_hash: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

/// 根据飞书 GUID 查找本地 task_id
pub fn get_task_id_by_guid(conn: &Connection, guid: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT task_id FROM task_feishu_links WHERE feishu_guid = ?1")?;
    let mut rows = stmt.query([guid])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// 保存或更新本地与飞书任务的绑定关系
pub fn upsert_task_link(
    conn: &Connection,
    task_id: &str,
    guid: &str,
    sync_hash: &str,
) -> Result<()> {
    let now = time::now_ms();
    conn.execute(
        "INSERT INTO task_feishu_links (task_id, feishu_guid, last_synced_at, sync_hash) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(task_id) DO UPDATE SET \
            feishu_guid = excluded.feishu_guid, \
            last_synced_at = excluded.last_synced_at, \
            sync_hash = excluded.sync_hash",
        params![task_id, guid, now, sync_hash],
    )?;
    Ok(())
}

/// 飞书官方任务 (Tasks v2) 同步引擎实现
pub struct FeishuSyncEngineImpl {
    auth: Arc<FeishuAuth>,
}

impl FeishuSyncEngineImpl {
    pub fn new(auth: Arc<FeishuAuth>) -> Self {
        Self { auth }
    }

    /// 在飞书官方任务列表中创建一条新任务
    pub fn create_remote_task(&self, task: &Task) -> Result<String> {
        let token = self.auth.get_tenant_access_token()?;
        let url = format!("{FEISHU_API_BASE}/open-apis/task/v2/tasks");

        let mut body = json!({
            "summary": task.title,
            "origin": {
                "platform_i18n_name": "{\"zh_cn\":\"horae GTD\",\"en_us\":\"horae GTD\"}"
            }
        });

        if !task.notes.trim().is_empty() {
            body["description"] = json!(task.notes);
        }

        if let Some(due) = effective_due(task) {
            body["due"] = json!({
                "timestamp": due.to_string(),
            });
        }

        let body_str = serde_json::to_string(&body)?;
        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(&body_str)
            .context("向飞书创建官方任务网络请求失败")?;

        let text = resp.into_string().context("读取飞书创建任务响应失败")?;
        let res_json: Value = serde_json::from_str(&text).context("解析飞书创建任务响应失败")?;

        let code = res_json["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = res_json["msg"].as_str().unwrap_or("unknown");
            bail!("飞书创建任务返回错误 (code {code}: {msg})");
        }

        let guid = res_json["data"]["task"]["guid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("飞书返回的 task 数据缺少 guid"))?;

        Ok(guid.to_string())
    }

    /// 在飞书侧将任务标记为已完成
    pub fn complete_remote_task(&self, guid: &str) -> Result<()> {
        let token = self.auth.get_tenant_access_token()?;
        let url = format!("{FEISHU_API_BASE}/open-apis/task/v2/tasks/{guid}");
        let body = json!({
            "task": {
                "completed_at": time::now_ms().to_string()
            },
            "update_fields": ["completed_at"]
        });
        let body_str = serde_json::to_string(&body)?;

        let resp = ureq::patch(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(&body_str)
            .context("向飞书标记完成任务网络请求失败")?;

        let text = resp.into_string().context("读取飞书完成任务响应失败")?;
        let res_json: Value = serde_json::from_str(&text)?;
        let code = res_json["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = res_json["msg"].as_str().unwrap_or("unknown");
            bail!("飞书标记任务完成返回错误 (code {code}: {msg})");
        }

        Ok(())
    }

    /// 拉取飞书侧的任务列表
    pub fn fetch_remote_tasks(&self) -> Result<Vec<Value>> {
        let token = self.auth.get_tenant_access_token()?;
        let url = format!("{FEISHU_API_BASE}/open-apis/task/v2/tasks?page_size=50");

        let resp = ureq::get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .context("拉取飞书官方任务列表网络请求失败")?;

        let text = resp.into_string().context("读取飞书任务列表响应失败")?;
        let res_json: Value = serde_json::from_str(&text)?;

        let code = res_json["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = res_json["msg"].as_str().unwrap_or("unknown");
            bail!("拉取飞书任务列表返回错误 (code {code}: {msg})");
        }

        let items = res_json["data"]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        Ok(items)
    }

    /// 将本地任务推送至飞书 (Push)
    pub fn push_local_tasks(&self, conn: &Connection) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let all_tasks = tasks::list(
            conn,
            &tasks::ListFilter {
                status: None,
                tags: Vec::new(),
                query: None,
                review_stale: false,
            },
        )?;

        for t in &all_tasks {
            let hash = compute_task_hash(t);
            let link = get_task_link(conn, &t.id)?;

            match link {
                Some(l) => {
                    // 已关联远程任务：若本地完成，则推进远程完成
                    if t.status == Status::Done
                        && l.sync_hash != hash
                        && self.complete_remote_task(&l.feishu_guid).is_ok()
                    {
                        let _ = upsert_task_link(conn, &t.id, &l.feishu_guid, &hash);
                        stats.completed += 1;
                    }
                }
                None => {
                    // 未关联且处于有效工作状态（Next / Scheduled / Inbox），推送到飞书
                    if t.status == Status::Next
                        || t.status == Status::Scheduled
                        || t.status == Status::Inbox
                    {
                        if let Ok(guid) = self.create_remote_task(t) {
                            let _ = upsert_task_link(conn, &t.id, &guid, &hash);
                            stats.created += 1;
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    /// 从飞书拉取任务并同步至本地 (Pull)
    pub fn pull_remote_tasks(&self, conn: &Connection) -> Result<SyncStats> {
        let mut stats = SyncStats::default();
        let remote_tasks = self.fetch_remote_tasks()?;

        for rt in remote_tasks {
            let guid = match rt["guid"].as_str() {
                Some(g) => g,
                None => continue,
            };
            let summary = rt["summary"].as_str().unwrap_or("飞书任务").trim();
            let completed_at_str = rt["completed_at"].as_str().unwrap_or("0");
            let is_remote_completed = completed_at_str != "0" && !completed_at_str.is_empty();

            match get_task_id_by_guid(conn, guid)? {
                Some(local_id) => {
                    // 本地已有对应任务：若飞书已完成而本地未完成，流转为 Done
                    if let Ok(local_task) = tasks::get(conn, &local_id) {
                        if is_remote_completed
                            && local_task.status != Status::Done
                            && tasks::transition(conn, &local_id, Status::Done).is_ok()
                        {
                            stats.completed += 1;
                        }
                    }
                }
                None => {
                    // 飞书端新建的任务，拉取并录入本地收件箱 (Inbox)
                    if !is_remote_completed {
                        let quick_add = crate::parser::parse_quick_add(summary);
                        let input = tasks::CaptureInput {
                            title: quick_add.title,
                            notes: rt["description"].as_str().unwrap_or_default().to_string(),
                            status: Status::Inbox,
                            due_at: None,
                            tag_names: quick_add.tags,
                            priority: quick_add.priority,
                            rrule: quick_add.rrule,
                            delegated_to: None,
                            checklist: Vec::new(),
                        };

                        if let Ok(created) = tasks::create_capture(conn, &input) {
                            let hash = compute_task_hash(&created);
                            let _ = upsert_task_link(conn, &created.id, guid, &hash);
                            stats.pulled += 1;
                        }
                    }
                }
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::test_conn;

    #[test]
    fn test_task_link_upsert_and_query() {
        let (_tmp, conn) = test_conn();

        let task = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "同步测试".into(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        let guid = "feishu-guid-123456";
        let hash = compute_task_hash(&task);

        // 初始不存在映射
        assert!(get_task_link(&conn, &task.id).unwrap().is_none());
        assert!(get_task_id_by_guid(&conn, guid).unwrap().is_none());

        // 插入映射
        upsert_task_link(&conn, &task.id, guid, &hash).unwrap();

        let link = get_task_link(&conn, &task.id)
            .unwrap()
            .expect("link exists");
        assert_eq!(link.task_id, task.id);
        assert_eq!(link.feishu_guid, guid);
        assert_eq!(link.sync_hash, hash);

        let mapped_id = get_task_id_by_guid(&conn, guid)
            .unwrap()
            .expect("found by guid");
        assert_eq!(mapped_id, task.id);

        // 更新映射
        let new_hash = format!("{hash}-updated");
        upsert_task_link(&conn, &task.id, guid, &new_hash).unwrap();
        let updated_link = get_task_link(&conn, &task.id).unwrap().unwrap();
        assert_eq!(updated_link.sync_hash, new_hash);
    }
}
