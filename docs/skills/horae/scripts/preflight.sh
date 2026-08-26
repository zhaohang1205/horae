#!/usr/bin/env bash
# horae 安装前体检（严格只读）：检测系统、前置条件与可选组件就绪度。
# 本脚本不写入任何文件、不安装任何东西；所有变更由使用者另行决定执行。
#
# 用法: ./preflight.sh [--json]
set -uo pipefail

REPO="zhaohang1205/horae"
MSRV_MAJOR=1
MSRV_MINOR=89

ok() { printf '  [\033[32m✓\033[0m] %s\n' "$1"; }
bad() { printf '  [\033[31mx\033[0m] %s\n' "$1"; }
info() { printf '  [\033[33m·\033[0m] %s\n' "$1"; }
warn() { printf '  [\033[33m!\033[0m] %s\n' "$1"; }
section() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

JSON_MODE=0
[[ ${1:-} == "--json" ]] && JSON_MODE=1

# ---------- 1. 系统与推荐产物 ----------
section "系统"
os="$(uname -s)"
arch="$(uname -m)"
target=""
case "$os" in
  Linux) base="Linux" ;;
  Darwin) base="macOS" ;;
  MINGW* | MSYS* | CYGWIN*) base="Windows" ;;
  *) base="$os" ;;
esac
if [[ $base == "Linux" ]]; then
  if [[ $arch == "x86_64" ]]; then
    target="x86_64-unknown-linux-musl"
    info "Linux x86_64 → 推荐预编译资产: horae-<tag>-${target}.tar.gz（musl 静态，通用免装依赖）"
    have ldd && info "glibc: $(ldd --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+$' | head -1)（选 gnu 构建亦可）"
  elif [[ $arch == "aarch64" || $arch == "arm64" ]]; then
    info "Linux $arch → 暂无对应预编译资产，需走源码编译（路径 C）"
  else
    info "Linux $arch → 暂无对应预编译资产，需走源码编译"
  fi
fi
macos_prefix=""
if [[ $base == "macOS" ]]; then
  if [[ $arch == "arm64" ]]; then target="aarch64-apple-darwin"; macos_prefix="/opt/homebrew"; else target="x86_64-apple-darwin"; macos_prefix="/usr/local"; fi
  info "macOS $arch → 推荐预编译资产: horae-<tag>-${target}.tar.gz"
  info "macOS 推荐安装前缀: ${macos_prefix}/bin（Homebrew 标准目录）"
elif [[ $base == "Windows" ]]; then
  target="x86_64-pc-windows-msvc"
  info "Windows → 推荐预编译资产: horae-<tag>-${target}.zip"
fi

# ---------- 2. 必需前置 ----------
section "必需前置"
have git && ok "git: $(git --version 2>/dev/null)" || bad "git 缺失（克隆源码必需）"

dl="curl"
have curl || dl="wget"
{ have curl || have wget; } && ok "下载工具: $dl" || bad "curl/wget 均缺失"

rust_ok=0
if have rustc; then
  ver="$(rustc --version 2>/dev/null)" # e.g. rustc 1.89.0 (...)
  major="$(echo "$ver" | sed -E 's/rustc ([0-9]+)\..*/\1/')"
  minor="$(echo "$ver" | sed -E 's/rustc [0-9]+\.([0-9]+).*/\1/')"
  if [[ ${major:-0} -gt $MSRV_MAJOR || ( $major -eq $MSRV_MAJOR && ${minor:-0} -ge $MSRV_MINOR ) ]]; then
    ok "rustc $ver （≥1.89，满足 MSRV）"
    rust_ok=1
  else
    bad "rustc $ver 过旧（MSRV=1.89），编译前需 rustup update 或安装 rustup"
  fi
else
  info "未安装 Rust 工具链（仅源码编译需要；预编译二进制不需要）"
fi

cc_found=""
for c in cc gcc clang; do
  if have "$c"; then cc_found="$($c --version 2>/dev/null | head -1)"; break; fi
done
if [[ -n $cc_found ]]; then
  ok "C 编译器: $cc_found （rusqlite bundled 编译 SQLite 源码所需）"
else
  if [[ $rust_ok -eq 1 ]]; then
    if [[ $base == "macOS" ]]; then
      bad "缺少 C 编译器（clang）——源码编译会在 bundled sqlite 处失败；macOS 请先运行: xcode-select --install"
    else
      bad "缺少 C 编译器（gcc/clang）——源码编译会在 bundled sqlite 处失败"
    fi
  else
    info "无 C 编译器（不影响预编译二进制路径）"
  fi
fi

# PATH 检查
path_ok=0
for d in "$HOME/.local/bin" "$HOME/.cargo/bin" "${macos_prefix:+$macos_prefix/bin}"; do
  [[ -n $d ]] || continue
  case ":$PATH:" in *":$d:"*) path_ok=1 ;; esac
done
if [[ $path_ok -eq 1 ]]; then
  ok "PATH 包含可用的 bin 目录（~/.local/bin / ~/.cargo/bin${macos_prefix:+/ ${macos_prefix}/bin}）"
else
  if [[ $base == "macOS" ]]; then
    warn "PATH 不含常见 bin 目录 —— 安装后请把 ${macos_prefix}/bin 加入 ~/.zshrc：export PATH=\"${macos_prefix}/bin:\$PATH\""
  else
    warn "PATH 不含 ~/.local/bin 与 ~/.cargo/bin —— 安装后需手动加 PATH 或选用系统目录"
  fi
fi

