#!/bin/bash
# Rust 重写进度追踪脚本

PROGRESS_FILE="$HOME/my-media-sub/.rust_progress"

# 初始化进度文件
if [ ! -f "$PROGRESS_FILE" ]; then
    echo "3" > "$PROGRESS_FILE"
fi

# 读取当前进度
CURRENT=$(cat "$PROGRESS_FILE")

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🦀 Rust 重写进度报告"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "当前进度: ${CURRENT}%"
echo ""
echo "进度条: ["
for i in {1..100}; do
    if [ $i -le $CURRENT ]; then
        echo -n "▓"
    else
        echo -n "░"
    fi
    if [ $((i % 10)) -eq 0 ]; then
        echo -n " "
    fi
done
echo "]"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
