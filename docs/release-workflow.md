# 发版工作流程

发布需要明确授权。顺序是：**准备版本与文档 → 本地验证 → 推送 main → 等待该提交 CI 全绿 → 推送版本 tag → 核验 Release 与镜像**。不要先打 tag 再等待质量检查。

## 工具链与版本准备

开发、CI、Release 和 Docker builder 使用 `rust-toolchain.toml` 固定的 Rust 1.99.1。`Cargo.toml` 的 `rust-version = "1.89"` 仅为 MSRV，由独立 job 验证；不是正式构建的编译器版本。

选择未使用的 `X.Y.Z` 版本，同步以下内容：

- `Cargo.toml` 与 `Cargo.lock` 中本项目的版本，以及 `static/openapi.json` 的 `info.version`。
- README 的 `### X.Y.Z` 摘要和「当前版本」链接；详细记录放入 CHANGELOG 的 `## X.Y.Z`、`docs/upgrade-vX.Y.Z.md`，不要让 README 再积累完整版本历史。
- `docs/roadmap.md` 的当前阶段与发布基线、`docs/usage.md` 的下载版本和健康检查示例。
- `static/partials/*.html` 的资源版本、`static/service-worker.js` 的导入版本、`ASSET_VERSION` 与 `CACHE_VERSION`；用 `node scripts/build-frontend.mjs` 重新生成 `static/index.html`。

`scripts/bump-version.sh` 只是生成模板的辅助工具，不能代替上述逐项检查、填写变更说明与独立编译验证。它不会完成所有文档维护；不要只运行脚本就发布。修改 OpenAPI 时保留已有契约与紧凑格式，不要把接口回退为通用模板。

依赖升级遵循 [安全审计策略](security-audit.md)：检查最新稳定版和公告，保留 MSRV 验证，不以增加忽略项掩盖漏洞。只做同版本重新构建不足以让在线更新器发现新版本，必须增加应用版本。

## 本地发布门禁

完整常用命令见 [开发文档](development.md)。至少执行：

```bash
node scripts/build-frontend.mjs --check
node --test tests/frontend_*.test.js
npx --yes eslint@10.8.0 'static/**/*.js'
python3 scripts/check-rust-toolchain.py
python3 scripts/check-css-classes.py
python3 scripts/check-openapi.py
python3 scripts/check-doc-drift.py
cargo fmt --all -- --check
cargo check --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
cargo deny --locked check
CARGO_TARGET_DIR=target/msrv cargo +1.89.0 check --all-targets --all-features --locked
cargo build --release --locked
scripts/smoke-release.sh target/release/my-media-sub
scripts/smoke-docker-entrypoint.sh target/release/my-media-sub static
scripts/smoke-browser.sh target/release/my-media-sub
node scripts/smoke-search-browser.mjs target/release/my-media-sub
git diff --check
```

MSRV 检查需要已安装 Rust 1.89.0；浏览器回归需要 Chrome。没有 Docker 时，入口脚本 smoke **不等同于真实镜像验证**，必须等待 main CI 的 Docker build and smoke job。

## 先推送 main 并等待 CI

检查 `git status` 与差异，只暂存本次发布涉及的文件，确认不包含 `.env`、Cookie、Token、业务数据或备份。测试夹具中的公开测试密钥不得用于部署。

```bash
VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
git diff --cached --check
git diff --cached --stat
git commit -m "release: v$VERSION"
git push origin main
SHA=$(git rev-parse HEAD)
gh run list --workflow ci.yml --commit "$SHA"
```

等待刚推送的 **同一个 SHA** 的 CI 完成，不能以其他提交的绿色结果代替。本流程的 main CI 会阻断版本说明、升级指南或路线图漂移，并验证 Rust / MSRV / 前端 / 安全审计 / Docker / 浏览器 / 跨版本升级。CI 成功后另一个 Docker Image 工作流发布 main/latest 镜像。

## 再推送 tag 并核验产物

确认 CI 结论为 success，工作树无遗漏、HEAD 未改变后：

```bash
git tag -a "v$VERSION" -m "Release v$VERSION"
git push origin "v$VERSION"
gh run list --commit "$SHA"
gh release view "v$VERSION"
```

tag 同时触发 Release 和 Docker Image 工作流。等待它们都成功，不要只看到 tag 就宣布发布完成；失败时修复或按失败原因重跑，不覆盖已有 tag。

Release 在 Ubuntu 22.04 构建 Linux x86_64 二进制，以兼容 bookworm 的 glibc，发布以下资产：

- `my-media-sub-vX.Y.Z-linux-x86_64.tar.gz`：程序、完整 `static/`、`docs/`、README 与 CHANGELOG。
- 同名 `.tar.gz.sha256`：下载完整性校验。
- 同名 `.tar.gz.minisig`：仅在配置 `MINISIGN_SECRET_KEY` 时生成；未配置会明确告警。SHA-256 不能替代发布方签名，详见 [在线更新信任配置](docker-online-update.md)。

发布说明从 README 的对应版本提取，到下一个同级或更高标题停止，并将文档链接转为版本 tag 下的绝对链接。Release 会重新执行 Rust / 前端 / 依赖审计与 smoke；tag 必须与 Cargo 版本严格一致。虽然 Release 内的部分文档检查仍只是警告，**main CI 的同类检查会阻断**。

最后下载实际云端归档及校验文件，在临时目录执行 `sha256sum -c`，核对内容并运行该二进制的版本检查和 smoke。确认 GHCR 的版本标签与提交标识正确，Docker 升级 smoke 成功。Docker 构建会拉取最新基础镜像、刷新运行层系统补丁；用户只做 WebUI 在线更新不会获得系统层更新，应在升级说明中提醒拉取镜像并重建容器。
