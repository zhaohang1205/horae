//! `horae feishu` 子命令：飞书群机器人提醒与消息卡片操作。

use anyhow::Result;
use rusqlite::Connection;

use crate::cli::FeishuAction;
use crate::commands::watch::default_sync_dir;
use horae_core::config::{Config, FeishuConfig};
use horae_core::feishu::{push_due, send_summary, send_test, UreqTransport};

/// 解析当前 profile 的飞书配置；未配置时输出清晰引导错误。
fn load_feishu(profile: Option<&str>) -> Result<FeishuConfig> {
    let cfg = Config::load()?;
    let (name, p) = cfg.resolve_profile(profile)?;
    p.feishu.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "profile `{name}` 未配置飞书 (feishu)。请在 config.json 的该 profile 下添加 `feishu` 块：\n\
            {{\"webhook_url\":\"https://open.feishu.cn/open-apis/bot/v2/hook/...\"}}"
        )
    })
}

pub fn run(conn: &Connection, action: FeishuAction, profile: Option<&str>) -> Result<()> {
    let cfg = load_feishu(profile)?;
    match action {
        FeishuAction::Test => {
            send_test(&cfg, &UreqTransport)?;
            println!("已向飞书发送测试卡片，请检查手机或桌面飞书是否收到。");
            Ok(())
        }
        FeishuAction::Due => {
            let dir = default_sync_dir();
            let count = push_due(conn, &dir, &cfg, &UreqTransport)?;
            println!("飞书扫描完成：本轮推送了 {count} 条到期提醒。");
            Ok(())
        }
        FeishuAction::Summary => {
            send_summary(conn, &cfg, &UreqTransport)?;
            println!("已向飞书发送今日任务简报卡片。");
            Ok(())
        }
    }
}
