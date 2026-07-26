# 代码评审报告 2026-07-26

评审范围：`main` 分支，v2.2.14（`0683428`）已提交，工作树干净。约 47k 行 Rust + 8k 行原生 JS。

## 总体结论

项目质量高于同规模项目的常见水平，评审重点因此不是"补基础"，而是堵几个特定的漏网缺口。

CI 门禁本身扎实：fmt、clippy `-D warnings`、全量 `cargo test --all --locked`、RustSec 审计、OpenAPI 契约校验、7 个 smoke 脚本中的 5 个都在跑。评审中实际执行了 fmt / clippy / 前端测试，全部干净（367 个 `src/` 测试 + 53 个 `tests/` 测试 + 83 个前端测试）。

真正的问题集中在**发布链路的门禁缺失**和**几处"静默通过"的校验**。另有一个功能性 bug（浏览器 Push 开关完全不可用，问题 4.5），由前端无 lint 导致，是本次评审唯一有用户可见影响的缺陷。

## 按影响排序的问题

### 1. Docker 镜像发布没有 CI 门禁——红色构建可直达生产

`docker-image.yml` 在 `push: branches: [main]` 上触发，直接推送 `type=raw,value=latest`（`:55`），而 `grep 'needs:\|workflow_run'` 结果为空。clippy 挂了、测试红了，`:latest` 照样发布。而 `docker-compose.yml:3` 正好钉在 `ghcr.io/hellomrli/my-media-sub:latest`，所以坏提交会自动流向所有默认部署。

全项目爆炸半径最大的缺口，也最好修。

修法：触发改为 `on: workflow_run: workflows: [CI], types: [completed]`，配 `if: github.event.workflow_run.conclusion == 'success'`。另外 `README.md:85` 自己就在建议生产环境钉版本号，compose 却没照做。

### 2. 浏览器 E2E 在静默通过——与 `7a830dc` 修的是同一类 bug

`scripts/smoke-browser.sh:24` 从 `file://$TMP/page.html` 渲染页面，但 `index.html` 用相对路径引用资源（`js/core/api.js?v=2.2.14`），在 `file:///tmp/` 下全部 404，Alpine 根本不会启动。而 `:27` 断言的 6 个 DOM 标记全部存在于**原始 HTML 文本**中，所以哪怕一行 JS 都没执行，测试也是绿的。截图只检查了非空。

修法：经 `$BASE` 用 HTTP 提供页面，断言只有 JS 才能产生的东西（渲染出的列表行、`x-data` 水合标记、`document.querySelectorAll('[data-testid]').length`），并把 `chrome.log` 中的 `ERR_FILE_NOT_FOUND` / `Uncaught` 视为失败。

### 3. `/api/update/*` 是全仓最危险的路径，零集成测试

`src/api/update.rs`（1364 行）替换正在运行的二进制和整个 `static/`，然后经 `src/restart.rs:75` 触发 `execve`。11 个测试全是纯函数级（版本比较、校验和解析、资产查找），`grep '/api/update' tests/*.rs` 无匹配——5 条路由无一有端到端覆盖。

这条链路的**设计**是稳的，逐项确认过：`GITHUB_REPO` 是编译期常量（更新源无法被配置污染）、SHA256 校验在解压前、tag 校验拒绝 `/`（阻断路径穿越）、`tar -xzf` 参数化调用无 shell 注入、`execve` 前先等 Axum 和 job queue 优雅关闭。

缺的是把这些保证钉死的测试：坏校验和必须拒绝且不动二进制、部分 static 载荷必须回滚（`update.rs:112-179` 的逻辑已有单测，抬升为端到端断言即可）、`SELF_UPDATE_ENABLED=false` 必须拒绝。

### 4. `openapi.json` 的版本漂移无人拦截

`scripts/check-openapi.py:233` 收集了 `info.version`，但 `check()` 从不与 `Cargo.toml` 比对。评审中实测将其改为 `0.0.1-DRIFT`，脚本仍返回 `passed: 93 paths, 105 operations`，退出码 0（文件已还原）。

其他版本面**都已强制**，这是发布流程做得好的地方：

