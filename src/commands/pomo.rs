use crate::model::{event, pomodoro::Phase};
use crate::repo::{pomodoro, tasks};
use crate::time;
use anyhow::Result;
use rusqlite::Connection;
use std::process::Command as StdCommand;
use std::thread;
use std::time::Duration;

fn kill_daemon() {
    let _ = StdCommand::new("pkill")
        .args(["-f", "horae pomo daemon"])
        .status();
    // 等待旧进程实际退出，防止新旧 daemon 同时写 pomo.json 导致文件损坏
    thread::sleep(Duration::from_millis(200));
}

pub fn start(conn: &Connection, task_id: &str) -> Result<()> {
    let task = tasks::get(conn, task_id)?;
    kill_daemon();
    let mut state = pomodoro::get_state()?;
    let now = time::now_ms();
    let duration_ms = (state.config.work_mins as i64) * 60 * 1000;

    state.phase = Phase::Work;
    state.task_id = Some(task.id.clone());
    state.task_title = Some(task.title.clone());
    state.start_ts = Some(now);
    state.end_ts = Some(now + duration_ms);
    pomodoro::save_state(&state)?;

    let exe_path = std::env::current_exe().unwrap_or_else(|_| "horae".into());
    StdCommand::new(exe_path).args(["pomo", "daemon"]).spawn()?;

    notify(
        "🎯 专注模式开启",
        &format!(
            "开始专注任务: {}\n预计时长: {} 分钟",
            task.title, state.config.work_mins
        ),
    );
    Ok(())
}

pub fn stop() -> Result<()> {
    kill_daemon();
    let mut state = pomodoro::get_state()?;
    state.phase = Phase::Idle;
    state.task_id = None;
    state.task_title = None;
    state.start_ts = None;
    state.end_ts = None;
    // 显式停止就是中断：streak 归零 + cycle 归零（下次 start 从第 1 个开始计）
    state.streak = 0;
    state.cycle = 0;
    pomodoro::save_state(&state)?;
    notify("⏹️ 专注已终止", "番茄钟与专注模式已停止");
    Ok(())
}

pub fn waybar() -> Result<()> {
    let state = pomodoro::get_state()?;
    println!("{}", waybar_payload(&state, time::now_ms()));
    Ok(())
}

/// 构造 waybar 状态栏的 JSON 负载（纯函数，便于测试）。
fn waybar_payload(state: &crate::model::pomodoro::PomoState, now_ms: i64) -> serde_json::Value {
    use crate::model::pomodoro::Phase;

    if state.phase == Phase::Idle {
        return serde_json::json!({
            "text": "🍅",
            "class": "idle",
            "tooltip": "Pomodoro (Idle)"
        });
    }

    let end_ts = state.end_ts.unwrap_or(now_ms);
    let mut diff = (end_ts - now_ms) / 1000;
    if diff < 0 {
        diff = 0;
    }
    let m = diff / 60;
    let s = diff % 60;

    let icon = match state.phase {
        Phase::Work => "🍅",
        Phase::ShortBreak | Phase::LongBreak => "☕",
        Phase::Idle => "🍅",
    };

    let title = state.task_title.as_deref().unwrap_or("");
    let text = if title.is_empty() {
        format!("{} {:02}:{:02}", icon, m, s)
    } else {
        let mut short_title = title.to_string();
        if short_title.chars().count() > 15 {
            let truncated: String = short_title.chars().take(14).collect();
            short_title = format!("{}…", truncated);
        }
        format!("{} {} {:02}:{:02}", icon, short_title, m, s)
    };

    let class = match state.phase {
        Phase::Work => "work",
        Phase::ShortBreak => "short_break",
        Phase::LongBreak => "long_break",
        Phase::Idle => "idle",
    };
    let tooltip = format!("{} - {:?}", title, state.phase);
    let mut obj = serde_json::json!({
        "text": text,
        "class": class,
        "tooltip": tooltip
    });
    // 供状态栏识别当前聚焦任务：任务窗口据此把正在做的任务置顶高亮
    if let Some(ref id) = state.task_id {
        obj["id"] = serde_json::json!(id);
    }
    if !title.is_empty() {
        obj["title"] = serde_json::json!(title);
    }
    obj
}

