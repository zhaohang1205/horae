#!/usr/bin/env bash
# setup-omarchy.sh - 一键配置 Horae 在 Omarchy 系统上的最佳原生集成
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

log()   { printf "${BLUE}[Horae-Omarchy]${NC} %s\n" "$*"; }
ok()    { printf "${GREEN}[✓]${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}[!]${NC} %s\n" "$*"; }
err()   { printf "${RED}[✗]${NC} %s\n" "$*" >&2; }

# 解析命令行参数
DO_INSTALL_CARGO=false
DO_INSTALL_NPM=false
DO_ALIASES=false
DO_SHORTCUTS=false
DO_BAR=false
DO_COMPLETIONS=false
DO_DESKTOP=false

if [ $# -eq 0 ]; then
    DO_ALIASES=true
    DO_SHORTCUTS=true
    DO_BAR=true
    DO_COMPLETIONS=true
    DO_DESKTOP=true
fi

while [ $# -gt 0 ]; do
    case "$1" in
        --all)
            DO_ALIASES=true
            DO_SHORTCUTS=true
            DO_BAR=true
            DO_COMPLETIONS=true
            DO_DESKTOP=true
            ;;
        --install-cargo)
            DO_INSTALL_CARGO=true
            ;;
        --install-npm)
            DO_INSTALL_NPM=true
            ;;
        --aliases)
            DO_ALIASES=true
            ;;
        --shortcuts)
            DO_SHORTCUTS=true
            ;;
        --bar)
            DO_BAR=true
            ;;
        --completions)
            DO_COMPLETIONS=true
            ;;
        --desktop)
            DO_DESKTOP=true
            ;;
        -h|--help)
            echo "用法: $0 [选项]"
            echo "选项:"
            echo "  --all             配置除安装之外的所有项目 (别名、快捷键、状态栏、补全、桌面图标)"
            echo "  --install-cargo   使用 cargo install horae 安装/更新"
            echo "  --install-npm     使用 npm i -g horae-cli 安装/更新"
            echo "  --aliases         配置 hc / hcc shell 别名"
            echo "  --shortcuts       安装快捷启动脚本并配置 Hyprland 快捷键与浮动窗口"
            echo "  --bar             安装并启用 Omarchy 状态栏组件 (io.github.zhaohang1205.horae-tasks)"
            echo "  --completions     安装 shell 自动补全 (bash, zsh, fish)"
            echo "  --desktop         安装 desktop 应用菜单图标"
            exit 0
            ;;
        *)
            err "未知选项: $1"
            exit 1
            ;;
    esac
    shift
done

# 0. 权限与私有配置目录保障
mkdir -p -m 700 "$HOME/.config/horae"
mkdir -p "$HOME/.local/bin"

find_horae() {
    if command -v horae >/dev/null 2>&1; then
        command -v horae
    elif [ -x "$HOME/.cargo/bin/horae" ]; then
        echo "$HOME/.cargo/bin/horae"
    elif [ -x "$HOME/.local/bin/horae" ]; then
        echo "$HOME/.local/bin/horae"
    else
        echo ""
    fi
}

# 1. 安装/更新 Horae
if [ "$DO_INSTALL_CARGO" = true ]; then
    log "正在使用 Cargo 安装 horae..."
    cargo install horae
    ok "Cargo 安装完成: $(find_horae)"
fi

if [ "$DO_INSTALL_NPM" = true ]; then
    log "正在使用 npm 安装 horae-cli..."
    npm install -g horae-cli
    ok "npm 安装完成: $(find_horae)"
fi

HORAE_BIN=$(find_horae)
if [ -z "$HORAE_BIN" ]; then
    warn "未检测到已安装的 horae 二进制文件。"
    warn "您可以稍后运行: cargo install horae 或 npm i -g horae-cli"
fi

