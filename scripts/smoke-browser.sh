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
timeout 20 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome" \
  --window-size=390,844 --virtual-time-budget=1500 --dump-dom "file://$TMP/page.html" >"$TMP/dom.html" 2>"$TMP/chrome.log" || [[ -s "$TMP/dom.html" ]]
grep -F '<title>媒体订阅管理系统' "$TMP/dom.html" >/dev/null
grep -F 'manifest.webmanifest' "$TMP/dom.html" >/dev/null
grep -F '日历项目' "$TMP/dom.html" >/dev/null
timeout 20 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome-shot" \
  --window-size=390,844 --screenshot="$TMP/calendar-390.png" "file://$TMP/page.html" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/calendar-390.png" ]]
[[ -s "$TMP/calendar-390.png" ]]
echo 'real browser smoke passed at 390x844 with authenticated calendar/PWA shell'
