//! 飞书交互式消息卡片 (Interactive Message Card) JSON 构造器。
//!
//! 针对手机端与桌面端排版优化，支持自适应多列字段、状态色彩标识与 Markdown 格式。

use serde_json::{json, Value};

use crate::model::task::Task;
use crate::time;

/// 构建任务到期/逾期提醒卡片。
pub fn build_due_card(task: &Task, due_ms: i64, is_overdue: bool, tags: &[String]) -> Value {
    let (status_label, header_color) = if is_overdue {
        ("⚠️ 逾期", "red")
    } else {
        ("⏰ 即将到期", "orange")
    };
    let header_title = format!("{status_label}：{}", task.title);

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

    // 任务主标题与结构化详情（全端兼容排版，避免多列折叠丢失）
    let mut main_content = format!("### 📋 {}\n\n", task.title);
    main_content.push_str(&format!("* ⏰ **截止/计划**: {}\n", formatted_time));
    main_content.push_str(&format!("* ⚡ **优先级**: {}\n", priority_str));
    main_content.push_str(&format!("* 🏷 **标签**: {}\n", tags_str));
    main_content.push_str(&format!("* 📌 **状态**: {}", task.status));

    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": main_content
        }
    }));

    // 若有备注，显示前三行备注
    if !task.notes.trim().is_empty() {
        let snippet: String = task.notes.lines().take(3).collect::<Vec<_>>().join("\n");
        let truncated = if snippet.chars().count() > 150 {
            format!("{}...", snippet.chars().take(150).collect::<String>())
        } else {
            snippet
        };
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": format!("📝 **备注**\n> {}", truncated.replace('\n', "\n> "))
            }
        }));
    }

    // 分割线与交互按钮
    elements.push(json!({
        "tag": "hr"
    }));

    elements.push(json!({
        "tag": "action",
        "actions": [
            {
                "tag": "button",
                "text": {
                    "tag": "plain_text",
                    "content": "✅ 标为完成"
                },
                "type": "primary",
                "value": {
                    "action": "done",
                    "task_id": task.id
                }
            },
            {
                "tag": "button",
                "text": {
                    "tag": "plain_text",
                    "content": "📅 顺延 1 天"
                },
                "type": "default",
                "value": {
                    "action": "postpone_1d",
                    "task_id": task.id
                }
            }
        ]
    }));

    // 底部注解（包含 ID 与标题，确保预览与通知中心均清晰可见）
    let id8 = &task.id[..task.id.len().min(8)];
    elements.push(json!({
        "tag": "note",
        "elements": [
            {
                "tag": "plain_text",
                "content": format!("horae GTD • #{id8} • {}", task.title)
            }
        ]
    }));

    json!({
        "config": {
            "summary": {
                "content": format!("{status_label}：{} (截止 {})", task.title, formatted_time)
            },
            "update_multi": true
        },
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

/// 构建任务成功录入后的交互确认卡片（带【标为完成】与【顺延 1 天】按钮）
pub fn build_captured_card(task: &Task, tags: &[String]) -> Value {
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

    let due_str = match task.due_at {
        Some(ms) => time::format_local(Some(ms)),
        None => "—".to_string(),
    };

    let rrule_str = task.rrule.as_deref().unwrap_or("—");

    let mut elements = Vec::new();

    // 任务标题
    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{}**", task.title)
        }
    }));

    // 字段属性
    elements.push(json!({
        "tag": "div",
        "fields": [
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**状态**\n{}", task.status)
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
                    "content": format!("**截止时间**\n{}", due_str)
                }
            },
            {
                "is_short": true,
                "text": {
                    "tag": "lark_md",
                    "content": format!("**标签**\n{}", tags_str)
                }
            }
        ]
    }));

    if rrule_str != "—" {
        elements.push(json!({
            "tag": "div",
            "text": {
                "tag": "lark_md",
                "content": format!("🔄 **循环规则**: `{}`", rrule_str)
            }
        }));
    }

    elements.push(json!({
        "tag": "hr"
    }));

    // 交互按钮
    elements.push(json!({
        "tag": "action",
        "actions": [
            {
                "tag": "button",
                "text": {
                    "tag": "plain_text",
                    "content": "✅ 标为完成"
                },
                "type": "primary",
                "value": {
                    "action": "done",
                    "task_id": task.id
                }
            },
            {
                "tag": "button",
                "text": {
                    "tag": "plain_text",
                    "content": "📅 顺延 1 天"
                },
                "type": "default",
                "value": {
                    "action": "postpone_1d",
                    "task_id": task.id
                }
            }
        ]
    }));

    let id8 = &task.id[..task.id.len().min(8)];
    elements.push(json!({
        "tag": "note",
        "elements": [
            {
                "tag": "plain_text",
                "content": format!("horae GTD • #{id8} • 随手记已入库")
            }
        ]
    }));

    json!({
        "config": {
            "summary": {
                "content": format!("📥 已录入：{}", task.title)
            },
            "update_multi": true
        },
        "header": {
            "title": {
                "tag": "plain_text",
                "content": format!("📥 已录入：{}", task.title)
            },
            "template": "turquoise"
        },
        "elements": elements
    })
}

