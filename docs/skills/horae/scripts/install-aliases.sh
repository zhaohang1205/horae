#!/usr/bin/env bash
# horae 别名一键安装器（幂等）：把高频别名块追加到 bash/zsh 的 rc 文件。
#
# 用法:
#   ./install-aliases.sh            # 自动按 $SHELL 选择 ~/.bashrc 或 ~/.zshrc
#   ./install-aliases.sh -z         # 强制装进 ~/.zshrc
#   ./install-aliases.sh -b         # 强制装进 ~/.bashrc
#   ./install-aliases.sh --remove   # 从两个 rc 文件移除已装的块
set -euo pipefail

MARK_BEGIN="# >>> horae aliases >>>"
MARK_END="# <<< horae aliases <<<"

ALIAS_BODY="alias h='horae'                          # 入口：无参数即 TUI
alias hc='horae capture'                 # 捕获:   hc \"买牛奶 @home ~今天 !b\"
alias hl='horae list'                    # 列出全部
alias hin='horae list --status inbox'    # 收件箱堆积
alias hn='horae list --status next'      # 下一步行动
alias hs='horae show'                    # 详情+时间线
alias hd='horae done'                    # 完成(循环任务=重排)
alias hw='horae wait'                    # 转等待中
alias hsch='horae schedule'              # 排期/加循环
alias hf='horae do'                      # 推荐当前最该做的一件事
alias hfp='horae do --start'             # 推荐 + 直接开番茄钟
alias hlog='horae log'                   # 时间戳日志
alias hrev='horae review'                # 周回顾摘要
alias hstats='horae stats'               # 看板
alias hbk='horae export'                 # 备份到 JSON
alias htags='horae tags'                 # 标签库"

HEADER="# horae (GTD terminal task manager) 高频别名 —— 由 docs/skills/horae/scripts/install-aliases.sh 生成"

BLOCK_BASH="$MARK_BEGIN
$HEADER
$ALIAS_BODY
$MARK_END"

BLOCK_ZSH="$MARK_BEGIN
$HEADER
$ALIAS_BODY

# 子命令补全（如未生效，请确认 rc 文件中已启用 compinit）
if command -v horae >/dev/null 2>&1; then
  eval \"\$(horae completions zsh)\" 2>/dev/null || true
fi
$MARK_END"

usage() { echo "用法: $0 [-b|-z|--remove]" >&2; exit 1; }

mode="auto"
for arg in "$@"; do
  case "$arg" in
    -b|--bash) mode="bash" ;;
    -z|--zsh) mode="zsh" ;;
    --remove) mode="remove" ;;
    -h|--help) usage ;;
    *) usage ;;
  esac
done

if [[ $mode == "remove" ]]; then
  removed=0
  for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
    if grep -qF "$MARK_BEGIN" "$rc" 2>/dev/null; then
      sed -i "/^${MARK_BEGIN//\//\\/}$/,/^${MARK_END//\//\\/}$/d" "$rc"
      echo "已从 $rc 移除 horae 别名块"
      removed=1
    fi
  done
  [[ $removed -eq 1 ]] || echo "未发现已安装的 horae 别名块"
  exit 0
fi

if [[ $mode == "auto" ]]; then
  case "${SHELL:-}" in
    */zsh) mode="zsh" ;;
    */bash) mode="bash" ;;
    *) echo "无法识别 \$SHELL='${SHELL:-}'，请用 -b 或 -z 指定" >&2; exit 1 ;;
  esac
fi

rc="$HOME/.bashrc"
block="$BLOCK_BASH"
if [[ $mode == "zsh" ]]; then
  rc="$HOME/.zshrc"
  block="$BLOCK_ZSH"
fi

if [[ ! -f $rc ]] && ! touch "$rc" 2>/dev/null; then
  echo "错误：无法创建 $rc" >&2
  exit 1
fi

if grep -qF "$MARK_BEGIN" "$rc" 2>/dev/null; then
  echo "跳过：$rc 已包含 horae 别名块（幂等）"
  exit 0
fi

{
  echo ""
  printf '%s\n' "$block"
} >> "$rc"
echo "已安装 horae 别名到 $rc"
echo "生效方式: source $rc （或重开终端）"
echo "验证: h stats ; hc \"第一条任务 @home\""
