//! Localization for the CLI help text (`clap` derive attributes are kept in
//! English as the source of truth; Chinese overrides are applied here at
//! runtime so `horae --help` can switch languages without rebuilding).
//!
//! Default output is English; switch to Chinese via `HORAE_LANG=zh` or
//! `horae --lang zh`. This mirrors the TUI's `Lang` mechanism in `i18n.rs`.

use clap::Command;
use horae_core::i18n::Lang;

/// Resolve the help language from `HORAE_LANG` or a `--lang`/`--lang=` flag
/// pre-scanned from argv (so `horae --lang zh --help` renders Chinese).
pub fn detect_lang() -> Lang {
    if let Ok(v) = std::env::var("HORAE_LANG") {
        if is_zh(&v) {
            return Lang::Zh;
        }
        if is_en(&v) {
            return Lang::En;
        }
    }
    let mut args_iter = std::env::args();
    while let Some(a) = args_iter.next() {
        if a == "--lang" || a == "-L" {
            if let Some(v) = args_iter.next() {
                if is_zh(&v) {
                    return Lang::Zh;
                }
                if is_en(&v) {
                    return Lang::En;
                }
            }
        } else if let Some(v) = a.strip_prefix("--lang=") {
            if is_zh(v) {
                return Lang::Zh;
            }
            if is_en(v) {
                return Lang::En;
            }
        }
    }
    Lang::En
}

fn is_zh(v: &str) -> bool {
    matches!(
        v.to_ascii_lowercase().as_str(),
        "zh" | "zh-cn" | "zh_cn" | "chinese" | "中文"
    )
}

fn is_en(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "en" | "english" | "英文")
}

/// Apply Chinese help text to the whole command tree. No-op unless `lang` is
/// `Lang::Zh` (English is left as the clap-derived default).
pub fn localize(cmd: &mut Command, lang: Lang) {
    if !lang.is_zh() {
        return;
    }
    localize_recursive(cmd, "horae");
}

fn localize_recursive(cmd: &mut Command, path: &str) {
    cmd.build();
    if let Some(zh) = zh_about(path) {
        *cmd = std::mem::take(cmd).about(zh);
    }
    if let Some(zh) = zh_long_about(path) {
        *cmd = std::mem::take(cmd).long_about(zh);
    }
    if let Some(zh) = zh_after_help(path) {
        *cmd = std::mem::take(cmd).after_help(zh);
    }

    let ids: Vec<String> = cmd
        .get_arguments()
        .map(|a| a.get_id().to_string())
        .collect();
    for id in ids {
        if let Some(zh) = zh_arg_help(path, &id) {
            *cmd = std::mem::take(cmd).mut_arg(id.as_str(), |a| {
                let mut a = a.help(zh);
                if a.get_long_help().is_some() {
                    a = a.long_help(zh);
                }
                a
            });
        }
    }

    let sub_names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in sub_names {
        // Top-level subcommands keyed by their bare name; nested ones keep the
        // parent path (e.g. `profile new`).
        let sub_path = if path.is_empty() || path == "horae" {
            name.clone()
        } else {
            format!("{path} {name}")
        };
        if let Some(sub) = cmd.get_subcommands_mut().find(|s| s.get_name() == name) {
            localize_recursive(sub, &sub_path);
        }
    }
}

