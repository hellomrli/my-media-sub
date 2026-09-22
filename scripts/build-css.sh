#!/usr/bin/env bash
# 编译 Tailwind CSS：扫描 static/ 下的 HTML/JS，把 tailwind/input.css 编译为 static/styles.css。
#
# 依赖 Tailwind standalone CLI（单二进制，无需 npm/node_modules）。
# 该二进制不入库；首次使用请下载到 PATH（或设置 TAILWIND_BIN 指向它）：
#   https://github.com/tailwindlabs/tailwindcss/releases  （选 tailwindcss-linux-x64 等对应平台）
#   chmod +x tailwindcss-linux-x64 && mv tailwindcss-linux-x64 ~/.local/bin/tailwindcss
#
# 用法：
#   scripts/build-css.sh            # 编译并 minify
#   scripts/build-css.sh --watch    # 监听变更持续编译（开发用，不 minify）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export BROWSERSLIST_IGNORE_OLD_DATA=1

BIN="${TAILWIND_BIN:-tailwindcss}"
if ! command -v "$BIN" >/dev/null 2>&1; then
  echo "错误：找不到 Tailwind CLI（'$BIN'）。" >&2
  echo "请从 https://github.com/tailwindlabs/tailwindcss/releases 下载 standalone 二进制，" >&2
  echo "放到 PATH 中（如 ~/.local/bin/tailwindcss），或用 TAILWIND_BIN 环境变量指定路径。" >&2
  exit 1
fi

# 必须使用 v3：本项目用的是 v3 风格配置（tailwind.config.js 的 content/theme.extend
# 与 @tailwind base|components|utilities 指令）。v4 会产出结构不同的 preflight 与
# 工具类输出，直接跑会产生大面积无意义 diff 甚至视觉回归。
# 固定的版本：v3.4.17（用它可以逐字节复现既有的 static/styles.css）。
TW_VERSION="$("$BIN" --help 2>&1 | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' | head -n1 || true)"
case "$TW_VERSION" in
  v3.*) ;;
  "")
    echo "警告：无法识别 Tailwind CLI 版本（'$BIN'），请确认是 v3.x。" >&2
    ;;
  *)
    echo "错误：检测到 Tailwind $TW_VERSION，但本项目需要 v3.x（建议 v3.4.17）。" >&2
    echo "v4 的输出与现有 static/styles.css 不兼容，请改用 v3 的 standalone 二进制。" >&2
    exit 1
    ;;
esac

INPUT="tailwind/input.css"
OUTPUT="static/styles.css"

if [[ "${1:-}" == "--watch" ]]; then
  exec "$BIN" -c tailwind.config.js -i "$INPUT" -o "$OUTPUT" --watch
fi

"$BIN" -c tailwind.config.js -i "$INPUT" -o "$OUTPUT" --minify
echo "已生成 $OUTPUT"

# 产物必须覆盖所有被引用的类名，否则说明词表/标记有未定义的自定义类。
python3 "$(dirname "${BASH_SOURCE[0]}")/check-css-classes.py"
