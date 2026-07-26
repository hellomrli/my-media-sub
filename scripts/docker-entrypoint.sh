#!/usr/bin/env bash
# 容器入口：初始化可持久化运行载荷，再以非 root 用户启动服务。
#
# 镜像中的 /opt/my-media-sub 是只读发布种子；实际二进制和 static 位于
# APP_RUNTIME_DIR。这样 WebUI 在线更新可以安全地原子替换文件，并在容器重启或
# 重建后继续使用已更新版本。切换到不同版本的 Docker 镜像时，入口脚本会用新
# 镜像载荷刷新运行目录，宿主机显式拉取/回滚镜像仍具有最高优先级。
set -euo pipefail

APP_RUN_USER="${APP_RUN_USER:-app}"
APP_UID="$(id -u "${APP_RUN_USER}")"
APP_GID="$(id -g "${APP_RUN_USER}")"
DATA_DIR="${DATA_DIR:-/app/data}"
IMAGE_DIR="${APP_IMAGE_DIR:-/opt/my-media-sub}"
RUNTIME_DIR="${APP_RUNTIME_DIR:-/app/runtime}"
IMAGE_BINARY="${IMAGE_DIR}/my-media-sub"
IMAGE_STATIC_DIR="${IMAGE_DIR}/static"
IMAGE_VERSION_FILE="${IMAGE_DIR}/VERSION"
IMAGE_PAYLOAD_ID_FILE="${IMAGE_DIR}/PAYLOAD_ID"
RUNTIME_BINARY="${RUNTIME_DIR}/my-media-sub"
STATIC_DIR="${STATIC_DIR:-${RUNTIME_DIR}/static}"
IMAGE_VERSION_MARKER="${RUNTIME_DIR}/.image-version"
INSTALLED_VERSION_MARKER="${RUNTIME_DIR}/.installed-version"
export STATIC_DIR

is_true() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

write_marker() {
  local path="$1" value="$2" tmp="${1}.tmp-$$"
  printf '%s\n' "${value}" >"${tmp}"
  mv -f "${tmp}" "${path}"
}

static_payload_complete() {
  local directory="$1" asset
  for asset in index.html manifest.webmanifest service-worker.js openapi.json; do
    [ -s "${directory}/${asset}" ] || return 1
  done
}

install_image_payload() {
  local image_version="$1" image_payload_id="$2"
  local token="$(date +%Y%m%d%H%M%S)-$$"
  local binary_stage="${RUNTIME_DIR}/.my-media-sub.image-${token}"
  local static_parent static_name static_stage binary_backup static_backup had_binary=0 had_static=0
  static_parent="$(dirname "${STATIC_DIR}")"
  static_name="$(basename "${STATIC_DIR}")"
  static_stage="${static_parent}/.${static_name}.image-${token}"
  binary_backup="${RUNTIME_BINARY}.image-backup-${token}"
  static_backup="${STATIC_DIR}.image-backup-${token}"

  mkdir -p "${RUNTIME_DIR}" "${static_parent}"
  cp "${IMAGE_BINARY}" "${binary_stage}"
  chmod 0755 "${binary_stage}"
  mkdir -p "${static_stage}"
  cp -a "${IMAGE_STATIC_DIR}/." "${static_stage}/"

  if [ -e "${RUNTIME_BINARY}" ]; then
    cp -a "${RUNTIME_BINARY}" "${binary_backup}"
    had_binary=1
  fi
  if [ -e "${STATIC_DIR}" ]; then
    mv "${STATIC_DIR}" "${static_backup}"
    had_static=1
  fi

  if ! mv "${static_stage}" "${STATIC_DIR}"; then
    if [ "${had_static}" = "1" ]; then mv "${static_backup}" "${STATIC_DIR}" || true; fi
    rm -f "${binary_stage}" "${binary_backup}"
    return 1
  fi
  if ! mv -f "${binary_stage}" "${RUNTIME_BINARY}"; then
    rm -rf "${STATIC_DIR}"
    if [ "${had_static}" = "1" ]; then mv "${static_backup}" "${STATIC_DIR}" || true; fi
    if [ "${had_binary}" = "1" ]; then mv -f "${binary_backup}" "${RUNTIME_BINARY}" || true; fi
    return 1
  fi

  rm -f "${binary_backup}"
  rm -rf "${static_backup}"
  write_marker "${IMAGE_VERSION_MARKER}" "${image_payload_id}"
  write_marker "${INSTALLED_VERSION_MARKER}" "${image_version}"
  echo "entrypoint: 已从镜像初始化运行载荷 ${image_version} (${image_payload_id:0:12}) -> ${RUNTIME_DIR}"
}