# 2. 安装辅助快捷脚本 (horae-flash, horae-normal, horae-clip)
install_helper_scripts() {
    log "正在安装快捷脚本至 ~/.local/bin/ ..."
    cp "$SCRIPT_DIR/horae-flash" "$HOME/.local/bin/horae-flash"
    cp "$SCRIPT_DIR/horae-normal" "$HOME/.local/bin/horae-normal"
    cp "$SCRIPT_DIR/horae-clip" "$HOME/.local/bin/horae-clip"
    chmod +x "$HOME/.local/bin/horae-flash" "$HOME/.local/bin/horae-normal" "$HOME/.local/bin/horae-clip"
    ok "快捷脚本安装完成 (~/.local/bin/horae-{flash,normal,clip})"
}

# 3. 配置 Hyprland 快捷键与窗口规则
setup_hyprland() {
    install_helper_scripts

    local BINDINGS_LUA="$HOME/.config/hypr/bindings.lua"
    local HYPRLAND_LUA="$HOME/.config/hypr/hyprland.lua"

    if [ -f "$BINDINGS_LUA" ]; then
        log "正在检查 Hyprland 快捷键配置 ($BINDINGS_LUA)..."
        local NEED_RELOAD=false

        # 检查是否已有 horae-flash 快捷键
        if ! grep -q "horae-flash" "$BINDINGS_LUA"; then
            log "添加闪念录入快捷键 (SUPER + I)..."
            cat >> "$BINDINGS_LUA" << 'EOF'

-- Horae: Flash Capture (闪念录入)
o.bind("SUPER + I", "Horae (Flash)", { tui = "horae-flash", focus = true })
EOF
            NEED_RELOAD=true
        else
            ok "闪念录入快捷键已有配置，跳过重复写入"
        fi

        # 检查是否已有 horae-normal 快捷键
        if ! grep -q "horae-normal" "$BINDINGS_LUA"; then
            log "添加主应用一般模式快捷键 (SUPER + SHIFT + I)..."
            cat >> "$BINDINGS_LUA" << 'EOF'

-- Horae: Normal Mode (主应用交互)
o.bind("SUPER + SHIFT + I", "Horae (Normal)", { tui = "horae-normal", focus = true })
EOF
            NEED_RELOAD=true
        else
            ok "主应用模式快捷键已有配置，跳过重复写入"
        fi

        # 检查是否已有 horae-clip 快捷键
        if ! grep -q "horae-clip" "$BINDINGS_LUA"; then
            log "添加剪贴板快速录入快捷键 (CTRL + 1)..."
            cat >> "$BINDINGS_LUA" << 'EOF'

-- Horae: Clipboard Capture (剪贴板录入)
o.bind("CTRL + 1", "Horae clip capture", os.getenv("HOME") .. "/.local/bin/horae-clip")
EOF
            NEED_RELOAD=true
        else
            ok "剪贴板录入快捷键已有配置，跳过重复写入"
        fi
    fi

    if [ -f "$HYPRLAND_LUA" ]; then
        log "正在检查 Hyprland 浮动窗口规则 ($HYPRLAND_LUA)..."
        if ! grep -q "org\.omarchy\.horae-flash" "$HYPRLAND_LUA"; then
            cat >> "$HYPRLAND_LUA" << 'EOF'

-- Horae Flash Capture (居中浮动小窗)
o.window("^org\\.omarchy\\.horae-flash$", {
	float = true,
	center = true,
	size = { 800, 600 },
})

-- Horae TUI (居中浮动 3/4 屏幕)
o.window("^(org\\.omarchy\\.horae|org\\.omarchy\\.horae-normal)$", {
	float = true,
	center = true,
	size = { "(monitor_w*0.75)", "(monitor_h*0.75)" },
})
EOF
            ok "已注入 Horae 浮动窗口规则"
            NEED_RELOAD=true
        else
            ok "Horae 浮动窗口规则已有配置，跳过重复写入"
        fi
    fi

    if [ "${NEED_RELOAD:-false}" = true ] && command -v hyprctl >/dev/null 2>&1; then
        hyprctl reload >/dev/null 2>&1 || true
        ok "Hyprland 配置已即时重载"
    fi
}

