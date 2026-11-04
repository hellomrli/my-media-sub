# 从源码构建与验证

在仓库根目录执行以下命令。安装 rustup、Node.js 24 LTS、Python 3；OpenSSL CLI 用于生成示例管理员密码。

rustup 自动读取 `rust-toolchain.toml` 固定的 **Rust 1.99.1**，开发、CI、Release 与 Docker 构建保持一致。`Cargo.toml` 的 `rust-version = "1.89"` 仅表示最低源码兼容版本（MSRV）；独立 MSRV job 会验证它，正式产物不用该旧编译器构建。前端不使用打包器，`static/index.html` 由模板与 partials 拼装生成，**不要手改**。

```bash
export SERVER_PASSWORD="$(openssl rand -hex 24)"
printf '管理员密码：%s\n' "$SERVER_PASSWORD"
SERVER_HOST=127.0.0.1 cargo run --release --locked
```

常用本地检查（完整门禁见 `.github/workflows/ci.yml`，依赖维护与 MSRV 验证见 [安全审计说明](security-audit.md)）：

```bash
node scripts/build-frontend.mjs --check          # 前端产物是否过期
find static -type f -name '*.js' -print0 | sort -z | xargs -0 -n1 node --check
node --test tests/frontend_*.test.js             # 前端单元与回归测试
npx --yes eslint@10.8.0 'static/**/*.js'         # no-undef 是原生 JS 的静态安全网

python3 scripts/check-rust-toolchain.py          # 开发、CI、Release 与 Docker 工具链对齐
python3 scripts/check-css-classes.py             # 编译后 CSS 覆盖全部引用类名
python3 scripts/check-openapi.py                 # 路由与规范双向契约
python3 scripts/check-doc-drift.py               # 文档与发布基线一致

cargo fmt --all -- --check
cargo check --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked                        # Rust 单元、集成与文档测试
cargo build --release --locked
```

改动任何静态资源后要同步提升 `static/service-worker.js` 的 `CACHE_VERSION`，否则 PWA 客户端可能继续命中旧缓存。未提升应用版本时，也应同步修改 HTML 与 Service Worker 中对应资源的 `rev` 参数，避免继续命中 HTTP 旧缓存。

独立进程不会自动读取 `.env`，不要仅复制文件就启动。日志和错误报告中不要包含密码。发布与 MSRV 的完整要求见 [发布流程](release-workflow.md) 和 [依赖审计](security-audit.md)。

## 浏览器回归与截图

安装 Chrome（可用 `CHROME_BIN` 指定路径）后执行：

```bash
cargo build --locked
node scripts/smoke-search-browser.mjs target/debug/my-media-sub
```

该脚本使用临时数据目录、独立 Chrome 配置与本地测试响应，验证资源搜索、图片回退、并发请求，以及 1440px / 390px 的剧名匹配界面。标题清洗请求访问本地后端；元数据搜索响应由本地 fixture 提供，不访问真实 TMDB 或网盘。后端候选生成与重试控制另由 Rust 测试覆盖。

更新 README 截图时，显式加上 `--metadata-screenshot`：

```bash
node scripts/smoke-search-browser.mjs target/debug/my-media-sub --metadata-screenshot
```

此选项覆盖 `docs/assets/metadata-match.png`；正常 smoke 不改文档资产。截图保留真实页面布局，使用明确标注的虚构剧集数据。
