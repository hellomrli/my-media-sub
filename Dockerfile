# 多阶段构建 Dockerfile for Rust 版本
# Stage 1: 构建阶段
# 使用 latest 确保获得最新稳定版 Rust (支持 edition2024)
FROM rust:latest as builder

WORKDIR /app

# 安装构建依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 复制 Cargo 文件
COPY Cargo.toml Cargo.lock ./

# 复制源代码
COPY src ./src
COPY tests ./tests

# 构建 release 版本
RUN cargo build --release

# Stage 2: 运行阶段
FROM debian:bookworm-slim

WORKDIR /app

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/my-media-sub /usr/local/bin/my-media-sub

# 创建数据目录
RUN mkdir -p /app/data

# 设置环境变量
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=56001
ENV DATA_DIR=/app/data

# 暴露端口
EXPOSE 56001

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:56001/health || exit 1

# 运行
CMD ["my-media-sub"]
