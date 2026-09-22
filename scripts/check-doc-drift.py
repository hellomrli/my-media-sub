#!/usr/bin/env python3
"""校验文档里的「散文事实」与代码一致。

CI 的版本一致性检查只覆盖版本面（README 版本说明段、CHANGELOG 段、upgrade
文档、文档索引链接、OpenAPI 的 info.version）。评审发现有三类漂移结构上逃过了
所有门禁，因为它们写在自然语言里：

1. `docs/roadmap.md` 的「当前发布基线 / 当前阶段」指针——长期停在 v2.5.1，
   而实际版本已经到 v2.7.1；
2. 文档里手写的 OpenAPI 路径/操作数量——同一个数字在四处写法互不相同；
3. README 文档索引与实际 `docs/` 目录不一致。

本脚本把这些改成可校验的断言。

用法：
    python3 scripts/check-doc-drift.py
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, re.M)
    if not match:
        raise SystemExit("无法从 Cargo.toml 读取版本")
    return match.group(1)


def openapi_counts() -> tuple[int, int]:
    spec = json.loads((ROOT / "static" / "openapi.json").read_text(encoding="utf-8"))
    paths = spec.get("paths", {})
    methods = {"get", "post", "put", "patch", "delete", "head", "options", "trace"}
    operations = sum(len([m for m in ops if m.lower() in methods]) for ops in paths.values())
    return len(paths), operations


def main() -> int:
    version = cargo_version()
    failures: list[str] = []

    # 1) roadmap 执行指针必须与 Cargo.toml 版本一致
    roadmap = (ROOT / "docs" / "roadmap.md").read_text(encoding="utf-8")
    baseline = re.search(r"当前发布基线\*\*：v([0-9]+\.[0-9]+\.[0-9]+)", roadmap)
    if not baseline:
        failures.append("docs/roadmap.md 找不到「当前发布基线：vX.Y.Z」")
    elif baseline.group(1) != version:
        failures.append(
            f"docs/roadmap.md 的当前发布基线是 v{baseline.group(1)}，"
            f"但 Cargo.toml 是 v{version}（执行指针漂移）"
        )
    stage = re.search(r"当前阶段\*\*：v([0-9]+\.[0-9]+\.[0-9]+)", roadmap)
    if stage and stage.group(1) != version:
        failures.append(
            f"docs/roadmap.md 的当前阶段是 v{stage.group(1)}，但 Cargo.toml 是 v{version}"
        )

    # 2) 散文里的 OpenAPI 数量必须与现实一致。
    #    只校验「当前基线」类表述；历史段落里的旧数字（如 v1.12.0 基线快照）
    #    是刻意保留的历史记录，不参与比对。
    paths, operations = openapi_counts()
    api_contract = (ROOT / "docs" / "api-contract.md").read_text(encoding="utf-8")
    actual = f"{paths} 条路径、{operations} 个操作"
    if actual not in api_contract:
        # 允许写成「当前 N 条路径、M 个操作，以脚本输出为准」
        alt = f"当前 {paths} 条路径、{operations} 个操作"
        if alt not in api_contract:
            failures.append(
                f"docs/api-contract.md 未包含当前 OpenAPI 规模（{actual}）；"
                "手写数量已漂移，请更新为脚本输出"
            )
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    if f"{paths} 个路径、{operations} 个操作" not in readme:
        failures.append(f"README.md 未包含当前 OpenAPI 规模（{paths} 个路径、{operations} 个操作）")

    # 3) roadmap 必须可从 README 文档索引到达（它自称唯一计划入口，
    #    却曾经完全不在索引里）。
    if "docs/roadmap.md" not in readme:
        failures.append("README.md 文档索引缺少 docs/roadmap.md 链接")

    # 4) 评审文档若存在，必须在 roadmap 里被引用，避免再次变成孤儿
    review_docs = sorted((ROOT / "docs").glob("code-review-*.md"))
    for doc in review_docs:
        if doc.name not in roadmap and doc.name not in readme:
            failures.append(f"{doc.relative_to(ROOT)} 既不在 README 也不在 roadmap 中被引用")

    if failures:
        print("❌ 文档散文漂移：", file=sys.stderr)
        for item in failures:
            print(f"   - {item}", file=sys.stderr)
        return 1

    print(
        f"✅ 文档散文一致：roadmap 基线 v{version}，"
        f"OpenAPI {paths} 路径/{operations} 操作，roadmap 与评审文档均已挂到索引"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
