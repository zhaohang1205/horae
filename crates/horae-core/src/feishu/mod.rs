//! 飞书 (Feishu / Lark) 跨平台通知与交互模块。

pub mod card;
pub mod client;
pub mod notify;
pub mod sync;

#[cfg(test)]
pub use client::FakeTransport;
pub use client::{generate_signature, wrap_card_payload, FeishuTransport, UreqTransport};
pub use notify::{check_daily_briefing, push_due, send_summary, send_test};
