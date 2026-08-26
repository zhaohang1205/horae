# horae 获取与安装手册

仓库：https://github.com/zhaohang1205/horae （单 Rust 二进制，SQLite 内置，无系统依赖）

## 安装路径决策

```text
跑 scripts/preflight.sh 体检
  ├─ Release 附带预编译二进制？ ──是──→ 【路径 A：预编译】首选，秒装零工具链
  │                                    （注意：tag 版可能落后 main 分支）
  ├─ 没有（如 v0.1.0 只有源码包）─┬→ 已有 Rust 1.89+ 与 C 编译器？
  │                              ├─ 是 → 【路径 B：cargo install --git】装 main 最新
  │                              └─ 否 → 先装 rustup + C 编译器，再走 B
  └─ 想审计源码 / 参与开发 ────────→ 【路径 C：git clone 构建】
```

**第 0 步永远是体检**：`bash <skill-dir>/scripts/preflight.sh`（只读，不改系统），
把报告给用户看，再按下面路径执行。

## 前置条件分级

| 级别 | 条件 | 说明 |
| --- | --- | --- |
| 所有路径必需 | github.com 可达；PATH 中有可用的 bin 目录（`~/.local/bin` 或 `~/.cargo/bin`） | |
| 仅路径 B/C 必需 | **rustc ≥ 1.89**（MSRV）、cargo、git、**C 编译器（gcc/clang）** | rusqlite 用 bundled 特性，编译时要现场编译 SQLite C 源码——最小化系统常缺 cc，这是最常踩的隐藏前置 |
| 可选（体验增强） | Nerd Font / Kitty 协议终端 / libnotify / Syncthing / waybar / shell 补全 | 见下方"可选项对比表"，逐项询问用户 |

## 路径 A：预编译二进制（可用时首选）

资产命名规则（由 `.github/workflows/release.yml` 定义）：`horae-<tag>-<target>.tar.gz`
+ 同名 `.sha256`；Windows 为 `.zip`。target 对照：

