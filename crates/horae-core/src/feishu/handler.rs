//! 飞书事件与卡片交互分发处理器。
//!
//! 负责：
//! 1. 响应 `im.message.receive_v1`（私聊随手记），经 quick-add 解析后落库，并私聊回复交互卡片。
//! 2. 响应 `card.action.trigger`（卡片按钮点击），执行任务状态流转（标为完成、顺延）并在原卡片原位更新反馈。

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;

use super::auth::FeishuAuth;
use super::card::{build_action_result_card, build_captured_card};
use super::proto::Frame;
use crate::model::task::Status;
use crate::repo::tasks;
use crate::time;

const FEISHU_API_BASE: &str = "https://open.feishu.cn";

/// 内存中用于去重的近期事件 ID 缓存（最多保留 256 条，避免飞书网络重试导致重复建任务）
static DEDUP_EVENTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn is_event_processed(event_id: &str) -> bool {
    let mut guard = DEDUP_EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    let set = guard.get_or_insert_with(HashSet::new);
    if set.contains(event_id) {
        return true;
    }
    if set.len() >= 256 {
        set.clear();
    }
    set.insert(event_id.to_string());
    false
}

/// 向指定消息发送卡片回复
pub fn reply_message(auth: &FeishuAuth, message_id: &str, card: &Value) -> Result<()> {
    let token = auth.get_tenant_access_token()?;
    let url = format!("{FEISHU_API_BASE}/open-apis/im/v1/messages/{message_id}/reply");
    let body = json!({
        "msg_type": "interactive",
        "content": serde_json::to_string(card)?,
    });
    let body_str = serde_json::to_string(&body)?;

    let resp_res = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json; charset=utf-8")
        .send_string(&body_str);

    let text = match resp_res {
        Ok(resp) => resp.into_string().context("读取飞书卡片回复响应内容失败")?,
        Err(ureq::Error::Status(status, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            bail!("飞书卡片回复接口返回 HTTP {status}: {err_body}");
        }
        Err(e) => bail!("向飞书发送卡片回复网络请求失败: {e}"),
    };

    let resp_val: Value = serde_json::from_str(&text).context("解析飞书卡片回复响应体失败")?;
    let code = resp_val["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        let msg = resp_val["msg"].as_str().unwrap_or("unknown error");
        bail!("飞书卡片回复接口返回错误 (code {code}: {msg})");
    }

    Ok(())
}

/// 向指定会话 (chat_id) 直接发送卡片消息（当引用回复不可用时的降级通道）
pub fn send_chat_card(auth: &FeishuAuth, chat_id: &str, card: &Value) -> Result<()> {
    let token = auth.get_tenant_access_token()?;
    let url = format!("{FEISHU_API_BASE}/open-apis/im/v1/messages?receive_id_type=chat_id");
    let body = json!({
        "receive_id": chat_id,
        "msg_type": "interactive",
        "content": serde_json::to_string(card)?,
    });
    let body_str = serde_json::to_string(&body)?;

    let resp_res = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json; charset=utf-8")
        .send_string(&body_str);

    let text = match resp_res {
        Ok(resp) => resp.into_string().context("读取飞书发消息响应内容失败")?,
        Err(ureq::Error::Status(status, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            bail!("飞书发消息接口返回 HTTP {status}: {err_body}");
        }
        Err(e) => bail!("向飞书发送消息网络请求失败: {e}"),
    };

    let resp_val: Value = serde_json::from_str(&text).context("解析飞书发消息响应体失败")?;
    let code = resp_val["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        let msg = resp_val["msg"].as_str().unwrap_or("unknown error");
        bail!("飞书发消息接口返回错误 (code {code}: {msg})");
    }

    Ok(())
}

/// 处理长连接数据帧，返回应答 Frame（若无需回发 ACK 则返回 None）
pub fn handle_data_frame(
    conn: &Connection,
    auth: &FeishuAuth,
    frame: &Frame,
) -> Result<Option<Frame>> {
    let payload_bytes = match &frame.payload {
        Some(b) => b,
        None => {
            return Ok(Some(Frame::build_ack(
                frame.seq_id,
                frame.service,
                frame.method,
                None,
            )))
        }
    };

    let val: Value = match serde_json::from_slice(payload_bytes) {
        Ok(v) => v,
        Err(_) => {
            // 非 JSON 报文，直接 ACK
            return Ok(Some(Frame::build_ack(
                frame.seq_id,
                frame.service,
                frame.method,
                None,
            )));
        }
    };

    // 1. 检查是否为事件或卡片动作
    let event_type = val["header"]["event_type"].as_str().unwrap_or_default();

    // ── 场景 A: 私聊文本消息接收 (随手记) ──
    if event_type == "im.message.receive_v1" {
        let event_id = val["header"]["event_id"].as_str().unwrap_or_default();
        if !event_id.is_empty() && is_event_processed(event_id) {
            let ack_body = serde_json::to_vec(&json!({"code": 200}))?;
            return Ok(Some(Frame::build_ack(
                frame.seq_id,
                frame.service,
                frame.method,
                Some(ack_body),
            )));
        }

        let sender_type = val["event"]["sender"]["sender_type"]
            .as_str()
            .unwrap_or_default();
        if sender_type == "app" {
            // 忽略机器人自身发送的消息，避免死循环
            let ack_body = serde_json::to_vec(&json!({"code": 200}))?;
            return Ok(Some(Frame::build_ack(
                frame.seq_id,
                frame.service,
                frame.method,
                Some(ack_body),
            )));
        }

        let message = &val["event"]["message"];
        let msg_type = message["message_type"].as_str().unwrap_or_default();
        let message_id = message["message_id"].as_str().unwrap_or_default();
        let chat_id = message["chat_id"].as_str().unwrap_or_default();

        if msg_type == "text" && !message_id.is_empty() {
            if let Some(content_str) = message["content"].as_str() {
                if let Ok(content_json) = serde_json::from_str::<Value>(content_str) {
                    if let Some(raw_text) = content_json["text"].as_str() {
                        let trimmed = raw_text.trim();
                        if !trimmed.is_empty() {
                            println!("[feishu-ws] 收到私聊随手记: \"{trimmed}\"");
                            // 执行 GTD quick-add 解析
                            let quick_add = crate::parser::parse_quick_add(trimmed);
                            let scheduled_start = quick_add
                                .time_str
                                .as_deref()
                                .and_then(|t| time::parse_time(t).ok());

                            let is_quote = quick_add.tags.iter().any(|t| t == tasks::QUOTE_TAG);

                            let status = if is_quote {
                                Status::Reference
                            } else if scheduled_start.is_some() {
                                Status::Scheduled
                            } else {
                                Status::Inbox
                            };

                            let input = tasks::CaptureInput {
                                title: quick_add.title,
                                notes: String::new(),
                                status,
                                due_at: None,
                                tag_names: quick_add.tags.clone(),
                                priority: quick_add.priority,
                                rrule: if scheduled_start.is_some() {
                                    None
                                } else {
                                    quick_add.rrule.clone()
                                },
                                delegated_to: None,
                                checklist: Vec::new(),
                            };

                            match tasks::create_capture(conn, &input) {
                                Ok(task) => {
                                    if let Some(start) = scheduled_start {
                                        let _ = tasks::schedule(
                                            conn,
                                            &task.id,
                                            start,
                                            None,
                                            quick_add.rrule,
                                        );
                                    }
                                    println!(
                                        "[feishu-ws] ✅ 成功录入任务: [{}] {}",
                                        task.id, task.title
                                    );

                                    // 重新读取最新任务属性与标签以构造反馈卡片
                                    if let Ok(latest_task) = tasks::get(conn, &task.id) {
                                        let tag_map = crate::repo::tags::get_tags_for_tasks(
                                            conn,
                                            &[&task.id],
                                        )
                                        .unwrap_or_default();
                                        let task_tags =
                                            tag_map.get(&task.id).cloned().unwrap_or_default();
                                        let reply_card =
                                            build_captured_card(&latest_task, &task_tags);

                                        println!("[feishu-ws] 正在向飞书回复交互卡片...");
                                        match reply_message(auth, message_id, &reply_card) {
                                            Ok(_) => {
                                                println!("[feishu-ws] ✅ 交互卡片回复成功！");
                                            }
                                            Err(e) => {
                                                eprintln!("[feishu-ws] ⚠️ 引用回复失败: {e:#}");
                                                if !chat_id.is_empty() {
                                                    eprintln!("[feishu-ws] 正在尝试直接向会话 ({chat_id}) 发送卡片...");
                                                    match send_chat_card(auth, chat_id, &reply_card) {
                                                        Ok(_) => println!("[feishu-ws] ✅ 直接发送卡片到会话成功！"),
                                                        Err(err2) => eprintln!("[feishu-ws] ❌ 直接发送卡片亦失败: {err2:#}"),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[feishu-ws] ❌ 录入任务失败: {e:#}");
                                }
                            }
                        }
                    }
                }
            }
        }

        let ack_body = serde_json::to_vec(&json!({"code": 200}))?;
        return Ok(Some(Frame::build_ack(
            frame.seq_id,
            frame.service,
            frame.method,
            Some(ack_body),
        )));
    }

    // ── 场景 B: 交互卡片按钮点击 (Card Action) ──
    // 卡片动作回调可能在 event["action"] 或根对象的 action 中
    let action_val = if val["event"]["action"]["value"].is_object() {
        &val["event"]["action"]["value"]
    } else if val["action"]["value"].is_object() {
        &val["action"]["value"]
    } else {
        &Value::Null
    };

    if let Some(action) = action_val["action"].as_str() {
        if let Some(task_id) = action_val["task_id"].as_str() {
            println!("[feishu-ws] 收到卡片动作触发: action={action}, task_id={task_id}");
            if let Ok(task) = tasks::get(conn, task_id) {
                let (toast_msg, updated_card) = match action {
                    "done" => {
                        let _ = tasks::transition(conn, &task.id, Status::Done);
                        let latest = tasks::get(conn, &task.id).unwrap_or(task);
                        (
                            "已标记为完成 ✅".to_string(),
                            build_action_result_card(
                                &latest,
                                "该任务已在本地 horae 成功流转为完成。",
                                true,
                            ),
                        )
                    }
                    "postpone_1d" => {
                        let one_day_ms = 86_400_000i64;
                        let new_due = task
                            .due_at
                            .map(|d| d + one_day_ms)
                            .or_else(|| task.scheduled_start_at.map(|s| s + one_day_ms))
                            .unwrap_or_else(|| time::now_ms() + one_day_ms);

                        let _ = tasks::schedule(
                            conn,
                            &task.id,
                            new_due,
                            task.scheduled_end_at.map(|e| e + one_day_ms),
                            task.rrule.clone(),
                        );
                        let latest = tasks::get(conn, &task.id).unwrap_or(task);
                        (
                            "已顺延 1 天 📅".to_string(),
                            build_action_result_card(
                                &latest,
                                "截止/计划时间已向后顺延 1 天。",
                                false,
                            ),
                        )
                    }
                    _ => (
                        "已接收操作".to_string(),
                        build_action_result_card(&task, "操作已处理。", false),
                    ),
                };

                let card_action_result = json!({
                    "toast": {
                        "type": "success",
                        "content": toast_msg,
                    },
                    "card": updated_card,
                });

                let result_str = serde_json::to_string(&card_action_result)?;
                let base64_data = BASE64.encode(result_str);

                let ack_body = serde_json::to_vec(&json!({
                    "code": 200,
                    "headers": {},
                    "data": base64_data,
                }))?;

                return Ok(Some(Frame::build_ack(
                    frame.seq_id,
                    frame.service,
                    frame.method,
                    Some(ack_body),
                )));
            }
        }
    }

    // 默认回发标准 200 ACK
    let ack_body = serde_json::to_vec(&json!({"code": 200}))?;
    Ok(Some(Frame::build_ack(
        frame.seq_id,
        frame.service,
        frame.method,
        Some(ack_body),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::test_conn;

    #[test]
    fn test_handle_card_action_done() {
        let (_tmp, conn) = test_conn();
        let auth = FeishuAuth::new("test_app".into(), "test_secret".into());

        let task = tasks::create_capture(
            &conn,
            &tasks::CaptureInput {
                title: "测试任务".into(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        let payload = json!({
            "event": {
                "action": {
                    "value": {
                        "action": "done",
                        "task_id": task.id
                    }
                }
            }
        });

        let frame = Frame {
            seq_id: 100,
            log_id: 200,
            service: 1,
            method: 1,
            headers: vec![],
            payload_encoding: None,
            payload_type: Some("json".into()),
            payload: Some(serde_json::to_vec(&payload).unwrap()),
            log_id_new: None,
        };

        let ack = handle_data_frame(&conn, &auth, &frame).unwrap().unwrap();
        assert_eq!(ack.seq_id, 100);

        // 验证数据库中该任务已被标记为 Done
        let updated = tasks::get(&conn, &task.id).unwrap();
        assert_eq!(updated.status, Status::Done);
    }
}
