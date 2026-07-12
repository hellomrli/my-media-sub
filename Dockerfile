# 多阶段构建 Dockerfile for Rust 版本
# Stage 1: 构建阶段
# 与运行阶段的 bookworm glibc 保持一致，避免构建出的二进制依赖更新的 glibc。
FROM rust:1-bookworm AS builder

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

# 构建 release 版本
RUN cargo build --release --locked

# Stage 2: 运行阶段
FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="My Media Sub" \
      org.opencontainers.image.description="Media subscription and Quark drive automation service" \
      org.opencontainers.image.source="https://github.com/hellomrli/my-media-sub" \
      org.opencontainers.image.licenses="MIT"

WORKDIR /app

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/my-media-sub /usr/local/bin/my-media-sub

# 复制静态文件
COPY static /app/static

# 创建非 root 运行用户和数据目录。
# 注意：/app/data 通常由 docker-compose 以宿主目录 bind mount 挂载，
# 挂载后目录属主以宿主机为准。请确保宿主目录对 UID/GID 1000 可写，
# 例如：chown -R 1000:1000 ./data；或在 compose 中用 `user:` 覆盖为宿主用户。
RUN groupadd --gid 1000 app \
    && useradd --uid 1000 --gid 1000 --home-dir /app --no-create-home app \
    && mkdir -p /app/data \
    && chown -R app:app /app

USER app

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
