#!/usr/bin/env bash
# 容器入口：以 root 启动，修正数据目录属主后降权到非 root 用户运行。
#
# 目的：既保留“进程以非 root（UID/GID 1000）运行”的安全收益，又兼容从旧的
# root 镜像升级——旧版本以 root 写入的 bind mount / 命名卷中的 data 文件，
# 对 UID 1000 不可读。启动时把 DATA_DIR 归属修正为运行用户即可无缝升级。
set -euo pipefail

APP_UID="$(id -u app)"
APP_GID="$(id -g app)"
DATA_DIR="${DATA_DIR:-/app/data}"

# 仅当当前为 root 时才尝试修正属主并降权；若用户通过 `docker run --user` 或
# compose 的 `user:` 已经指定了非 root 身份，则直接以该身份运行。
if [ "$(id -u)" = "0" ]; then
  mkdir -p "${DATA_DIR}"
  # 只在属主不匹配时递归 chown，避免大目录每次启动的无谓开销。
  current_owner="$(stat -c '%u:%g' "${DATA_DIR}" 2>/dev/null || echo '')"
  if [ "${current_owner}" != "${APP_UID}:${APP_GID}" ]; then
    echo "entrypoint: 修正 ${DATA_DIR} 属主为 ${APP_UID}:${APP_GID}（兼容旧 root 数据）"
    chown -R "${APP_UID}:${APP_GID}" "${DATA_DIR}" || \
      echo "entrypoint: 警告 - 无法修正 ${DATA_DIR} 属主，若为只读挂载可忽略" >&2
  fi
  exec gosu app "$@"
fi

# 已经是非 root（用户显式覆盖了身份），直接运行。
exec "$@"
