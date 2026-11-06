# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.6] - 2026-11-06

### Security

- **语音录制临时文件与目录隔离防护（Voice Capture Security Hardening）**：
  - 强制语音临时目录（`/tmp/horae-<uid>`）权限为 `0700`，严格校验属主为当前有效用户（EUID），严厉拒绝攻击者预先创建的同名非属主目录或软链接。
  - 递归遍历校验目录父链权限（reject symlinks, non-sticky world-writable directories and untrusted UIDs）。
  - 音频 WAV 文件采用 `O_CREAT | O_EXCL | O_NOFOLLOW` 模式原子创建，权限设为 `0600` 并使用随机 UUID 命名。
  - 在唤起录音器（`pw-record` / `arecord`）进程时通过 `/proc/self/fd/<fd>` 保留文件标识（`dev`, `ino`），并在 `pre_exec`、进程启动后及录制完成后多阶段校验目录与文件防篡改。

### Added

- **以用户为中心的操作反馈体系（User Feedback Principle）**：
  - TUI 状态提示全面引入 `ToastKind` 级别体系（`Success` / `Info` / `Warning` / `Error`），严重错误警告（`Error`）保持常驻直至下次按键，避免瞬时闪烁遗漏重要故障。
  - 录入与排程（Capture / Schedule）操作保持视图连贯，显式展示任务流转目的地与智能解析出的具体时间。
  - 对排程到过去时间的任务主动发出警示，在列表为空时提供上下文操作引导。
  - CLI `modify` / `schedule` / `done` 全面增强时间解析回显与周期性习惯打卡状态确认。
- **命令行完整国际化支持**：
  - 补全 `voice`、`calendar`、`feishu`、`password` 等子命令与参数的中文本地化帮助信息（`cli_i18n.rs`）。

### Fixed

- **TUI 撤销/重做防崩溃保护**：
  - 修复在任务归档、清除后撤销/重做极端情况下可能引发的崩溃隐患，将致命退出降级为友好 Toast 提醒，确保主事件循环稳健运行。

## [0.2.5] - 2026-10-24

### Added

- **英文自然语言/语音捕获全面支持（English Natural Language & Voice Capture）**：
  - 核心解析器 (`natural.rs`) 完整支持英文口语输入，实现自然语言到 GTD 任务的零门槛转录。
  - 严格词界保护 (`find_word_boundary`)，避免 ASCII 关键字在复合英文词中造成误触发。
  - 支持 0~99 英文数字与 1st~31st 序数词、英文优先级关键词识别（`urgent`, `critical`, `asap`, `p1/p2/p3` 等）。
  - 支持英文标签显式提取与五大多场景语义智能推断（`work`, `errands`, `calls`, `learning`, `home`）。
  - 支持丰富多样的英文循环规则（`every day`, `every weekday`, `every N days`, `every Monday`, `every month on the 1st` 等）。
  - 支持英文相对时间（`in 30 minutes`, `in 2 weeks`）、日历时刻（`tomorrow at 3pm`, `by Friday at 6pm`, `noon`, `midnight`）与口语噪音清洗。
  - 时间解析引擎新增 `"day after tomorrow"`（后天）支持。

### Fixed

- **修复中文口语时钟后缀误判**：
  - 修复语音记录如 `"每天早晨九点钟吃饭"` 时误将口语后缀 `"钟"` 留在标题并解析为 `"钟吃饭"` 的 bug，同时对 `"钟楼"`、`"钟表"` 等真实名词设置复合词防吞保护。

## [0.2.4] - 2026-10-22

### Added

- **密码安全模式（Password Protection）**：
  - 新增基于 PBKDF2-HMAC-SHA256（50,000 次安全迭代与随机 16 字节盐）的数据加密保护机制。
  - TUI 启动时自动锁屏，支持快捷键 `L` 随手锁定，设置视图 (`M` -> `p`) 与 `F7` 开关支持密码启用/修改/停用。
  - CLI 全面适配密码鉴权，支持 `horae password [enable|disable|change|status]` 子命令，提供 `--password` 参数及 `HORAE_PASSWORD` 环境变量。
- **快速录入沉浸无打扰模式（Distraction-Free Quick Capture）**：
  - 在快速录入界面激活时（`Mode::Input`）彻底静默到期通知弹窗与音频提醒，确保用户录入思绪不被任何弹窗和声效打断。
