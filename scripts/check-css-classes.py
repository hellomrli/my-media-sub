#!/usr/bin/env python3
"""校验 static/styles.css 覆盖了前端实际引用的所有类名。

为什么需要这个门禁
==================
`static/styles.css` 是**入库的构建产物**，源码（`tailwind/input.css` +
`static/**`）改动后必须重新编译。CI 之前只检查了 HTML 拼接
（`build-frontend.mjs --check`）、JS 语法与 eslint，**没有任何步骤校验 CSS**，
于是 v2.7.0 新增的 `py-1.5`、`opacity-75` 一直到 v2.7.1 都没进产物，
线上按钮内边距与徽标透明度静默失效（见 docs/code-review-2026-09-21.md P0-2）。

本脚本不需要 Tailwind CLI：它只做「用到的类名是否在产物里有对应选择器」的
集合比对，同时能抓住产物过期与类名拼错两类问题。

用法：
    python3 scripts/check-css-classes.py [--verbose]

退出码 0 表示全部覆盖；1 表示存在未被覆盖的类名。
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
STYLES = ROOT / "static" / "styles.css"
INDEX = ROOT / "static" / "index.html"
JS_DIR = ROOT / "static" / "js"

# 类名 token 的允许形状：可选的负号前缀、小写字母开头，只含 Tailwind 允许的字符。
TOKEN_RE = re.compile(r"^-?[a-z][A-Za-z0-9_:/.\-\[\]%#(),!@&*+~'\"<>?^$|={}]*$")
SAFE_CHAR_RE = re.compile(r"[A-Za-z0-9_\-]")

# 这些类由元素选择器统一接管（tailwind/input.css 的 `input, select, textarea`），
# 产物里没有同名类选择器是**预期行为**，不是产物过期。
ALLOWLIST = {
    # 输入类控件由元素选择器统一接管，产物里没有同名类选择器是预期行为。
    "app-input",
    "app-select",
    # `darkMode: 'class'` 的主题标记：挂在 <html> 上供 CSS 变量切换，
    # Tailwind 不会为它生成 `.dark` 工具类，因此这里不算缺失。
    "dark",
}

# 长得像工具类、实际不是类名的字符串。
# `content-type` 命中 `content-` 前缀（fetch 的 headers 字面量），
# 属于 JS 侧的已知误报。
IGNORED_NON_CLASS = {"content-type", "content-length", "content-disposition"}

# JS 字符串里判定「这看起来像 Tailwind 工具类」的强特征。
# 只靠 looks_like_class 会把资源名（app-pagehide）、HTTP 头（cache-control）、
# 状态键（currentTab）当成类名，因此 JS 侧必须额外命中以下之一。
UTILITY_PREFIX_RE = re.compile(
    r"^(?:"
    r"bg|text|border|divide|ring|shadow|opacity|"
    r"p|px|py|pt|pb|pl|pr|m|mx|my|mt|mb|ml|mr|"
    r"w|h|min-w|min-h|max-w|max-h|size|"
    r"gap|space-x|space-y|order|col-span|row-span|grid-cols|grid-rows|"
    r"font|leading|tracking|rounded|z|top|bottom|left|right|inset|"
    r"items|justify|self|content|place|overflow|transition|duration|ease|"
    r"flex|inline-flex|grid|inline-grid|hidden|block|inline-block|inline|table|"
    r"absolute|relative|sticky|fixed|static|"
    r"cursor|select|pointer-events|whitespace|break|line-clamp|aspect|object|"
    r"fill|stroke|list|underline|uppercase|lowercase|capitalize|italic|antialiased"
    r")-"
)
STANDALONE_UTILITIES = {
    "flex", "grid", "hidden", "block", "inline", "inline-block", "inline-flex",
    "absolute", "relative", "sticky", "fixed", "static", "truncate", "underline",
    "italic", "uppercase", "lowercase", "capitalize", "antialiased", "container",
    "table", "contents", "isolate", "sr-only", "not-sr-only", "invisible", "visible",
}
VARIANT_RE = re.compile(
    r"^(?:hover|focus|focus-visible|focus-within|active|disabled|checked|group-hover|"
    r"peer-checked|first|last|odd|even|sm|md|lg|xl|2xl|dark|motion-safe|motion-reduce|"
    r"print|rtl|ltr):"
)


def is_strong_utility(token: str) -> bool:
    """token 是否具有明确的 Tailwind 工具类特征（用于 JS 侧过滤）。"""
    base = token
    while VARIANT_RE.match(base):
        base = base.split(":", 1)[1]
    if base in STANDALONE_UTILITIES:
        return True
    return bool(UTILITY_PREFIX_RE.match(base))


def css_escape(token: str) -> str:
    """按 Tailwind 的转义规则把类名 token 转成 CSS 选择器文本。"""
    out = []
    for ch in token:
        if SAFE_CHAR_RE.fullmatch(ch):
            out.append(ch)
        else:
            out.append("\\" + ch)
    return "." + "".join(out)


def looks_like_class(token: str) -> bool:
    """排除 Alpine/JS 表达式片段，只保留真正的类名。"""
    if not token or len(token) > 64:
        return False
    if not TOKEN_RE.match(token):
        return False
    # 明显的 JS 语法残留
    if any(bad in token for bad in ("..", "(", ")", "{", "}", "$", "'", '"', "=", "?", "&&", "||")):
        return False
    # 属性访问/方法调用形态，如 item.action、store.status
    if re.search(r"\.[a-z]", token):
        return False
    # 纯数字或纯标点
    if not re.search(r"[a-z]", token):
        return False
    # Tailwind 类名一律小写；出现大写基本是 JS 标识符（currentTab、calendarView）
    if token != token.lower():
        return False
    return True


def collect_from_html(path: pathlib.Path) -> set[str]:
    tokens: set[str] = set()
    if not path.exists():
        return tokens
    text = path.read_text(encoding="utf-8")
    # 只匹配真正的 `class` 属性；负向后顾排除 `:class=` / `x-bind:class=`，
    # 否则 Alpine 的绑定表达式（`currentTab === tab.id ? 'is-active' : ''`）
    # 会被当成类名列表。
    for match in re.finditer(r'(?<![\w:-])class="([^"]*)"', text):
        for token in match.group(1).split():
            if looks_like_class(token):
                tokens.add(token)
    return tokens


def collect_from_js(directory: pathlib.Path) -> set[str]:
    """从 JS 字符串字面量里收集类名。

    只接受「整段字符串都由空白分隔的合法类名组成」且至少含一个**强特征**
    Tailwind 工具类的字面量，这样资源名、HTTP 头、状态键与 URL 都会被丢弃。
    """
    tokens: set[str] = set()
    if not directory.exists():
        return tokens
    literal_re = re.compile(r"[`'\"]([^`'\"\n]{1,200})[`'\"]")
    for path in sorted(directory.rglob("*.js")):
        text = path.read_text(encoding="utf-8")
        for match in literal_re.finditer(text):
            raw = match.group(1)
            if not raw.strip() or "://" in raw:
                continue
            parts = raw.split()
            if not all(looks_like_class(part) for part in parts):
                continue
            if not any(is_strong_utility(part) for part in parts):
                continue
            tokens.update(parts)
    return tokens


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verbose", action="store_true", help="列出全部被检查的类名")
    parser.add_argument(
        "--styles",
        type=pathlib.Path,
        default=STYLES,
        help="要校验的样式产物（默认 static/styles.css，供测试与排查使用）",
    )
    args = parser.parse_args()
    styles_path = args.styles

    if not styles_path.exists():
        print(f"❌ 缺少样式产物：{styles_path}", file=sys.stderr)
        return 1
    styles = styles_path.read_text(encoding="utf-8")

    used = collect_from_html(INDEX) | collect_from_js(JS_DIR)
    if not used:
        print("❌ 未从 static/index.html 或 static/js 收集到任何类名，检查脚本可能失效", file=sys.stderr)
        return 1

    missing = sorted(
        token
        for token in used
        if token not in ALLOWLIST
        and token not in IGNORED_NON_CLASS
        and css_escape(token) not in styles
    )

    if args.verbose:
        for token in sorted(used):
            print(("  ok  " if token not in missing else " MISS ") + token)

    if missing:
        print(
            f"❌ static/styles.css 未覆盖 {len(missing)} 个被引用的类名"
            f"（共检查 {len(used)} 个）：",
            file=sys.stderr,
        )
        for token in missing[:40]:
            print(f"   - {token}", file=sys.stderr)
        if len(missing) > 40:
            print(f"   … 其余 {len(missing) - 40} 个省略", file=sys.stderr)
        print(
            "\n修复：在 tailwind/input.css 补上定义（若是自定义组件类），"
            "或在标记里改用合法的 Tailwind token，然后运行 scripts/build-css.sh。",
            file=sys.stderr,
        )
        return 1

    print(f"✅ CSS 类名覆盖检查通过：{len(used)} 个被引用的类名全部存在于 static/styles.css")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
