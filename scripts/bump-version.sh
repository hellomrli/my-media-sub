#!/bin/bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "Example: $0 2.2.26"
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

# 1. 更新 Cargo.toml
echo "1. Updating Cargo.toml..."
sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# 2. 更新 README.md - 添加新版本说明模板
echo "2. Updating README.md..."
TEMPLATE="## 版本说明

### $NEW_VERSION

- TODO: 在这里填写版本更新内容

### $CURRENT_VERSION"

sed -i "s/## 版本说明/$TEMPLATE/" README.md

# 3. 更新 CHANGELOG.md - 添加新版本模板
echo "3. Updating CHANGELOG.md..."
CHANGELOG_TEMPLATE="升级步骤见对应的 [\`docs/upgrade-v*.md\`](docs/)；当前版本发布说明摘要也写在 [\`README.md\`](README.md) 的「版本说明」中。

## $NEW_VERSION

### TODO

- TODO: 在这里填写详细的变更说明

## $CURRENT_VERSION"

sed -i "/^升级步骤见对应的/,/^## $CURRENT_VERSION/{
  s|^升级步骤见对应的.*|$CHANGELOG_TEMPLATE|
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

# 6. 更新 static/index.html 中的版本号
echo "6. Updating static/index.html version tags..."
sed -i "s/\?v=$CURRENT_VERSION/?v=$NEW_VERSION/g" static/index.html

echo
echo "✅ Version bumped to $NEW_VERSION"
echo
echo "Next steps:"
echo "1. Edit README.md and CHANGELOG.md to fill in the TODO sections"
echo "2. Edit docs/upgrade-v$NEW_VERSION.md if needed"
echo "3. git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
echo "4. git push origin main"
echo "5. git tag v$NEW_VERSION && git push origin v$NEW_VERSION"
