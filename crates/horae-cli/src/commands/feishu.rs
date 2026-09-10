//! `horae feishu` 子命令：飞书群机器人提醒、长连接双向随手记与原生任务同步。

use anyhow::{bail, Result};
use rusqlite::Connection;
use std::sync::Arc;

use crate::cli::FeishuAction;
use crate::commands::watch::default_sync_dir;
use horae_core::config::{Config, FeishuConfig};
use horae_core::feishu::sync::FeishuSyncEngineImpl;
use horae_core::feishu::{
    push_due, send_summary, send_test, FeishuAuth, FeishuWsClient, UreqTransport,
};

/// 解析当前 profile 的飞书配置；未配置时输出清晰引导错误。
fn load_feishu(profile: Option<&str>) -> Result<(String, FeishuConfig)> {
    let cfg = Config::load()?;
    let (name, p) = cfg.resolve_profile(profile)?;
    let feishu_cfg = p.feishu.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "profile `{name}` 未配置飞书 (feishu)。请在 config.json 的该 profile 下添加 `feishu` 块：\n\
            {{\n  \
              \"webhook_url\": \"https://open.feishu.cn/open-apis/bot/v2/hook/...\",\n  \
              \"app_id\": \"cli_axxxxxxxxxxxx\",\n  \
              \"app_secret_env\": \"FEISHU_APP_SECRET\"\n\
            }}"
        )
    })?;
    Ok((name.to_string(), feishu_cfg))
}

pub fn run(conn: &Connection, action: FeishuAction, profile: Option<&str>) -> Result<()> {
    let (profile_name, cfg) = load_feishu(profile)?;
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
        FeishuAction::Listen => {
            if !cfg.is_app_configured() {
                bail!(
                    "未配置飞书自建应用凭据 (app_id 与 app_secret)。\n\
                    WebSocket 免公网 IP 长连接监听需要自建应用权限，请在 config.json 中配置：\n\
                    \"app_id\": \"cli_...\",\n\
                    \"app_secret_env\": \"FEISHU_APP_SECRET\""
                );
            }

            let app_id = cfg.app_id.clone().unwrap();
            let app_secret = cfg.resolve_app_secret().unwrap();
            let auth = Arc::new(FeishuAuth::new(app_id, app_secret));
            let client = FeishuWsClient::new(auth);

            println!("=======================================================");
            println!("⚡ horae 飞书 WebSocket 长连接随手记监听服务已启动");
            println!("   Profile:  {profile_name}");
            println!("   App ID:   {}", cfg.app_id.as_deref().unwrap_or_default());
            println!("   通信协议: 纯 Rust 同步阻塞 WebSocket (免公网 IP)");
            println!("   功能支持: 手机私聊随手记 (@tag ~time !pri) + 卡片按钮交互");
            println!("   提示: 按 Ctrl+C 可停止监听");
            println!("=======================================================");

            client.run(conn);
            Ok(())
        }
        FeishuAction::Doctor => {
            println!("=== horae 飞书功能环境体检 (Doctor) ===");
            println!("Profile: {profile_name}");

            // 1. Webhook 检查
            if let Some(wh) = &cfg.webhook_url {
                println!("✅ Webhook: 已配置 ({}...)", &wh[..wh.len().min(40)]);
                if cfg.resolve_secret().is_some() {
                    println!("✅ Webhook 签名密钥: 已配置并成功解析");
                } else {
                    println!("ℹ️  Webhook 签名密钥: 未配置 (建议在飞书机器人后台开启加签校验)");
                }
            } else {
                println!("ℹ️  Webhook: 未配置 (若无需单向群通知可忽略)");
            }

            // 2. 自建应用 App ID 检查
            if let Some(id) = &cfg.app_id {
                println!("✅ App ID: {id}");
            } else {
                println!("❌ App ID: 未配置 (无法使用 WebSocket 长连接随手记与原生任务同步)");
            }

            // 3. App Secret 检查
            let secret = cfg.resolve_app_secret();
            if secret.is_some() {
                println!("✅ App Secret: 已成功读取");
            } else {
                println!("❌ App Secret: 未配置或环境变量无法解析");
            }

            // 4. OpenAPI 通讯与 Token 获取测试
            if cfg.is_app_configured() {
                let app_id = cfg.app_id.clone().unwrap();
                let app_secret = secret.unwrap();
                let auth = FeishuAuth::new(app_id, app_secret);

                print!("⏳ 测试获取飞书 tenant_access_token... ");
                match auth.get_tenant_access_token() {
                    Ok(tok) => {
                        println!("✅ 成功 (token: {}...)", &tok[..tok.len().min(12)]);
                    }
                    Err(e) => {
                        println!("❌ 失败: {e:#}");
                    }
                }

                print!("⏳ 测试请求 WebSocket 长连接网关地址... ");
                match auth.fetch_ws_endpoint() {
                    Ok((url, conf)) => {
                        println!("✅ 成功获取网关通道");
                        println!("   Endpoint: {}...", &url[..url.len().min(45)]);
                        if let Some(c) = conf {
                            println!(
                                "   服务端建议重连间隔: {}s, 重试上限: {}",
                                c.reconnect_interval, c.reconnect_count
                            );
                        }
                    }
                    Err(e) => {
                        println!("❌ 失败: {e:#}");
                    }
                }
            }

            println!("========================================");
            Ok(())
        }
        FeishuAction::Sync { push, pull } => {
            if !cfg.is_app_configured() {
                bail!(
                    "未配置飞书自建应用凭据 (app_id 与 app_secret)。\n\
                    飞书官方任务 (Tasks v2) 同步需要自建应用权限，请在 config.json 中配置：\n\
                    \"app_id\": \"cli_...\",\n\
                    \"app_secret_env\": \"FEISHU_APP_SECRET\""
                );
            }

            let app_id = cfg.app_id.clone().unwrap();
            let app_secret = cfg.resolve_app_secret().unwrap();
            let auth = Arc::new(FeishuAuth::new(app_id, app_secret));
            let engine = FeishuSyncEngineImpl::new(auth);

            let do_push = push || !pull;
            let do_pull = pull || !push;

            println!("正在与飞书官方任务 (Tasks v2) 对账同步...");

            if do_pull {
                print!("⏳ 正在拉取飞书端新增与完成的任务... ");
                match engine.pull_remote_tasks(conn) {
                    Ok(stats) => {
                        println!(
                            "✅ 拉取完成（入库 {} 项，标记完成 {} 项）",
                            stats.pulled, stats.completed
                        );
                    }
                    Err(e) => {
                        println!("❌ 拉取失败: {e:#}");
                    }
                }
            }

            if do_push {
                print!("⏳ 正在将本地任务推送至飞书... ");
                match engine.push_local_tasks(conn) {
                    Ok(stats) => {
                        println!(
                            "✅ 推送完成（新建 {} 项，标记完成 {} 项）",
                            stats.created, stats.completed
                        );
                    }
                    Err(e) => {
                        println!("❌ 推送失败: {e:#}");
                    }
                }
            }

            Ok(())
        }
    }
}