/// 构建卡片动作执行完毕后的更新卡片
pub fn build_action_result_card(task: &Task, message: &str, is_done: bool) -> Value {
    let header_template = if is_done { "green" } else { "blue" };
    let (action_label, header_title) = if is_done {
        ("✅ 已完成", format!("✅ 已完成：{}", task.title))
    } else {
        ("📅 已顺延", format!("📅 已顺延：{}", task.title))
    };

    let mut elements = Vec::new();
    elements.push(json!({
        "tag": "div",
        "text": {
            "tag": "lark_md",
            "content": format!("**{}**\n\n> {}", task.title, message)
        }
    }));

    let id8 = &task.id[..task.id.len().min(8)];
    elements.push(json!({
        "tag": "note",
        "elements": [
            {
                "tag": "plain_text",
                "content": format!("horae GTD • #{id8} • 状态: {}", task.status)
            }
        ]
    }));

    json!({
        "config": {
            "summary": {
                "content": format!("{action_label}：{}", task.title)
            },
            "update_multi": true
        },
        "header": {
            "title": {
                "tag": "plain_text",
                "content": header_title
            },
            "template": header_template
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
        assert_eq!(card["header"]["title"]["content"], "⏰ 即将到期：写汇报");
        assert_eq!(card["header"]["template"], "orange");
        assert_eq!(card["config"]["update_multi"], true);

        let overdue_card = build_due_card(&task, 1720000000000, true, &[]);
        assert_eq!(
            overdue_card["header"]["title"]["content"],
            "⚠️ 逾期：写汇报"
        );
        assert_eq!(overdue_card["header"]["template"], "red");
    }

    #[test]
    fn test_build_test_card() {
        let card = build_test_card();
        assert_eq!(card["header"]["title"]["content"], "🔔 horae 飞书提醒测试");
        assert_eq!(card["header"]["template"], "green");
    }

    #[test]
    fn test_build_captured_card() {
        let task = Task {
            id: "cap123".into(),
            title: "买咖啡".into(),
            notes: "".into(),
            status: Status::Inbox,
            rrule: None,
            priority: Some("high".into()),
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
        };

        let card = build_captured_card(&task, &["life".into()]);
        assert_eq!(card["header"]["title"]["content"], "📥 已录入：买咖啡");
        assert_eq!(card["header"]["template"], "turquoise");
        assert_eq!(card["config"]["update_multi"], true);

        let result_card = build_action_result_card(&task, "操作已完成", true);
        assert_eq!(
            result_card["header"]["title"]["content"],
            "✅ 已完成：买咖啡"
        );
        assert_eq!(result_card["header"]["template"], "green");
        assert_eq!(result_card["config"]["update_multi"], true);
    }
}