initialize_runtime() {
  local image_version image_payload_id recorded_image_payload_id=""
  [ -x "${IMAGE_BINARY}" ] || { echo "entrypoint: 镜像二进制不存在：${IMAGE_BINARY}" >&2; return 1; }
  static_payload_complete "${IMAGE_STATIC_DIR}" || { echo "entrypoint: 镜像 static 不完整：${IMAGE_STATIC_DIR}" >&2; return 1; }
  [ -s "${IMAGE_VERSION_FILE}" ] || { echo "entrypoint: 镜像版本文件不存在：${IMAGE_VERSION_FILE}" >&2; return 1; }
  [ -s "${IMAGE_PAYLOAD_ID_FILE}" ] || { echo "entrypoint: 镜像载荷指纹不存在：${IMAGE_PAYLOAD_ID_FILE}" >&2; return 1; }
  image_version="$(tr -d '\r\n' <"${IMAGE_VERSION_FILE}")"
  image_payload_id="$(tr -d '\r\n' <"${IMAGE_PAYLOAD_ID_FILE}")"
  [ -n "${image_version}" ] || { echo "entrypoint: 镜像版本文件为空" >&2; return 1; }
  [ -n "${image_payload_id}" ] || { echo "entrypoint: 镜像载荷指纹为空" >&2; return 1; }
  mkdir -p "${RUNTIME_DIR}"
  if [ -f "${IMAGE_VERSION_MARKER}" ]; then
    recorded_image_payload_id="$(tr -d '\r\n' <"${IMAGE_VERSION_MARKER}")"
  fi

  if [ ! -x "${RUNTIME_BINARY}" ] || is_true "${APP_RUNTIME_RESET:-false}"; then
    install_image_payload "${image_version}" "${image_payload_id}"
  elif [ -n "${recorded_image_payload_id}" ] && [ "${recorded_image_payload_id}" != "${image_payload_id}" ]; then
    echo "entrypoint: 检测到镜像应用载荷变化 ${recorded_image_payload_id:0:12} -> ${image_payload_id:0:12}"
    install_image_payload "${image_version}" "${image_payload_id}"
  elif ! static_payload_complete "${STATIC_DIR}"; then
    echo "entrypoint: 运行时 static 不完整，正在从镜像恢复"
    install_image_payload "${image_version}" "${image_payload_id}"
  else
    # 运行目录已存在但没有镜像标记时，视作用户提供的自定义载荷并保留。
    if [ -z "${recorded_image_payload_id}" ]; then
      write_marker "${IMAGE_VERSION_MARKER}" "${image_payload_id}"
      [ -f "${INSTALLED_VERSION_MARKER}" ] || write_marker "${INSTALLED_VERSION_MARKER}" "custom"
      echo "entrypoint: 保留运行目录中已有的自定义二进制"
    fi
  fi
}

initialize_runtime

# 仅当当前为 root 时修正挂载目录属主并降权；显式 `--user` 时要求挂载目录已经
# 对该用户可写，入口脚本仍会完成初始化但不会尝试提升权限。
if [ "$(id -u)" = "0" ]; then
  mkdir -p "${DATA_DIR}"
  data_owner="$(stat -c '%u:%g' "${DATA_DIR}" 2>/dev/null || echo '')"
  if [ "${data_owner}" != "${APP_UID}:${APP_GID}" ]; then
    echo "entrypoint: 修正 ${DATA_DIR} 属主为 ${APP_UID}:${APP_GID}（兼容旧 root 数据）"
    chown -R "${APP_UID}:${APP_GID}" "${DATA_DIR}" || \
      echo "entrypoint: 警告 - 无法修正 ${DATA_DIR} 属主，若为只读挂载可忽略" >&2
  fi

  runtime_owner="$(stat -c '%u:%g' "${RUNTIME_DIR}" 2>/dev/null || echo '')"
  binary_owner="$(stat -c '%u:%g' "${RUNTIME_BINARY}" 2>/dev/null || echo '')"
  static_owner="$(stat -c '%u:%g' "${STATIC_DIR}" 2>/dev/null || echo '')"
  if [ "${runtime_owner}" != "${APP_UID}:${APP_GID}" ] \
    || [ "${binary_owner}" != "${APP_UID}:${APP_GID}" ] \
    || [ "${static_owner}" != "${APP_UID}:${APP_GID}" ]; then
    echo "entrypoint: 修正 ${RUNTIME_DIR} 属主为 ${APP_UID}:${APP_GID}"
    chown -R "${APP_UID}:${APP_GID}" "${RUNTIME_DIR}"
  fi
  if [[ "${STATIC_DIR}" != "${RUNTIME_DIR}"/* ]]; then
    chown -R "${APP_UID}:${APP_GID}" "${STATIC_DIR}"
  fi
fi

case "${1:-}" in
  my-media-sub|/usr/local/bin/my-media-sub|"${IMAGE_BINARY}")
    shift
    cd "${RUNTIME_DIR}"
    set -- "${RUNTIME_BINARY}" "$@"
    ;;
esac

if [ "$(id -u)" = "0" ]; then
  exec gosu "${APP_RUN_USER}" "$@"
fi
exec "$@"
