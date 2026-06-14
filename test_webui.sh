#!/bin/bash
cd /home/lain/my-media-sub

echo "=== 停止现有服务器 ==="
pkill -9 -f "my-media-sub" 2>/dev/null
sleep 2

echo "=== 启动服务器 ==="
./target/release/my-media-sub &
PID=$!
echo "PID: $PID"

echo "=== 等待启动 ==="
sleep 5

echo ""
echo "=== 测试结果 ==="
echo "1. API 健康检查:"
curl -s http://localhost:56001/health

echo ""
echo ""
echo "2. 访问首页 HTTP 状态:"
curl -s -o /dev/null -w "HTTP Status: %{http_code}\n" http://localhost:56001/

echo ""
echo "3. 访问首页内容（前200字符）:"
curl -s http://localhost:56001/ | head -c 200
echo ""

echo ""
echo "4. 访问 app.js:"
curl -s -o /dev/null -w "HTTP Status: %{http_code}\n" http://localhost:56001/app.js

echo ""
echo "5. 访问 API 订阅列表:"
curl -s http://localhost:56001/api/subscriptions | head -c 100

echo ""
echo ""
echo "=== 停止服务器 ==="
kill $PID 2>/dev/null
sleep 1

echo "✅ 测试完成"
