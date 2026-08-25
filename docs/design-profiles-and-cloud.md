# 设计文档：Profile 多库 + 前端化配置 + 云数据库

> 状态：设计讨论稿（未实现）
> 适用项目：`horae`（GTD 终端任务管理器，Rust，SQLite 本地优先）

## 1. 需求与现状

用户的诉求可归纳为三点：

1. **多场景多数据库**：用户在不同场景需要连接不同的数据库（如生产/测试、生产1/生产2、工作/个人）。对 horae 而言，"数据库"本质是**数据集合（Profile）**，不是传统意义的部署环境。
2. **配置前端化**：Profile/连接配置不应靠手改文件，而应在 TUI 内可视化管理。
3. **云数据库**：提供云端数据库能力（备份、多端）。

### 现状（相关代码）

| 位置 | 现状 |
|---|---|
| `src/db/conn.rs:10` | `open()` 写死 `~/.config/horae/horae.db`，唯一数据库 |
| `src/main.rs:23` | `db::conn::open()` 后执行 `commands::run` |
| `src/db/migrate.rs` | 基于 `user_version` 的增量迁移，幂等 |
| `migrations/0006_settings.sql` | `settings` 键值表（`lang`/`theme`/`quotes`），已由 TUI F5/F6/F7 读写 |
| `src/repo/settings.rs` | `get`/`set` 键值读写 |
| `src/commands/watch.rs` | Syncthing 桥接（文件级同步）——已有的"云"路径 |
| `src/tui/handlers.rs` | 已有 F5 主题 / F6 语言 / F7 功能开关(含金句与图标) 快捷键体系 |

## 2. 设计目标与非目标

**目标**
- Profile 概念：多数据集合（SQLite 文件）一键切换。
- 前端化：TUI 内管理 Profile（列表/新建/重命名/删除/默认/切换）。
- 云数据库：本地优先 + 可选的云端能力，不破坏本地优先架构。

**非目标**
- 不做多写并发（GTD 是单用户工具；SQLite 就是单写者）。
- 不引入重量级 DB 服务器（不走 Postgres/MySQL）。
- 不把 `watch.rs`（Syncthing 手机桥）替换掉——它是文件级补充，可并存。

## 3. Profile（多数据库）设计

### 3.1 概念

Profile = 一个独立的 SQLite 数据库文件 + 元数据（名称、云同步目标、默认标记）。

目录布局（`dirs::config_dir()/horae/`）：
```
~/.config/horae/
  config.json            # 前端化配置（Profile 列表、默认、云目标）  ← 新增
  horae.db                 # 默认 Profile（向后兼容，等价于"默认"Profile）
  profiles/
    work.db
    personal.db
    prod1.db
```

### 3.2 配置 Schema（`config.json`）

```json
{
  "default_profile": "default",
  "profiles": {
    "default":  { "db": "horae.db" },
    "work":     { "db": "profiles/work.db", "cloud": { "url": "libsql://xxx.turso.io", "token_env": "HORAE_TURSO_TOKEN" } },
    "personal": { "db": "profiles/personal.db" }
  }
}
```

- `config.json` 由 `db::conn` 上移一层的新的 `config` 模块负责读写（`src/config.rs`）。
- 缺失时自动生成，`default_profile` 指回 `horae.db`，保证零配置向后兼容。
- 云凭据走环境变量（`token_env`），**绝不写入 config.json**（安全）。

### 3.3 代码改动点

| 改动 | 说明 |
|---|---|
| `src/db/conn.rs` | `open()` → `open(profile: &Profile) -> Connection`，解析 profile 的 db 路径 |
| `src/main.rs` | 读 `--profile` 参数 → 解析 profile → 传入 `open` |
| `src/config.rs`（新） | `Config::load()/save()`，`resolve_profile(name)`，CRUD |
| `src/cli.rs` | 新增全局 `--profile <name>` 参数；新增 `horae profile` 子命令（`list`/`new`/`rm`/`rename`/`set-default`/`switch`）|
| `src/repo/` | **不感知 Profile**——它只操作 `Connection`，Profile 是"打开哪个文件"的问题，域逻辑不变 |
| pomo/alarm/watch | `pomo.rs:126`、`alarm.rs:84/165`、`watch.rs` 内部 `conn::open()` 也要走 profile 解析（用默认/当前 profile）|

**关键原则**：Profile 只影响"打开哪个 SQLite 文件"，`repo`/`model`/`migrations` 完全无感。migrations 在每次打开时仍按 `user_version` 增量跑，天然适用于任意 profile 文件。

### 3.4 CLI 形态

```
horae --profile work capture "buy milk"
horae profile list                  # 列出所有 profile
horae profile new work               # 新建
horae profile set-default work
horae profile switch work            # 改 config.json 默认（影响后续无 --profile 调用）
```

