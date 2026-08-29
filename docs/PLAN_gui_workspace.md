# horae Workspace 化 + Tauri GUI 构建计划

> 目标：将 horae（Rust CLI+TUI GTD 任务管理器）重构为 Cargo workspace，
> 抽出 `horae-core` 核心 crate，并基于它新建 Tauri 2 + Svelte 5 的 GUI 项目。
> 新 session 执行时按 Phase 顺序推进，每个 Phase 有独立验收标准。

## 0. 已确认决策（不要重新询问）

| 决策项 | 结论 |
|---|---|
| GUI 框架 | Tauri 2（Rust 后端 + WebView） |
| 前端 | Svelte 5 + Vite + TypeScript |
| 仓库布局 | 同仓库，根目录转 workspace，GUI 为 `crates/horae-gui` |
| core 接口形态 | GUI 直接调 `repo::*` 函数，不加 Store 封装层 |
| DB 线程模型 | `Arc<Mutex<rusqlite::Connection>>` 放入 Tauri State，async command 短临界区 |

## 1. 现状关键事实（已勘察，直接使用）

- 单 crate `horae` 0.1.1，edition 2021，rust-version 1.89，GPL-3.0。
- 依赖：clap 4(derive) + clap_complete、rusqlite 0.32(bundled)、uuid v4、
  chrono(clock,std 无默认特性)、serde、serde_json、anyhow、thiserror、dirs 5、
  ratatui 0.30(crossterm,layout-cache,underline-color)、crossterm 0.29、
  unicode-width 0.2、ureq 2(tls)；
  dev-deps：assert_cmd 2、predicates 3、tempfile 3.27。
- `[profile.release]`：strip=true, lto=true, codegen-units=1, panic="abort", opt-level="z"（上移到 workspace 根）。
- 分层现状（关键，core 已零反向依赖）：
  - `model/`(task,tag,event,pomodoro,backup) 数据类型
  - `db/`(conn::open(profile)->Connection, migrate 用 include_str! 内嵌 SQL)
  - `repo/`(tasks/{query,quotes,transition}, tags, pomodoro, settings, state,
    alarm, notify, quotes, modules, backup) —— 业务逻辑为 `&Connection` 上的
    自由函数，`repo::mutate` 统一做事务+审计(log_event)原子化
  - 服务层：`parser`(quick-add), `schedule`(rrule), `time`, `config`(dirs::config_dir),
    `error`, `i18n`(Lang::tr), `notification`(JsonStateStore("notify_tui.json"),
    tick(&Connection)->Vec<NotificationEvent>), `ntfy`(ureq 阻塞 HTTP)
  - 前端层：`cli`+`cli_i18n`+`commands/`(clap 分发)、`tui/`(app/handlers/render/keys/theme/icons/splash/tests)
- 已验证：`repo`/`model`/`db` 中无任何 `crate::tui`/`crate::cli` 引用；
  唯一交叉点是 `commands/mod.rs` 分发 `Command::Tui`。
