#!/bin/bash
# Docker 构建和运行脚本

set -e

echo "🐳 开始构建 Docker 镜像..."

# 构建镜像
docker build -t my-media-sub:rust-v0.6.0 -t my-media-sub:latest .

echo "✅ 镜像构建完成！"
echo ""
echo "📊 镜像信息："
docker images | grep my-media-sub

echo ""
echo "🚀 运行选项："
echo ""
echo "1. 使用 docker run："
echo "   docker run -d \\"
echo "     --name my-media-sub \\"
echo "     -p 56001:56001 \\"
echo "     -v \$(pwd)/data:/app/data \\"
echo "     my-media-sub:rust-v0.6.0"
echo ""
echo "2. 使用 docker-compose："
echo "   docker-compose up -d"
echo ""
echo "3. 查看日志："
echo "   docker logs -f my-media-sub"
echo ""
echo "4. 健康检查："
echo "   curl http://localhost:56001/health"