| 版本面 | 是否强制 |
|---|---|
| tag == Cargo 版本 | 是 — `release.yml:56` |
| `README.md` `### X.Y.Z` | 是 — `release.yml:61` |
| `CHANGELOG.md` `## X.Y.Z` | 是 — `release.yml:69` |
| `docs/upgrade-vX.Y.Z.md` 非空 | 是 — `release.yml:73` |
| SW `CACHE_VERSION` | 是 — `frontend_pwa.test.js:51` |
| `index.html` 25+ 处 `?v=` | 是 — `frontend_dom_safety.test.js:27` |
| `openapi.json` `info.version` | **否** |
| README docker 拉取命令版本号 | **否** |

`info.version` 是唯一漏网的一处，在 `check()` 中补两行比对即可闭环。

### 4.5 【功能性 bug】浏览器 Push 开关完全不可用

本次评审发现的唯一功能性 bug，已亲手验证。

`static/js/features/pwa.js` 没有 `MediaSubApi` 依赖头（模块开头只声明了 `SHORTCUTS`），但在 `:115`、`:123`、`:126` 三处调用 `apiData(...)`。三处都是**裸标识符**，无 `this.` 前缀，所以 `composeStores` 的摊平 store 救不了它——词法作用域里根本没有这个名字，`'use strict'` 下直接抛 `ReferenceError: apiData is not defined`。

验证过程：`require('./static/js/features/pwa.js').createStore()` 后 `apiData` 为 `undefined`；模块源码中无任何 `apiData` 声明；受影响函数是 `toggleBrowserPush`。

**为什么长期没被发现**：`toggleBrowserPush` 整个函数体包在 `try` 里，`:129` 的 `catch` 把 `ReferenceError` 转成通用 toast「浏览器 Push 操作失败」。用户看到的是一次普通失败，不是崩溃，所以看起来像"Push 服务有问题"而不是"代码有 bug"。

**影响是真实的**：`static/partials/nav.html:90` 有绑定 `@click="toggleBrowserPush()"` 的导航栏按钮（`x-show="browserPushSupported"`），启用和关闭两条路径都走同一个函数，因此浏览器 Push 功能整体失效。

**为什么没被拦住**：`grep -rn 'toggleBrowserPush' tests/ static/js/` 除定义处外零命中——`tests/frontend_pwa.test.js` 存在但只测 `CACHE_VERSION` 一致性，不碰这个函数。仓库中也没有任何 ESLint 配置，而 `no-undef` 这条最基础的规则本来就能在 CI 里抓到自由标识符。

修法：给 `pwa.js` 补上标准的 `MediaSubApi` 依赖头（与其余 12 个模块一致）。更重要的是配一份 ESLint，至少开 `no-undef`——这类 bug 在 8k 行无 lint 的原生 JS 里可能还有第二个。

### 4.6 PWA 中一处无界监听器泄漏

`features/pwa.js:71` 和 `:74` 是全前端唯一绕过 `core/polling.js` 注册表的两处 `addEventListener`（已 grep 确认）：

- `:71` `registration.addEventListener('updatefound', …)` —— 未跟踪、无对应 remove、`destroy()` 不清理。`ServiceWorkerRegistration` 的生命周期长于 Alpine 组件，闭包捕获 `this`（738 键的完整 store），整个 store 被留存。
- `:74` `worker.addEventListener('statechange', …)` 在 `updatefound` 回调**内部**注册 —— 每触发一次 `updatefound` 就新增一个监听器，页面生命周期内无上界，每个都捕获 `this`。这是两者中更严重的一个。

对比之下 `core/polling.js` 的注册表设计是好的：`track()` 会先 `stop(name)`，所以同名重复注册会自动移除旧实例；`stopAll()` 由 `pagehide` 驱动的 `destroy()` 调用。全部 11 个应用级监听器都走注册表且名称唯一，`stores/jobs.js:316` 的 EventSource 也用 `ownLifecycle` 显式 `close()`。

修法：把这两处改走 `listenLifecycle('pwa-updatefound', registration, …)` 和 `listenLifecycle('pwa-worker-statechange', worker, …)`。机械改动。

### 4.7 前端模块图：18 条死依赖边与静默的加载顺序脆弱性

