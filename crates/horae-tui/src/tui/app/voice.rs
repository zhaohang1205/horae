use anyhow::Result;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Instant;

use super::App;
use crate::tui::icons::Icon;

/// 单次语音录制的最大秒数（超时自动结束并进入转写）。
pub(crate) const VOICE_MAX_SECS: u64 = 30;

/// WAV RIFF 头部最小字节数（小于等于该值说明无有效采样数据）。
const MIN_WAV_BYTES: u64 = 44;

/// 默认录音源是否被静音。
///
/// 静音不会让 `pw-record` 失败——它照样写出一段合法但全为零的 WAV，转写模型
/// 于是从静音里"幻听"出几句无关的话，默默粘进输入框。用户看到的现象是
/// "麦克风在闪，但什么都没录进去"。
///
/// `@DEFAULT_AUDIO_SOURCE@` / `@DEFAULT_SOURCE@` 会跟随用户切换默认设备，
/// 因此不需要知道具体设备名。两个工具都不可用时返回 false：宁可放行，
/// 也不要在没法判断的情况下拦住用户。
fn default_capture_source_muted() -> bool {
    if let Ok(out) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SOURCE@"])
        .output()
    {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).contains("MUTED");
        }
    }

    if let Ok(out) = Command::new("pactl")
        .args(["get-source-mute", "@DEFAULT_SOURCE@"])
        .output()
    {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("yes");
        }
    }

    false
}

pub(crate) struct VoiceRecording {
    pub(crate) child: Child,
    pub(crate) wav_path: PathBuf,
    pub(crate) started_at: Instant,
    pub(crate) cleanup_wav: bool,
}

impl Drop for VoiceRecording {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.cleanup_wav {
            let _ = std::fs::remove_file(&self.wav_path);
        }
    }
}

impl<'a> App<'a> {
    pub(crate) fn is_voice_recording(&self) -> bool {
        self.voice_recording.is_some()
    }

    pub(crate) fn start_voice_recording(&mut self) -> Result<()> {
        // 若已有录音在运行，先安全终止并清理
        if self.voice_recording.is_some() {
            self.cancel_voice_recording();
        }

        // 静音时录出来的是全零采样，转写只会得到幻听出来的字。与其让用户
        // 以为录上了，不如直接说明原因并拦住，避免把垃圾文本写进待办。
        if default_capture_source_muted() {
            let icon = self.icon(Icon::Voice);
            self.set_toast(
                tr!(
                    self.lang,
                    "{} 默认麦克风已静音，录音只会得到静音。请先按键盘麦克风静音键或点状态栏麦克风图标解除",
                    "{} Default microphone is muted, so recording would capture silence. Unmute it first",
                    icon
                ),
                false,
            );
            return Ok(());
        }
        let wav_path =
            std::env::temp_dir().join(format!("horae_tui_voice_{}.wav", std::process::id()));
        let _ = std::fs::remove_file(&wav_path);
        let Some(wav_str) = wav_path.to_str() else {
            anyhow::bail!("invalid temp wav path");
        };

        // 优先使用 PipeWire 原生 pw-record，降级使用 ALSA arecord
        let child = if super::command_in_path("pw-record") {
            Command::new("pw-record")
                .args(["--rate", "16000", "--channels", "1", wav_str])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        } else {
            Command::new("arecord")
                .args(["-q", "-r", "16000", "-f", "S16_LE", "-c", "1", wav_str])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };

        match child {
            Ok(c) => {
                self.voice_recording = Some(VoiceRecording {
                    child: c,
                    wav_path,
                    started_at: Instant::now(),
                    cleanup_wav: true,
                });
                let icon = self.icon(Icon::Voice);
                self.set_toast(
                    tr!(
                        self.lang,
                        "{} 开始录音... (请说话，再次按 F8/Alt+V/回车 结束)",
                        "{} Recording started... (Speak now, F8/Alt+V/Enter to finish)",
                        icon
                    ),
                    true,
                );
            }
            Err(e) => {
                self.set_toast(
                    tr!(
                        self.lang,
                        "无法启动录音工具 (需要 pw-record 或 arecord): {}",
                        "Failed to start recorder (pw-record or arecord required): {}",
                        e
                    ),
                    false,
                );
            }
        }
        Ok(())
    }

