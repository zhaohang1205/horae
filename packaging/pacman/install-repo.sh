#!/usr/bin/env bash
set -euo pipefail

# horae pacman repository installer for Arch Linux & Omarchy
REPO_NAME="horae"
REPO_URL="https://zhaohang1205.github.io/horae/arch/\$arch"
PACMAN_CONF="/etc/pacman.conf"

if [[ $EUID -ne 0 ]]; then
    echo "❌ 错误: 请以 root 权限运行此脚本 (例如使用 sudo):"
    echo "   curl -sSL https://zhaohang1205.github.io/horae/install-repo.sh | sudo bash"
    exit 1
fi

echo "==> 正在为 Arch Linux / Omarchy 配置 [$REPO_NAME] 官方软件源..."

# 检查是否已配置该源
if grep -q "^\[$REPO_NAME\]" "$PACMAN_CONF"; then
    echo "ℹ️  $PACMAN_CONF 中已配置 [$REPO_NAME] 源，跳过追加。"
else
    echo "==> 追加 [$REPO_NAME] 源到 $PACMAN_CONF..."
    cat << EOF >> "$PACMAN_CONF"

[$REPO_NAME]
SigLevel = Optional TrustAll
Server = $REPO_URL
EOF
    echo "✅ 源配置已写入 $PACMAN_CONF"
fi

echo "==> 正在刷新 pacman 数据库..."
pacman -Sy

echo "--------------------------------------------------------"
echo "🎉 horae 软件源配置完成！"
echo ""
echo "现在你可以直接运行以下命令安装 horae:"
echo "   sudo pacman -S horae"
echo ""
echo "安装后立即可享:"
echo "  • 原生预编译二进制秒级安装"
echo "  • 自动配置 Bash / Zsh / Fish 命令补全"
echo "  • 系统升级 (pacman -Syu) 自动跟进最新版本"
echo "--------------------------------------------------------"