pub fn daemon() -> Result<()> {
    let conn = crate::db::conn::open(None)?;
    loop {
        let mut state = pomodoro::get_state().unwrap_or_default();
        if state.phase == Phase::Idle {
            thread::sleep(Duration::from_secs(1));
            continue;
        }

        let now = time::now_ms();
        if advance_phase(&conn, &mut state, now) {
            let _ = pomodoro::save_state(&state);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

/// 推进番茄钟相位机（纯逻辑，便于测试）：
/// 若当前相位已到期，转换到下一相位并返回 true；未到期或 Idle 返回 false。
pub(crate) fn advance_phase(
    conn: &Connection,
    state: &mut crate::model::pomodoro::PomoState,
    now: i64,
) -> bool {
    if state.phase == Phase::Idle {
        return false;
    }
    let end_ts = state.end_ts.unwrap_or(now);
    if now < end_ts {
        return false;
    }

    match state.phase {
        Phase::Work => {
            if let Some(ref tid) = state.task_id {
                let duration = state.config.work_mins * 60;
                let _ = crate::repo::mutate(conn, |tx, at| {
                    crate::repo::log_event(
                        tx,
                        tid,
                        event::EV_POMODORO,
                        None,
                        None,
                        Some(&duration.to_string()),
                        at,
                    )
                });
            }

            // 跨天检测：若日期已切换，重置当日计数和循环计数
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            if state.last_date.as_deref() != Some(&today) {
                state.today_count = 0;
                state.cycle = 0;
                state.streak = 0;
                state.last_date = Some(today);
            }

            state.cycle += 1;
            state.total_count += 1;
            state.today_count += 1;
            state.streak += 1;
            state.last_completed_task_title = state.task_title.clone();

            let current_title = state.task_title.as_deref().unwrap_or("无标题");
            let total_mins = state.today_count * state.config.work_mins;

            if state.cycle.is_multiple_of(state.config.long_break_interval) {
                state.phase = Phase::LongBreak;
                let long_break_ms = (state.config.long_break_mins as i64) * 60 * 1000;
                state.end_ts = Some(now + long_break_ms);
                notify(
                    "🏆 专注成就战报达成！",
                    &format!(
                        "🎯 任务: {}\n⏱️ 专注: {}m | 🔥 今日第 {} 个番茄 ({:.1}h)\n💪 连击 Streak: {} 连击！建议长休 {} 分钟 ☕",
                        current_title,
                        state.config.work_mins,
                        state.today_count,
                        (total_mins as f64) / 60.0,
                        state.streak,
                        state.config.long_break_mins
                    ),
                );
            } else {
                state.phase = Phase::ShortBreak;
                let short_break_ms = (state.config.short_break_mins as i64) * 60 * 1000;
                state.end_ts = Some(now + short_break_ms);
                notify(
                    "🎉 专注成就战报达成！",
                    &format!(
                        "🎯 任务: {}\n⏱️ 专注: {}m | 🔥 今日第 {} 个番茄 ({:.1}h)\n💪 连击 Streak: {} 连击！开启小休 {} 分钟 ☕",
                        current_title,
                        state.config.work_mins,
                        state.today_count,
                        (total_mins as f64) / 60.0,
                        state.streak,
                        state.config.short_break_mins
                    ),
                );
            }
        }
        Phase::ShortBreak | Phase::LongBreak => {
            state.phase = Phase::Idle;
            notify(
                "⏰ 休息结束！战报结清",
                "休息已完成！按 [Space / P] 再接再厉开启新一轮，或按 [S] 结束专注。💪",
            );
        }
        Phase::Idle => {}
    }
    true
}

pub(crate) fn notify(summary: &str, body: &str) {
    // 桌面图形通知（同步，通常很快）
    let _ = StdCommand::new("notify-send")
        .args(["-u", "normal", "-i", "appointment-soon", summary, body])
        .status();

    // 音效提醒：在独立线程中异步播放，防止音频服务卡顿时阻塞 daemon 计时
    let sound_paths = [
        "/usr/share/sounds/freedesktop/stereo/complete.oga",
        "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga",
        "/usr/share/sounds/freedesktop/stereo/service-login.oga",
        "/usr/share/sounds/alsa/Front_Center.wav",
    ];

    let playable: Vec<String> = sound_paths
        .iter()
        .filter(|p| std::path::Path::new(p).exists())
        .map(|p| p.to_string())
        .collect();

    if playable.is_empty() {
        print!("\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        return;
    }

    thread::spawn(move || {
        let mut played = false;
        for path in &playable {
            if StdCommand::new("paplay")
                .arg(path)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
                || StdCommand::new("pw-play")
                    .arg(path)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                || StdCommand::new("aplay")
                    .arg(path)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                || StdCommand::new("mpv")
                    .args(["--no-terminal", path])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            {
                played = true;
                break;
            }
        }
        if !played {
            print!("\x07");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pomodoro::{PomoConfig, PomoState};
    use crate::testutil::test_conn;

    fn work_state(task_id: Option<&str>, title: &str, end_ts: i64) -> PomoState {
        PomoState {
            phase: Phase::Work,
            task_id: task_id.map(|s| s.to_string()),
            task_title: Some(title.to_string()),
            start_ts: Some(end_ts - 25 * 60 * 1000),
            end_ts: Some(end_ts),
            ..Default::default()
        }
    }

    fn count_pomo_events(conn: &Connection, task_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM task_events WHERE task_id = ?1 AND event_type = 'pomodoro'",
            rusqlite::params![task_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    // ---------- waybar_payload ----------

    #[test]
    fn waybar_idle_shows_tomato_and_idle_class() {
        let state = PomoState::default();
        let obj = waybar_payload(&state, 1_000);
        assert_eq!(obj["text"], "🍅");
        assert_eq!(obj["class"], "idle");
        assert!(obj.get("id").is_none(), "Idle 不携带任务 id");
    }

    #[test]
    fn waybar_work_shows_countdown_class_and_task_fields() {
        let now = 1_000_000;
        let state = work_state(Some("t-abc"), "写周报", now + 90_000);
        let obj = waybar_payload(&state, now);
        assert_eq!(obj["class"], "work");
        assert_eq!(obj["id"], "t-abc");
        assert_eq!(obj["title"], "写周报");
        let text = obj["text"].as_str().unwrap();
        assert!(text.contains("01:30"), "剩余 90s 应显示 01:30，实际 {text}");
    }

    #[test]
    fn waybar_truncates_long_title_with_ellipsis() {
        let now = 1_000_000;
        let long_title = "这是一个超过十五个字符的超长任务标题示例";
        let state = work_state(None, long_title, now + 60_000);
        let text = waybar_payload(&state, now)["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains('…'), "长标题应截断加省略号：{text}");
        assert!(!text.contains(long_title), "不应包含完整标题");
    }

    #[test]
    fn waybar_clamps_expired_countdown_to_zero() {
        let now = 1_000_000;
        let state = work_state(None, "", now - 5_000);
        let text = waybar_payload(&state, now)["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.ends_with("00:00"), "过期应钳为 00:00，实际 {text}");
    }

    // ---------- advance_phase ----------

    #[test]
    fn work_end_moves_to_short_break_and_counts() {
        let (_dir, conn) = test_conn();
        let t = crate::repo::tasks::create_capture(
            &conn,
            &crate::repo::tasks::CaptureInput {
                title: "专注任务".into(),
                status: crate::model::task::Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        let mut state = work_state(Some(&t.id), "专注任务", /*已到期*/ 1_000);
        assert!(advance_phase(&conn, &mut state, 2_000));

        assert_eq!(state.phase, Phase::ShortBreak);
        assert_eq!(state.cycle, 1);
        assert_eq!(state.streak, 1);
        assert_eq!(state.total_count, 1);
        assert_eq!(state.today_count, 1);
        assert_eq!(state.last_completed_task_title.as_deref(), Some("专注任务"));
        assert_eq!(
            count_pomo_events(&conn, &t.id),
            1,
            "完成番茄应为任务写入一条 pomodoro 事件"
        );
        // 新 end_ts = now + short_break_mins
        let expected = 2_000 + (PomoConfig::default().short_break_mins as i64) * 60 * 1000;
        assert_eq!(state.end_ts, Some(expected));
    }

    #[test]
    fn fourth_pomodoro_enters_long_break() {
        let (_dir, conn) = test_conn();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut state = work_state(None, "x", 1_000);
        state.cycle = 3;
        state.streak = 3;
        state.today_count = 3;
        state.last_date = Some(today);

        assert!(advance_phase(&conn, &mut state, 2_000));
        assert_eq!(state.phase, Phase::LongBreak);
        assert_eq!(state.cycle, 4);
        let expected = 2_000 + (PomoConfig::default().long_break_mins as i64) * 60 * 1000;
        assert_eq!(state.end_ts, Some(expected));
    }

    #[test]
    fn custom_interval_enters_long_break_early() {
        let (_dir, conn) = test_conn();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut state = work_state(None, "x", 1_000);
        state.cycle = 1;
        state.streak = 1;
        state.today_count = 1;
        state.last_date = Some(today);
        state.config.long_break_interval = 2;

        assert!(advance_phase(&conn, &mut state, 2_000));
        assert_eq!(
            state.phase,
            Phase::LongBreak,
            "interval=2 时第 2 个番茄应进入长休"
        );
        assert_eq!(state.cycle, 2);
        let expected = 2_000 + (PomoConfig::default().long_break_mins as i64) * 60 * 1000;
        assert_eq!(state.end_ts, Some(expected));
    }

    #[test]
    fn break_end_returns_to_idle_without_counting() {
        let (_dir, conn) = test_conn();
        let mut state = PomoState {
            phase: Phase::ShortBreak,
            cycle: 1,
            streak: 1,
            total_count: 1,
            today_count: 1,
            end_ts: Some(1_000),
            ..Default::default()
        };

        assert!(advance_phase(&conn, &mut state, 2_000));
        assert_eq!(state.phase, Phase::Idle);
        assert_eq!(state.cycle, 1, "休息结束不增加计数");
        assert_eq!(state.streak, 1);
    }

    #[test]
    fn stale_date_resets_daily_counters_before_counting() {
        let (_dir, conn) = test_conn();
        let mut state = work_state(None, "跨天", 1_000);
        // last_date 是很久以前 → 触发跨天重置
        state.last_date = Some("2000-01-01".to_string());
        state.cycle = 9;
        state.streak = 9;
        state.today_count = 5;

        assert!(advance_phase(&conn, &mut state, 2_000));
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(state.last_date.as_deref(), Some(today.as_str()));
        assert_eq!(state.today_count, 1, "重置后再累计 1 次");
        assert_eq!(state.cycle, 1, "cycle 同样先清零再 +1（不会进入长休）");
        assert_eq!(state.streak, 1);
        assert_eq!(state.phase, Phase::ShortBreak);
    }

    #[test]
    fn advance_before_end_and_idle_are_noop() {
        let (_dir, conn) = test_conn();
        // 未到期
        let mut state = work_state(None, "x", 10_000_000);
        assert!(!advance_phase(&conn, &mut state, 2_000));
        assert_eq!(state.phase, Phase::Work);
        // Idle
        let mut idle = PomoState {
            end_ts: Some(0), // 即使"已到期"
            ..Default::default()
        };
        assert!(!advance_phase(&conn, &mut idle, 2_000));
        assert_eq!(idle.phase, Phase::Idle);
    }
}