    pub(crate) fn finish_voice_recording(&mut self) -> Result<()> {
        let mut rec = match self.voice_recording.take() {
            Some(r) => r,
            None => return Ok(()),
        };

        #[cfg(unix)]
        {
            let pid = rec.child.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGINT);
            }
            let start = Instant::now();
            loop {
                match rec.child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if start.elapsed() > std::time::Duration::from_millis(1500) {
                            let _ = rec.child.kill();
                            let _ = rec.child.wait();
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(40));
                    }
                    Err(_) => break,
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = rec.child.kill();
            let _ = rec.child.wait();
        }

        let wav_path = rec.wav_path.clone();
        // 交由转写流程管理 wav_path 的删除，避免 Drop 提前删掉音频文件
        rec.cleanup_wav = false;
        drop(rec);

        if !wav_path.exists()
            || std::fs::metadata(&wav_path).map(|m| m.len()).unwrap_or(0) <= MIN_WAV_BYTES
        {
            self.set_toast(
                tr!(
                    self.lang,
                    "录音文件为空或时间过短",
                    "Audio file is empty or too short"
                ),
                false,
            );
            let _ = std::fs::remove_file(&wav_path);
            return Ok(());
        }

        let Some(wav_str) = wav_path.to_str() else {
            let _ = std::fs::remove_file(&wav_path);
            return Ok(());
        };

        // 异步启动 voxtype，避免阻塞 TUI 渲染循环导致界面卡死
        let child = match Command::new("voxtype")
            .args(["-q", "transcribe", wav_str])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&wav_path);
                self.set_toast(
                    tr!(
                        self.lang,
                        "未找到 voxtype 或转写失败: {}",
                        "voxtype not found or failed: {}",
                        e
                    ),
                    false,
                );
                return Ok(());
            }
        };

        let (tx, rx) = mpsc::channel();
        self.voice_transcribing = true;
        self.voice_transcribe_rx = Some(rx);

        std::thread::spawn(move || {
            let res = child.wait_with_output();
            let _ = std::fs::remove_file(&wav_path);
            let _ = tx.send(res);
        });

        Ok(())
    }

    pub(crate) fn apply_transcription_output(&mut self, output: std::io::Result<Output>) {
        let raw = match output {
            Ok(out) => {
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr);
                    let err_msg = if err.trim().is_empty() {
                        String::from_utf8_lossy(&out.stdout).to_string()
                    } else {
                        err.to_string()
                    };
                    self.set_toast(
                        tr!(
                            self.lang,
                            "voxtype 转写失败: {}",
                            "voxtype transcription failed: {}",
                            err_msg.trim()
                        ),
                        false,
                    );
                    return;
                }
                String::from_utf8_lossy(&out.stdout).to_string()
            }
            Err(e) => {
                self.set_toast(
                    tr!(
                        self.lang,
                        "未找到 voxtype 或转写失败: {}",
                        "voxtype not found or failed: {}",
                        e
                    ),
                    false,
                );
                return;
            }
        };

        // 使用核心自然语言解析器提取结构化数据
        let task = horae_core::parser::natural::parse_natural_language(&raw);
        let syntax = task.to_quick_add_syntax();

        if syntax.trim().is_empty() || task.title == "未命名语音任务" {
            self.set_toast(
                tr!(self.lang, "未识别到有效语音内容", "No speech recognized"),
                false,
            );
            return;
        }

        self.input = syntax;
        self.input_cursor = self.input.len();
        self.clear_completion();
        let icon = self.icon(Icon::Voice);
        self.set_toast(
            tr!(
                self.lang,
                "{} 语音识别并解析成功，按 Enter 确认保存 (Esc 取消)",
                "{} Speech parsed to quick-add syntax, press Enter to submit",
                icon
            ),
            true,
        );
    }

    pub(crate) fn cancel_voice_recording(&mut self) {
        let was_active = self.voice_recording.is_some() || self.voice_transcribing;
        if let Some(rec) = self.voice_recording.take() {
            #[cfg(unix)]
            {
                let pid = rec.child.id();
                unsafe {
                    libc::kill(pid as i32, libc::SIGINT);
                }
            }
            // Drop 实现会自动 kill + wait 子进程并删除 wav_path
            drop(rec);
        }
        self.voice_transcribing = false;
        self.voice_transcribe_rx = None;
        if was_active {
            self.set_toast(
                tr!(self.lang, "已取消语音录入", "Voice capture cancelled"),
                false,
            );
        }
    }

    pub(crate) fn check_voice_timeout(&mut self) {
        if let Some(ref rx) = self.voice_transcribe_rx {
            match rx.try_recv() {
                Ok(out) => {
                    self.voice_transcribing = false;
                    self.voice_transcribe_rx = None;
                    self.apply_transcription_output(out);
                }
                Err(TryRecvError::Disconnected) => {
                    self.voice_transcribing = false;
                    self.voice_transcribe_rx = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if let Some(ref rec) = self.voice_recording {
            if rec.started_at.elapsed().as_secs() >= VOICE_MAX_SECS {
                let _ = self.finish_voice_recording();
            }
        }
    }
}
