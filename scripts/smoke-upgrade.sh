#!/usr/bin/env bash
set -euo pipefail

OLD_BINARY="${1:?usage: smoke-upgrade.sh OLD_BINARY OLD_STATIC CURRENT_BINARY CURRENT_STATIC}"
OLD_STATIC="${2:?missing old static directory}"
CURRENT_BINARY="${3:?missing current binary}"
CURRENT_STATIC="${4:?missing current static directory}"
HOST=127.0.0.1
PORT="${UPGRADE_SMOKE_PORT:-56192}"
USER="upgrade-admin"
PASSWORD="upgrade-password-not-for-production"
BASE_URL="http://${HOST}:${PORT}"
TMP_DIR="$(mktemp -d)"
DATA_DIR="${TMP_DIR}/data"
PID=""

cleanup_process() {
  if [[ -n "${PID}" ]] && kill -0 "${PID}" 2>/dev/null; then
    kill "${PID}" 2>/dev/null || true
    wait "${PID}" 2>/dev/null || true
  fi
  PID=""
}
cleanup() { cleanup_process; rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

start_server() {
  local binary="$1" static_dir="$2" log="$3"
  SERVER_HOST="${HOST}" SERVER_PORT="${PORT}" SERVER_USERNAME="${USER}" \
  SERVER_PASSWORD="${PASSWORD}" DATA_DIR="${DATA_DIR}" STATIC_DIR="${static_dir}" \
  BACKUP_INTERVAL_HOURS=0 RUST_LOG=warn "${binary}" >"${log}" 2>&1 &
  PID=$!
  for _ in $(seq 1 80); do
    if curl --fail --silent "${BASE_URL}/health" >/dev/null; then return 0; fi
    if ! kill -0 "${PID}" 2>/dev/null; then cat "${log}" >&2; return 1; fi
    sleep 0.25
  done
  cat "${log}" >&2
  return 1
}

for path in "${OLD_BINARY}" "${CURRENT_BINARY}"; do
  [[ -x "${path}" ]] || { echo "binary is not executable: ${path}" >&2; exit 1; }
done
for path in "${OLD_STATIC}" "${CURRENT_STATIC}"; do
  [[ -s "${path}/index.html" ]] || { echo "static directory is invalid: ${path}" >&2; exit 1; }
done

mkdir -p "${DATA_DIR}"
start_server "${OLD_BINARY}" "${OLD_STATIC}" "${TMP_DIR}/old.log"

curl --fail --silent --show-error --user "${USER}:${PASSWORD}" \
  -H 'Content-Type: application/json' -X POST "${BASE_URL}/api/subscriptions" \
  --data '{"title":"升级烟雾测试剧集","url":"https://pan.quark.cn/s/upgrade-smoke","media_type":"series","season":1,"start_episode_number":1,"enabled":false}' \
  >"${TMP_DIR}/created.json"
grep -F '升级烟雾测试剧集' "${TMP_DIR}/created.json" >/dev/null
cleanup_process

cp -a "${DATA_DIR}" "${TMP_DIR}/pre-upgrade-data"
start_server "${CURRENT_BINARY}" "${CURRENT_STATIC}" "${TMP_DIR}/current.log"

curl --fail --silent --show-error --user "${USER}:${PASSWORD}" \
  "${BASE_URL}/api/subscriptions" >"${TMP_DIR}/subscriptions.json"
grep -F '升级烟雾测试剧集' "${TMP_DIR}/subscriptions.json" >/dev/null
curl --fail --silent --show-error --user "${USER}:${PASSWORD}" \
  "${BASE_URL}/api/diagnostics" >"${TMP_DIR}/diagnostics.json"
grep -F '"schema_version":1' "${TMP_DIR}/diagnostics.json" >/dev/null

if find "${DATA_DIR}" -type f -name '*.corrupt-*' -print -quit | grep -q .; then
  echo 'upgrade produced corrupt quarantine files' >&2
  exit 1
fi

echo "binary upgrade smoke test passed: old -> current (${BASE_URL})"
