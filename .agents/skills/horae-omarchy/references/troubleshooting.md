# Omarchy 环境下 Horae 常见问题排查 (Troubleshooting)

## 1. 终端提示 `horae: command not found`

- **原因**：通过 `cargo` 或 `npm` 安装的全局二进制路径未加入当前 Shell 的 `PATH`。
- **解决**：
  - 若使用 Cargo 安装，确保 `~/.bashrc` 或 `~/.zshrc` 包含：
    ```sh
    export PATH="$HOME/.cargo/bin:$PATH"
    ```
  - 若使用 npm 安装，确保全局 bin 路径在 PATH 中（如 `$(npm prefix -g)/bin` 或 `~/.local/share/npm/bin`）。
  - 执行 `source ~/.zshrc`（或对应 rc）重载环境。

## 2. Hyprland 快捷键未生效或报错

- **排查步骤**：
  1. 运行 `hyprctl configerrors` 检查 Lua 语法是否存在错误；
  2. 运行 `hyprctl reload` 强制重载 Hyprland；
  3. 运行 `omarchy menu keybindings --print | grep -i horae` 确认快捷键是否已被合成器正确识别；
  4. 确认 `~/.local/bin/horae-flash`、`horae-normal`、`horae-clip` 拥有执行权限（`chmod +x ~/.local/bin/horae-*`）。

## 3. 按 `CTRL + 1` 剪贴板录入后没有桌面通知提示

- **排查步骤**：
  1. 运行 `which notify-send` 确认桌面通知工具存在；
  2. 手动执行测试：`notify-send "测试" "Horae 提示"`；
  3. 确认剪贴板有内容（空文本不会创建待办）。

## 4. 状态栏未显示 Horae 组件

- **排查步骤**：
  1. 运行 `omarchy plugin list`，确认列表中存在 `io.github.zhaohang1205.horae-tasks`；
  2. 若已存在但状态栏没显示，运行：
     ```bash
     omarchy bar move io.github.zhaohang1205.horae-tasks --section center
     omarchy-shell shell rescanPlugins
     ```
  3. 若依然无响应，可重启 Omarchy Shell：`omarchy restart shell`。

## 5. 图标显示为方块或乱码

- **原因**：终端未正确配置 Nerd Font。
- **解决**：
  - Omarchy 默认自带 Nerd Font，若终端字库被更改，可在终端配置中选择包含 Nerd Font 的等宽字体（如 JetBrainsMono Nerd Font）；
  - 或者强制使用 ASCII 纯文本图标模式：在 `~/.zshrc` 中添加：
    ```sh
    export HORAE_ICONS=ascii
    ```
  - 或在 TUI 中按下 `F7` 打开模块开关弹窗，切换图标样式为 ASCII。

## 6. 主题与 Omarchy 系统主题不一致

- Horae 原生内置 23 款官方主题，且具备 Omarchy 主题自动感知能力；
- 若需手动切换，可在 TUI 中按 `F5` 快速轮换主题；
- 或在命令行启动时指定 `--theme <主题名>`（例如 `horae --theme catppuccin`）。
