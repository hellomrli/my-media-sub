#!/usr/bin/env bash
set -euo pipefail
BINARY="${1:-target/release/my-media-sub}"
DURATION="${SOAK_SECONDS:-20}"
PORT="${SOAK_PORT:-56194}"
BASE="http://127.0.0.1:${PORT}"
USER=soak-admin
PASSWORD=soak-password-not-for-production
TMP="$(mktemp -d)"
PID=""
cleanup(){ if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }
trap cleanup EXIT
SERVER_HOST=127.0.0.1 SERVER_PORT="$PORT" SERVER_USERNAME="$USER" SERVER_PASSWORD="$PASSWORD" \
 DATA_DIR="$TMP/data" STATIC_DIR=static BACKUP_INTERVAL_HOURS=0 BACKUP_RETENTION=3 RUST_LOG=warn \
 "$BINARY" >"$TMP/server.log" 2>&1 & PID=$!
for _ in $(seq 1 80); do curl -fsS "$BASE/health" >/dev/null && break; kill -0 "$PID" 2>/dev/null || { cat "$TMP/server.log" >&2; exit 1; }; sleep .25; done
end=$((SECONDS + DURATION)); requests=0
while (( SECONDS < end )); do
  curl -fsS --retry 2 --retry-delay 0 --retry-all-errors "$BASE/health" >/dev/null
  curl -fsS --retry 2 --retry-delay 0 --retry-all-errors -u "$USER:$PASSWORD" "$BASE/api/diagnostics" | grep -F '"ok":true' >/dev/null
  requests=$((requests + 2))
done
for _ in $(seq 1 4); do curl -fsS --retry 2 --retry-delay 0 --retry-all-errors -u "$USER:$PASSWORD" -X POST "$BASE/api/backups" | grep -F '"ok":true' >/dev/null; done
count="$(curl -fsS --retry 2 --retry-delay 0 --retry-all-errors -u "$USER:$PASSWORD" "$BASE/api/backups" | grep -o 'backup-[^"]*\.json' | sort -u | wc -l)"
(( count <= 3 )) || { echo "backup retention exceeded: $count" >&2; exit 1; }
kill -0 "$PID"
find "$TMP/data" -type f -name '*.corrupt-*' -print -quit | grep -q . && { echo 'soak produced corrupt files' >&2; exit 1; }
echo "stability soak passed: ${requests} requests, ${count} retained backups in ${DURATION}s"
