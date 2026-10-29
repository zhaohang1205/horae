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

pub(crate) struct VoiceRecording {
    pub(crate) child: Child,
    pub(crate) wav_path: PathBuf,
    pub(crate) started_at: Instant,
    pub(crate) cleanup_wav: bool,
    #[cfg(unix)]
    pub(crate) _wav_file: std::fs::File,
    #[cfg(unix)]
    pub(crate) dir_identity: (u64, u64),
    #[cfg(unix)]
    pub(crate) file_identity: (u64, u64),
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

        // 1. 获取专属私有目录 (Unix 0700)，隔离多用户共享临时目录风险
        let voice_dir = horae_core::fs_util::private_temp_dir("voice")?;

        // 2. 使用高熵随机 UUID v4 作为文件名，杜绝 PID 枚举预测与文件抢占劫持
        let wav_path = voice_dir.join(format!("horae_tui_voice_{}.wav", uuid::Uuid::new_v4()));

        // 3. 显式以 0600 (rw-------) 权限原子排他预创建目标文件并记录目录与文件身份
        #[cfg(unix)]
        let (wav_file, file_identity, dir_identity) = {
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::fs::PermissionsExt;

            let dir_meta = std::fs::symlink_metadata(&voice_dir)?;
            if dir_meta.file_type().is_symlink() || !dir_meta.is_dir() {
                anyhow::bail!("security check failed: voice directory is invalid or symlink");
            }
            let current_uid = unsafe { libc::getuid() };
            if dir_meta.uid() != current_uid {
                anyhow::bail!(
                    "security check failed: voice directory owned by uid {}, expected {}",
                    dir_meta.uid(),
                    current_uid
                );
            }
            let dir_identity = (dir_meta.dev(), dir_meta.ino());

            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW)
                .mode(0o600)
                .open(&wav_path)?;

