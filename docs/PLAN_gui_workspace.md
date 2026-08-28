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

1. 任务详情页（checklist 勾选、notes、标签编辑）
2. quick-add 输入条（复用 parser 全语法：时间/循环/标签）
3. 番茄钟视图 + 桌面通知（notification/tauri-plugin-notification）
4. 周回顾 / 统计图表（若 commands/stats 聚合逻辑被 GUI 复用，上移进 horae-core）
5. 打包发布：tauri-action 独立 workflow，与 CLI release 分离

## 6. 风险与注意事项

- `tui/tests.rs` 2851 行搬迁是纯路径改动，但量大，建议先挪 core+tui 再跑测试。
- `panic = "abort"` 在 workspace profile 继承，对 tauri 无影响（GUI crate 不受
  workspace profile 影响也可单独覆盖）。
- `chrono` 已是 no-default-features，core/tui 各自声明特性时保持一致。
- release.yml 的 matrix 目标平台构建 GUI 属新流水线，Phase 1 只需保证 CLI 不回归。
- don't forget：`.gitignore` 需加 `crates/horae-gui/node_modules`、`dist`。
```
