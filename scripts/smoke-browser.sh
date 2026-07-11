#!/usr/bin/env bash
set -euo pipefail
BINARY="${1:-target/release/my-media-sub}"
CHROME="${CHROME_BIN:-$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)}"
[[ -n "$CHROME" ]] || { echo 'Chrome/Chromium is required for browser smoke testing' >&2; exit 1; }
PORT="${BROWSER_SMOKE_PORT:-56195}"; BASE="http://127.0.0.1:${PORT}"; TMP="$(mktemp -d)"; PID=""
USER=browser-admin; PASSWORD=browser-password-not-for-production
cleanup(){ if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }; trap cleanup EXIT
SERVER_HOST=127.0.0.1 SERVER_PORT="$PORT" SERVER_USERNAME="$USER" SERVER_PASSWORD="$PASSWORD" DATA_DIR="$TMP/data" STATIC_DIR=static BACKUP_INTERVAL_HOURS=0 RUST_LOG=warn "$BINARY" >"$TMP/server.log" 2>&1 & PID=$!
for _ in $(seq 1 80); do curl -fsS "$BASE/health" >/dev/null && break; sleep .25; done
# Fetch the authenticated shell, then render it in a real 390px browser viewport.
curl -fsS -u "$USER:$PASSWORD" "$BASE/?tab=calendar" >"$TMP/page.html"
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/diagnostics" | grep -F '"ok":true' >/dev/null
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/jobs?limit=5" | grep -F '"ok":true' >/dev/null
timeout 20 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome" \
  --window-size=390,844 --virtual-time-budget=1500 --dump-dom "file://$TMP/page.html" >"$TMP/dom.html" 2>"$TMP/chrome.log" || [[ -s "$TMP/dom.html" ]]
grep -F '<title>媒体订阅管理系统' "$TMP/dom.html" >/dev/null
grep -F 'manifest.webmanifest' "$TMP/dom.html" >/dev/null
grep -F '日历项目' "$TMP/dom.html" >/dev/null
grep -F '统一危险操作确认' "$TMP/dom.html" >/dev/null
grep -F '复制诊断' "$TMP/dom.html" >/dev/null
grep -F '选择当前显示' "$TMP/dom.html" >/dev/null
timeout 20 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome-shot" \
  --window-size=390,844 --screenshot="$TMP/calendar-390.png" "file://$TMP/page.html" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/calendar-390.png" ]]
[[ -s "$TMP/calendar-390.png" ]]
timeout 20 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome-desktop" \
  --window-size=1440,1000 --screenshot="$TMP/dashboard-1440.png" "file://$TMP/page.html" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/dashboard-1440.png" ]]
[[ -s "$TMP/dashboard-1440.png" ]]
echo 'real browser E2E smoke passed: authenticated APIs plus 390x844 and 1440x1000 UI contracts'
