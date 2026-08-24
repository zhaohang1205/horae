//! 图标系统：Nerd Font 字形 + 纯 ASCII 回退。
//!
//! 选择顺序：环境变量 `HORAE_ICONS`（nerd|ascii）→ settings 表 `icons` 键
//! → 自动探测（`fc-list` 输出含 "nerd" 即认为装了 Nerd Font）。
//! 探测失败（无 fontconfig / 非桌面环境）一律回退 ASCII，保证不出现豆腐块。

use rusqlite::Connection;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconStyle {
    Nerd,
    Ascii,
}

/// 所有会出现在界面上的图标种类。ASCII 回退全部是宽度为 1 的字符，
/// 与 `visual_len` 的对齐假设保持一致。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Inbox,
    Today,
    Tomorrow,
    Next,
    Waiting,
    Scheduled,
    Someday,
    Reference,
    Done,
    Review,
    Archived,
    Tags,
    Quotes,
    Settings,
    /// 当前选中行的前缀标记（Nerd: 󰄾）
    Active,
    GroupDay,
    GroupActive,
    GroupWaiting,
    GroupArchive,
    GroupModules,
    GroupKeys,
    /// 番茄钟计数
    Tomato,
    /// 每日成就结清标记
    Achievement,
}

impl IconStyle {
    /// env 覆盖 > settings 持久化值 > 自动探测。
    pub fn load(conn: &Connection) -> Self {
        match std::env::var("HORAE_ICONS").as_deref() {
            Ok("nerd") => return Self::Nerd,
            Ok("ascii") => return Self::Ascii,
            _ => {}
        }
        if let Ok(Some(v)) = crate::repo::settings::get(conn, "icons") {
            match v.as_str() {
                "nerd" => return Self::Nerd,
                "ascii" => return Self::Ascii,
                _ => {}
            }
        }
        if nerd_font_detected() {
            Self::Nerd
        } else {
            Self::Ascii
        }
    }

    /// 写回 settings 表的规范值。
    pub fn key(self) -> &'static str {
        match self {
            Self::Nerd => "nerd",
            Self::Ascii => "ascii",
        }
    }
}

fn nerd_font_detected() -> bool {
    // fc-list 不存在 / 无法 spawn（Windows、精简容器等）时静默回退 ASCII。
    match std::process::Command::new("fc-list").output() {
        Ok(out) => {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .to_ascii_lowercase()
                    .contains("nerd")
        }
        Err(_) => false,
    }
}

/// 返回某图标在指定风格下的字形。Nerd 列沿用原 render.rs 的私有区字形；
/// ASCII 列只用纯 ASCII，保证任何字体都不缺字且视觉宽度恒为 1。
pub fn glyph(kind: Icon, style: IconStyle) -> &'static str {
    use Icon::*;
    match style {
        IconStyle::Nerd => match kind {
            Inbox => "\u{f01c}",
            Today => "\u{f005}",
            Tomorrow => "\u{f133}",
            Next => "\u{f0a9}",
            Waiting => "\u{f252}",
            Scheduled => "\u{f073}",
            Someday => "\u{f0eb}",
            Reference => "\u{f02d}",
            Done => "\u{f058}",
            Review => "\u{f021}",
            Archived => "\u{f187}",
            Tags => "\u{f02b}",
            Quotes => "\u{f10d}",
            Settings => "\u{f013}",
            Active => "\u{f013e}",
            GroupDay => "\u{f017}",
            GroupActive => "\u{f192}",
            GroupWaiting => "\u{f10c}",
            GroupArchive => "\u{f187}",
            GroupModules => "\u{f009}",
            GroupKeys => "\u{f0eb}",
            Tomato => "\u{f2f2}",
            Achievement => "\u{f05e0}",
        },
        IconStyle::Ascii => match kind {
            Inbox => "I",
            Today => "T",
            Tomorrow => "t",
            Next => "N",
            Waiting => "W",
            Scheduled => "S",
            Someday => "?",
            Reference => "R",
            Done => "X",
            Review => "^",
            Archived => "#",
            Tags => "@",
            Quotes => "\"",
            Settings => "*",
            Active => ">",
            GroupDay | GroupActive | GroupWaiting | GroupArchive | GroupModules | GroupKeys => "",
            Tomato => "o",
            Achievement => "!",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_fallback_is_single_width_ascii() {
        let kinds = [
            Icon::Inbox,
            Icon::Today,
            Icon::Tomorrow,
            Icon::Next,
            Icon::Waiting,
            Icon::Scheduled,
            Icon::Someday,
            Icon::Reference,
            Icon::Done,
            Icon::Review,
            Icon::Archived,
            Icon::Tags,
            Icon::Quotes,
            Icon::Settings,
            Icon::Active,
            Icon::Tomato,
            Icon::Achievement,
        ];
        for k in kinds {
            let g = glyph(k, IconStyle::Ascii);
            assert!(g.is_ascii(), "{k:?} 回退应全 ASCII，实际 {g:?}");
            assert_eq!(g.chars().count(), 1, "{k:?} 回退应为单字符，实际 {g:?}");
        }

        // 视图图标彼此可区分（大小写也算不同，如 Today=T / Tomorrow=t）
        let mut glyphs: Vec<_> = kinds.iter().map(|k| glyph(*k, IconStyle::Ascii)).collect();
        let n = glyphs.len();
        glyphs.sort_unstable();
        glyphs.dedup();
        assert_eq!(glyphs.len(), n, "视图 ASCII 回退图标不得重复");
    }

    #[test]
    fn group_prefixes_are_empty_in_ascii_mode() {
        for k in [
            Icon::GroupDay,
            Icon::GroupActive,
            Icon::GroupWaiting,
            Icon::GroupArchive,
            Icon::GroupModules,
            Icon::GroupKeys,
        ] {
            assert_eq!(glyph(k, IconStyle::Ascii), "");
        }
    }

    #[test]
    fn style_key_round_trips() {
        assert_eq!(IconStyle::Nerd.key(), "nerd");
        assert_eq!(IconStyle::Ascii.key(), "ascii");
    }
}