            let file_meta = file.metadata()?;
            if file_meta.uid() != current_uid || (file_meta.permissions().mode() & 0o777) != 0o600 {
                anyhow::bail!(
                    "security check failed: voice file has unexpected permissions or owner"
                );
            }
            let file_identity = (file_meta.dev(), file_meta.ino());
            (file, file_identity, dir_identity)
        };
        #[cfg(not(unix))]
        let _file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&wav_path)?;

        let Some(wav_str) = wav_path.to_str() else {
            anyhow::bail!("invalid temp wav path");
        };

        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;
        #[cfg(unix)]
        let raw_fd = wav_file.as_raw_fd();
        #[cfg(unix)]
        let proc_fd_path = format!("/proc/self/fd/{}", raw_fd);
        #[cfg(unix)]
        let target_arg = if std::path::Path::new("/proc/self/fd").exists() {
            proc_fd_path.as_str()
        } else {
            wav_str
        };
        #[cfg(not(unix))]
        let target_arg = wav_str;

        // 4. 为录音子进程设置 umask 0077，并在 pre_exec 验证打开的文件描述符身份并清除 CLOEXEC
        #[cfg(unix)]
        let spawn_recorder = |mut cmd: Command| -> std::io::Result<Child> {
            use std::os::unix::process::CommandExt;
            let current_uid = unsafe { libc::getuid() };
            unsafe {
                cmd.pre_exec(move || {
                    let flags = libc::fcntl(raw_fd, libc::F_GETFD);
                    if flags >= 0 {
                        libc::fcntl(raw_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
                    }
                    libc::umask(0o077);

                    let mut st: libc::stat = std::mem::zeroed();
                    if libc::fstat(raw_fd, &mut st) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if st.st_uid != current_uid || (st.st_mode & 0o777) != 0o600 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "security check failed: raw_fd identity mismatch",
                        ));
                    }
                    if (st.st_dev as u64) != file_identity.0
                        || (st.st_ino as u64) != file_identity.1
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "security check failed: raw_fd dev/ino mismatch",
                        ));
                    }
                    Ok(())
                });
            }
            cmd.spawn()
        };
        #[cfg(not(unix))]
        let spawn_recorder = |mut cmd: Command| -> std::io::Result<Child> { cmd.spawn() };

        let backend_override = std::env::var("HORAE_RECORDER_BACKEND").ok();
        let use_pw = match backend_override.as_deref() {
            Some("pw-record") => true,
            Some("arecord") => false,
            _ => super::command_in_path("pw-record"),
        };

        // 优先使用 PipeWire 原生 pw-record，降级使用 ALSA arecord
        let child = if use_pw {
            let mut cmd = Command::new("pw-record");
            cmd.args(["--rate", "16000", "--channels", "1", target_arg])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            spawn_recorder(cmd)
        } else {
            let mut cmd = Command::new("arecord");
            cmd.args(["-q", "-r", "16000", "-f", "S16_LE", "-c", "1", target_arg])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            spawn_recorder(cmd)
        };

        let child = match child {
            Ok(c) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    use std::os::unix::fs::PermissionsExt;
                    let cur_dir_meta = std::fs::symlink_metadata(&voice_dir);
                    let cur_file_meta = std::fs::symlink_metadata(&wav_path);
                    let valid = match (cur_dir_meta, cur_file_meta) {
                        (Ok(dm), Ok(fm)) => {
                            let current_uid = unsafe { libc::getuid() };
                            !dm.file_type().is_symlink()
                                && dm.is_dir()
                                && dm.uid() == current_uid
                                && (dm.dev(), dm.ino()) == dir_identity
                                && !fm.file_type().is_symlink()
                                && fm.is_file()
                                && fm.uid() == current_uid
                                && (fm.dev(), fm.ino()) == file_identity
                                && (fm.permissions().mode() & 0o777) == 0o600
                        }
                        _ => false,
                    };
                    if !valid {
                        let mut killed = c;
                        let _ = killed.kill();
                        let _ = killed.wait();
                        let _ = std::fs::remove_file(&wav_path);
                        anyhow::bail!(
                            "security check failed: directory or file identity changed during recorder launch"
                        );
                    }
                }
                c
            }
            Err(e) => {
                let _ = std::fs::remove_file(&wav_path);
                self.set_toast(
                    tr!(
                        self.lang,
                        "无法启动录音工具 (需要 pw-record 或 arecord): {}",
                        "Failed to start recorder (pw-record or arecord required): {}",
                        e
                    ),
                    false,
                );
                return Ok(());
            }
        };

        self.voice_recording = Some(VoiceRecording {
            child,
            wav_path,
            started_at: Instant::now(),
            cleanup_wav: true,
            #[cfg(unix)]
            _wav_file: wav_file,
            #[cfg(unix)]
            dir_identity,
            #[cfg(unix)]
            file_identity,
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::PermissionsExt;

            let parent_dir = wav_path.parent().unwrap_or(&wav_path);
            let dir_check = std::fs::symlink_metadata(parent_dir).map(|dm| {
                let current_uid = unsafe { libc::getuid() };
                !dm.file_type().is_symlink()
                    && dm.is_dir()
                    && dm.uid() == current_uid
                    && (dm.dev(), dm.ino()) == rec.dir_identity
            });
            let file_check = std::fs::symlink_metadata(&wav_path).map(|fm| {
                let current_uid = unsafe { libc::getuid() };
                !fm.file_type().is_symlink()
                    && fm.is_file()
                    && fm.uid() == current_uid
                    && (fm.dev(), fm.ino()) == rec.file_identity
                    && (fm.permissions().mode() & 0o777) == 0o600
            });

            if !matches!(dir_check, Ok(true)) || !matches!(file_check, Ok(true)) {
                drop(rec);
                let _ = std::fs::remove_file(&wav_path);
                self.set_toast(
                    tr!(
                        self.lang,
                        "安全检查失败: 录音文件或目录身份在录音期间被篡改",
                        "Security check failed: audio file or directory identity substituted during recording"
                    ),
                    false,
                );
                return Ok(());
            }
        }

        // 交由转写流程管理 wav_path 的删除，避免 Drop 提前删掉音频文件
        rec.cleanup_wav = false;
        drop(rec);

        // 录音子进程退出后二次确保 WAV 文件保持 0600 私有访问权限并传播错误
        if let Err(e) = horae_core::fs_util::ensure_private_file(&wav_path) {
            let _ = std::fs::remove_file(&wav_path);
            self.set_toast(
                tr!(
                    self.lang,
                    "安全加固失败: 录音文件权限异常: {}",
                    "Security hardening failed: invalid audio file permissions: {}",
                    e
                ),
                false,
            );
            return Ok(());
        }

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
