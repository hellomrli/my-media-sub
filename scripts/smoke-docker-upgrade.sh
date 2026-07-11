#!/usr/bin/env bash
set -euo pipefail
OLD_IMAGE="${1:-ghcr.io/hellomrli/my-media-sub:1.3.0}"
CURRENT_IMAGE="${2:?usage: smoke-docker-upgrade.sh OLD_IMAGE CURRENT_IMAGE}"
PORT="${DOCKER_UPGRADE_SMOKE_PORT:-56193}"
NAME="media-sub-upgrade-smoke-$$"
VOLUME="${NAME}-data"
USER=upgrade-admin
PASSWORD=upgrade-password-not-for-production
BASE_URL="http://127.0.0.1:${PORT}"

cleanup() {
  docker rm -f "${NAME}" >/dev/null 2>&1 || true
  docker volume rm "${VOLUME}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker pull "${OLD_IMAGE}" >/dev/null
docker pull "${CURRENT_IMAGE}" >/dev/null
docker volume create "${VOLUME}" >/dev/null
start() {
  local image="$1"
  docker run -d --name "${NAME}" -p "127.0.0.1:${PORT}:56001" \
    -v "${VOLUME}:/app/data" -e SERVER_USERNAME="${USER}" -e SERVER_PASSWORD="${PASSWORD}" \
    -e BACKUP_INTERVAL_HOURS=0 "${image}" >/dev/null
  for _ in $(seq 1 100); do
    if curl --fail --silent "${BASE_URL}/health" >/dev/null; then return 0; fi
    if [[ "$(docker inspect -f '{{.State.Running}}' "${NAME}" 2>/dev/null || true)" != true ]]; then
      docker logs "${NAME}" >&2; return 1
    fi
    sleep 0.3
  done
  docker logs "${NAME}" >&2
  return 1
}

start "${OLD_IMAGE}"
curl --fail --silent --show-error --user "${USER}:${PASSWORD}" -H 'Content-Type: application/json' \
  -X POST "${BASE_URL}/api/subscriptions" \
  --data '{"title":"docker-upgrade-smoke-series","url":"https://pan.quark.cn/s/docker-upgrade-smoke","media_type":"series","season":1,"start_episode_number":1}' \
  | grep -F 'docker-upgrade-smoke-series' >/dev/null
docker rm -f "${NAME}" >/dev/null

start "${CURRENT_IMAGE}"
curl --fail --silent --show-error --user "${USER}:${PASSWORD}" "${BASE_URL}/api/subscriptions" \
  | grep -F 'docker-upgrade-smoke-series' >/dev/null
curl --fail --silent --show-error --user "${USER}:${PASSWORD}" "${BASE_URL}/api/diagnostics" \
  | grep -F '"schema_version":1' >/dev/null

echo "Docker volume upgrade smoke test passed: ${OLD_IMAGE} -> ${CURRENT_IMAGE}"