`core/router.js:10-15`、`core/notifications.js:10-15`、`core/shell.js:10-15` 三个 core 模块各自读取 5 个 `features/*` 全局加 `MediaSubFormatters`，构成 18 条 core→features 的分层倒置边。但这些标识符在各文件中**只出现在自己的声明行上**——全是复制粘贴的死代码，删掉行为不变。`stores/downloads.js`、`stores/drive.js`、`stores/jobs.js` 的 `:11-15` 同样全死。唯一真实的 stores→features 依赖是 `stores/subscriptions.js` → `subscription-detail` / `source-switch` / `automation-events`。

更值得注意的是**加载顺序当前正确但静默脆弱**：68 处 `const X = root.MediaSubY || {}` 的即时捕获配合下一行解构，意味着依赖缺失时得到的是 `undefined` 绑定而不是加载错误。审查中实测把 `core/api.js` 移到消费者之后：24 个文件全部无错加载，`app()` 仍返回 738 键 store，直到用户点击才抛 `TypeError: getApiErrorMessage is not a function`——故障被推迟且无法归因到真正原因。`app.js:31` 是整条链上唯一的守卫，且只检查 `createStore` 存在，不覆盖这 68 处捕获。

另外 `composeStores` 把 15 个 store 摊平到一个对象，导致 core 通过 `this` 调用 stores/features 的方法达 39 处，形成静态图看不见的运行时循环（`core/shell.js ↔ features/pwa.js` 等）。这一点无法靠 import 纪律约束，属于架构选择而非缺陷，记录备查。

键名冲突当前为零（738 个键无重叠），但 `composeStores` 对重复键是静默后者胜出，这个不变量目前只靠人工纪律维持。

### 5. 前端构建产物的 `--check` 不在 CI 里

`index.html` 和 `styles.css` 都是提交进仓库的生成物，但 `grep -rn build-frontend .github/` 无命中，`build-css.sh` 同样不在任何 workflow 中。改了 `static/partials/*.html` 而忘记重新构建，CI 依然是绿的。当前 `index.html` 是最新的，但这靠人工纪律而非 CI 拦截。

`frontend_dom_safety.test.js` 部分遮掩了这个问题（它读 `index.html`，所以陈旧的 `?v=` 会失败），但任何不涉及版本号的 partial 改动会静默漏过。

修法：往 `ci.yml:36` 和 `release.yml:78` 各加一行 `node scripts/build-frontend.mjs --check`。`build-css.sh` 同理，但 Tailwind CLI 在本环境不可用，至少可断言 `styles.css` 不早于 `tailwind/`。

### 6. 前端轮询不感知页面可见性

`grep -rn 'visibilitychange\|document.hidden' static/` 在整个前端零命中。轮询间隔：

- 下载页 2 秒 — `stores/downloads.js:309`
- 通知 30 秒 — `core/notifications.js:199`
- 更新进度 800 毫秒 — `features/updates.js:329`

标签页切到后台后全部继续跑。停在下载页就是每分钟 30 次请求，手机上尤其耗电。

轮询架构本身是好的：`core/polling.js` 有统一注册表，同名 timer 重复注册会先清理旧实例，不会泄漏。

修法：加 `visibilitychange` 监听，隐藏时 `stopAll()`，恢复时重启并立即拉一次。局部改动。

### 7. 容器运行时加固缺几项

`docker-compose.yml` 只有 6 行有效配置。缺失项：

- **无日志轮转和大小上限**（最省力的修复）。日志是纯 stdout（无 `tracing-appender`），compose 未设 `logging.options.max-size` / `max-file`，长期运行主机上 json-file 驱动会无界增长。
- 无 `read_only: true`、`cap_drop: [ALL]`、`security_opt: [no-new-privileges:true]`、内存上限。入口脚本只需写 `/app/runtime` 和 `/app/data`，所以 `read_only` 加这两个 volume 是可行的。
- 钉在 `:latest`（见问题 1）。

