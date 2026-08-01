#!/bin/bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "Example: $0 2.2.29"
  exit 1
fi

NEW_VERSION="$1"

# 验证版本号格式
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Version must be in format X.Y.Z (e.g., 2.2.26)" >&2
  exit 1
fi

# 获取当前版本
CURRENT_VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
echo "Current version: $CURRENT_VERSION"
echo "New version: $NEW_VERSION"
echo

# GNU sed 不接受替换串中的字面换行，必须转义成 \n 序列。
escape_sed_replacement() {
  printf '%s' "$1" | sed ':a;N;$!ba;s/\n/\\n/g'
}

# 1. 更新 Cargo.toml
echo "1. Updating Cargo.toml..."
sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# 2. 更新 README.md - 添加新版本说明模板
echo "2. Updating README.md..."
TEMPLATE="## 版本说明

### $NEW_VERSION

- TODO: 在这里填写版本更新内容"

sed -i "s/## 版本说明/$(escape_sed_replacement "$TEMPLATE")/" README.md

# 3. 更新 CHANGELOG.md - 添加新版本模板
echo "3. Updating CHANGELOG.md..."
CHANGELOG_TEMPLATE="升级步骤见对应的 [\`docs/upgrade-v*.md\`](docs/)；当前版本发布说明摘要也写在 [\`README.md\`](README.md) 的「版本说明」中。

## $NEW_VERSION

### TODO

- TODO: 在这里填写详细的变更说明"

CHANGELOG_TEMPLATE_ESCAPED="$(escape_sed_replacement "$CHANGELOG_TEMPLATE")"
sed -i "/^升级步骤见对应的/,/^## $CURRENT_VERSION/{
  s|^升级步骤见对应的.*|$CHANGELOG_TEMPLATE_ESCAPED|
}" CHANGELOG.md

# 4. 创建 upgrade 文档
echo "4. Creating docs/upgrade-v$NEW_VERSION.md..."
cat > "docs/upgrade-v$NEW_VERSION.md" <<EOF
# v$NEW_VERSION 升级指南

\`\`\`bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
\`\`\`

v$NEW_VERSION 不修改 JSON Store schema，可直接复用 v$CURRENT_VERSION 的 \`data/\`。

本版本 TODO: 简要描述主要变更。前端资源 URL 带有 \`?v=$NEW_VERSION\`。

如果使用二进制部署，必须同时替换归档中的整个 \`static/\` 目录；只替换二进制会继续运行旧前端代码。
EOF

# 5. 更新 Cargo.lock
echo "5. Updating Cargo.lock..."
cargo check --quiet 2>/dev/null || true

# 6. 更新前端资源版本标签。partials 是 index.html 的源头（由 build-frontend.mjs
#    拼装），service-worker.js 另有 ASSET_VERSION / CACHE_VERSION 常量。
echo "6. Updating frontend version tags (partials + service-worker.js)..."
VERSION_ESCAPED="$(printf '%s' "$CURRENT_VERSION" | sed 's/\./\\./g')"
for partial in static/partials/*.html; do
  sed -i "s/?v=$VERSION_ESCAPED/?v=$NEW_VERSION/g" "$partial"
done
sed -i "s|importScripts('/js/pwa-policy.js?v=$VERSION_ESCAPED')|importScripts('/js/pwa-policy.js?v=$NEW_VERSION')|" static/service-worker.js
sed -i "s/ASSET_VERSION = '$VERSION_ESCAPED'/ASSET_VERSION = '$NEW_VERSION'/" static/service-worker.js
sed -i "s/CACHE_VERSION = 'v$VERSION_ESCAPED-/CACHE_VERSION = 'v$NEW_VERSION-/" static/service-worker.js

# 7. 从 partials 重新生成 index.html，并同步 openapi.json 的 info.version
echo "7. Regenerating static/index.html and updating openapi.json..."
node scripts/build-frontend.mjs
python3 scripts/check-openapi.py --update

echo
echo "✅ Version bumped to $NEW_VERSION"
echo
echo "Next steps:"
echo "1. Edit README.md and CHANGELOG.md to fill in the TODO sections"
echo "2. Edit docs/upgrade-v$NEW_VERSION.md if needed"
echo "3. git diff --stat 检查版本面（index.html/openapi.json 已由脚本重新生成）"
echo "4. git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
echo "5. git push origin main"
echo "6. git tag v$NEW_VERSION && git push origin v$NEW_VERSION"