- **声学物理建模语义化音效与设置开关（Acoustic Sound System）**：
  - 引入基于纯数学合成的声学打击乐音效算法（`sound.rs`），彻底取代机械单调蜂鸣，实现多声部温暖和弦与任务状态语义化回响。
  - 设置中心及 SQLite 数据库新增 `sound_enabled` 配置项（默认开启），支持在设置视图使用 `b` 键实时开启/关闭全局提示音。

## [0.2.3] - 2026-10-20

### Added

- **Omarchy 桌面环境与五套核心主题深度适配**：
  - 新增 `Tokyo Night`（东京之夜深色）、`Gruvbox`（复古暖色深色）与 `Retro 82`（复古未来深色）色板，与既有的 `Catppuccin Mocha`（深色）及 `Catppuccin Latte`（浅色）构成五大官方调校主题。
  - 在 Omarchy 环境下自动探测系统当前生效的主题（`~/.local/state/omarchy/current/theme.name` 或 `~/.config/omarchy`），开箱即用，并在运行中实时热重载跟随桌面主题切换。
  - GTD 状态语义色与三级优先级色标全量对接主题色系，避免异色主题下的视觉突兀与对比度缺失。
  - TUI 支持 `F5` 键在全部主题间平滑循环切换与持久化，命令行支持 `--theme <NAME>` 全局参数与 `HORAE_THEME` 环境变量覆盖。
- **循环排程时间预览与通知语义优化**：
  - 录入时实时预览循环排程的下一个发生时隙；
  - 启动与后台通知语义精细化调校。

### Changed

- **文档结构重整**：`docs/` 目录解散，文档全部平铺到仓库根目录——`README.md`（英文手册，crates.io / npm 入口）、`README_CN.md`（中文手册）、`ARCHITECTURE.md`（架构与领域词汇，由原 `architecture-handoff.md` / `CONTEXT.md` / `design-profiles-and-cloud.md` 合并精简）、`CONTRIBUTING.md`（新增贡献流程）、`AGENTS.md`、`CHANGELOG.md`；AI 技能包迁入 `.agents/skills/horae/`。原 `issues.md`（问题均已修复）与 `focus-algorithm.md`（内容并入手册「专注推荐」一节）删除。手册同步去除营销化表述，改为事实性说明。

## [0.2.2] - 2026-10-10

### Fixed

- **修复 `calendar --short` 输出断言的日期依赖型偶发失败**：当日附近无节气/节日时，紧凑单行仅含农历简述一段、不出现 `·` 分隔符，原测试因此偶发失败；改为校验农历简述（必含「月」），并将分隔符 `·` 的校验移到固定日期（中秋节 2026-09-25）。

## [0.2.1] - 2026-09-10

### Performance

- **极速冷启动性能优化（12ms → 2ms）**：
  - **快速字体扫描与持久化**：新增纯文件系统字体目录快速检查（`fast_check_nerd_font`），耗时仅 0.3ms，直接规避耗时 10ms+ 的 `fc-list` 外部子进程开销；首次探测后自动将 `icons: nerd/ascii` 持久化到 `settings` 表，后续启动单点查库耗时 < 5µs。
  - **输入法切换完全异步化**：将 `switch_to_english_ime` 放入后台线程执行，消除外部输入法指令对冷启动首帧渲染的同步阻塞（主线程耗时降至 ~20µs）。
  - **配置项单次批量载入（Batch Query）**：新增 `settings::get_all` 单次 SQL 批量加载所有配置，消除 10+ 次独立点查准备与执行开销。
  - **去除重复加载**：消除 `ModuleVisibility` 重复加载，直接复用已解析语言与图标风格透传给开屏页。

## [0.2.0] - 2026-09-10

### Added

- **独立优先级字段（Priority Column）与快捷语法**：
  - 彻底废弃旧版 `p1/p2/p3` 标签方案，在底层数据表与模型中引入专门的 `tasks.priority` 列（`high`/`medium`/`low`）；
  - CLI 支持 `--high`、`--medium`、`--low` 与 `--clear-priority`，一句话 Quick-Add 语法支持 `!high`、`!medium`、`!low`（及全角标点 `！`）；
  - `horae focus` / `horae do` 专注推荐算法深度升级（结合高优统治级加权、截止时间与创建时间精确定位首要任务）。
- **年循环规则（`FREQ=YEARLY`）与 `BYMONTH` 规范化**：
  - 全面支持年循环 `*y`、`*Ny`、`*y[jan,jul]`（按月循环 `BYMONTH`）；
  - 自动对输入的月份代码升序排序与去重归一化，单调递增展开全部年循环排程槽位；`rrule_valid` 全面放开并统一校验。
