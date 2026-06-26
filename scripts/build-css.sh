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

BIN="${TAILWIND_BIN:-tailwindcss}"
if ! command -v "$BIN" >/dev/null 2>&1; then
  echo "错误：找不到 Tailwind CLI（'$BIN'）。" >&2
  echo "请从 https://github.com/tailwindlabs/tailwindcss/releases 下载 standalone 二进制，" >&2
  echo "放到 PATH 中（如 ~/.local/bin/tailwindcss），或用 TAILWIND_BIN 环境变量指定路径。" >&2
  exit 1
fi

INPUT="tailwind/input.css"
OUTPUT="static/styles.css"

if [[ "${1:-}" == "--watch" ]]; then
  exec "$BIN" -c tailwind.config.js -i "$INPUT" -o "$OUTPUT" --watch
fi

"$BIN" -c tailwind.config.js -i "$INPUT" -o "$OUTPUT" --minify
echo "已生成 $OUTPUT"
