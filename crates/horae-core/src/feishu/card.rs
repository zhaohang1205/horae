//! 飞书交互式消息卡片 (Interactive Message Card) JSON 构造器。
//!
//! 针对手机端与桌面端排版优化，支持自适应多列字段、状态色彩标识与 Markdown 格式。

use serde_json::{json, Value};

use crate::model::task::Task;
use crate::time;

/// 构建任务到期/逾期提醒卡片。
pub fn build_due_card(task: &Task, due_ms: i64, is_overdue: bool, tags: &[String]) -> Value {
    let (header_title, header_color) = if is_overdue {
        ("⚠️ 任务已逾期", "red")
    } else {
        ("⏰ 任务即将到期", "orange")
    };

    let formatted_time = time::format_local(Some(due_ms));
    let priority_str = match task.priority.as_deref() {
        Some("high") => "🔥 高 (High)",
        Some("medium") => "⚡ 中 (Medium)",
        Some("low") => "🌱 低 (Low)",
        _ => "—",
    };

    let tags_str = if tags.is_empty() {
        "—".to_string()
    } else {
        tags.iter()
            .map(|t| format!("`{t}`"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let mut elements = Vec::new();

    // 任务主标题
    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{}**", task.title)
        }
    }));

    // 结构化元数据字段
    elements.push(json!({
        "tag": "div",
        "fields": [
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**截止时间**\n{}", formatted_time)
                }
            },
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**优先级**\n{}", priority_str)
                }
            },
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**标签**\n{}", tags_str)
                }
            },
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**状态**\n{}", task.status)
                }
            }
        ]
    }));

    // 若有备注，显示备注前两行
    if !task.notes.trim().is_empty() {
        let snippet: String = task.notes.lines().take(2).collect::<Vec<_>>().join("\n");
        let truncated = if snippet.chars().count() > 150 {
            format!("{}...", snippet.chars().take(150).collect::<String>())
        } else {
            snippet
        };
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": format!("**备注**\n{}", truncated)
            }
        }));
    }

    // 底部注解
    let id8 = &task.id[..task.id.len().min(8)];
    elements.push(json!({
        "tag": "note",
        "elements": [
            {
                "tag": "plain_text",
                "content": format!("来自 horae GTD • ID: #{id8}")
            }
        ]
    }));

    json!({
        "header": {
            "title": {
                "tag": "plain_text",
                "content": header_title
            },
            "template": header_color
        },
        "elements": elements
    })
}

/// 构建测试验证卡片。
pub fn build_test_card() -> Value {
    json!({
        "header": {
            "title": {
                "tag": "plain_text",
                "content": "🔔 horae 飞书提醒测试"
            },
            "template": "green"
        },
        "elements": [
            {
                "tag": "div",
                "text": {
                    "tag": "lark_md",
                    "content": "**恭喜！飞书通知已成功打通。**\n当任务即将到期或逾期时，horae 将自动在此发送卡片提醒，手机与桌面端均可接收。"
                }
            },
            {
                "tag": "note",
                "elements": [
                    {
                        "tag": "plain_text",
                        "content": "来自 horae GTD"
                    }
                ]
            }
        ]
    })
}

/// 构建今日任务简报卡片。
pub fn build_summary_card(today_tasks: &[Task]) -> Value {
    let count = today_tasks.len();
    let mut elements = Vec::new();

    if count == 0 {
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": "🎉 **今日暂无待办任务，享受轻松的一天吧！**"
            }
        }));
    } else {
        let mut list_md = String::new();
        for (i, t) in today_tasks.iter().enumerate().take(10) {
            let pri = match t.priority.as_deref() {
                Some("high") => "🔥",
                Some("medium") => "⚡",
                Some("low") => "🌱",
                _ => "▫️",
            };
            let due_str = match crate::schedule::effective_due(t) {
                Some(ms) => format!(" `~{}`", time::format_local(Some(ms))),
                None => String::new(),
            };
            list_md.push_str(&format!("{}. {} **{}**{}\n", i + 1, pri, t.title, due_str));
        }
        if count > 10 {
            list_md.push_str(&format!("*... 以及另外 {} 项待办*", count - 10));
        }

        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": list_md
            }
        }));
    }

    elements.push(json!({
        "tag": "note",
        "elements": [
            {
                "tag": "plain_text",
                "content": format!("来自 horae GTD • 今日待办共 {} 项", count)
            }
        ]
    }));

    json!({
        "header": {
            "title": {
                "tag": "plain_text",
                "content": "☀️ 今日任务简报 (Today Overview)"
            },
            "template": "blue"
        },
        "elements": elements
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::Status;

    #[test]
    fn test_build_due_card_structure() {
        let task = Task {
            id: "12345678abcdef".into(),
            title: "写汇报".into(),
            notes: "第一行备注\n第二行备注\n第三行备注".into(),
            status: Status::Next,
            rrule: None,
            priority: Some("high".into()),
            created_at: 0,
            clarified_at: None,
            due_at: Some(1720000000000),
            scheduled_start_at: None,
            scheduled_end_at: None,
            completed_at: None,
            archived_at: None,
            archive_reason: None,
            updated_at: 0,
            delegated_to: None,
            checklist: Vec::new(),
        };

        let card = build_due_card(
            &task,
            1720000000000,
            false,
            &["work".into(), "report".into()],
        );
        assert_eq!(card["header"]["title"]["content"], "⏰ 任务即将到期");
        assert_eq!(card["header"]["template"], "orange");

        let overdue_card = build_due_card(&task, 1720000000000, true, &[]);
        assert_eq!(overdue_card["header"]["title"]["content"], "⚠️ 任务已逾期");
        assert_eq!(overdue_card["header"]["template"], "red");
    }

    #[test]
    fn test_build_test_card() {
        let card = build_test_card();
        assert_eq!(card["header"]["title"]["content"], "🔔 horae 飞书提醒测试");
        assert_eq!(card["header"]["template"], "green");
    }
}
