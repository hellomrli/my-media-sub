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
    gosu \
    && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/my-media-sub /usr/local/bin/my-media-sub

# 复制静态文件
COPY static /app/static

# 入口脚本：以 root 启动、修正数据目录属主后 gosu 降权到非 root 用户。
COPY scripts/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# 创建非 root 运行用户和数据目录。进程最终以 UID/GID 1000 的 app 用户运行。
# /app/data 常由 bind mount / 命名卷挂载；入口脚本会在启动时把其属主修正为
# 运行用户，兼容从旧 root 镜像升级的数据。如需完全固定身份，可用 compose 的
# `user:` 覆盖，此时入口脚本检测到非 root 会跳过 chown 直接运行。
RUN groupadd --gid 1000 app \
    && useradd --uid 1000 --gid 1000 --home-dir /app --no-create-home app \
    && mkdir -p /app/data \
    && chown -R app:app /app

# 设置环境变量
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=56001
ENV DATA_DIR=/app/data

# 暴露端口
EXPOSE 56001

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:56001/health || exit 1

# 以入口脚本启动（root → 修正属主 → gosu 降权），再运行主程序
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["my-media-sub"]