# 4. 配置 Shell 别名 (hc, hcc)
setup_aliases() {
    log "正在检查并添加 Shell 别名 (hc, hcc)..."
    
    local TARGET_RCS=()
    [ -f "$HOME/.zshrc" ] && TARGET_RCS+=("$HOME/.zshrc")
    [ -f "$HOME/.bashrc" ] && TARGET_RCS+=("$HOME/.bashrc")

    for rc in "${TARGET_RCS[@]}"; do
        local MODIFIED=false
        if ! grep -Eq '^[[:space:]]*alias[[:space:]]+hc=' "$rc" 2>/dev/null; then
            echo "alias hc='horae capture'                 # 快速捕获任务" >> "$rc"
            MODIFIED=true
            ok "已在 $rc 中添加 alias hc='horae capture'"
        else
            ok "$rc 中已存在 alias hc，保留用户配置"
        fi

        if ! grep -Eq '^[[:space:]]*alias[[:space:]]+hcc=' "$rc" 2>/dev/null; then
            echo "alias hcc='horae capture --clip'         # 快速捕获剪贴板内容" >> "$rc"
            MODIFIED=true
            ok "已在 $rc 中添加 alias hcc='horae capture --clip'"
        else
            ok "$rc 中已存在 alias hcc，保留用户配置"
        fi
    done

    # fish shell 支持
    local FISH_CONFIG="$HOME/.config/fish/config.fish"
    if [ -f "$FISH_CONFIG" ]; then
        if ! grep -Eq '^[[:space:]]*alias[[:space:]]+hc=' "$FISH_CONFIG" 2>/dev/null; then
            echo "alias hc='horae capture'" >> "$FISH_CONFIG"
            ok "已在 $FISH_CONFIG 中添加 alias hc"
        fi
        if ! grep -Eq '^[[:space:]]*alias[[:space:]]+hcc=' "$FISH_CONFIG" 2>/dev/null; then
            echo "alias hcc='horae capture --clip'" >> "$FISH_CONFIG"
            ok "已在 $FISH_CONFIG 中添加 alias hcc"
        fi
    fi
}

# 5. 安装 Shell 自动补全
setup_completions() {
    if [ -z "$HORAE_BIN" ]; then
        warn "未找到 horae 命令，跳过自动补全生成"
        return
    fi
    log "正在生成 Shell 自动补全脚本..."

    # Bash 补全
    local BASH_COMP_DIR="$HOME/.local/share/bash-completion/completions"
    mkdir -p "$BASH_COMP_DIR"
    "$HORAE_BIN" completions bash > "$BASH_COMP_DIR/horae" 2>/dev/null || true
    ok "Bash 补全已写入 $BASH_COMP_DIR/horae"

    # Zsh 补全
    local ZSH_COMP_DIR="$HOME/.local/share/zsh/site-functions"
    mkdir -p "$ZSH_COMP_DIR"
    "$HORAE_BIN" completions zsh > "$ZSH_COMP_DIR/_horae" 2>/dev/null || true
    ok "Zsh 补全已写入 $ZSH_COMP_DIR/_horae"

    # Fish 补全
    local FISH_COMP_DIR="$HOME/.config/fish/completions"
    mkdir -p "$FISH_COMP_DIR"
    "$HORAE_BIN" completions fish > "$FISH_COMP_DIR/horae.fish" 2>/dev/null || true
    ok "Fish 补全已写入 $FISH_COMP_DIR/horae.fish"
}

