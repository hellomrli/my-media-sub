#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:-target/release/my-media-sub}"
STATIC_DIR="${STATIC_DIR:-static}"
HOST="127.0.0.1"
PORT="${SMOKE_PORT:-56191}"
USERNAME="${SMOKE_USERNAME:-smoke-admin}"
PASSWORD="${SMOKE_PASSWORD:-smoke-password-not-for-production}"
BASE_URL="http://${HOST}:${PORT}"
TMP_DIR="$(mktemp -d)"
LOG_FILE="${TMP_DIR}/server.log"
PID=""

cleanup() {
  if [[ -n "${PID}" ]] && kill -0 "${PID}" 2>/dev/null; then
    kill "${PID}" 2>/dev/null || true
    wait "${PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

[[ -x "${BINARY}" ]] || { echo "release binary is not executable: ${BINARY}" >&2; exit 1; }
for asset in index.html manifest.webmanifest service-worker.js openapi.json; do
  [[ -s "${STATIC_DIR}/${asset}" ]] || { echo "required static asset is missing: ${STATIC_DIR}/${asset}" >&2; exit 1; }
done

SERVER_HOST="${HOST}" SERVER_PORT="${PORT}" \
SERVER_USERNAME="${USERNAME}" SERVER_PASSWORD="${PASSWORD}" \
DATA_DIR="${TMP_DIR}/data" STATIC_DIR="${STATIC_DIR}" RUST_LOG=warn \
"${BINARY}" >"${LOG_FILE}" 2>&1 &
PID=$!

for _ in $(seq 1 60); do
  if curl --fail --silent --show-error "${BASE_URL}/health" >"${TMP_DIR}/health.json"; then
    break
  fi
  if ! kill -0 "${PID}" 2>/dev/null; then
    cat "${LOG_FILE}" >&2
    echo "release binary exited before becoming healthy" >&2
    exit 1
  fi
  sleep 0.25
done

curl --fail --silent --show-error "${BASE_URL}/health" | grep -F '"status":"ok"' >/dev/null
curl --fail --silent --show-error --user "${USERNAME}:${PASSWORD}" "${BASE_URL}/" | grep -F '<title>' >/dev/null
curl --fail --silent --show-error --user "${USERNAME}:${PASSWORD}" "${BASE_URL}/manifest.webmanifest" | grep -F 'MEDIA/SUB' >/dev/null
curl --fail --silent --show-error --user "${USERNAME}:${PASSWORD}" "${BASE_URL}/service-worker.js" | grep -F 'CACHE_VERSION' >/dev/null
curl --fail --silent --show-error --user "${USERNAME}:${PASSWORD}" "${BASE_URL}/api/diagnostics" | grep -F '"ok":true' >/dev/null

unauthorized_status="$(curl --silent --output /dev/null --write-out '%{http_code}' "${BASE_URL}/api/diagnostics")"
[[ "${unauthorized_status}" == "401" ]] || { echo "protected API returned ${unauthorized_status}, expected 401" >&2; exit 1; }

echo "release smoke test passed: ${BINARY} (${BASE_URL})"
