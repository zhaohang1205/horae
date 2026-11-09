#!/usr/bin/env bash
set -euo pipefail

# Manually deploy built pacman repository from local to GitHub gh-pages branch
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist/pacman"

echo "==> 检查软件源构建产物..."
if [[ ! -f "$DIST_DIR/arch/x86_64/horae.db" ]]; then
    echo "ℹ️  未找到已构建的软件源，正在执行本地构建..."
    "$ROOT_DIR/scripts/build-pacman-repo.sh"
fi

ORIGIN_URL="$(git -C "$ROOT_DIR" remote get-url origin)"
echo "==> 目标远程仓库: $ORIGIN_URL"

DEPLOY_TMP="$(mktemp -d)"
trap 'rm -rf "$DEPLOY_TMP"' EXIT

cp -r "$DIST_DIR"/* "$DEPLOY_TMP/"

cd "$DEPLOY_TMP"
git init -b gh-pages
git config user.name "$(git -C "$ROOT_DIR" config user.name || echo 'Horae Bot')"
git config user.email "$(git -C "$ROOT_DIR" config user.email || echo 'horae@local')"

git add .
git commit -m "Deploy Arch Linux Pacman repository $(date '+%Y-%m-%d %H:%M:%S')"

echo "==> 正在推送到远程仓库 gh-pages 分支..."
git push --force "$ORIGIN_URL" gh-pages

echo "--------------------------------------------------------"
echo "🎉 软件源已成功推送到 gh-pages 分支！"
echo ""
echo "若首次部署，请进入 GitHub 仓库后台启用 Pages:"
echo "   Settings -> Pages -> Build and deployment"
echo "   Source: Deploy from a branch"
echo "   Branch: gh-pages / (root) -> 点击 Save"
echo ""
echo "稍等约 1 分钟后，软件源即可生效访问:"
echo "   https://zhaohang1205.github.io/horae/"
echo "--------------------------------------------------------"
