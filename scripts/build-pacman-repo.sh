#!/usr/bin/env bash
set -euo pipefail

# Build Arch Linux pacman repository for horae
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${1:-$ROOT_DIR/dist/pacman}"
ARCH="x86_64"
TARGET_DIR="$OUTPUT_DIR/arch/$ARCH"

echo "==> 正在准备构建 Arch Linux Pacman 软件源..."
mkdir -p "$TARGET_DIR"

# 从 Cargo.toml 提取当前版本
PKGVER="$(grep '^version =' "$ROOT_DIR/Cargo.toml" | head -n1 | cut -d'"' -f2)"
echo "==> 当前软件版本: $PKGVER"

BUILD_TMP="$(mktemp -d)"
trap 'rm -rf "$BUILD_TMP"' EXIT

# 准备 PKGBUILD
cat << EOF > "$BUILD_TMP/PKGBUILD"
# Maintainer: Alex Zhao <zhaohang1205@gmail.com>
pkgname=horae
pkgver=${PKGVER}
pkgrel=1
pkgdesc="A terminal GTD task manager with TUI and CLI powered by Rust & SQLite"
arch=('x86_64')
url="https://github.com/zhaohang1205/horae"
license=('GPL-3.0-only')
depends=('gcc-libs' 'glibc')
optdepends=(
    'libnotify: desktop notifications for timers and tasks'
    'ttf-nerd-fonts-symbols: Nerd Font glyphs for TUI'
    'syncthing: mobile sync bridge'
)
options=('!strip')

source_x86_64=(
    "${PKGVER}.tar.gz::https://github.com/zhaohang1205/horae/releases/download/v${PKGVER}/horae-v${PKGVER}-x86_64-unknown-linux-gnu.tar.gz"
    "LICENSE-${PKGVER}::https://raw.githubusercontent.com/zhaohang1205/horae/v${PKGVER}/LICENSE"
    "README-${PKGVER}.md::https://raw.githubusercontent.com/zhaohang1205/horae/v${PKGVER}/README.md"
)

package() {
    install -Dm755 "\${srcdir}/horae" "\${pkgdir}/usr/bin/horae"
    install -Dm644 "\${srcdir}/LICENSE-${PKGVER}" "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
    install -Dm644 "\${srcdir}/README-${PKGVER}.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"

    # Shell completions
    "\${srcdir}/horae" completions bash | install -Dm644 /dev/stdin "\${pkgdir}/usr/share/bash-completion/completions/horae"
    "\${srcdir}/horae" completions zsh | install -Dm644 /dev/stdin "\${pkgdir}/usr/share/zsh/site-functions/_horae"
    "\${srcdir}/horae" completions fish | install -Dm644 /dev/stdin "\${pkgdir}/usr/share/fish/vendor_completions.d/horae.fish"
}
EOF

echo "==> 正在计算校验和并构建软件包..."
(
    cd "$BUILD_TMP"
    updpkgsums
    makepkg -f
)

echo "==> 复制软件包到软件源目录: $TARGET_DIR ..."
cp "$BUILD_TMP"/horae-*.pkg.tar.zst "$TARGET_DIR/"

echo "==> 使用 repo-add 更新数据库..."
(
    cd "$TARGET_DIR"
    # 删除旧的数据库压缩包（保持符号链接干净）
    rm -f horae.db horae.files horae.db.tar.gz horae.files.tar.gz
    repo-add horae.db.tar.gz horae-*.pkg.tar.zst
)

echo "==> 复制落地页与安装脚本..."
cp "$ROOT_DIR/packaging/pacman/install-repo.sh" "$OUTPUT_DIR/"
cp "$ROOT_DIR/packaging/pacman/index.html" "$OUTPUT_DIR/"

echo "--------------------------------------------------------"
echo "✅ Pacman 软件源构建完成！"
echo "产物目录: $OUTPUT_DIR"
ls -la "$TARGET_DIR"
echo "--------------------------------------------------------"
