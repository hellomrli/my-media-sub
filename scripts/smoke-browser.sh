#!/usr/bin/env bash
set -euo pipefail
BINARY="${1:-target/release/my-media-sub}"
CHROME="${CHROME_BIN:-$(command -v google-chrome || command -v google-chrome-stable || command -v chromium || command -v chromium-browser || true)}"
[[ -n "$CHROME" ]] || { echo 'Chrome/Chromium is required for browser smoke testing' >&2; exit 1; }
PORT="${BROWSER_SMOKE_PORT:-56195}"; BASE="http://127.0.0.1:${PORT}"
# Chrome 已不支持 URL 内嵌 Basic 凭据，渲染流量经本地代理注入 Authorization。
PROXY_PORT="${BROWSER_SMOKE_PROXY_PORT:-56196}"; PROXY_BASE="http://127.0.0.1:${PROXY_PORT}"
TMP="$(mktemp -d)"; PID=""; PROXY_PID=""
USER=browser-admin; PASSWORD=browser-password-not-for-production
fail(){ echo "browser smoke failed: $*" >&2; for log in server.log proxy.log chrome.log; do [[ -f "$TMP/$log" ]] && { echo "--- $log ---" >&2; tail -n 80 "$TMP/$log" >&2; }; done; exit 1; }
cleanup(){ for pid in "$PID" "$PROXY_PID"; do [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null || continue; kill "$pid" 2>/dev/null || true; for _ in $(seq 1 10); do kill -0 "$pid" 2>/dev/null || break; sleep .2; done; kill -9 "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; done; rm -rf "$TMP"; }; trap cleanup EXIT
[[ -x "$BINARY" ]] || fail "release binary is not executable: $BINARY"
SERVER_HOST=127.0.0.1 SERVER_PORT="$PORT" SERVER_USERNAME="$USER" SERVER_PASSWORD="$PASSWORD" DATA_DIR="$TMP/data" STATIC_DIR=static BACKUP_INTERVAL_HOURS=0 RUST_LOG=warn "$BINARY" >"$TMP/server.log" 2>&1 & PID=$!
cat >"$TMP/authproxy.py" <<'PY'
import base64, http.server, shutil, sys, urllib.error, urllib.request

proxy_port, upstream, credentials = int(sys.argv[1]), sys.argv[2], sys.argv[3]
token = base64.b64encode(credentials.encode()).decode()

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'
    def handle_one(self):
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else None
        request = urllib.request.Request(upstream + self.path, data=body, method=self.command)
        for name in ('Content-Type', 'Accept', 'Origin', 'Sec-Fetch-Site', 'Sec-Fetch-Mode', 'Sec-Fetch-Dest'):
            value = self.headers.get(name)
            if value: request.add_header(name, value)
        request.add_header('Authorization', 'Basic ' + token)
        try:
            response = urllib.request.urlopen(request)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            self.send_response(response.status if hasattr(response, 'status') else response.code)
            for name, value in response.getheaders():
                if name.lower() not in ('transfer-encoding', 'connection', 'keep-alive'):
                    self.send_header(name, value)
            self.end_headers()
            try: shutil.copyfileobj(response, self.wfile)
            except (BrokenPipeError, ConnectionResetError): pass
    def log_message(self, *args): sys.stderr.write(self.command + ' ' + self.path + '\n')
    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = handle_one

server = http.server.ThreadingHTTPServer(('127.0.0.1', proxy_port), Handler)
server.daemon_threads = True  # SSE 转发线程不得阻碍进程退出
server.serve_forever()
PY
python3 "$TMP/authproxy.py" "$PROXY_PORT" "$BASE" "$USER:$PASSWORD" >/dev/null 2>"$TMP/proxy.log" & PROXY_PID=$!
ready=0
for _ in $(seq 1 80); do
  if curl -fsS "$BASE/health" >/dev/null 2>&1; then ready=1; break; fi
  if ! kill -0 "$PID" 2>/dev/null; then fail "server exited before becoming healthy"; fi
  sleep .25
done
[[ "$ready" -eq 1 ]] || fail "server health check timed out on $BASE/health"
proxy_ready=0
for _ in $(seq 1 40); do
  if curl -fsS "$PROXY_BASE/health" >/dev/null 2>&1; then proxy_ready=1; break; fi
  if ! kill -0 "$PROXY_PID" 2>/dev/null; then fail "auth proxy exited before becoming ready"; fi
  sleep .25
done
[[ "$proxy_ready" -eq 1 ]] || fail "auth proxy health check timed out on $PROXY_BASE/health"
curl -fsS -u "$USER:$PASSWORD" "$BASE/?tab=calendar" >"$TMP/page.html" || fail "failed to fetch authenticated shell"
[[ -s "$TMP/page.html" ]] || fail "authenticated shell response was empty"
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/diagnostics" | grep -F '"ok":true' >/dev/null || fail "diagnostics API contract failed"
curl -fsS -u "$USER:$PASSWORD" "$BASE/api/jobs?limit=5" | grep -F '"ok":true' >/dev/null || fail "jobs API contract failed"
# x-cloak 只会被运行中的 Alpine 摘除：源码里必须有，水合后的 DOM 里必须没有。
SOURCE_CLOAK="$(grep -c 'x-cloak' "$TMP/page.html" || true)"
[[ "$SOURCE_CLOAK" -gt 0 ]] || fail "page source lost its x-cloak markers; the hydration assertion below would be vacuous"
# 渲染必须经 HTTP：file:// 下相对路径资产全部 404，Alpine 根本不会启动，
# 而纯静态标记在未执行任何 JS 的原始 HTML 里同样存在，断言会静默通过。
timeout 40 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --enable-logging=stderr --v=0 --user-data-dir="$TMP/chrome" \
  --window-size=390,844 --timeout=8000 --dump-dom "$PROXY_BASE/?tab=calendar" >"$TMP/dom.html" 2>"$TMP/chrome.log" || fail "chrome dump-dom exited with $?"
[[ -s "$TMP/dom.html" ]] || fail "chrome dump-dom output was empty"
for needle in '<title>媒体订阅管理系统' 'manifest.webmanifest' '日历项目' '统一危险操作确认' '复制诊断' '选择当前显示'; do
  grep -F "$needle" "$TMP/dom.html" >/dev/null || fail "DOM missing expected marker: $needle"
done
# 只有执行了 JS 才可能出现的证据：Alpine 摘除 x-cloak、为 x-show 写入行内样式。
DOM_CLOAK="$(grep -c 'x-cloak' "$TMP/dom.html" || true)"
[[ "$DOM_CLOAK" -eq 0 ]] || fail "Alpine did not hydrate: $DOM_CLOAK x-cloak markers remain in the rendered DOM"
grep -F 'style="display: none;"' "$TMP/dom.html" >/dev/null || fail "Alpine did not evaluate x-show bindings (no runtime inline styles in DOM)"
# 页面渲染期间的 JS 异常与资源加载失败都视为失败。
if grep -E 'Uncaught|ERR_[A-Z_]+|Failed to load resource' "$TMP/chrome.log" >/dev/null; then
  fail "chrome console reported errors during render: $(grep -E 'Uncaught|ERR_[A-Z_]+|Failed to load resource' "$TMP/chrome.log" | head -3)"
fi
timeout 40 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --user-data-dir="$TMP/chrome-shot" \
  --window-size=390,844 --timeout=8000 --screenshot="$TMP/calendar-390.png" "$PROXY_BASE/?tab=calendar" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/calendar-390.png" ]] || fail "chrome 390px screenshot failed"
[[ -s "$TMP/calendar-390.png" ]] || fail "chrome 390px screenshot was empty"
timeout 40 "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-background-networking --user-data-dir="$TMP/chrome-desktop" \
  --window-size=1440,1000 --timeout=8000 --screenshot="$TMP/dashboard-1440.png" "$PROXY_BASE/?tab=calendar" >/dev/null 2>>"$TMP/chrome.log" || [[ -s "$TMP/dashboard-1440.png" ]] || fail "chrome 1440px screenshot failed"
[[ -s "$TMP/dashboard-1440.png" ]] || fail "chrome 1440px screenshot was empty"
echo 'real browser E2E smoke passed: authenticated APIs plus hydrated 390x844 and 1440x1000 UI contracts'
