#!/bin/bash
# 本地构建 Docker 镜像并推送到 GHCR
# 由于 GitHub Actions 缺乏 Rust 1.96+ 支持，改为本地构建

set -e

# 颜色输出
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查是否提供了版本号
if [ -z "$1" ]; then
    echo -e "${YELLOW}用法: $0 <version>${NC}"
    echo "示例: $0 v0.7.9"
    exit 1
fi

VERSION=$1
IMAGE_NAME="ghcr.io/hellomrli/my-media-sub"

echo -e "${BLUE}🔧 开始构建 my-media-sub ${VERSION}${NC}"
echo ""

# 1. 编译 Rust 二进制
echo -e "${BLUE}1️⃣ 编译 Rust 二进制...${NC}"
cargo build --release
echo -e "${GREEN}✅ 编译完成${NC}"
echo ""

# 2. 构建 Docker 镜像（使用本地二进制）
echo -e "${BLUE}2️⃣ 构建 Docker 镜像...${NC}"
docker build -f Dockerfile.local -t ${IMAGE_NAME}:${VERSION} .
docker tag ${IMAGE_NAME}:${VERSION} ${IMAGE_NAME}:latest
echo -e "${GREEN}✅ 镜像构建完成${NC}"
echo ""

# 3. 推送到 GHCR
echo -e "${BLUE}3️⃣ 推送镜像到 GHCR...${NC}"
docker push ${IMAGE_NAME}:${VERSION}
docker push ${IMAGE_NAME}:latest
echo -e "${GREEN}✅ 镜像推送完成${NC}"
echo ""

# 4. 打包 release 文件
echo -e "${BLUE}4️⃣ 打包 release 文件...${NC}"
cd target/release
tar -czf my-media-sub-${VERSION}-linux-x86_64.tar.gz my-media-sub
mv my-media-sub-${VERSION}-linux-x86_64.tar.gz ../../
cd ../..
echo -e "${GREEN}✅ Release 文件已生成: my-media-sub-${VERSION}-linux-x86_64.tar.gz${NC}"
echo ""

echo -e "${GREEN}🎉 完成！${NC}"
echo ""
echo "镜像信息："
echo "  - ${IMAGE_NAME}:${VERSION}"
echo "  - ${IMAGE_NAME}:latest"
echo ""
echo "Release 文件："
echo "  - my-media-sub-${VERSION}-linux-x86_64.tar.gz"
echo ""
echo "下一步："
echo "  1. 创建 Git tag: git tag ${VERSION} && git push origin ${VERSION}"
echo "  2. 在 GitHub 创建 Release 并上传 tar.gz 文件"
