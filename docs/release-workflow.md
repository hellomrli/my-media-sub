# 发版工作流程

## 快速发版（推荐）

使用自动化脚本一次性完成版本号更新：

```bash
# 1. 运行版本更新脚本
bash scripts/bump-version.sh 2.2.27

# 2. 编辑生成的模板，填写更新内容
# - README.md 的 ### 2.2.26 部分
# - CHANGELOG.md 的 ## 2.2.26 部分
# - docs/upgrade-v2.2.26.md（如需要）

# 3. 提交并推送
git add -A
git commit -m "chore: bump version to 2.2.26"
git push origin main

# 4. 创建并推送 tag 触发发布
git tag v2.2.26
git push origin v2.2.26
```

脚本会自动更新以下 5 处：
- `Cargo.toml` version 字段
- `Cargo.lock` 依赖锁
- `README.md` 添加新版本模板
- `CHANGELOG.md` 添加新版本模板
- `docs/upgrade-v*.md` 创建升级文档

## 手动发版

如果不使用脚本，需要手动更新以下文件：

1. **Cargo.toml** - `version = "2.2.26"`（必须与 tag 匹配）
2. **README.md** - 添加 `### 2.2.26` 版本说明
3. **CHANGELOG.md** - 添加 `## 2.2.26` 详细变更
4. **docs/upgrade-v2.2.26.md** - 创建升级指南
5. **Cargo.lock** - 运行 `cargo check` 自动更新

然后按照上面的步骤 3-4 提交和推送。

## GitHub Actions 构建流程

推送 tag 后，GitHub Actions 会自动：

1. **版本检查**
   - ✅ **严格检查**：Tag 必须与 Cargo.toml 版本匹配
   - ⚠️ **警告检查**：README.md、CHANGELOG.md、upgrade 文档缺失不会阻塞发布

2. **代码质量检查**
   - 前端 JavaScript 语法和 ESLint
   - OpenAPI 契约校验
   - Rust 格式化、编译、Clippy、测试

3. **构建和发布**
   - 编译 release 二进制
   - 打包 tar.gz 归档
   - 发布到 GitHub Releases
   - 构建并推送 Docker 镜像到 GHCR

## 注意事项

- **必须先提交版本号更新，再推送 tag**，否则 tag 指向的 commit 里版本号还未更新
- Tag 格式必须是 `v*.*.*`（如 `v2.2.26`）
- 使用脚本可以避免遗漏某个文件导致的文档不一致
- 文档检查现在只是警告，不会阻塞发布流程
