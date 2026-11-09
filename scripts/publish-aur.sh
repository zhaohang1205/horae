#!/usr/bin/env bash
set -euo pipefail

# Scripts to publish horae or horae-bin to AUR (Arch User Repository)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    echo "用法: $0 [horae|horae-bin|all]"
    echo ""
    echo "参数说明:"
    echo "  horae      仅推送源码包 horae 到 AUR"
    echo "  horae-bin  仅推送二进制包 horae-bin 到 AUR"
    echo "  all        同时推送 horae 与 horae-bin 到 AUR"
    exit 1
}

TARGET="${1:-all}"
if [[ "$TARGET" != "horae" && "$TARGET" != "horae-bin" && "$TARGET" != "all" ]]; then
    usage
fi

echo "==> 检查 SSH 连接到 aur.archlinux.org..."
if ! ssh -o BatchMode=yes -o ConnectTimeout=5 -T aur@aur.archlinux.org 2>&1 | grep -qE "Interactive shell is disabled|You've successfully authenticated"; then
    echo "----------------------------------------------------------------------"
    echo "❌ 无法通过 SSH 连接到 aur@aur.archlinux.org！"
    echo ""
    echo "排查建议:"
    echo "1. 确认已在 AUR (https://aur.archlinux.org/register) 注册账号"
    echo "2. 确认已在 AUR 后台 (My Account -> SSH Public Key) 粘贴了当前机器的公钥:"
    if [[ -f "$HOME/.ssh/id_ed25519.pub" ]]; then
        echo "   公钥路径: ~/.ssh/id_ed25519.pub"
        echo "   内容: $(cat "$HOME/.ssh/id_ed25519.pub")"
    elif [[ -f "$HOME/.ssh/id_rsa.pub" ]]; then
        echo "   公钥路径: ~/.ssh/id_rsa.pub"
        echo "   内容: $(cat "$HOME/.ssh/id_rsa.pub")"
    fi
    echo ""
    echo "3. 网络/代理问题 (重要):"
    echo "   当前如果开启了 Clash / Mihomo 的 TUN 模式，代理节点通常会丢弃 22 端口流量。"
    echo "   请在 Clash/Mihomo 中将 aur.archlinux.org 设为直连 (DIRECT)，或临时关闭 TUN 模式。"
    echo "----------------------------------------------------------------------"
    exit 1
fi

echo "✅ SSH 连通性测试通过！"

publish_pkg() {
    local pkg="$1"
    local pkg_dir="$ROOT_DIR/packaging/aur/$pkg"
    local work_dir
    work_dir="$(mktemp -d)"

    echo "==> 正在准备发布: $pkg ..."
    cd "$work_dir"

    # 尝试 clone 仓库（如果包还未创建，AUR 允许直接 clone 空仓库）
    git clone "ssh://aur@aur.archlinux.org/${pkg}.git" "$pkg" || {
        echo "❌ 克隆 AUR 仓库 ssh://aur@aur.archlinux.org/${pkg}.git 失败"
        rm -rf "$work_dir"
        return 1
    }

    cd "$pkg"
    cp "$pkg_dir/PKGBUILD" .
    cp "$pkg_dir/.SRCINFO" .

    git add PKGBUILD .SRCINFO
    if git diff --staged --quiet; then
        echo "ℹ️  $pkg 没有更新，跳过提交"
    else
        local ver
        ver="$(grep '^pkgver=' PKGBUILD | cut -d= -f2)"
        git commit -m "Update $pkg to $ver"
        echo "==> 推送 $pkg 到 AUR..."
        git push origin master
        echo "🎉 $pkg 成功发布到 AUR!"
    fi

    rm -rf "$work_dir"
}

if [[ "$TARGET" == "horae" || "$TARGET" == "all" ]]; then
    publish_pkg "horae"
fi

if [[ "$TARGET" == "horae-bin" || "$TARGET" == "all" ]]; then
    publish_pkg "horae-bin"
fi

echo "==> 全部发布任务完成！"