# 6. 配置 Omarchy 状态栏插件
setup_bar_widget() {
    local PLUGIN_ID="io.github.zhaohang1205.horae-tasks"
    local TARGET_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
    local LOCAL_REPO="$HOME/Projects/horae_for_omarchy_shell"

    log "正在配置 Omarchy 状态栏组件 ($PLUGIN_ID)..."
    mkdir -p "$HOME/.config/omarchy/plugins"

    if [ -d "$LOCAL_REPO" ]; then
        if [ "$LOCAL_REPO" != "$TARGET_DIR" ]; then
            rm -rf "$TARGET_DIR"
            ln -s "$LOCAL_REPO" "$TARGET_DIR"
            ok "已软链接本地组件仓库至: $TARGET_DIR"
        fi
    elif [ ! -d "$TARGET_DIR" ]; then
        if command -v omarchy >/dev/null 2>&1; then
            omarchy plugin add https://github.com/zhaohang1205/horae_for_omarchy_shell.git --enable 2>/dev/null || true
            ok "通过 omarchy plugin add 安装完成"
        fi
    fi

    # 启用并在状态栏中配置位置
    if command -v omarchy >/dev/null 2>&1; then
        omarchy plugin enable "$PLUGIN_ID" --section center 2>/dev/null || true
    fi

    if command -v omarchy-shell >/dev/null 2>&1; then
        omarchy-shell shell setBarWidget "$PLUGIN_ID" "maxWidth" "420" "{}" >/dev/null 2>&1 || true
        omarchy-shell shell setBarWidget "$PLUGIN_ID" "autoDismiss" "15" "{}" >/dev/null 2>&1 || true
        omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
        ok "已应用推荐状态栏参数 (maxWidth: 420, autoDismiss: 15s) 并热重载"
    fi
}

# 7. 安装 Desktop 菜单图标
setup_desktop_entry() {
    log "正在生成桌面应用启动器 (Desktop Entry)..."
    local APPS_DIR="$HOME/.local/share/applications"
    mkdir -p "$APPS_DIR"
    cat > "$APPS_DIR/horae.desktop" << 'EOF'
[Desktop Entry]
Name=Horae
GenericName=GTD Task Manager
Comment=Capture and manage GTD tasks with Pomodoro in terminal
Exec=omarchy-launch-or-focus-tui horae
Icon=task-due-symbolic
Terminal=false
Type=Application
Categories=Office;ProjectManagement;Utility;
Keywords=gtd;todo;task;pomodoro;horae;
StartupWMClass=org.omarchy.horae
EOF
    chmod 644 "$APPS_DIR/horae.desktop"
    ok "Desktop 文件已创建: $APPS_DIR/horae.desktop"
}

# 执行选中的模块
[ "$DO_SHORTCUTS" = true ] && setup_hyprland
[ "$DO_ALIASES" = true ] && setup_aliases
[ "$DO_COMPLETIONS" = true ] && setup_completions
[ "$DO_BAR" = true ] && setup_bar_widget
[ "$DO_DESKTOP" = true ] && setup_desktop_entry

printf "\n${BOLD}${GREEN}========================================${NC}\n"
printf "${BOLD}${GREEN}   Horae for Omarchy 配置完成！        ${NC}\n"
printf "${BOLD}${GREEN}========================================${NC}\n"
printf "体验指南:\n"
printf "  • 按 ${BOLD}SUPER + I${NC} 呼出闪念录入浮窗（打完回车秒级退出并保存入收件箱）\n"
printf "  • 按 ${BOLD}SUPER + SHIFT + I${NC} 呼出/聚焦 Horae 全功能 GTD 终端界面\n"
printf "  • 按 ${BOLD}CTRL + 1${NC} 直接将当前剪贴板文本写入待办（伴随桌面提示）\n"
printf "  • 终端中使用 ${BOLD}hc \"任务内容 @tag ~时间\"${NC} 极速录入\n"
printf "  • 状态栏已展示 Horae 番茄钟环与到期任务卡片（点击可开番茄/选任务）\n\n"
