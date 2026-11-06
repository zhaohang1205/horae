# Omarchy 状态栏组件 (Quickshell Bar Widget) 指南

Horae 为 Omarchy 的 Quattro Status Bar 量身打造了专属组件：`io.github.zhaohang1205.horae-tasks`（开源仓库：[horae_for_omarchy_shell](https://github.com/zhaohang1205/horae_for_omarchy_shell)）。

## 1. 核心功能

1. **🍅 番茄钟仪表与动态进度环**：
   - 专注进行中时，任务图标动态替换为递减进度圆环（`PomoRing`），倒计时实时内嵌呈现；
   - 弹窗顶部呈现大字倒计时与进度点（朝向长休的阶段点，如 `●●○`）；
   - 双击状态栏番茄入口可快速停止，右击直达 TUI。
2. **🔒 状态栏隐私防窥模式**：
   - 鼠标悬停组件最左侧呼出眼睛图标，一键开启隐私模式；
   - 状态栏上隐藏任务标题，仅保留时间（如 `⏰ 15:30` 或 `🍅 24:50`），防止旁人窥视敏感事项；
   - 鼠标悬停条目时仍会在浮窗 Tooltip 中明文展示。
3. **⚡ 闪念快速录入入口**：
   - 左击番茄钟旁录入入口，立即唤出浮动闪念录入窗口（`horae --flash`）。
4. **⚙️ 番茄钟参数可视化微调**：
   - 弹窗内置 2×2 网格，自由调整专注、短休、长休时长与轮数，修改实时同步至 `~/.config/horae/pomo.json`。
5. **极低系统资源占用**：
   - 纯文件事件驱动（监听 `horae.db` 与 `pomo.json`），毫秒级响应，无需轮询。

## 2. 安装与管理命令

### 安装并添加到状态栏
```bash
# 1. 添加并启用插件
omarchy plugin add https://github.com/zhaohang1205/horae_for_omarchy_shell.git --enable

# 2. 将组件移动到状态栏居中位置 (或 right / left)
omarchy bar move io.github.zhaohang1205.horae-tasks --section center

# 3. 注入推荐参数 (最大宽度 420px，15秒无操作自动收起弹窗)
omarchy-shell shell setBarWidget "io.github.zhaohang1205.horae-tasks" "maxWidth" "420" "{}"
omarchy-shell shell setBarWidget "io.github.zhaohang1205.horae-tasks" "autoDismiss" "15" "{}"
omarchy-shell shell rescanPlugins
```

### 本地开发链接方式
若本地已有克隆目录，可通过软链接至 `~/.config/omarchy/plugins/io.github.zhaohang1205.horae-tasks`：
```bash
ln -s /path/to/horae_for_omarchy_shell ~/.config/omarchy/plugins/io.github.zhaohang1205.horae-tasks
omarchy-shell shell rescanPlugins
```

### 安全卸载
卸载仅移除组件，绝不会破坏任何本地任务数据：
```bash
omarchy plugin remove io.github.zhaohang1205.horae-tasks
```
