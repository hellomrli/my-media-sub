#!/bin/bash
# GitHub 上传脚本 - 备份 Python 版本并推送 Rust 版本

set -e

cd /home/lain/my-media-sub

echo "📦 当前状态："
git branch -vv
echo ""

echo "1️⃣ 备份原 main 分支到 python 分支..."
git checkout main
git branch -D python 2>/dev/null || true
git checkout -b python
echo "✅ python 分支已创建"
echo ""

echo "2️⃣ 推送 python 分支到 GitHub..."
git push -u origin python
echo "✅ python 分支已推送"
echo ""

echo "3️⃣ 切换到 rust-rewrite 分支..."
git checkout rust-rewrite
echo ""

echo "4️⃣ 推送 rust-rewrite 分支到 GitHub..."
git push origin rust-rewrite
echo "✅ rust-rewrite 分支已推送"
echo ""

echo "5️⃣ 强制推送 rust-rewrite 为新的 main..."
read -p "⚠️  这将覆盖 GitHub 上的 main 分支！继续？(y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    git push origin rust-rewrite:main --force
    echo "✅ main 分支已更新为 Rust 版本"
    echo ""
    
    echo "6️⃣ 更新本地 main 分支..."
    git branch -D main
    git checkout -b main
    git branch -u origin/main
    echo "✅ 本地 main 分支已更新"
else
    echo "❌ 已取消 main 分支更新"
fi

echo ""
echo "🎉 完成！分支状态："
git branch -vv
echo ""
echo "📂 GitHub 分支："
echo "  • python - Python 原版（备份）"
echo "  • rust-rewrite - Rust 新版"
echo "  • main - Rust 新版（默认分支）"