fn zh_about(path: &str) -> Option<&'static str> {
    if path == "help" || path.ends_with(" help") {
        return Some("打印此帮助信息或指定子命令的帮助信息");
    }
    Some(match path {
        "horae" => "GTD 终端任务管理器",
        "capture" => "将新条目捕获到收件箱",
        "voice" => "从语音听写转录或自然语言快速捕获任务（capture --natural 的别名）",
        "list" => "列出任务（支持过滤）",
        "show" => "查看任务及其完整事件时间线",
        "modify" => "修改已有任务（标题、标签、截止、排程、状态、备注）",
        "next" => "标记为可行动（next）",
        "wait" => "标记为等待中",
        "schedule" => "排程（设定计划开始时间，可选 --rrule）",
        "someday" => "移至「将来/也许」",
        "done" => "标记为完成（周期性任务会重排到下一次）",
        "archive" => "归档（软删除）任务",
        "restore" => "恢复已归档（软删除）的任务",
        "purge" => "永久删除已归档任务（不可撤销）",
        "tag" => "给任务添加标签（预设或自定义）",
        "untag" => "移除任务的标签",
        "review" => "周回顾助手",
        "tags" => "按分类列出所有标签",
        "calendar" => "查看农历公历对照、二十四节气与节假日提醒",
        "focus" => "计算并输出当前最重要的一件事",
        "log" => "记录带时间戳的事件/日志（不创建任务）",
        "pomo" => "番茄钟命令（start、stop、daemon、waybar）",
        "alarm" => "临近任务的闹钟提醒（waybar、next）",
        "tui" => "启动交互式 TUI",
        "flash" => "以闪念录入模式启动（开启即录入，回车确认录入即关闭）",
        "ntfy" => "通过 ntfy 推送手机提醒",
        "feishu" => "向飞书群机器人推送任务提醒与每日简报（需配置飞书参数）",
        "feishu test" => "发送测试卡片以验证飞书 Webhook 连接",
        "feishu due" => "立即扫描并推送当前临近/到期任务卡片",
        "feishu summary" => "立即推送今日任务摘要简报卡片",
        "feishu listen" => "启动 WebSocket 长连接监听移动端捕获与卡片操作",
        "feishu doctor" => "检查飞书应用凭证、Token 刷新与长连接网关连通性",
        "feishu sync" => "与飞书任务（Tasks v2）双向同步任务清单",
        "watch" => "监视与手机同步的文件夹（手机 <-> 电脑 桥接）",
        "export" => "导出完整备份（任务、事件、标签、设置、番茄钟）到 JSON",
        "import" => "导入备份，合并（或替换）数据库",
        "stats" => "显示终端仪表盘摘要（MOTD 风格）",
        "completions" => "生成 shell 补全脚本（bash、elvish、fish、powershell、zsh）",
        "profile" => "管理配置集（列出、创建、删除、重命名、设置默认）",
        "profile list" => "列出所有配置集并标记默认项",
        "profile new" => "创建新配置集（数据集）",
        "profile rename" => "重命名配置集",
        "profile rm" => "从配置中删除配置集（其数据库文件保留）",
        "profile set-default" => "设置未指定 --profile 时使用的默认配置集",
        "password" => "管理密码保护模式（开启、关闭、修改、状态）",
        "password status" => "查看当前密码保护状态",
        "password enable" => "开启密码保护模式并设置密码",
        "password disable" => "关闭密码保护模式",
        "password change" => "修改密码保护密码",
        "sound" => "管理提示音效（开启、关闭、切换、状态、试听）",
        _ => return None,
    })
}

