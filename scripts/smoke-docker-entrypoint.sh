#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:-target/release/my-media-sub}"
STATIC_SOURCE="${2:-static}"
TMP_DIR="$(mktemp -d)"
IMAGE_DIR="${TMP_DIR}/image"
RUNTIME_DIR="${TMP_DIR}/runtime"
DATA_DIR="${TMP_DIR}/data"
STATIC_DIR="${RUNTIME_DIR}/static"

cleanup() { rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

mkdir -p "${IMAGE_DIR}"
cp "${BINARY}" "${IMAGE_DIR}/my-media-sub"
cp -a "${STATIC_SOURCE}" "${IMAGE_DIR}/static"
printf '%s\n' "2.2.13-smoke" >"${IMAGE_DIR}/VERSION"
printf '%s\n' "payload-a" >"${IMAGE_DIR}/PAYLOAD_ID"

run_entrypoint() {
  APP_RUN_USER="$(id -un)" APP_IMAGE_DIR="${IMAGE_DIR}" APP_RUNTIME_DIR="${RUNTIME_DIR}" \
  STATIC_DIR="${STATIC_DIR}" DATA_DIR="${DATA_DIR}" \
    bash scripts/docker-entrypoint.sh my-media-sub --version
}

run_entrypoint | grep -F 'my-media-sub ' >/dev/null
test -x "${RUNTIME_DIR}/my-media-sub"
test -s "${STATIC_DIR}/index.html"
test "$(tr -d '\r\n' <"${RUNTIME_DIR}/.image-version")" = "payload-a"
test "$(tr -d '\r\n' <"${RUNTIME_DIR}/.installed-version")" = "2.2.13-smoke"

printf '%s\n' 'runtime-persisted' >"${STATIC_DIR}/runtime-smoke.txt"
run_entrypoint | grep -F 'my-media-sub ' >/dev/null
grep -F 'runtime-persisted' "${STATIC_DIR}/runtime-smoke.txt" >/dev/null

# 核心 WebUI 文件缺失时必须从镜像恢复，不能继续运行半套前端。
rm "${STATIC_DIR}/openapi.json"
run_entrypoint | grep -F 'my-media-sub ' >/dev/null
test -s "${STATIC_DIR}/openapi.json"
test ! -e "${STATIC_DIR}/runtime-smoke.txt"

# 同一个应用版本也可能对应新的 latest/main 镜像；载荷变化必须刷新 runtime。
printf '%s\n' 'runtime-persisted-again' >"${STATIC_DIR}/runtime-smoke.txt"
printf '%s\n' "payload-b" >"${IMAGE_DIR}/PAYLOAD_ID"
printf '%s\n' "image-payload-b" >"${IMAGE_DIR}/static/payload-smoke.txt"
run_entrypoint | grep -F 'my-media-sub ' >/dev/null
test ! -e "${STATIC_DIR}/runtime-smoke.txt"
grep -F 'image-payload-b' "${STATIC_DIR}/payload-smoke.txt" >/dev/null
test "$(tr -d '\r\n' <"${RUNTIME_DIR}/.image-version")" = "payload-b"
test "$(tr -d '\r\n' <"${RUNTIME_DIR}/.installed-version")" = "2.2.13-smoke"

echo "Docker entrypoint runtime smoke test passed: ${BINARY}"
