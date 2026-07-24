#!/usr/bin/env bash
set -euo pipefail
BINARY="${1:-target/release/my-media-sub}"
CHROME="${CHROME_BIN:-$(command -v google-chrome || command -v google-chrome-stable || command -v chromium || command -v chromium-browser || true)}"
[[ -n "$CHROME" ]] || { echo 'Chrome/Chromium is required for browser smoke testing' >&2; exit 1; }
PORT="${BROWSER_SMOKE_PORT:-56195}"; BASE="http://127.0.0.1:${PORT}"; TMP="$(mktemp -d)"; PID=""
USER=browser-admin; PASSWORD=browser-password-not-for-production
fail(){ echo "browser smoke failed: $*" >&2; [[ -f "$TMP/server.log" ]] && { echo '--- server.log ---' >&2; tail -n 80 "$TMP/server.log" >&2; }; [[ -f "$TMP/chrome.log" ]] && { echo '--- chrome.log ---' >&2; tail -n 80 "$TMP/chrome.log" >&2; }; exit 1; }
cleanup(){ if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi; rm -rf "$TMP"; }; trap cleanup EXIT
[[ -x "$BINARY" ]] || fail "release binary is not executable: $BINARY"
SERVER_HOST=127.0.0.1 SERVER_PORT="$PORT" SERVER_USERNAME="$USER" SERVER_PASSWORD="$PASSWORD" DATA_DIR="$TMP/data" STATIC_DIR=static BACKUP_INTERVAL_HOURS=0 RUST_LOG=warn "$BINARY" >"$TMP/server.log" 2>&1 & PID=$!
ready=0
for _ in $(seq 1 80); do
  if curl -fsS "$BASE/health" >/dev/null 2>&1; then ready=1; break; fi
  if ! kill -0 "$PID" 2>/dev/null; then fail "server exited before becoming healthy"; fi
  sleep .25
done
[[ "$ready" -eq 1 ]] || fail "server health check timed out on $BASE/health"
# Fetch the authenticated shell, then render it in a real 390px browser viewport.
curl -fsS -u "$USER:$PASSWORD" "$BASE/?tab=calendar" >"$TMP/page.html" || fail "failed to fetch authenticated shell"
[[ -s "$TMP/page.html" ]] || fail "authenticated shell response was empty"
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/diagnostics" | grep -F '"ok":true' >/dev/null || fail "diagnostics API contract failed"
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/jobs?limit=5" | grep -F '"ok":true' >/dev/null || fail "jobs API contract failed"
timeout 30 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome" \
  --window-size=390,844 --virtual-time-budget=3000 --dump-dom "file://$TMP/page.html" >"$TMP/dom.html" 2>"$TMP/chrome.log" || [[ -s "$TMP/dom.html" ]] || fail "chrome dump-dom produced no output"
[[ -s "$TMP/dom.html" ]] || fail "chrome dump-dom output was empty"
for needle in '<title>媒体订阅管理系统' 'manifest.webmanifest' '日历项目' '统一危险操作确认' '复制诊断' '选择当前显示'; do
  grep -F "$needle" "$TMP/dom.html" >/dev/null || fail "DOM missing expected marker: $needle"
done
timeout 30 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome-shot" \
  --window-size=390,844 --screenshot="$TMP/calendar-390.png" "file://$TMP/page.html" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/calendar-390.png" ]] || fail "chrome 390px screenshot failed"
[[ -s "$TMP/calendar-390.png" ]] || fail "chrome 390px screenshot was empty"
timeout 30 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --allow-file-access-from-files --user-data-dir="$TMP/chrome-desktop" \
  --window-size=1440,1000 --screenshot="$TMP/dashboard-1440.png" "file://$TMP/page.html" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/dashboard-1440.png" ]] || fail "chrome 1440px screenshot failed"
[[ -s "$TMP/dashboard-1440.png" ]] || fail "chrome 1440px screenshot was empty"
echo 'real browser E2E smoke passed: authenticated APIs plus 390x844 and 1440x1000 UI contracts'
