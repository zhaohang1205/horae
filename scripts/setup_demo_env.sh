#!/usr/bin/env bash
# horae 演示环境一键准备脚本
# 用途：创建一个隔离的 demo 数据集，并填充真实高质感的 GTD 任务，确保录屏与截图专业、干净、无个人隐私泄露。

set -euo pipefail

# 优先使用 target/release/horae，其次使用系统 PATH 中的 horae
HORAE_BIN="${WORKSPACE_HORAE:-target/release/horae}"
if [ ! -f "$HORAE_BIN" ]; then
    if command -v horae >/dev/null 2>&1; then
        HORAE_BIN="horae"
    else
        echo "正在编译 release 版本..."
        cargo build --release -p horae-cli
    fi
fi

echo "✓ 使用二进制: $HORAE_BIN"

# 创建或重置 demo profile
echo "-> 准备 demo profile..."
$HORAE_BIN profile list | grep -q 'demo' && $HORAE_BIN profile rm demo || true
$HORAE_BIN profile new demo

# 填充真实 GTD 任务种子数据
echo "-> 注入精选任务样本..."

# 1. 计划与今日任务 (Scheduled / Next)
$HORAE_BIN --profile demo capture "发布 horae v0.2.6 官方文档与演示动图 @work !high ~今天 18:00"
$HORAE_BIN --profile demo capture "季度系统架构评审准备 @work !high ~明天 10:00"
$HORAE_BIN --profile demo capture "研读 Rust 2024 Edition 新特性与并发模型 @learning ~周日 15:00"
$HORAE_BIN --profile demo capture "阳台绿植浇水与修剪 @home *2d"
$HORAE_BIN --profile demo capture "给单车链条上油保养 @errands ~周六 10:00"

# 2. 循环习惯 (Recurring)
$HORAE_BIN --profile demo capture "晨间 GTD 收件箱清空与优先级校准 @focus *d 08:30 !medium"
$HORAE_BIN --profile demo capture "每周五个人工作与生活周回顾 @focus *w[5] 17:30"

# 3. 灵感金句 (Quotes)
$HORAE_BIN --profile demo capture "完成优于完美。Done is better than perfect. @quote"
$HORAE_BIN --profile demo capture "你的大脑是用来产生想法的，而不是用来存储想法的。—— David Allen @quote"

echo ""
echo "========================================================"
echo "🎉 演示环境准备就绪！"
echo ""
echo "你可以通过以下命令启动 demo 环境进行录制或体验："
echo "  $HORAE_BIN --profile demo -f        # 闪念录入录制 (Hero Shot)"
echo "  $HORAE_BIN --profile demo           # 打开完整 TUI 看板"
echo ""
echo "录制完成后，如需清理演示数据，只需运行："
echo "  $HORAE_BIN profile rm demo"
echo "========================================================"