# ---------- 3. 网络 ----------
section "网络"
if { have curl || have wget; } && { curl -sI --max-time 8 -o /dev/null https://github.com 2>/dev/null || wget -q --spider -T 8 https://github.com 2>/dev/null; }; then
  ok "github.com 可达"
else
  bad "无法访问 github.com（下载/克隆将失败；检查代理或网络）"
fi

# ---------- 4. 已安装情况 ----------
section "已安装"
if have horae; then
  ok "horae 已安装: $(horae --version 2>/dev/null) @ $(command -v horae)"
  info "升级方式见 references/install.md（数据不受升级影响）"
else
  info "horae 未安装（全新安装）"
fi

# ---------- 5. 最新 Release 是否附带预编译二进制 ----------
section "Release 资产探测"
latest_json=""
have curl && latest_json="$(curl -s --max-time 10 -H 'User-Agent: horae-preflight' -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null)"
bin_assets=""
if echo "$latest_json" | grep -q '"tag_name"'; then
  tag="$(echo "$latest_json" | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')"
  bin_assets="$(echo "$latest_json" | grep -o '"name": *"horae-[^"]*\.\(tar\.gz\|zip\)"' | sed 's/.*"\(horae[^"]*\)"$/\1/' | sort -u)"
  if [[ -n $bin_assets ]]; then
    ok "最新 Release $tag 附带预编译二进制（预编译路径可用）:"
    echo "$bin_assets" | while read -r a; do printf '      - %s\n' "$a"; done
  else
    info "最新 Release $tag 未附预编译二进制（仅源码包）→ 当前请走 cargo install --git"
  fi
else
  info "GitHub API 不可达/限流，无法确认 Release 资产（可稍后重试或直接看 releases 页面）"
fi

# ---------- 6. 终端能力 ----------
section "终端体验（纯观感）"
term_prog="${TERM_PROGRAM:-}${KITTY_WINDOW_ID:+kitty}"
case "$term_prog${TERM:-}" in
  *ghostty*) ok "Ghostty（支持 Kitty 图形协议 → 开屏像素艺术完整）" ;;
  *kitty*) ok "Kitty（图形协议开屏完整）" ;;
  *WezTerm*) ok "WezTerm（支持 Kitty 图形协议）" ;;
  *) info "当前终端未知是否支持 Kitty 协议；不支持则 ASCII 开屏回退，功能无损" ;;
esac
if have fc-list && fc-list 2>/dev/null | grep -qi nerd; then
  ok "检测到 Nerd Font（图标字形完整）"
elif [[ $base == "macOS" ]]; then
  nerdfont_hit=0
  for fd in "$HOME/Library/Fonts" "/Library/Fonts" "/System/Library/Fonts"; do
    [[ -d $fd ]] || continue
    if ls "$fd" 2>/dev/null | grep -qi nerd; then nerdfont_hit=1; break; fi
  done
  if [[ $nerdfont_hit -eq 1 ]]; then
    ok "检测到 Nerd Font（图标字形完整）"
  else
    info "未检测到 Nerd Font（图标自动回退 ASCII）；macOS 可: brew install --cask font-hack-nerd-font"
  fi
else
  info "未检测到 Nerd Font（图标自动回退 ASCII，不会出现豆腐块）"
fi

# ---------- 7. 可选组件（供批量勾选询问） ----------
section "可选组件状态"
if have notify-send; then
  ok "notify-send 已装 → 番茄钟/到期桌面弹窗可用"
elif [[ $base == "macOS" ]]; then
  if have terminal-notifier || have osascript; then
    ok "检测到 terminal-notifier/osascript → 可自建 macOS 原生通知（horae 不含内置，见 install.md）"
  else
    info "macOS 无系统弹窗（horae 不含内置）；可自建: brew install terminal-notifier，详见 install.md"
  fi
else
  info "notify-send 未装 → 桌面弹窗通知不可用（计时与 TUI 内提醒不受影响；macOS 可自建/走 ntfy）"
fi
have syncthing && ok "syncthing 已装 → 可启用手机采集桥" ||
  info "syncthing 未装 → 单机模式（手机桥需另行安装并常驻 watch）"
if have waybar || pgrep -x waybar >/dev/null 2>&1; then
  ok "waybar 在用 → 可挂 pomo/alarm 状态栏模块"
else
  info "未使用 waybar → 跳过状态栏模块集成"
fi
rc_hit=0
for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
  if grep -qF "# >>> horae aliases >>>" "$rc" 2>/dev/null; then rc_hit=1; fi
done
[[ $rc_hit -eq 1 ]] && ok "horae 别名块已安装于 shell rc" ||
  info "shell 别名未安装（可用技能自带 scripts/install-aliases.sh 一键配置）"
comp_hit=0
for f in \
  "$HOME/.local/share/bash-completions/completions/horae" \
  "$HOME/.local/share/zsh/site-functions/_horae"; do
  [[ -f $f ]] && comp_hit=1
done
[[ $comp_hit -eq 1 ]] && ok "shell 补全已安装" ||
  info "shell 补全未检测到（可选装，horae completions <shell> 生成）"

# ---------- 8. macOS 专属提示 ----------
if [[ $base == "macOS" ]]; then
  section "macOS 专属提示"
  info "从网络下载的二进制可能被 Gatekeeper 隔离，首次运行报『无法验证开发者』或 zsh: killed"
  info "解隔离: xattr -dr com.apple.quarantine \"$(command -v horae 2>/dev/null || echo /path/to/horae)\""
  info "若仍拦截：系统设置 → 隐私与安全性 → 仍要打开；源码编译需先 xcode-select --install"
fi

printf '\n体检完成（本脚本只读，未做任何变更）。\n'
