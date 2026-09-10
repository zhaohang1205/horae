//! 飞书凭据与 Token 管理器，以及 WebSocket Endpoint 获取。

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::sync::RwLock;

use crate::time;

const FEISHU_API_BASE: &str = "https://open.feishu.cn";

/// 获取 tenant_access_token 的响应体
#[derive(Debug, Deserialize)]
struct TenantTokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: Option<String>,
    expire: Option<i64>,
}

/// 获取 WebSocket endpoint 的响应体
#[derive(Debug, Deserialize)]
struct WsEndpointResponse {
    code: i32,
    msg: String,
    data: Option<WsEndpointData>,
}

#[derive(Debug, Deserialize)]
struct WsEndpointData {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: Option<WsClientConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsClientConfig {
    #[serde(rename = "ReconnectCount", default)]
    pub reconnect_count: i32,
    #[serde(rename = "ReconnectInterval", default)]
    pub reconnect_interval: u64,
}

/// 飞书应用鉴权凭据与令牌缓存管理器
pub struct FeishuAuth {
    app_id: String,
    app_secret: String,
    cached_token: RwLock<Option<(String, i64)>>, // (token, expires_at_ms)
}

impl FeishuAuth {
    pub fn new(app_id: String, app_secret: String) -> Self {
        Self {
            app_id,
            app_secret,
            cached_token: RwLock::new(None),
        }
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// 获取有效的 `tenant_access_token`。若缓存未过期则直接复用，过期前 5 分钟自动换发新令牌。
    pub fn get_tenant_access_token(&self) -> Result<String> {
        let now = time::now_ms();
        {
            let guard = self
                .cached_token
                .read()
                .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;
            if let Some((ref token, expires_at)) = *guard {
                // 留出 300 秒 (5 分钟) 缓冲余量
                if now < expires_at - 300_000 {
                    return Ok(token.clone());
                }
            }
        }

        // 缓存失效或不存在，请求刷新
        let mut guard = self
            .cached_token
            .write()
            .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;
        // 双重检查
        if let Some((ref token, expires_at)) = *guard {
            if now < expires_at - 300_000 {
                return Ok(token.clone());
            }
        }

        let url = format!("{FEISHU_API_BASE}/open-apis/auth/v3/tenant_access_token/internal");
        let body = serde_json::json!({
            "app_id": self.app_id,
            "app_secret": self.app_secret,
        });
        let body_str = serde_json::to_string(&body)?;

        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(&body_str)
            .context("向飞书请求 tenant_access_token 网络失败")?;
        let text = resp
            .into_string()
            .context("读取 tenant_access_token 响应内容失败")?;
        let resp: TenantTokenResponse =
            serde_json::from_str(&text).context("解析 tenant_access_token 响应失败")?;

        if resp.code != 0 {
            bail!(
                "获取飞书 tenant_access_token 失败 (code {}: {})",
                resp.code,
                resp.msg
            );
        }

        let token = resp
            .tenant_access_token
            .ok_or_else(|| anyhow::anyhow!("飞书返回空 tenant_access_token"))?;
        let expire_secs = resp.expire.unwrap_or(7200);
        let expires_at = now + expire_secs * 1000;

        *guard = Some((token.clone(), expires_at));
        Ok(token)
    }

    /// 请求飞书长连接专属网关地址 (WebSocket Endpoint)
    pub fn fetch_ws_endpoint(&self) -> Result<(String, Option<WsClientConfig>)> {
        let url = format!("{FEISHU_API_BASE}/callback/ws/endpoint");
        let body = serde_json::json!({
            "AppID": self.app_id,
            "AppSecret": self.app_secret,
        });
        let body_str = serde_json::to_string(&body)?;

        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_string(&body_str)
            .context("向飞书请求长连接 endpoint 失败")?;
        let text = resp
            .into_string()
            .context("读取飞书长连接 endpoint 响应内容失败")?;
        let resp: WsEndpointResponse =
            serde_json::from_str(&text).context("解析飞书长连接 endpoint 响应体失败")?;

        if resp.code != 0 {
            bail!("获取飞书长连接地址失败 (code {}: {})", resp.code, resp.msg);
        }

        let data = resp
            .data
            .ok_or_else(|| anyhow::anyhow!("飞书未返回长连接 URL 数据"))?;
        Ok((data.url, data.client_config))
    }
}