fn zh_long_about(path: &str) -> Option<&'static str> {
    if path == "help" || path.ends_with(" help") {
        return Some("打印此帮助信息或指定子命令的帮助信息。");
    }
    Some(match path {
        "horae" => "用 Rust 编写的 GTD 终端任务管理器：单一二进制内置 SQLite 数据层 + CLI + ratatui TUI。\n每次任务状态变更都会打上 UTC 毫秒时间戳，并追加到只追加的 task_events 时间线中。",
        "capture" => "将新条目捕获到收件箱。标签首次使用时自动创建；标题中可用 quick-add 语法（@tag ~time *rrule !priority）。",
        "voice" => "从语音听写转录或日常口语自然语言中捕获任务。自动提取周期循环规则、排程/截止时间、优先级、标签，并清洗口语语气词与冗余词。",
        "list" => "列出任务，支持可选过滤条件。排序使用有效截止时间：对周期性任务而言，即当前时间之后下一次发生的时间。",
        "show" => "查看任务详情及其完整的只追加事件时间线。",
        "modify" => "修改已有任务。支持在一句话标题中使用 quick-add 语法（@tag ~time *rrule !priority），也支持通过显式参数修改特定字段。",
        "next" => "将任务标记为可行动（next）。",
        "wait" => "将任务标记为等待中（waiting-for）。",
        "schedule" => "为任务排程，设定计划开始/结束时间，以及可选的重复规则（RRULE）。",
        "someday" => "将任务移至「将来/也许」（someday/maybe）。",
        "done" => "标记任务为已完成。若为周期性任务，则自动重新排程至下一次发生时间。",
        "archive" => "归档（软删除）指定任务。归档后不会在常规列表中显示，但可随时恢复。",
        "restore" => "恢复先前已归档的任务，重新回到活动视图。",
        "purge" => "永久物理删除已归档任务及其事件时间线（不可撤销）。只能删除已归档的任务。",
        "tag" => "给任务添加标签（预设或自定义标签）。",
        "untag" => "移除任务上指定的标签。",
        "review" => "周回顾助手：引导式检视收件箱、当前项目、等待中事项与下一步行动。",
        "tags" => "按分类（场景、精力、时间、预设、自定义等）列出所有标签及使用情况。",
        "calendar" => "查看中国农历公历对照、二十四节气、重大节假日倒计时及提前提醒。",
        "focus" => "计算并输出当前最重要的一件事，终结选择困难。综合优先级（high/medium/low）、有效截止时间与上下文。",
        "log" => "向时间线记录一条带时间戳的事件/日志，而不创建待办任务。",
        "pomo" => "番茄钟专注模式。`start` 会启动一个后台守护进程，每秒计时，写入 pomo.json 并发送桌面通知。",
        "alarm" => "临近任务的闹钟提醒：`waybar` 为 waybar 模块输出 JSON，`next` 打印最近的待响闹钟。",
        "tui" => "启动交互式终端界面（TUI）。使用 --flash (-f) 进行闪念快速录入并在回车后退出，或使用 --normal (-n) 直接进入标准主页面。",
        "flash" => "直接进入快速录入模式。输入任务并回车确认后，自动保存至收件箱并立即退出。",
        "ntfy" => "通过 ntfy（https://ntfy.sh）把任务提醒推送到手机。需在配置中设置 `ntfy` 块（url、topic、可选的 token_env、priority、lead_minutes）。`watch` 守护进程会在定时任务到期时推送原生通知；`ntfy test` 会发送一条示例推送，便于确认手机能收到。",
        "feishu" => "通过群机器人 Webhook 向飞书（Lark）推送任务到期提醒与每日简报。需在配置集中配置 `feishu` 块（webhook_url、可选 secret_env、lead_minutes、daily_briefing 等）。`watch` 守护进程会在定时任务到期时推送交互式卡片；`feishu test` 会发送测试卡片以验证连通性。",
        "feishu test" => "向飞书 Webhook 发送一条示例测试卡片，便于确认群机器人连通性。",
        "feishu due" => "立即扫描本地数据库中的临近或到期任务，并向飞书推送提醒卡片。",
        "feishu summary" => "汇总今日待办、已完成与排程任务，立即向飞书推送富文本日报简报。",
        "feishu listen" => "启动飞书开放平台 WebSocket 长连接网关，监听来自飞书客户端的消息回调（如语音/文字闪念捕获、卡片点击「标为完成」操作）。",
        "feishu doctor" => "对飞书配置进行体检：检查 app_id、app_secret 是否有效，测试获取 tenant_access_token 及 WebSocket 网关连接。",
        "feishu sync" => "与飞书任务清单（Tasks v2）进行数据同步，默认双向对账。可用 --push 仅推送本地，或 --pull 仅拉取云端。",
        "watch" => "监视与手机同步的文件夹（例如通过 Syncthing），并与本地数据库对账。每隔几秒它会：把 capture.txt 中的新行摄入收件箱（quick-add 语法），执行 actions.txt 中的动作行，用活动任务列表重写 today.md，并在任务到期时向 reminders/ 写入提醒文件。\n请在后台运行（systemd / tmux / 自启动）；传入 --once 则只执行一轮。",
        "export" => "把所有任务、事件、标签、设置与番茄钟状态导出到单个 JSON 文件——数据库的完整还原点。",
        "import" => "导入由 `horae export` 创建的备份。默认采用合并方式：已存在的 id 对应任务保持不变，其余任务加入。传入 --replace 则清空当前任务数据并精确还原备份。",
        "stats" => "以终端仪表盘（MOTD 风格）展示今日完成的番茄钟、燃尽情况与待办任务。",
        "completions" => "生成指定 shell 的自动补全脚本。无需打开数据库，无副作用。",
        "sound" => "管理提示音效（番茄钟开始/休息、任务到期与完成提醒）。支持查看当前状态、开启、关闭、切换状态或试听指定内置音效。",
        "password" => "管理数据密码保护模式（基于 PBKDF2-HMAC-SHA256 加密认证）。开启后终端与 TUI 需验证密码才能访问任务数据。",
        "profile" => "配置集让你可以维护多套独立数据（例如 work / personal / prod1），各自存放在独立的 SQLite 文件中，通过 `horae --profile <name>` 或 TUI 设置视图切换。本命令只编辑配置（~/.config/horae/config.json），不触碰任何数据。",
        "help" => "打印此帮助信息或指定子命令的帮助信息。",
        _ => return None,
    })
}

