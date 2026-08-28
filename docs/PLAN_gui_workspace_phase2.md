# horae Phase 2 实施计划：Tauri GUI 骨架

> 验收边界（已与用户确认）：
> - `cargo build -p horae-gui` 通过 + `npm run build` 前端打包通过。
> - 一个 Rust 集成测试（临时 DB 直接调 GUI command 内部自由函数）证明 GUI 写库与 CLI 读同一份数据（核心复用）。
> - 不实际启动带显示的 Tauri 窗口；手动验收清单仅文档化。
> - Profile：Phase 2 仅 `list_profiles()`，不切换连接。

## 0. 环境确认（已勘察）
- Node v25.9 / npm 11.19 可用；webkit2gtk-4.1 / gtk3 / libsoup3 系统库齐全 → Rust 侧可编译 Tauri 2。
- `horae-core` 已暴露全部所需 API：`model` 各类型均 `#[derive(Serialize)]`（Task/Tag/TaskEvent/ChecklistItem/PomoState）；`Status: FromStr`；`db::conn::open(Option<&str>)`；WAL 已在 `conn.rs` 开启；`parser::parse_quick_add` / `time::parse_time` 可用。

## 1. 对 `horae-core` 的最小胶水改动（`crates/horae-core/src/notification.rs`）
- 给 `pub enum NotificationEvent` 增加 `#[derive(Serialize)]`（command 需返回它）。
- `NotificationEngine` 增加字段 `state_file: &'static str`；`new()` 默认 `"notify_tui.json"`，新增 `pub fn new_gui() -> Self`（用 `"notify_gui.json"`）；`tick` 内 save/load 改用 `JsonStateStore::new(self.state_file)`。
- 其余核心代码零改动。

## 2. Workspace 成员
根 `Cargo.toml` 的 `members` 增加 `"crates/horae-gui/src-tauri"`；`default-members` 维持 `["crates/horae-cli"]`。

## 3. Rust 后端 `crates/horae-gui/src-tauri/`
- `Cargo.toml`：`[package] name="horae-gui"`；`[build-dependencies] tauri-build`；`[dependencies]` tauri 2、`serde`、`serde_json`、`horae-core = { path }`；`[lib] crate-type=["staticlib","cdylib","rlib"]`。
- `build.rs`：`tauri_build::build()`。
- `tauri.conf.json`：`identifier="com.horae.app"`；`build.frontendDist="../dist"`、`build.devUrl="http://localhost:1420"`；`app.windows[0]` 标题 "horae"。
- `src/state.rs`：`pub struct AppState(pub std::sync::Mutex<rusqlite::Connection>);`
- `src/lib.rs`：`run()` 在 `setup` 里 `db::conn::open(None)` → `manage(AppState(...))`；`invoke_handler` 注册所有 command；`src/main.rs` 调 `horae_gui::run()`。
- `src/commands/`（**每个 command 业务体抽成 `&Connection` 自由函数**，tauri 包装只做 `state.0.lock().unwrap()` + `map_err(|e| e.to_string())`；测试绕过 `State` 直接调自由函数）：
  - `tasks.rs`：`list_tasks(view)`、`capture(input)`、`transition(id,status)`、`set_due`、`schedule`、`archive`/`unarchive`/`purge`、`detail(id)->TaskDetail{task,events}`、`rename`、`update_notes`、`toggle_checklist_item`。`capture` 复用 CLI 逻辑（`parse_quick_add`→合并 tag→`parse_time`→`create_capture`+含 `~time` 再 `schedule`）。`list_tasks` 视图映射：inbox/next/scheduled/waiting/someday/reference/archived/all/today（today 在 Rust 侧用 `local_day_bounds` 过滤）。
  - `tags.rs`：`list_tags`、`create_tag`、`delete_tag`、`add_tag_to_task`、`remove_tag_from_task`、`get_task_tags`。
  - `pomo.rs`：`pomo_state()->PomoState`、`start_pomo(id)`（仅 ensure_ready，daemon 留 Phase 3）。
  - `settings.rs`：`get_setting`、`set_setting`。
  - `notifications.rs`：`tick_notifications()->Vec<NotificationEvent>`，用 `NotificationEngine::new_gui().tick(conn)`。
  - `profiles.rs`：`list_profiles()->{default,names}`，经 `config::Config::load()`。
- 所有 command 签名 `#[tauri::command] async fn ...(state: tauri::State<'_, AppState>) -> Result<T, String>`。

## 4. 前端骨架 `crates/horae-gui/`（Svelte 5 + Vite + TS）
- `package.json`：deps `@tauri-apps/api`；devDeps `@tauri-apps/cli`、`svelte`、`@sveltejs/vite-plugin-svelte`、`vite`、`typescript`；scripts `dev`/`build`/`tauri`。
- `vite.config.ts`：`port:1420, strictPort:true, clearScreen:false`。
- `tsconfig.json` + `svelte.config.js` + `index.html` + `src/main.ts`。
- `src/lib.ts`：类型化 `invoke` 封装；`src/types.ts`：镜像 Task/Tag/TaskEvent 的 TS 接口。
- `src/App.svelte`：挂载 `listTasks("today")` 渲染；顶部 capture 输入框（回车调 `capture` 后刷新）；每行复选框调 `transition(id,"done")` 后刷新。

## 5. `.gitignore`
已含 `node_modules` / `dist`；追加 `crates/horae-gui/src-tauri/target`。

## 6. 验证
1. `cargo fmt --check && cargo clippy -p horae-gui -- -D warnings`。
2. `cargo build -p horae-gui`。
3. `cd crates/horae-gui && npm install && npm run build` 产出 `dist/`。
4. 集成测试 `crates/horae-gui/src-tauri/tests/reuse.rs`：临时 `HORAE_CONFIG_DIR` 建库 → 直接调 `horae_gui::commands::tasks::{capture,transition}` 自由函数 → 用 `horae_core::repo::tasks::query` 断言任务存在且 Done。
5. 手动验收清单（文档化）：`npm run tauri dev` → capture → `horae list` 可见 → 勾完成 → `horae list` 同步。

## 7. 不在 Phase 2 范围（留 Phase 3）
详情页 checklist/notes 编辑 UI、quick-add 全语法输入框、番茄钟视图+桌面通知、周回顾/统计图表、profile 热切换、tauri-action 打包流水线。

## 8. Linux/Wayland 渲染兜底（Phase 2 补丁）
- 现象：Hyprland(Wayland) 下 `npm run tauri dev` 启动即报 `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display`，窗口打不开。
- 根因：webkit2gtk 默认 DMABUF 渲染器在部分 Wayland 合成器（Hyprland 等）触发协议错误。**代码无缺陷**，应用已正常编译并启动，仅卡在 WebView 渲染这一步。
- 修复（已写入 binary）：`crates/horae-gui/src-tauri/src/lib.rs::run()` 开头（`#[cfg(target_os = "linux")]`）在首个 webview 创建前设置
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`；若环境存在 `DISPLAY`（如 Hyprland 配套的 Xwayland）再追加 `GDK_BACKEND=x11` 作为兜底。macOS/Windows 不受影响。
- 兜底文档：个别合成器下仍异常的，可手动 `export GDK_BACKEND=x11`（已内置）或 `WEBKIT_DISABLE_COMPOSITING_MODE=1` 后再 `npm run tauri dev`。
- 验证：`cargo run -p horae-gui` 不再出现 protocol error，窗口正常出现。