TUI 内（`--profile` 或默认）打开后，可在设置页切换当前 profile 并**热重载**（重开 Connection，刷新视图）。

## 4. 前端化配置（TUI 设置页）

复用现有 `settings` 表 + F 键体系，新增一个 **设置/Profile 管理视图**：

- 新增 `View::Settings`（键位待定，如 `G` 或并入现有 F1 全屏帮助旁的入口）。
- 内容：
  - **外观**：语言（F6）、主题（F5）——已有。
  - **功能开关**：金句（F7）——已有。
  - **Profile 管理**（新增）：列表、`a` 新建、`d` 删除、`r` 重命名、`Enter` 切换当前、`s` 设为默认。
  - **云同步**（见 §5）：每 Profile 的云端地址配置、手动 `sync`。
- 所有变更写回 `config.json`（Profile）或 `settings` 表（已有键）。
- 删除 Profile 时若其文件存在，确认提示"仅从配置移除，还是删除文件"。

**配置访问统一入口**：`crate::config` 提供 `get/set/lang/theme/quotes` 之类，逐步把 TUI 里散落的 `settings::get/set`（`app.rs:239-256`、`handlers.rs:294-341`）收口。

## 5. 云数据库方案

### 5.1 方案对比

| 方案 | 机制 | 优点 | 缺点 |
|---|---|---|---|
| **A. Turso/libSQL**（推荐） | SQLite 兼容云端库，本地 embedded replica + 按需 `sync` | 与现有代码几乎零改动（`rusqlite` → `libsql` 或保留 rusqlite 本地副本）；云是"叠加层"不是主库；单用户天然无写冲突 | 需新增依赖 + 网络层；首次接入要迁移概念 |
| **B. Litestream WAL 复制** | 本地主库，WAL 流式复制到 S3 | 极简、只做备份/容灾 | 云只读，不是"云数据库"，多端访问弱 |
| **C. Syncthing（已有）** | 文件级同步 | 已实现、支持手机 | 不透明（文件副本），非数据库语义 |

### 5.2 推荐路线

**两阶段**：

1. **Phase 1（近期，纯本地）**：实现 §3 Profile 多库 + §4 前端化配置。零新依赖，立即解决"多场景多数据库 + 配置前端化"。
2. **Phase 2（云）**：给"可选 Profile"加 `cloud` 字段，接入 **Turso/libSQL**：
   - 本地 `profiles/*.db` 仍是主库（保证离线可用、速度）。
   - 云副本用于备份 + 多机（手机/电脑）读取。
   - 手动 `sync`（或 TUI 设置页按钮 + `watch.rs` 定时触发）。
   - 认证走环境变量 token，`config.json` 只存 URL。
   - 与 `watch.rs` 并存：文件桥做手机轻量采集，libSQL 做完整数据云同步。

### 5.3 依赖影响

- `rusqlite` 保持不变（本地主库仍用它，保持 bundled SQLite）。
- Phase 2 新增 `libsql`（Rust 客户端）或自建 HTTP 同步层；CI/MSRV 需重新验证（`rust-version = 1.89`）。

## 6. 迁移与兼容

- **数据库 schema 零改动**：Profile 只是文件层面，`user_version` 迁移逻辑不变（`migrate.rs` 已幂等，适合任意新文件）。
- `config.json` 是唯一新配置文件；缺失即生成默认，旧用户无感。
- `horae.db` 就是默认 profile 文件，老配置/脚本继续有效。

## 7. 风险与取舍

| 风险 | 应对 |
|---|---|
| 多 profile 文件之间数据不同步（同一任务分散在不同库） | GTD 语境下这是特性：工作/个人数据天然隔离；合并用 `horae export/import` 已有能力 |
| 热重载 Connection 时 TUI 状态失效 | 切换 profile 时重建 `App` 视图状态（复用现有 `App::new`） |
| libSQL 引入网络依赖、离线场景 | 保持本地主库优先，云仅同步层；断网照常使用 |
| `config.json` 写坏导致无法启动 | 启动时容错：解析失败回退默认 profile 并提示；写操作先写临时文件再 rename（原子） |

## 8. 落地顺序建议

1. `src/config.rs` + `config.json` 读写与默认回退。
2. `db::conn::open(profile)` + `main.rs`/`cli.rs` 接 `--profile`。
3. `horae profile` 子命令（CLI 侧完成 Profile CRUD，可先行验证）。
4. TUI 设置页 `View::Settings`（前端化）。
5. pomo/alarm/watch 内部 `open()` 改走 profile。
6. Phase 2：可选 Profile 云同步（libSQL）。