- **自动补全即时弹出与双模式切换（语法参考 vs 极速补全）**：
  - 输入 `@`、`~`、`*`、`!` 前缀时即时弹出智能补全卡片（无需手动按 Tab 触发）；
  - 支持**语法参考模式（Reference / Cheat-Sheet Guide，默认）**：三列结构化卡片展示 Token、双语语义说明及语法范式；
  - 支持**极速补全模式（Speed）**：紧凑单列匹配与输入行 Ghost Text 幽灵文本辅助盲打；
  - 在 `F7` 模块设置弹窗中提供第 11 项设置开关，按空格键实时切换并持久化至 SQLite `settings` 表的 `completion_style` 字段。
- **多语言习惯自适应候选与英文星期解析**：
  - 中文模式下剔除与 `today`/`tomorrow` 重复的 `今天`/`明天`/`后天` 候选，保留中英高频词与星期词，同时底层保留全部中文天词解析；
  - 英文模式下时间候选实现 100% 纯 ASCII 英文（`mon`~`sun`, `today`, `tomorrow`, `+1d` 等），绝无中文字符干扰；
  - 时间解析引擎全面支持英文星期词（`mon`~`sun`、`monday`~`sunday`、前缀 `next friday` 以及后接具体时间 `~fri 15:30`）。
- **GTD 核心工作流与心法视图**：完善 GTD 决策树与 David Allen 核心心法引导视图。
- **CLI 任务修改与编辑（`horae modify` / `horae edit`）**：
  - 新增 `horae modify <id>`（别名 `m`、`mod`、`edit`），支持全面修改已有任务的标题、标签、优先级、截止时间、排程、循环规则、状态和备注。
  - 支持直接在一句话中使用 Quick-Add 语法（`@tag`、`~time`、`*rrule`、`!priority`）修改，也支持通过显式参数精细调整（`--title`、`--due`、`--start`、`--rrule`、`--tag`、`--untag`、`--notes` 等）。
  - 支持清理字段（`--clear-due`、`--clear-schedule`、`--clear-tags`、`--clear-priority`，或传 `none` 值）。
  - 支持 `--edit-notes`（`-e`）调起系统 `$EDITOR`（默认 vim）进行交互式长备注编辑。
  - `horae archive` 新增 Unix 习惯别名 `horae rm` 与 `horae delete`。
- **中英文全角/半角标点统一解析支持**：全面打通 `@/＠`、`~/～/〜`、`*/＊/×`、`!/！`、`［ ］`、`【 】`、`：` 等全角标点，中文输入法下无需频繁切换即可无缝使用 Quick-Add 语法与实时 Tab 补全。
- **剪贴板一键瞬时捕获（Clipboard Ingest）**：
  - 新增 `horae c --clip` / `horae capture --clip` 命令，快速从系统剪贴板读取内容落库。
  - 智能拆分规则：短单行文本（$\le 30$ 字）直接作为标题；长文本/多行段落自动提取第一行前 30 个字（附加 `…`）作为标题，全文完整沉淀到 Notes 备注中。
  - 支持 Wayland（`wl-paste`）、X11（`xclip`/`xsel`）与全平台（`arboard`）原生适配。
- **CLI 终端输出表格排版优化**：引入 `comfy-table` 配合 CJK 字符宽度自适应对齐，彻底解决 `horae list` 与 `horae profile list` 等中英文混排、短横线、Emoji 等导致列错位的问题。
- CLI 帮助国际化：所有 `horae --help` 文本默认英文，新增全局 `--lang <en|zh>`
  参数与环境变量 `HORAE_LANG`，可一键切换为中文（与 TUI 的 F6 语言切换一致）；
  英文 clap 派生串保留为事实来源，仅在选择中文时由 `cli_i18n` 运行时覆盖。
- 手机提醒推送（ntfy）：`watch` 守护进程新增第五 stage，定时任务到点前
  （默认 10 分钟，`lead_minutes` 可调）向 ntfy 主题 POST 一条消息，手机上
  订阅该主题的 ntfy App 即收原生推送——补齐移动端提醒短板，零自建应用、
  零服务器。新增 `horae ntfy test` 发送样例推送验证链路；profile 配置新增
  `ntfy` 块（`url`/`topic`/`token_env`/`priority`/`lead_minutes`/`tags`），
  访问令牌只走环境变量、绝不落盘。仅推送带有效到期时间的任务；未配置时该
  stage 为空操作，单条推送失败不阻断其它阶段、下一轮自动重试（沿用 watch
  容错契约）。网络层抽象为 `NtfyTransport` trait（ureq + rustls），测试用
  `FakeTransport` 无网断言。

