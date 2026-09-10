//! 飞书 HTTP 客户端与传输抽象。
//!
//! 支持加签安全认证（HMAC-SHA256）与阻断式（blocking）发送。

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// 根据飞书自定义机器人协议生成加签签名。
/// 规范：以 `timestamp + "\n" + secret` 为 HMAC Key，对空数组做 HMAC-SHA256 并做 Base64 编码。
pub fn generate_signature(secret: &str, timestamp: i64) -> Result<String> {
    let string_to_sign = format!("{timestamp}\n{secret}");
    let mut mac =
        HmacSha256::new_from_slice(string_to_sign.as_bytes()).context("初始化 HMAC-SHA256 失败")?;
    mac.update(&[]);
    let code_bytes = mac.finalize().into_bytes();
    Ok(BASE64.encode(code_bytes))
}

/// 组装发送给 Webhook 的完整外层报文（支持可选的加签）。
pub fn wrap_card_payload(card: Value, secret: Option<&str>) -> Result<Value> {
    if let Some(sec) = secret {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("获取系统时间戳失败")?
            .as_secs() as i64;
        let sign = generate_signature(sec, ts)?;
        Ok(json!({
            "timestamp": ts.to_string(),
            "sign": sign,
            "msg_type": "interactive",
            "card": card
        }))
    } else {
        Ok(json!({
            "msg_type": "interactive",
            "card": card
        }))
    }
}

/// 飞书网络发送抽象。真实环境用 [`UreqTransport`]，测试用 [`FakeTransport`]。
pub trait FeishuTransport {
    fn send(&self, webhook_url: &str, payload: &Value) -> Result<()>;
}

/// 基于 ureq 的阻塞 HTTP 发送实现。
pub struct UreqTransport;

impl FeishuTransport for UreqTransport {
    fn send(&self, webhook_url: &str, payload: &Value) -> Result<()> {
        let resp = ureq::post(webhook_url)
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(5))
            .send_string(&payload.to_string())
            .map_err(|e| anyhow::anyhow!("飞书 Webhook 网络请求失败: {e}"))?;

        let text = resp
            .into_string()
            .context("读取飞书 Webhook 响应内容失败")?;

        let res_json: Value =
            serde_json::from_str(&text).context("解析飞书 Webhook 响应 JSON 失败")?;

        // 飞书 Webhook 成功通常返回: {"StatusCode": 0, "msg": "success"}
        // 或部分接口返回: {"code": 0, "msg": "success"}
        let code = res_json
            .get("StatusCode")
            .or_else(|| res_json.get("code"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if code != 0 {
            let msg = res_json
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("飞书返回错误码 {code}: {msg}");
        }

        Ok(())
    }
}

/// 测试用发送器：记录所有发送的请求，避免真实触网。
#[cfg(test)]
pub struct FakeTransport {
    pub sent: std::cell::RefCell<Vec<(String, Value)>>,
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
impl FeishuTransport for FakeTransport {
    fn send(&self, webhook_url: &str, payload: &Value) -> Result<()> {
        self.sent
            .borrow_mut()
            .push((webhook_url.to_string(), payload.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_signature_deterministic() {
        let sig = generate_signature("demo", 1599360473).unwrap();
        assert_eq!(sig, "l1N0gAcBjdwBvGm1xMjOF0XSyaLRpR7tuO5dHfhAYc8=");
    }

    #[test]
    fn test_wrap_card_payload_with_and_without_secret() {
        let card = json!({"test": "value"});
        let wrapped_no_sec = wrap_card_payload(card.clone(), None).unwrap();
        assert_eq!(wrapped_no_sec["msg_type"], "interactive");
        assert!(wrapped_no_sec.get("sign").is_none());

        let wrapped_with_sec = wrap_card_payload(card, Some("test_secret")).unwrap();
        assert_eq!(wrapped_with_sec["msg_type"], "interactive");
        assert!(wrapped_with_sec.get("sign").is_some());
        assert!(wrapped_with_sec.get("timestamp").is_some());
    }
}
