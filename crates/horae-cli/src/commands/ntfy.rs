//! `horae ntfy` 子命令：发送手机提醒相关的操作（当前仅 `test`）。

use anyhow::Result;

use horae_core::config::{Config, NtfyConfig};
use horae_core::ntfy::{send_test, UreqTransport};

/// 解析当前 profile 的 ntfy 配置；未配置时报清晰错误。
fn load_ntfy(profile: Option<&str>) -> Result<NtfyConfig> {
    let cfg = Config::load()?;
    let (name, p) = cfg.resolve_profile(profile)?;
    p.ntfy.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "profile `{name}` 未配置 ntfy。请在 config.json 的该 profile 下加 `ntfy` 块：\
            {{\"url\":\"https://ntfy.sh\",\"topic\":\"你的主题\"}}"
        )
    })
}

pub fn run(action: &str, profile: Option<&str>) -> Result<()> {
    match action {
        "test" => {
            let cfg = load_ntfy(profile)?;
            send_test(&cfg, &UreqTransport)?;
            println!("已发送测试提醒，请检查手机 ntfy 是否收到。");
            Ok(())
        }
        other => anyhow::bail!("unknown ntfy action: {other} (expected `test`)"),
    }
}