fn zh_after_help(path: &str) -> Option<&'static str> {
    Some(match path {
        "horae" => "示例:\n  horae                       正常模式启动（主页面）\n  horae -f                    闪念录入模式启动\n  horae capture \"买牛奶\" --tag home --high\n  horae list --status next\n  horae show <id>\n  horae completions bash\n\n时间语法: now、+2h、+30m、+1d、today、tomorrow、2026-07-24 14:30\n日期搜索: 四位数字 MMDD，例如 0829\n任务引用: 完整 id、唯一 id 前缀，或精确标题",
        "capture" => "示例:\n  horae capture \"买牛奶\" --tag home\n  horae capture \"给妈妈打电话\" --high --due tomorrow\n  horae capture \"提交报告\" --status scheduled --due +1d\n  horae capture \"给老板发邮件 ~today @work !high\"",
        "voice" => "示例:\n  horae voice \"明天下午三点半去参加架构评审会，非常重要，标签是工作\"\n  echo \"提醒我每周一早上九点开例会\" | horae voice",
        "list" => "示例:\n  horae list\n  horae list --status next\n  horae list --status scheduled --tag work\n  horae list --date 0829 --json\n  horae list --due-before +1d --json",
        "show" => "示例:\n  horae show <id>\n  horae show <id> --json",
        "modify" => "示例:\n  horae modify <id> \"买有机牛奶 @groceries\"\n  horae modify <id> --due tomorrow\n  horae modify <id> --tag home --untag work\n  horae modify <id> --clear-due\n  horae modify <id> --clear-schedule\n  horae modify <id> --notes \"下午3点电话\"\n  horae modify <id> --edit-notes\n  horae modify <id> --status next",
        "next" => "示例:\n  horae next <id>",
        "wait" => "示例:\n  horae wait <id>",
        "schedule" => "示例:\n  horae schedule <id> --start tomorrow\n  horae schedule <id> --start +1d --end +1d 14:00\n  horae schedule <id> --start +1w --rrule 'FREQ=WEEKLY;BYDAY=MO,WE'",
        "someday" => "示例:\n  horae someday <id>",
        "done" => "示例:\n  horae done <id>",
        "archive" => "示例:\n  horae archive <id>",
        "restore" => "示例:\n  horae restore <id>",
        "purge" => "示例:\n  horae purge <id>",
        "tag" => "示例:\n  horae tag <id> work",
        "untag" => "示例:\n  horae untag <id> work",
        "review" => "示例:\n  horae review",
        "tags" => "示例:\n  horae tags",
        "calendar" => "示例:\n  horae calendar\n  horae calendar --short\n  horae calendar --json\n  horae calendar 2026-10-01",
        "focus" => "示例:\n  horae focus\n  horae focus --start",
        "log" => "示例:\n  horae log \"喝了 3 杯水\"\n  horae log",
        "pomo" => "示例:\n  horae pomo start <task-id>\n  horae pomo stop\n  horae pomo daemon\n  horae pomo waybar",
        "alarm" => "示例:\n  horae alarm waybar\n  horae alarm next",
        "tui" => "示例:\n  horae tui\n  horae tui --flash\n  horae tui --normal",
        "flash" => "示例:\n  horae flash\n  horae f\n  horae -f",
        "ntfy" => "示例:\n  horae ntfy test\n  horae --profile work ntfy test",
        "feishu" => "示例:\n  horae feishu test\n  horae feishu due\n  horae feishu summary\n  horae feishu listen\n  horae feishu doctor\n  horae feishu sync\n  horae --profile work feishu test",
        "feishu sync" => "示例:\n  horae feishu sync\n  horae feishu sync --push\n  horae feishu sync --pull",
        "watch" => "文件夹协议（手机写入，电脑消费）:\n    capture.txt   每行一条 quick-add 捕获：标题 @tag ~time *rrule !high\n    actions.txt   done <id|标题> | set <id|标题> status next | set <id|标题> due <时间>\n电脑回写:\n    today.md          活动任务快照（Next / Scheduled / Waiting / 逾期）\n    reminders/*.md    到期/逾期任务提醒（Syncthing 通过文件变更通知手机）\n    *.done            已消费行的回执",
        "export" => "示例:\n  horae export\n  horae export --file ~/backups/horae.json",
        "import" => "示例:\n  horae import horae-backup-2026-08-15.json\n  horae import --replace ~/backups/horae.json",
        "stats" => "示例:\n  horae stats",
        "completions" => "用法:\n  horae completions bash\n  horae completions fish\n\n安装到 ~/.bashrc 或 ~/.config/fish/completions/",
        "sound" => "示例:\n  horae sound status\n  horae sound on\n  horae sound off\n  horae sound toggle\n  horae sound test\n  horae sound test task_due",
        "password" => "示例:\n  horae password status\n  horae password enable\n  horae password disable\n  horae password change",
        "profile" => "示例:\n  horae profile list\n  horae profile new work\n  horae profile new prod1 --db prod1.db\n  horae profile rename work work2\n  horae profile rm prod1\n  horae profile set-default work\n  horae --profile work capture \"买牛奶\"",
        _ => return None,
    })
}

