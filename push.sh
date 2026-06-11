#!/bin/bash
# 使用你的 GitHub Token 推送
# 将 YOUR_TOKEN 替换为你的实际 token

cd ~/my-media-sub
git push https://YOUR_TOKEN@github.com/hellomrli/my-media-sub.git main

echo "✅ 推送完成！"