安全与可观测性基线本身是好的，逐项确认过：容器经 `gosu` 降权到 UID 1000（`Dockerfile:65-71`，进程非 root）；认证是全局中间件，只有 `/health` 在白名单外（`api/mod.rs:165`，供 HEALTHCHECK 使用）；`/metrics` 和 `/api/metrics` 均需 `diagnostics:read` scope（`api/mod.rs:273-274`，没有意外公开）；CSRF 用 Origin + Fetch Metadata 判断且有测试覆盖；前端零 `innerHTML`，唯一的 4 处 `x-html`（`nav.html`）只渲染代码内定义的 SVG 图标，不含用户数据；密钥脱敏专门测了掩码值不会覆盖真实密钥这个易错点（`api/settings.rs:1318`）；`LOG_FORMAT=json` 结构化日志带 request/correlation/subscription/job ID 且过滤器可运行时重载；优雅关闭同时处理 SIGTERM 和 Ctrl+C，排空 job queue 且发生在 `execve` 之前（`main.rs:84-112`）；文件写入是 tmp + fsync + rename 原子模式且放在 `spawn_blocking` 中，未阻塞 async 运行时；数值型设置项都做了 clamp 钳制。

### 8. 工程治理缺三样

- 无 `.github/dependabot.yml`
- 无覆盖率度量（`tarpaulin` / `llvm-cov` / `codecov` 全无命中）
- 无 `clippy.toml` / `rustfmt.toml` / `Cargo.toml` 的 `[lints]`——clippy 严格度只存在于 CI 调用参数里，本地 `cargo clippy` 比 CI 宽松，问题要 push 后才暴露。把 `-D warnings` 移进 `[lints]` 即可对齐。

集成测试覆盖 93 条路径中的 35 条。核心 CRUD 到位，缺口集中在 aria2 / drive（`/api/drive/aria2/*` 共 12 个 operation，任何测试都未引用）、push、subscription_source。

行数较大且无直接测试的模块：`telegram_bot/menus.rs`(734)、`subscription_transfer/helpers.rs`(624)、`api/push.rs`(459)、`subscription_check/file_filter_methods.rs`(435)、`api/subscription_source.rs`(437)、`api/drive/aria2.rs`(404)。

### 9. 文档负担和产物陈旧

27 个 `docs/upgrade-v*.md` 共 48KB，而 `release.yml:73` **强制**每个版本都要有非空升级文档，导致近期几个只有 391-1105 字节，内容基本是"无 schema 变更，拉新 tag 即可"。同一内容还在 README（12 个版本标题，20KB）和 CHANGELOG（53KB，28 条）里重复第二、三遍。

建议：门禁放宽为仅当 CHANGELOG 条目含 `BREAKING` / `迁移` 标记时才要求升级文档（2.2.14 的 data/runtime 卷拆分确实够格）；README 只留最近 3 个版本；发布说明改为从 CHANGELOG 生成（`release.yml:136` 目前解析 README，这正是 README 必须背着重复历史的原因）。

其他：`docs/architecture.dot` 和 `.md` 是 07-26 改的，`architecture.svg` / `.png` 仍是 07-15 的，无再生校验；523KB 的 PNG 建议删除只留 SVG。`docs/api-contract.md` 只覆盖 39 条路径（规范有 93 条），55 个端点无记录且无漂移检测——相比之下 `openapi.json` ↔ 路由是双向强制的（`check-openapi.py:243-256`，同时检查缺失和未注册的 operation，另有针对 `openapi-baseline-v1.12.0.json` 的破坏性变更基线）。建议该表改为从 `openapi.json` 生成，或删表改为链接。

## 建议的动手顺序

**先修坏功能**：给 `pwa.js` 补 `MediaSubApi` 依赖头（问题 4.5）。这是唯一有用户可见影响的 bug，一行改动。

**再堵发布门禁**，都是小而机械的改动，性价比最高：

1. 给 `docker-image.yml` 加 `workflow_run` 门禁（问题 1）
2. 修 `smoke-browser.sh` 的 `file://` 问题（问题 2）
3. 加 `build-frontend.mjs --check` 与 `info.version` 比对（问题 4、5）
4. 给 compose 加日志上限（问题 7）
5. 配 ESLint 并开 `no-undef`（问题 4.5 的根因；8k 行无 lint 的原生 JS 里可能还有同类问题）
6. 删掉 18 条死依赖边（问题 4.7，纯删除，零行为变化）

