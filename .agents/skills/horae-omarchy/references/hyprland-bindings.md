# Hyprland 快捷键与窗口规则指南 (Omarchy)

Omarchy 桌面环境使用 Lua 作为 Hyprland 的配置语言。用户级快捷键位于 `~/.config/hypr/bindings.lua`，窗口行为规则位于 `~/.config/hypr/hyprland.lua`。

## 1. 核心快捷键架构

Horae 与 Omarchy 系统结合时，推荐配置三组核心交互方式：

| 快捷键 | 动作 | 原理与调用目标 | 体验特色 |
|---|---|---|---|
| `SUPER + I` | 闪念录入模式 (Flash) | `{ tui = "horae-flash", focus = true }` | 居中浮动小窗，快速输入标题/标签/周期，回车后瞬间保存入库并秒级关闭，零上下文干扰 |
| `SUPER + SHIFT + I` | 应用一般模式 (Normal) | `{ tui = "horae-normal", focus = true }` | 呼出或聚焦（已打开时自动切到前台）完整的 GTD 交互视图 |
| `CTRL + 1` | 剪贴板快速捕获 (Clip) | `~/.local/bin/horae-clip` | 静默捕获系统剪贴板（或选中文本）至收件箱，并弹出 `notify-send` 确认提示 |
| `SUPER + ALT + V` | （可选）语音录入待办 | `~/.local/bin/horae-voice` | 调用 Omarchy 内置的 Voxtype 离线 Whisper 模型，自然语言抽取时间与优先级 |

## 2. bindings.lua 配置示例

在 `~/.config/hypr/bindings.lua` 中追加：

```lua
-- ============================================================================
-- Horae GTD 快捷键
-- ============================================================================

-- 闪念录入模式：按下立即弹出小型录入框
o.bind("SUPER + I", "Horae (Flash)", { tui = "horae-flash", focus = true })

-- 一般模式：按下平滑呼出或聚焦完整交互视图
o.bind("SUPER + SHIFT + I", "Horae (Normal)", { tui = "horae-normal", focus = true })

-- 剪贴板录入：将系统剪切板内容瞬间录入为任务
o.bind("CTRL + 1", "Horae clip capture", os.getenv("HOME") .. "/.local/bin/horae-clip")
```

### 快捷键冲突排查
在添加快捷键前，可运行以下命令检查是否有按键冲突：
```bash
omarchy menu keybindings --print | grep "SUPER + I"
```
若发现冲突，可选择使用 `o.rebind(...)` 替代原有绑定，或改为用户偏好的其他键位（如 `SUPER + ALT + C`）。

## 3. hyprland.lua 浮动与尺寸规则

为了确保弹窗不会以平铺（Tile）方式破坏当前桌面的窗口排布，在 `~/.config/hypr/hyprland.lua` 中配置窗口规则：

```lua
-- Horae Flash Capture: 紧凑居中浮动小窗 (800x600)
o.window("^org\\.omarchy\\.horae-flash$", {
	float = true,
	center = true,
	size = { 800, 600 },
})

-- Horae TUI: 居中浮动视窗 (占屏幕 75% 宽高，兼顾沉浸感与空间感)
o.window("^(org\\.omarchy\\.horae|org\\.omarchy\\.horae-normal)$", {
	float = true,
	center = true,
	size = { "(monitor_w*0.75)", "(monitor_h*0.75)" },
})
```

## 4. 辅助启动脚本说明

`~/.local/bin/` 目录下的辅助脚本封装了 `HORAE_BIN` 探测与终端隔离启动：
- `horae-flash`：`exec "$HORAE_BIN" -f "$@"`
- `horae-normal`：`exec "$HORAE_BIN" -n "$@"`
- `horae-clip`：`horae capture --clip "$@"` 并处理返回信息通过 `notify-send` 桌面通知。
