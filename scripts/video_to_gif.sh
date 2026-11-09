#!/usr/bin/env bash
# horae 演示视频转极品高清 GIF 脚本
# 用法: ./scripts/video_to_gif.sh [输入视频路径] [输出gif路径] [可选宽度，默认960]

set -euo pipefail

INPUT_FILE="${1:-assets/videos/hero-capture.mp4}"
OUTPUT_FILE="${2:-assets/screenshots/hero-capture.gif}"
TARGET_WIDTH="${3:-960}"

if [ ! -f "$INPUT_FILE" ]; then
    echo "❌ 找不到输入视频文件: $INPUT_FILE"
    echo "使用方式: $0 <视频文件.mp4/.webm> [输出.gif] [宽度]"
    exit 1
fi

mkdir -p "$(dirname "$OUTPUT_FILE")"

echo "-> 开始转换: $INPUT_FILE -> $OUTPUT_FILE"
echo "-> 目标分辨率宽度: ${TARGET_WIDTH}px, 帧率: 18 fps, 调色板优化: 双通道 192 色 Lanczos"

# 使用两阶段调色板生成最高画质且体积精简的 GIF
ffmpeg -y -i "$INPUT_FILE" \
    -vf "fps=18,scale=${TARGET_WIDTH}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192:stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3" \
    "$OUTPUT_FILE"

ORIG_SIZE=$(du -h "$INPUT_FILE" | cut -f1)
GIF_SIZE=$(du -h "$OUTPUT_FILE" | cut -f1)

echo "✓ 转换成功！"
echo "  原始视频大小: $ORIG_SIZE"
echo "  生成 GIF 大小: $GIF_SIZE"
echo "  保存位置: $OUTPUT_FILE"