之后再做 `/api/update/*` 集成测试（问题 3）、前端 visibility 暂停（问题 6）、PWA 监听器改走注册表（问题 4.6），工作量稍大但都不涉及架构调整。

## 修复落地记录（2026-07-26，评审当日）

"先修坏功能 + 堵发布门禁"一批已全部落地（工作树中，未提交）：

- **问题 4.5**：pwa.js 补 `MediaSubApi` 依赖头，浏览器 Push 开关恢复可用；新增 2 个 `toggleBrowserPush` 回归测试（做过红-绿验证：还原 bug 版本时恰好这 2 个测试失败）。SW `CACHE_VERSION` 已 bump。
- **问题 1**：docker-image.yml 的 main 分支发布改为 `workflow_run` 触发，要求 CI `conclusion == success` 且事件为 main 的 push；统一用通过了 CI 的 `head_sha` 检出与打 `sha-` 标签，避免"CI 过的是 A、构建的是更新的 B"竞态；tag 与手动触发保留原行为。
- **问题 2**：smoke-browser.sh 改为经 HTTP 渲染真实服务器（Chrome 已完全禁用 URL 内嵌凭据，改用脚本内嵌的 Authorization 注入代理）。新断言只认 JS 才能产生的证据：Alpine 摘除全部 `x-cloak`（源码计数 >0、DOM 计数 =0）、`x-show` 写入的行内样式；chrome console 的 `Uncaught` / `ERR_*` / 资源加载失败均视为失败。注意 `--virtual-time-budget` 会被 SSE 长连接卡死，已改用真实时间 `--timeout=8000`。绿色 + 双红场景验证过：移走 alpine.min.js 被水合断言抓住，移走 core/api.js 被 console 断言抓住。
- **问题 4**：check-openapi.py 的 `check()` 比对 `info.version` 与 Cargo.toml，`--update` 自动同步版本。注入 `0.0.1-DRIFT` 实测被拦截。
- **问题 5**：ci.yml 与 release.yml 的前端检查步骤各加 `node scripts/build-frontend.mjs --check`。styles.css ↔ tailwind/ 的新鲜度检查未做：本地与 CI 均无 Tailwind CLI，git checkout 不保留 mtime，暂无可靠信号，留待后续。
- **问题 7（部分）**：docker-compose.yml 加 json-file 日志轮转（10m × 3）。`read_only`/`cap_drop`/内存上限/`:latest` 钉版涉及部署行为变化，未动。
- **问题 4.5 根因**：新增 `eslint.config.mjs`（`no-undef` + `no-unused-vars`，globals 手工枚举、零 npm 依赖），CI 与 release 各加 `npx --yes eslint@10.8.0 'static/**/*.js'` 步骤。红测试：对未修复的 pwa.js 恰好报出 :115/:123/:126 三处 `apiData is not defined`。全量 lint 干净——本报告猜测的"第二个同类 bug"经全量扫描确认不存在。
- **问题 4.7**：死依赖清理比上文清单更彻底——扫描发现共 **12 个文件**有死头部声明（上文点名的 core 3 个 + stores 3 个之外，calendar-page / dashboard / search-page / settings / updates / subscriptions 也有），全部删除并按实际用量精简 destructure；重扫描零残留，`no-unused-vars` 从此在 CI 拦截回归。85 个前端测试全绿。

本批未动的项：问题 3（`/api/update/*` 集成测试）、问题 6（visibility 暂停轮询）、问题 4.6（PWA 监听器改走注册表）、问题 8/9（工程治理与文档）。

## 未能验证的部分

- `v2.2.14` 的 tag 尚不存在，发布 workflow 对该版本未实际运行过。
- `docs/api-contract.md` 的 39 条路径是脚本统计，未逐条核对语义准确性。
- smoke 脚本卫生已确认（7 个全部 `set -euo pipefail` + `trap cleanup EXIT` + 显式 `exit 1`，无因缺 `-e` 导致的静默通过），但 `smoke-docker-upgrade.sh` 只在 `docker-image.yml:73` 且限 `push` 事件，`smoke-telegram.sh` 无 token 时 `exit 0` 跳过——两者在 PR / fork 上均为空操作。