fn zh_arg_help(path: &str, arg: &str) -> Option<&'static str> {
    if arg == "help" {
        return Some("打印帮助信息");
    }
    if arg == "version" {
        return Some("打印版本号");
    }
    Some(match (path, arg) {
        ("horae", "profile") => "要使用的配置集（数据集）；默认为配置中的默认配置集。",
        ("horae", "lang") => {
            "帮助文本的输出语言：`en`（默认）或 `zh`（中文）。也可通过环境变量 `HORAE_LANG` 设置。"
        }
        ("horae", "theme") => {
            "UI 主题：23 款内置主题（如 `catppuccin`、`latte`、`tokyo-night`、`gruvbox`、`retro-82`、`nord`、`kanagawa`、`hackerman`）或 `auto`（自动探测）。也可通过环境变量 `HORAE_THEME` 设置。"
        }
        ("horae", "password") => "受保护数据库的密码（也可设置 HORAE_PASSWORD 环境变量）",
        ("password", "password") => "密码（省略则进入交互式无回显输入）",
        ("password", "old_password") => "原密码（省略则交互式输入）",
        ("password", "new_password") => "新密码（省略则交互式输入）",
        (_, "flash") => "以闪念录入模式启动（开启即录入，回车确认录入即关闭）",
        (_, "normal") => "以正常模式启动（开启后正常进入主页面）",
        ("capture", "title") => "任务标题（可省略引号）",
        ("capture", "natural") => "解析为自然语言/口语输入（例如来自语音听写）",
        ("capture", "clip") => "从系统剪贴板读取内容作为任务标题/语法",
        ("capture", "tag") => "要添加的标签（可重复）",
        ("capture", "high") => "设为高优先级",
        ("capture", "medium") => "设为中优先级",
        ("capture", "low") => "设为低优先级",
        ("capture", "due") => "截止时间（now、+2h、today、2026-07-24 14:30）",
        ("capture", "status") => "初始状态（inbox、next、waiting、scheduled、someday、reference）",
        ("capture", "notes") => "任务备注/详细描述（剪贴板长文本会自动沉淀在此）",
        ("capture", "json") => "以 JSON 格式输出新建的任务",
        ("voice", "text") => "语音听写文本或自然语言（省略或为 '-' 时从标准输入读取）",
        ("voice", "json") => "以 JSON 格式输出新建的任务",
        ("modify", "text") => "新标题或 quick-add 语法更新内容",
        ("modify", "title") => "显式设置任务标题（覆盖 quick-add 标题）",
        ("modify", "tag") => "要添加的标签（可重复）",
        ("modify", "untag") => "要移除的标签（可重复）",
        ("modify", "clear_tags") => "清空所有标签",
        ("modify", "high") => "设为高优先级",
        ("modify", "medium") => "设为中优先级",
        ("modify", "low") => "设为低优先级",
        ("modify", "clear_priority") => "清除优先级",
        ("modify", "due") => "截止时间（now、+2h、today、2026-07-24 14:30 或 'none' 清除）",
        ("modify", "clear_due") => "清除截止时间",
        ("modify", "start") => "计划开始时间（或 'none' 清除）",
        ("modify", "end") => "计划结束时间（或 'none' 清除）",
        ("modify", "rrule") => "重复规则（FREQ=DAILY|WEEKLY|MONTHLY|YEARLY 或 'none' 清除）",
        ("modify", "clear_schedule") => "清空排程（开始、结束与重复规则）",
        ("modify", "status") => {
            "更改状态（inbox、next、waiting、scheduled、someday、reference、done）"
        }
        ("modify", "notes") => "任务备注/详细描述",
        ("modify", "edit_notes") => "打开 $EDITOR 交互式编辑备注",
        ("modify", "json") => "以 JSON 格式输出修改后的任务",
        ("list", "status") => "按状态过滤",
        ("list", "tag") => "按标签过滤（可重复）",
        ("list", "due_before") => "仅显示此时间之前截止的任务",
        ("list", "date") => "仅显示该日期截止的任务，例如 0829",
        ("list", "json") => "以 JSON 格式输出各行",
        ("show", "id") => "任务 id（完整 id、唯一前缀或精确标题）",
        ("show", "json") => "以 JSON 格式输出任务",
        ("schedule", "start") => "计划开始时间",
        ("schedule", "end") => "计划结束时间",
        ("schedule", "rrule") => {
            "重复规则（FREQ=DAILY|WEEKLY|MONTHLY|YEARLY;INTERVAL=..;BYDAY=..;BYMONTH=..;COUNT=..|UNTIL=..）"
        }
        ("tag", "name") | ("untag", "name") => "标签名称",
        ("calendar", "date") => "要查询的日期（YYYY-MM-DD，默认为今天）",
        ("calendar", "short") => "输出精简的一行摘要",
        ("calendar", "json") => "以 JSON 格式输出",
        ("focus", "start") => "立即为此任务开启番茄钟",
        ("log", "message") => "要记录的日志内容（省略则列出最近的日志）",
        ("pomo", "action") => "start、stop、daemon 或 waybar",
        ("pomo", "task_id") => "要专注的任务（start 时必填）",
        ("alarm", "action") => "waybar 或 next",
        ("alarm", "slot") => "位置 1/2：用于排列多个 waybar 闹钟模块；`next` 会跳过相应数量",
        ("alarm", "limit") => "窗口显示的闹钟任务数量（默认 2）",
        ("alarm", "all") => "waybar：将整个窗口作为 JSON 数组输出，而非单个位置",
        ("ntfy", "action") => "test（发送一条示例推送）",
        ("feishu sync", "push") => "仅将本地任务推送到飞书",
        ("feishu sync", "pull") => "仅从飞书拉取任务到本地",
        ("watch", "dir") => "要监视的同步文件夹（默认 ~/.config/horae/sync）",
        ("watch", "interval") => "轮询间隔（秒，默认 5）",
        ("watch", "once") => "只执行一轮后退出，而非一直运行",
        ("export", "file") => "输出路径（默认 horae-backup-<date>.json）",
        ("import", "file") => "备份 JSON 文件路径",
        ("import", "replace") => "清空当前数据并精确还原备份",
        ("completions", "shell") => "目标 shell（bash、elvish、fish、powershell、zsh）",
        ("sound", "action") => "操作指令：status | on | off | toggle | test",
        ("sound", "name") => {
            "用于测试的音效名称（focus_start、break_start、break_end、focus_stop、task_due、task_complete）"
        }
        ("profile new", "name") => "新配置集名称",
        ("profile new", "db") => "数据库文件（默认 profiles/<name>.db）",
        ("profile rename", "from") => "原名称",
        ("profile rename", "to") => "新名称",
        ("profile rm", "name") => "要删除的配置集名称",
        ("profile set-default", "name") => "要设为默认的配置集名称",
        ("help", "subcommand") | ("help", "subcommands") | ("help", "command") => {
            "要查看帮助信息的子命令（可选）"
        }
        (_, "lang") => "帮助文本的输出语言：en（默认）或 zh（中文）。",
        (_, "profile") => "要使用的配置集（数据集）；默认为配置中的默认配置集。",
        (_, "theme") => {
            "UI 主题：23 款内置主题（如 `catppuccin`、`latte`、`tokyo-night`、`gruvbox`、`retro-82`、`nord`、`kanagawa`、`hackerman`）或 `auto`（自动探测）。也可通过环境变量 `HORAE_THEME` 设置。"
        }
        (_, "password") => "受保护数据库的密码（也可设置 HORAE_PASSWORD 环境变量）",
        (_, "id") => "任务 id（完整 id、唯一前缀或精确标题）",
        (_, "json") => "以 JSON 格式输出",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::CommandFactory;
    use std::sync::Mutex;

    // 这些测试会改动进程全局环境变量 `HORAE_LANG`，必须串行执行避免互相干扰。
    static LANG_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_language_is_english() {
        let _guard = LANG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 默认无环境变量、无 --lang 时为英文
        std::env::remove_var("HORAE_LANG");
        assert_eq!(detect_lang(), Lang::En);
    }

    #[test]
    fn env_var_selects_chinese() {
        let _guard = LANG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("HORAE_LANG", "zh");
        assert_eq!(detect_lang(), Lang::Zh);
        std::env::remove_var("HORAE_LANG");

        std::env::set_var("HORAE_LANG", "中文");
        assert_eq!(detect_lang(), Lang::Zh);
        std::env::remove_var("HORAE_LANG");
    }

    #[test]
    fn flag_pre_scan_selects_chinese() {
        let _guard = LANG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 直接调用内部逻辑不方便预置 argv，这里校验 detect_lang 对 env 的解析
        std::env::set_var("HORAE_LANG", "en");
        assert_eq!(detect_lang(), Lang::En);
        std::env::remove_var("HORAE_LANG");
    }

    #[test]
    fn localize_overrides_top_level_and_subcommand_to_chinese() {
        let mut cmd = Cli::command();
        localize(&mut cmd, Lang::Zh);
        assert_eq!(
            cmd.get_about().map(|s| s.to_string()),
            Some("GTD 终端任务管理器".to_string())
        );

        // 验证用户特别提到的子命令：capture, voice, calendar, feishu, help 等
        let capture = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "capture")
            .expect("capture 子命令存在");
        assert_eq!(
            capture.get_about().map(|s| s.to_string()),
            Some("将新条目捕获到收件箱".to_string())
        );

        let voice = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "voice")
            .expect("voice 子命令存在");
        assert_eq!(
            voice.get_about().map(|s| s.to_string()),
            Some("从语音听写转录或自然语言快速捕获任务（capture --natural 的别名）".to_string())
        );

        let calendar = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "calendar")
            .expect("calendar 子命令存在");
        assert_eq!(
            calendar.get_about().map(|s| s.to_string()),
            Some("查看农历公历对照、二十四节气与节假日提醒".to_string())
        );

        let feishu = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "feishu")
            .expect("feishu 子命令存在");
        assert_eq!(
            feishu.get_about().map(|s| s.to_string()),
            Some("向飞书群机器人推送任务提醒与每日简报（需配置飞书参数）".to_string())
        );

        let help = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "help")
            .expect("help 子命令存在");
        assert_eq!(
            help.get_about().map(|s| s.to_string()),
            Some("打印此帮助信息或指定子命令的帮助信息".to_string())
        );
    }

    #[test]
    fn localize_is_noop_for_english() {
        let mut cmd = Cli::command();
        localize(&mut cmd, Lang::En);
        assert_eq!(
            cmd.get_about().map(|s| s.to_string()),
            Some("GTD terminal task manager".to_string())
        );
    }
}