| 系统/架构 | target |
| --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-musl`（静态通用）或 `x86_64-unknown-linux-gnu` |
| macOS Apple Silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Windows x86_64 | `x86_64-pc-windows-msvc`（`.zip`） |

```sh
TAG=v0.1.1   # 以实际最新 release 为准
T=x86_64-unknown-linux-musl
mkdir -p ~/.local/bin && cd "$(mktemp -d)"
curl -LO "https://github.com/zhaohang1205/horae/releases/download/${TAG}/horae-${TAG}-${T}.tar.gz"
curl -LO "https://github.com/zhaohang1205/horae/releases/download/${TAG}/horae-${TAG}-${T}.tar.gz.sha256"
sha256sum -c "horae-${TAG}-${T}.tar.gz.sha256"   # 校验失败立即停止并重下
tar xzf "horae-${TAG}-${T}.tar.gz"
install -m755 horae ~/.local/bin/
horae --version
```

> 自 v0.1.1 起正式 Release 均附带全平台预编译二进制（2026-08 实测确认）。
> 仅当最新 Release 又出现"只有源码包"的异常时，才需要退回路径 B。

## 路径 B：cargo install --git（装 main 最新特性）

```sh
cargo install --git https://github.com/zhaohang1205/horae
```

- 自动编译并把 `horae` 放进 `~/.cargo/bin`（确认在 PATH 中）。
- 装的是 main 分支最新提交，比已发布的 tag 新。
- 首次编译需几分钟（bundled SQLite 占大头），属正常现象。
- Windows：Rust 1.89+ 从源码构建即可，数据目录 `%APPDATA%\horae\`。

## 路径 C：源码克隆构建（开发者/审计）

```sh
git clone https://github.com/zhaohang1205/horae.git && cd horae
cargo build --release          # 产物 target/release/horae
install -m755 target/release/horae ~/.local/bin/   # 或 cargo install --path .
```

想固定到某个版本：clone 后 `git checkout <tag>` 再构建。

## 可选项对比表（安装时逐项向用户说明并勾选）

| 可选项 | 安了得到什么 | 不安会怎样 | 日后补装 |
| --- | --- | --- | --- |
| **Nerd Font**（任选一款设为终端字体） | 界面图标为丰富字形 | 自动回退纯 ASCII 字符，不会出现"豆腐块"；功能无损 | [nerdfonts.com](https://www.nerdfonts.com/) 下载或发行版包（Arch: `pacman -S ttf-nerd-fonts-symbols`），设为终端字体即可；也可用 `HORAE_ICONS=nerd\|ascii` 强制指定 |
| **Kitty 协议终端**（Kitty/Ghostty/WezTerm） | 开屏"时间女神"像素艺术完整渲染 | ASCII 文字版开屏，功能无损 | 换装终端即可，无需重装 horae |
| **libnotify / notify-send** | 番茄钟结束、任务到期的桌面弹窗 | 计时、TUI 内提醒完全正常，只是没有系统级弹窗；**Windows/macOS 本就不支持系统弹窗** | Arch: `sudo pacman -S libnotify`；Debian/Ubuntu: `sudo apt install libnotify-bin`；Fedora: `sudo dnf install libnotify` |
| **Syncthing**（手机桥） | 手机笔记 App 写一行即采集任务、回看今日快照、收到期提醒；零服务器 | 纯单机使用，其余功能全部正常 | 见 workflows.md「手机采集桥」一节；需常驻 `horae watch` |
| **waybar 模块** | 状态栏常驻 🍅 倒计时与最近到期提醒 | 手动跑 `horae stats` 查看 | waybar 配置里加 custom module 指向 `horae pomo waybar` / `horae alarm waybar` |
| **shell 补全** | Tab 补全子命令与 flag | 手打全名，不影响其他 | `horae completions bash > ~/.local/share/bash-completions/completions/horae`；zsh 输出到 fpath 目录后 `compinit` |
| **shell 别名** | 两三个字母完成高频操作（hc/hd/hf…） | 手打完整命令 | 技能自带 `scripts/install-aliases.sh`，幂等可卸载 |

交互守则：**一次性列出所有未就绪的可选项 + 各自的装/不装区别，让用户批量勾选；
绝不静默安装任何系统包。** 用户勾选的项才执行，并明确告知将运行的命令。

## 平台差异速记

| 平台 | 数据目录 | 备注 |
| --- | --- | --- |
| Linux | `~/.config/horae/` | 体验最完整（桌面通知、waybar、Kitty 协议均支持） |
| macOS | `~/Library/Application Support/horae/` | 系统级弹窗可能不触发，计时/TUI 提醒正常 |
| Windows | `%APPDATA%\horae\` | 源码构建；桌面通知不触发；WezTerm 可获完整开屏 |

## 升级

- 路径 B 升级 = 重跑同一条命令（cargo 会重新编译替换）；升级不动数据目录，任务数据无感保留。
- 路径 A 升级 = 下载新版本覆盖二进制。
- 升级前求稳可先 `horae export` 一份备份（几秒钟）。

## 卸载

1. `horae export` 留底（可选但建议）。
2. 删二进制：`rm $(command -v horae)` 或 `cargo uninstall horae`。
3. 删数据：`rm -rf ~/.config/horae/`（Linux；macOS/Windows 对应上表目录）——含全部任务与配置，谨慎执行。

## 故障排查

| 症状 | 原因与处理 |
| --- | --- |
| 编译报错 `rustc X is not supported` | 工具链低于 MSRV 1.89：`rustup update stable` 或先[安装 rustup](https://rustup.rs/) |
| 编译在 bundled sqlite 处报 C 错误 | 缺 C 编译器：Debian/Ubuntu `apt install build-essential`；Arch `pacman -S base-devel`; Fedora `dnf install gcc` |
| 装完敲 `horae` 无命令 | 二进制目录不在 PATH：把 `~/.cargo/bin`（或安装目录）加入 PATH 后重开终端 |
| `cargo install --git` 卡在网络 | 克隆 github 失败：配 git 代理（`git config --global http.proxy ...`）或改走路径 C 用镜像 |
| `sha256sum -c` 校验失败 | 下载损坏或被篡改：删除重下；反复失败则暂停安装并核对来源 |
| GitHub API 限流（preflight 显示无法确认资产） | 稍后重试，或直接浏览器打开 releases 页面人工确认 |
