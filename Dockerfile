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
RUN cargo build --release --locked \
    && awk -F '"' '/^version = / { print $2; exit }' Cargo.toml > target/release/VERSION

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

# 镜像内只保留一份不可变的发布载荷。入口脚本会把它原子初始化到
# /app/runtime；应用实际从可写持久化卷运行，在线升级不会修改镜像层。
COPY --from=builder /app/target/release/my-media-sub /opt/my-media-sub/my-media-sub
COPY --from=builder /app/target/release/VERSION /opt/my-media-sub/VERSION
COPY static /opt/my-media-sub/static

# 版本号不足以区分同一开发版本的不同 latest/main 镜像；对实际应用载荷生成
# 内容指纹。这样显式拉取到二进制或 WebUI 已变化的镜像时会刷新 runtime，
# 仅基础镜像变化而应用载荷相同时则保留 WebUI 在线更新结果。
RUN (sha256sum /opt/my-media-sub/my-media-sub; \
     find /opt/my-media-sub/static -type f -print0 \
       | LC_ALL=C sort -z \
       | xargs -0 sha256sum) \
    | sha256sum \
    | awk '{ print $1 }' > /opt/my-media-sub/PAYLOAD_ID

# 入口脚本：以 root 启动、修正数据目录属主后 gosu 降权到非 root 用户。
COPY scripts/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# 创建非 root 运行用户及数据/运行目录。进程最终以 UID/GID 1000 的 app 用户
# 运行；入口脚本会修正默认挂载的属主。若用 compose `user:` 覆盖身份，data 和
# runtime 挂载必须预先对该用户可写，入口脚本不会尝试提升权限。
RUN groupadd --gid 1000 app \
    && useradd --uid 1000 --gid 1000 --home-dir /app --no-create-home app \
    && mkdir -p /app/data /app/runtime \
    && chmod 0755 /opt/my-media-sub/my-media-sub \
    && ln -s /opt/my-media-sub/my-media-sub /usr/local/bin/my-media-sub \
    && ln -s /app/runtime/static /app/static \
    && chown -R app:app /app

# 设置环境变量
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=56001
ENV DATA_DIR=/app/data
ENV APP_IMAGE_DIR=/opt/my-media-sub
ENV APP_RUNTIME_DIR=/app/runtime
ENV STATIC_DIR=/app/runtime/static
ENV SELF_UPDATE_ENABLED=true
ENV SELF_UPDATE_BACKUP_RETENTION=3

# 未显式挂载时也为 docker run 创建匿名持久化卷；Compose 配置使用宿主机
# ./runtime 绑定挂载，方便备份、检查和跨容器重建保留在线更新结果。
VOLUME ["/app/runtime"]

# 暴露端口
EXPOSE 56001

# 健康检查
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:56001/health || exit 1

# 以入口脚本启动（root → 修正属主 → gosu 降权），再运行主程序
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["my-media-sub"]