## [0.1.1] - 2026-08-26

### Fixed

- 番茄“成就结清”横幅不再整天常驻：`PomoState` 新增 `break_ended_at`
  （休息结束时由 daemon 盖章，开启新一轮/显式停止时清空），横幅仅在
  休息结束后的 10 分钟窗口内提示“再接再厉”，当天重启 TUI 不再弹出旧提示。
- `horae pomo start <id>` 现在与所有其他接受任务引用的命令一致，支持
  「精确 id > 唯一前缀 > 唯一精确标题」的 git 式解析（此前只认完整 UUID）。
- CLI 路径补齐 RRULE 校验（与 TUI 输入层同源防呆）：`horae capture "… *y"` 与
  `horae schedule --rrule FREQ=YEARLY` 在写库前报错并提示支持的三种频率
  （DAILY/WEEKLY/MONTHLY），不再把引擎无法展开的年循环静默存成一次性任务；
  `horae watch` 手机桥复用 capture 路径，一并覆盖。无法识别的裸词（如
  `every day`）同样被拦截。
- `pomo start` 拉起的后台 daemon 不再继承父进程的终端/管道 stdio——脚本或
  测试捕获其输出时不会因管道 EOF 迟迟不闭合而挂起。

### Added

- 端到端 CLI 集成测试（`tests/cli.rs`）：在隔离的 `HORAE_CONFIG_DIR` 下驱动真实
  二进制，锁定核心契约——capture→流转→done 全链路、循环任务打卡推进锚点、
  归档/恢复/清除门禁、id 前缀与精确标题解析、export/import 跨库往返、日志命令。
- 备份路径补测试：`repo::backup` 新增检查单逐字段往返、设置表 merge/replace
  双模式还原、合并模式下跳过任务不产生重复时间线、pomodoro 备份块存在性与
  导入标记等用例。
- `horae watch` 容错降级：`process_once` 四个阶段相互隔离（单阶段 I/O 失败
  不阻断其余阶段）；队列文件按字节 lossy 读取（畸形行不再让整份队列丢失）；
  提醒文件写失败跳过并在下一轮自动重试——常驻守护进程不再因单个文件错误死亡。

- `horae stats` 控制台看板，使用 Catppuccin 摩卡配色的“时间女神” ASCII 像素艺术展示今日番茄钟进度与任务统计。
- TUI 启动专属“开屏页”（Splash Screen）：复用 `stats` 绝美画面与统计数据，并伴随随机 GTD 哲思标语（“大脑是用来思考的，记忆交给 HORAE”等），增强操作仪式感。
- 完整备份与还原：`horae export [--file PATH]` 把全部任务（含所有列）、
  `task_events` 时间线、标签、设置与番茄钟状态打包成一个 JSON 文件
  （默认 `horae-backup-<日期>.json`）；`horae import <FILE> [--replace]` 默认按 id
  合并（已存在整行跳过），`--replace` 清空任务数据后精确还原。备份是纯文件，
  可随 git/网盘同步；导入为原始 INSERT，不伪造时间线事件。
- Permanent delete for archived tasks: `horae purge <id>` (CLI) and `D` in the
  Archived view (TUI, y/n confirm). Purge only works on archived tasks and
  cascade-deletes their events/tags via `ON DELETE CASCADE`; no event is logged.
- Batch purge in the Archived view: enter visual mode with `v`, move with
  `j`/`k` to range-select, then `D` + y to permanently delete the whole
  selection at once. Visual mode now takes priority over the left pane so
  `j`/`k` always move the selection. ConfirmArchive also clears the visual
  selection after confirming/cancelling (was silently kept before).
- Open-source release: GPL-3.0 `LICENSE`, `README.md`, `CHANGELOG.md`, Cargo
  metadata (`license`, `repository`, `rust-version`), and GitHub Actions CI +
  release workflows.
- Count caching for the guide sidebar (`App::counts`) so rendering performs
  zero database queries per frame; one-pass today/tomorrow list computation
  (`day_lists`) with a single RRULE expansion per recurring task; batched tag
  fetch (`get_tags_for_tasks`) replacing per-row queries.
- `horae completions <bash|zsh|fish|...>` generates shell completion scripts via
  `clap_complete`, handled before the database is opened so it has no side
  effects.