- `unicode-width` 仅被 `src/tui/render/input.rs` 使用 → 归 tui crate。
- `src/tui/tests.rs` 有 2851 行（模块内 `#[cfg(test)]`），整体随 tui 搬迁。
- 集成测试 `tests/cli.rs`（assert_cmd）→ 归 cli crate。
- `main.rs` 42 行：解析 CLI → Completions/Profile 提前返回 → `db::conn::open(profile)` → `commands::run`。
- CI：`.github/workflows/ci.yml`（fmt --check / clippy --all-targets -D warnings /
  cargo test --all-targets）、`release.yml`（根目录 cargo build --release --target ${{matrix.target}}，dist/* 产物）。
- 迁移文件：根目录 `migrations/0001..0007*.sql`，被 `db/migrate.rs` 以
  `include_str!("../../migrations/…")` 引用（共 7 处）。

## 2. 目标结构

```
horae/                      # 根 Cargo.toml → [workspace]
├── migrations/             # 保留在根（include_str 用 ../../ 仍可达 core）
│                           # 或移入 crates/horae-core/migrations（二选一，见 3.4）
├── crates/
│   ├── horae-core/         # lib "horae_core"
│   ├── horae-tui/          # lib "horae_tui"
│   ├── horae-cli/          # bin "horae"
│   └── horae-gui/          # Phase 2 创建（src-tauri/ + 前端）
├── .github/workflows/      # 调整 release 构建 --package
└── Cargo.toml              # workspace 定义 + [workspace.package] + [profile.release]
```

## 3. Phase 1：Workspace 拆分（零行为变更）

### 3.1 根 Cargo.toml

```toml
[workspace]
resolver = "2"
members = ["crates/horae-core", "crates/horae-tui", "crates/horae-cli"]
# horae-gui 在 Phase 2 加入

[workspace.package]
version = "0.1.1"
edition = "2021"
rust-version = "1.89"
license = "GPL-3.0"
repository = "https://github.com/zhaohang1205/horae"

[profile.release]
strip = true
lto = true
codegen-units = 1
panic = "abort"
opt-level = "z"
```

### 3.2 各 crate 依赖分配

- **horae-core**（lib）：rusqlite(bundled)、uuid(v4)、chrono(clock,std)、serde、
  serde_json、anyhow、thiserror、dirs、ureq(tls)。
  可选特性：`[features] test-util = []` 导出 testutil。
- **horae-tui**（lib）：horae-core、ratatui 0.30、crossterm 0.29、unicode-width、anyhow。
- **horae-cli**（bin "horae"）：horae-core、horae-tui、clap 4(derive)、clap_complete、anyhow；
  dev-deps：assert_cmd、predicates、tempfile。

### 3.3 文件搬迁清单

| 目标 crate | 移入的模块 |
|---|---|
| horae-core | model/ db/ repo/ parser.rs schedule.rs time.rs config.rs error.rs i18n.rs notification.rs ntfy.rs testutil.rs(feature test-util) |
| horae-tui | tui/ 整目录（mod.rs 改为 crate 根 lib.rs 或保留目录结构） |
| horae-cli | cli.rs cli_i18n.rs commands/ + 新 main.rs + tests/cli.rs |

### 3.4 需要修改的点

1. 所有 `crate::repo` / `crate::model` 等路径：tui 内改为 `horae_core::`，
   commands 内改为 `horae_core::`；tui 内部互引改为 `crate::`（同 crate）。
   机械替换，注意 `crate::time` 这类。
2. `db/migrate.rs`：若 migrations 留在根目录，路径改为
   `include_str!("../../../migrations/…")`（core 在 crates/horae-core/src 下）；
   建议直接把 `migrations/` 移到 `crates/horae-core/migrations/`，路径改为
   `"../migrations/…"`，根目录保留软链或 README 说明。
3. `testutil.rs`：core 中 `#[cfg(feature = "test-util")] pub mod testutil;`
   tui/cli 的 dev-dependencies 加 `horae-core = { path = "../horae-core", features = ["test-util"] }`。
4. horae-cli 新 `main.rs`：保持现逻辑，`mod` 声明换成
   `use horae_core::…; use horae_tui::…;`
5. `Cargo.toml` 各 crate 元数据继承 `workspace = true`。
6. `.github/workflows/release.yml`：构建命令改
   `cargo build --release --package horae --target ${{matrix.target}}`
   （bin 名保持 `horae`）；ci.yml 无需改（workspace 根跑全成员）。
7. 可选：`[workspace] default-members = ["crates/horae-cli"]` 让裸 `cargo build/test`
   只碰 CLI（GUI 加入后避免 Node 工具链牵连），显式 `--workspace` 跑全部。

### 3.5 验收（Phase 1 完成标准）

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
cargo build --release -p horae   # 二进制行为与拆分前一致（手动 smoke：horae list / horae tui）
```

## 4. Phase 2：Tauri GUI 骨架

### 4.1 脚手架

```
crates/horae-gui/
├── src-tauri/                 # tauri 2（lib 模式）
│   ├── Cargo.toml             # tauri 2, tauri-build, horae-core
│   ├── tauri.conf.json        # frontendDist 指向 ../dist（Vite 输出）
│   └── src/
│       ├── lib.rs             # run() 注册 commands + manage(state)
│       ├── state.rs           # AppState(Arc<Mutex<Connection>>)，setup 里 db::conn::open
│       └── commands/          # tasks.rs tags.rs pomo.rs settings.rs …
├── package.json               # svelte 5 + vite + @tauri-apps/cli + api
└── src/                       # Svelte 组件
```

根 workspace members 加入 `crates/horae-gui/src-tauri`。

### 4.2 DB 线程模型（固定决策，写代码时遵守）

- setup：`let conn = horae_core::db::conn::open(profile)?;` →
  `app.manage(AppState(Mutex::new(conn)))`
- command 写法：`#[tauri::command] async fn xxx(state: tauri::State<'_, AppState>) -> Result<…, String>`
  内部 `state.0.lock().unwrap()` 短临界区调 `horae_core::repo::*`；
  重聚合（stats/review 周统计）用 `tauri::async_runtime::spawn_blocking` + 克隆 Arc。
- 返回 serde 序列化类型（model 已 derive serde，直接复用）。
- 错误：command 层 `map_err(|e| e.to_string())` 即可，前端 toast 展示。

### 4.3 首批 command（对齐 repo 面）

- `list_tasks(view)` → repo::tasks::query（today/inbox/next 等视图）
- `capture(input)` → repo::tasks::transition::create_capture（复用 parser）
- `transition(id, status)` / `set_due` / `archive` / `schedule` → transition.rs 对应函数
- `detail(id)` → 任务 + checklist + 事件时间线（repo 查询函数）
- `tags_*` → repo::tags
- `start_pomo(id)` → ensure_ready_for_pomodoro + repo::pomodoro
- `tick_notifications()` → notification::tick（GUI 用独立 state 文件 notify_gui.json）

### 4.4 profile 机制

GUI 与 CLI/TUI 共享 `config`（dirs::config_dir）与 `db::conn::open(profile)`；
设置页可切换 profile（重启生效或重开连接）。同一库文件多端并发由 SQLite 文件锁兜底，
Phase 3 可评估开 WAL。

### 4.5 验收

`horae-gui` dev 运行：今日列表可见 → capture 建任务 → 勾完成 →
CLI `horae list` 中同步可见（证明核心复用完整）。

## 5. Phase 3：功能迭代（每步小提交）

### 5.0 已确认决策（Phase 3 专属，不要重新询问）

| 决策项 | 结论 |
|---|---|
| 前端布局 | 三栏（左视图栏 + 中任务列表 + 右详情抽屉 slide-over），区别于 TUI 键盘布局，鼠标优先 |
| 快速录入 | 顶栏常驻捕获框 + `Ctrl/Cmd+N` 全局聚焦，复用 `parser` 全语法 |
| 番茄钟 | 前端 Svelte `setInterval` 倒计时 + `tauri-plugin-notification` 系统通知；不引 daemon |
| 周回顾 | 引导式 4 步导航（Inbox→Waiting→Someday→Done），**无统计图表/无聚合**（用户明确不需要） |
| 打包 | 独立 `release-gui.yml`（tauri-action），tag 匹配 `gui-v*`，与 CLI `v*` 分离 |
| 视觉方向 | 「chronographer's desk」深暖墨主题，见 5.6 |

### 5.1 子项 1：任务详情页（checklist / notes / 标签）

后端已完备（Phase 2 已注册 `detail`/`update_notes`/`toggle_checklist_item`/`add_tag_to_task`/
`remove_tag_from_task`/`get_task_tags`/`rename`/`set_due`/`schedule`）。前端 `TaskDetail.svelte`
（右侧抽屉）：点击列表行打开；checklist 逐条 `toggle_checklist_item` 后本地乐观更新 + 重 `detail`；
notes `<textarea>` 失焦/防抖 500ms → `updateNotes`；标签 `getTaskTags` 列出 + `listTags` 联想下拉
`add`/`remove`；标题 `rename`、状态 `transition`、日期 `set_due`/`schedule` 同页操作。
验收：改完后 CLI `horae detail <id>` 同步可见。

### 5.2 子项 2：quick-add 输入条

后端 `capture` 已调 `parse_quick_add`+`rrule_valid`+`time::parse_time`，全语法已支持。`TopBar.svelte`
常驻顶栏，输入时下方实时解析预览（`@tag`/`~时间`/`*rrule`/`!pN` 提示 chips），Enter → `capture` →
清空 → 刷新当前视图；`App.svelte` 挂 `window` keydown 监听 `Ctrl/Cmd+N` 聚焦。
验收：`买牛奶 ~18:00 @home *daily` 一次 capture 后 CLI 可见带 due/home 标签/rrule 的任务。

### 5.3 子项 3：番茄钟视图 + 桌面通知

后端新增：
1. `horae-core/src/pomo.rs`：抽 `pub fn begin_session(conn, id) -> Result<()>`（= 现有 `start`
   中设 `PomoState{phase=Work, task_id, start_ts, end_ts=now+work_mins}` 部分，**不 spawn daemon、
   不 notify**）；保留 `start` 给 CLI。
2. `commands/pomo.rs`：`start_pomo` 改调 `pomo::begin_session`（真正进入 Work 相位写状态）并返回
   `PomoState`；新增 `pomo_complete(conn) -> PomoState` 按 `config` 推进相位（Work→Short/LongBreak→
   Work），累加 `total_count/today_count/cycle/streak`，跨天重置（复用 `last_date`），`save_state` 返回。
   Work 结束**默认不自动完成任务**（与原 daemon 行为一致）。
3. `Cargo.toml` 加 `tauri-plugin-notification = "2"`；`tauri.conf.json` `plugins` 注册 `notification`；
   `capabilities/default.json` 加 `"notification:default"`；`lib.rs` handler 增加 `pomo_complete`。
4. 前端 `package.json` 加 `@tauri-apps/plugin-notification`。

前端 `PomoWidget.svelte`（右下浮动环形进度，SVG）+ `notifications.ts`：`start_pomo(id)` 拿状态后本地
倒计时（对齐 `end_ts`）；相位结束 → `pomo_complete` 取下一相位继续；进入休息/结束用
`Notification.send` 弹系统通知。`App.svelte` 挂载 `setInterval(30s)` 调 `tick_notifications()`，
对 `InOneHour/InTenMins/Now` 走系统通知。
验收：GUI 启动番茄钟→倒计时结束弹系统通知→`pomo_state().today_count` 递增；任务到点 GUI 弹 `Now`。

### 5.4 子项 4：周回顾（无统计图表）

后端无需新聚合。前端 `ReviewModal.svelte`：仿 `tui/handlers/normal.rs:218` 的 4 步——进入后 banner
「每周回顾 第 n/4 步」，依次切中列表到 Inbox→Waiting→Someday→Done，`Esc`/完成退出；复用
`list_tasks(view)` 渲染，不引图表库、不做聚合。
验收：点「周回顾」能 4 步走完并显示进度横幅。

### 5.5 子项 5：tauri-action 独立打包

新增 `.github/workflows/release-gui.yml`：`tag` 匹配 `gui-v*`；`tauri-apps/tauri-action@v0`，
`projectPath: crates/horae-gui`，`matrix: [ubuntu-latest, macos-latest, windows-latest]`；
`args: --release`；Linux runner 补装 Tauri 系统库（webkit2gtk-4.1/libsoup3/libjavascriptcoregtk-4.1
等官方 apt 清单）；产物上传对应 Release（非签名）。`release.yml`（CLI）不变。
注：`ci.yml` 跑 `cargo test --all-targets` 现在会编 `horae-gui`（纯 Rust lib，不需 npm，但 Linux 需
webkit2gtk 系统库）——建议 Linux 步骤补装该依赖，或测试限定 `-p horae-core -p horae-tui -p horae-cli`。

### 5.6 GUI 视觉设计：「chronographer's desk」（计时匠的书桌）

概念：horae=时序女神，核心是时间数据化。GUI 不做通用扁平待办，而是一张夜里的书桌——深暖墨底、纸肌理、
列表里贯穿任务的「时间轴」发丝线、会呼吸的琥珀焦点。签名元素=中列表左侧连接 due 的竖细线 + 番茄环进度。

- 西文显示体 **Fraunces**（optical sizing 衬线）；西文正文 **Hanken Grotesk**；中文 **LXGW WenKai
  （霞鹜文楷，CDN）**；等宽 **JetBrains Mono**。避开 Inter/Roboto/system。
- 令牌（写 `src/theme.css` CSS variables）：`--ink-900:#15120d` / `--ink-800:#1d190f` /
  `--ink-700:#262017` / `--paper:#efe7d6` / `--paper-dim:#a89c86` / `--rule:#3a3225` /
  `--amber:#e8b04b`（唯一高饱和强调）/ `--sage:#9bb08a` / `--rose:#d98a7b`。背景叠加径向微晕 + SVG
  噪点 grain + 列表区极淡横线纸纹。
- 布局：顶栏(⌁ wordmark + 常驻捕获框 + profile) / 左栏(活的迷你模拟时钟 SVG + 视图点击 + 标签过滤展开
  + 周回顾) / 中列表(时间轴细线 + due 节点，amber 正常/rose overdue 微颤) / 右抽屉(slide-over 字段
  stagger 揭示) / 底栏(右下浮动番茄环 widget)。
- 动效：三栏载入 stagger 淡入；捕获成功新任务落入列表 + amber 扫光；勾选 SVG 描边对勾；抽屉 spring 滑入；
  输入框聚焦 amber 光晕 + 解析 chips；番茄环 arc 收缩 + 相位脉冲；行悬停上浮 + 动作图标淡入。
- 组件树：`store.svelte.ts` / `App.svelte` / `components/{TopBar,Sidebar,TaskList,TaskRow,
  TaskDetail,PomoWidget,ReviewModal}.svelte` + `notifications.ts`。全部令牌走变量，便于后续加浅色「纸」主题。

### 5.7 验收（Phase 3 完成标准）

1. `cargo fmt --check && cargo clippy --all-targets -- -D warnings`（GUI crate 干净）。
2. `npm run build` 出 `dist/`；`npm run tauri dev` 可用。
3. 纯鼠标即可：捕获→勾完成→改 notes/标签→开始番茄(系统通知)→周回顾 4 步；且 CLI `horae list`/
   `horae detail` 与 GUI 双向同步；`Ctrl/Cmd+N` 随时聚焦捕获框。
4. `git tag gui-v* && git push --tags` 触发 `release-gui.yml` 产出三平台安装包。

## 6. 风险与注意事项

- `tui/tests.rs` 2851 行搬迁是纯路径改动，但量大，建议先挪 core+tui 再跑测试。
- `panic = "abort"` 在 workspace profile 继承，对 tauri 无影响（GUI crate 不受
  workspace profile 影响也可单独覆盖）。
- `chrono` 已是 no-default-features，core/tui 各自声明特性时保持一致。
- release.yml 的 matrix 目标平台构建 GUI 属新流水线，Phase 1 只需保证 CLI 不回归。
- don't forget：`.gitignore` 需加 `crates/horae-gui/node_modules`、`dist`。
```