- Checklist overhaul: `Tab` enters per-item management — `j/k` move the cursor,
  `Space` ticks the selected item, `d` deletes it, `J`/`K` reorder, `e` renames.
  The list shows an `n/total` progress badge; ticking the last item no longer
  auto-resets the list, and a fully-checked list shows a "press x to complete"
  hint (it never auto-marks the task done). Every structural change is recorded
  as a `checklist` event in the audit timeline.
- Richer `--help` output: top-level `long_about` + usage examples, per-command
  examples for `capture`/`list`/`schedule`/`pomo`/`alarm`, and value-name/help
  hints for flags. `--p1`/`--p2`/`--p3` are now mutually exclusive.

### Changed

- TUI 大文件拆分（纯移动、行为不变）：`render.rs`（2430 行）拆为
  `render/{mod,banners,input,popups,help,detail}.rs` 按渲染职责分组，
  `handlers.rs`（1653 行）拆为 `handlers/{mod,normal,actions,confirm,input,checklist}.rs`
  按 `Mode` 分组；trait 与其实现留在各自 mod.rs，跨文件入口提为 `pub(super)`。
  `crate::tui::render::*` / `crate::tui::handlers::*` API 路径不变，消费方零改动。
- TUI 状态模块拆分（纯移动、行为不变）：`app.rs`（1544 行）拆为
  `app/{mod,completion,data,profiles,ops}.rs` 按职责分组——mod.rs 保留共享类型
  （View/Mode/Pane/Popup/Row/DetailData/App 字段）、构造器与状态管理方法，
  输入编辑+Tab 补全 / refresh 数据管线+计数 / 设置页 profile 管理 / 任务操作
  各自成块。私有辅助随调用者迁移后保持私有，跨文件入口本就是 `pub(crate)`，
  零可见性改动。`crate::tui::app::*` API 路径不变，消费方零改动。
- RRULE 单字母简写：`*d`/`*w`/`*m`/`*y` 现在解析为完整的
  `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`，不再把裸 `"d"` 存进数据库。`horae capture`
  也会保留 quick-add 里的 `*rrule` 令牌（此前只有 TUI 生效）。
- 新增迁移 v6，把历史遗留的裸 `"d"`/`"w"`/`"m"`/`"y"` 循环规则规范化为完整 RRULE。

- `README.md` is now a single-language index that links to `README_cn.md` (中文) and `README_en.md` (English); the user manual is kept as two separate single-language files rather than one bilingual file.
- Removed the stale `CODEBUDDY.md`; `AGENTS.md` is the single authoritative
  contributor guide, with test locations and CI usage corrected.
- List-row status now follows the human's perspective for habits/scheduled
  tasks: a recurring task checked in today is marked `✓` and shows
  `已打卡·下次:<time>` (its next occurrence), while a missed slot is treated as
  overdue and reported uniformly (`逾期X分钟/小时/天`). `effective_due` now
  returns the most recent missed occurrence for recurring tasks, so the alarm
  window, `horae list --due-before`, and the daily digest all count a missed
  today's slot as overdue.

### Fixed

- CLI 的任务引用解析与文档对齐：`resolve_id` 现在按「精确 id > 唯一前缀 >
  唯一精确标题（仅未归档任务）」回退，歧义标题报错；此前标题解析只存在于
  `watch.rs`，主命令链路并不支持。
- Checklist-adding keybinding was unreachable (`Shift+K` shadowed by the
  Tomorrow view). `Shift+C` now adds checklist items, and the pomodoro-length
  configuration moved to `[`; help/syntax panels updated.
- Due-notification checks mixed milliseconds and seconds, so 1h/10m/due-now
  desktop notifications never fired. The check now uses a consistent seconds
  scale and queries only tasks within the relevant window (`due_in_range`).
- TUI tests read the live `pomo.json`, so a running `horae pomo daemon` made the
  rendering tests fail. Added a test-only idle override
  (`set_pomo_idle_for_tests`).
- `relative_due` / `relative_past` built strings via repeated `replace`;
  switched to a single `format!`-style substitution.

### Performance

- `check_notifications` no longer scans the full task table on every tick;
  it selects only tasks with `due_at` in the ±1h window.
- `refresh()` loads the visible list and all its tag names in two queries
  instead of one query per row.

## [0.1.0] - unreleased

Initial development build of the GTD terminal task manager (see git history for
the full list of `feat`/`fix`/`refactor` commits).
