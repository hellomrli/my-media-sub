<p align="center">
  <img src="./static/icons/icon-512.png" width="180" alt="my-media-sub 应用图标">
</p>

<h1 align="center">My Media Sub</h1>

<p align="center">面向夸克网盘的自托管追更服务：把资源发现、订阅检查、自动转存、模板重命名、可选下载与通知串成一条可审计的自动化链路。</p>

<p align="center">
  <a href="https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml"><img src="https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg" alt="CI 状态"></a>
  <a href="https://github.com/hellomrli/my-media-sub/releases"><img src="https://img.shields.io/github/v/release/hellomrli/my-media-sub?display_name=tag" alt="最新发布版本"></a>
  <a href="https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub"><img src="https://img.shields.io/badge/GHCR-my--media--sub-blue?logo=docker" alt="GHCR 容器镜像"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2021-orange?logo=rust" alt="Rust 2021"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT 许可证"></a>
</p>

## 它解决什么问题

追更的麻烦不在于找不到资源，而在于每次更新都要把同一套动作重做一遍：点开分享链接、翻有没有新集、挑一个版本、转存、改名，可能还要丢给下载器。订阅一多，这件事就变成负担。

my-media-sub 把这段流程交给一个常驻服务：你给一个分享链接和季集范围，它按设定的间隔去检查，命中新集后按规则转存到自己的网盘、按模板重命名，需要时再交给 Aria2 下到本地，并把结果推到手机上。整个过程有持久化任务队列、稳定幂等键和审计记录，失败可重试、可回溯。

它面向单实例、单管理员的家庭自托管场景，不是多租户平台：没有外部数据库，业务状态写在本地的版本化 JSON 里，一个二进制加一个 WebUI 就能跑起来。

## 核心亮点

| 亮点 | 说明 |
|---|---|
| 定时检查与自动转存 | 调度器按订阅各自的间隔触发检查，命中的新集进入持久化 JobQueue 幂等转存；重试、并发与进程重启都不会重复入账 |
| 分享失效自动换源 | 候选按质量评分排序，换源前校验进度避免倒退，带冷却、完整审计与回滚入口 |
| 九种通知渠道 | 企业微信、WxPusher、Telegram、Bark、Gotify、PushPlus、Server 酱、Browser Push 与签名 Webhook，支持安静时段和摘要聚合 |
| 手机遥控 | Telegram Bot 提供白名单、写操作二次确认、限流与脱敏审计；粘贴豆瓣链接即可搜索、订阅或转存 |
| 不引入数据库也能安全落盘 | `schema_version` JSON 信封、临时文件加 `fsync` 再原子 rename、`0600` 权限、损坏文件自动隔离并在诊断页持续告警 |
| 在线更新 | 校验 SHA256 后把二进制与整个 WebUI 分别以同目录 rename 原子切换（任一失败回滚静态资源），保留多份回滚副本，等后台任务优雅停机后才重启进程 |

## 架构

```text
┌────────────────────────────────────────────────────────────────────────┐
│  入口    浏览器  Basic Auth + 同源 CSRF                                │
│          自动化客户端  scoped Bearer Token                             │
│          Telegram Bot  随机路径 + Header Secret                        │
└────────────────┬───────────────────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────────────┐
│  src/api/       路由 · 认证 · CSRF · 登录限流 · 统一响应信封           │
└────────────────┬───────────────────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────────────┐
│  src/services/  检查 · 转存 · 换源 · 日历 · 通知 · 下载监控 · 备份     │
└────────────────┬──────────────────────────────────────┬────────────────┘
                │                                      │
                ▼                                      ▼
┌──────────────────────────────┐    ┌────────────────────────────────────┐
│  src/jobs/                   │    │  src/providers/ · src/clients/     │
│  持久化队列 · 优先级 · 取消  │    │  Quark · PanSou · Aria2 · TMDB     │
│  重试 · 重启恢复             │    └────────────────────────────────────┘
└────────────────┬─────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────────────────────┐
│  src/store/     schema_version JSON · 原子落盘 · 0600 · 损坏隔离       │
└────────────────────────────────────────────────────────────────────────┘
```

HTTP、调度器、Job Worker 与 Telegram 命令复用同一套 Service 与 Store 合同，不在适配层复制业务规则。云盘能力由 `CloudDriveProvider` 抽象，生产只注册夸克，Mock 实现用于确定性测试。完整分层说明见[架构文档](docs/architecture.md)。

## 使用示例

下面这条链路对应「订阅一部剧，检查后看到转存结果」。所有请求使用 HTTP Basic Auth，响应统一为 `{"ok": true, "data": ...}` 信封。

```bash
# 1. 创建订阅（季号支持 "1"、"1-4"、"1,3" 这类写法）
curl -u admin:"$SERVER_PASSWORD" -X POST http://127.0.0.1:56001/api/subscriptions \
  -H 'Content-Type: application/json' \
  -d '{
        "title": "葬送的芙莉莲",
        "url": "https://pan.quark.cn/s/替换为真实分享",
        "password": "",
        "media_type": "series",
        "season_spec": "1",
        "target_dir": "/追更"
      }'

# 2. 立即检查该订阅，返回新增文件与集数
curl -u admin:"$SERVER_PASSWORD" -X POST \
  http://127.0.0.1:56001/api/subscriptions/替换为订阅ID/check

# 3. 查看任务队列与通知
curl -u admin:"$SERVER_PASSWORD" http://127.0.0.1:56001/api/jobs
curl -u admin:"$SERVER_PASSWORD" http://127.0.0.1:56001/api/notifications
```

检查接口返回 `new_files`、`new_episodes` 与逐文件识别明细；转存作为 Job 异步执行，可在 `GET /api/jobs` 或 `GET /api/jobs/events`（SSE）观察进度。

自动化脚本可以改用最小权限的 Bearer Token，不必携带管理员密码：

```bash
# 轮换出一个只读订阅的 Token，明文只在本次响应中返回一次
curl -u admin:"$SERVER_PASSWORD" -X POST http://127.0.0.1:56001/api/automation-token \
  -H 'Content-Type: application/json' \
  -d '{"scopes": ["subscriptions:read"], "expires_days": 30}'

# 之后用 Token 访问
curl -H "Authorization: Bearer 替换为Token" http://127.0.0.1:56001/api/subscriptions
```

设置、Token 管理、备份恢复、存储清理与在线升级不对任何 Token scope 开放，只能使用管理员凭据。可用 scope 以 `GET /api/automation-token/scopes` 为准。

## 快速安装

推荐 Docker Compose。需要 Docker 与 Compose v2，容器以 uid/gid `1000` 运行。

```bash
mkdir -p my-media-sub/data my-media-sub/runtime && cd my-media-sub

curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml
printf 'SERVER_PASSWORD=替换为至少12位的强密码\nTZ=Asia/Shanghai\n' > .env
docker compose up -d
```

`SERVER_PASSWORD` 是必填项：未在 `.env` 里设置时 compose 会直接拒绝启动。数据写在 `./data`，可在线更新的二进制与 WebUI 写在 `./runtime`，两者都不受容器重建影响。

不使用 Docker 时可以直接跑发布二进制。当前发布产物只提供 **linux-x86_64**：

```bash
VERSION=2.7.1
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/v$VERSION/my-media-sub-v$VERSION-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/v$VERSION/my-media-sub-v$VERSION-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-v$VERSION-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-v$VERSION-linux-x86_64.tar.gz"
cd "my-media-sub-v$VERSION-linux-x86_64"

SERVER_PASSWORD='替换为至少12位的强密码' \
DATA_DIR=./data SERVER_PORT=56001 ./my-media-sub
```

手工升级时必须同时替换二进制与整个 `static/` 目录，只换二进制会继续运行旧的 WebUI。

## 快速开始

打开 `http://服务器地址:56001`，用 `admin`（或你设置的 `SERVER_USERNAME`）与 `SERVER_PASSWORD` 登录。默认密码 `change-me` 会被服务端拒绝，必须配置真实密码。

1. 进入系统设置，填入**夸克 Cookie**——没有它无法探测分享或执行转存，检查接口会直接返回「未配置夸克 Cookie」。
2. 按需填入 PanSou 地址、TMDB API Key、Aria2 RPC 与至少一个通知渠道；每个集成在设置页都有对应的测试按钮。
3. 到资源搜索或订阅管理新增订阅：粘贴夸克分享链接后，编辑器会探测该分享包含哪些季度，勾选要追的季即可。
4. 保存后点「立即检查」验证链路：命中新集会出现 Job 与通知，转存结果出现在你的夸克网盘目标目录。

想先确认服务活着，可以不登录直接探测健康检查，它返回状态与当前版本：

```bash
curl http://127.0.0.1:56001/health
# {"status":"ok","version":"2.7.1"}
```

## 配置与集成

配置有两个来源：进程环境变量，以及 WebUI 里写入 `settings.json` 的设置。环境变量在启动时覆盖设置文件，适合容器化部署。完整清单见 [`.env.example`](.env.example)。

| 变量 | 作用 |
|---|---|
| `SERVER_HOST` / `SERVER_PORT` | 监听地址与端口，默认 `0.0.0.0:56001` |
| `SERVER_USERNAME` / `SERVER_PASSWORD` | 管理凭据；密码为空的部署会拒绝所有请求。旧别名 `APP_USERNAME` / `APP_PASSWORD` 仍被接受，但优先级低于 `SERVER_*` |
| `STATIC_DIR` | WebUI 静态资源目录，默认 `./static`；容器内指向运行时目录（`/app/runtime/static`）。**它与 `APP_RUNTIME_DIR` 共同决定在线更新是否可用** |
| `APP_RUNTIME_DIR` | 在线更新写入的持久化运行时目录（容器内为 `/app/runtime`），需可写。改动前请先读 [Docker 在线更新](docs/docker-online-update.md) |
| `DATA_DIR` | 业务数据目录，默认 `./data` |
| `TZ` | 进程时区，影响推送免打扰时段等基于本地时间的功能 |
| `QUARK_COOKIE` / `QUARK_SIGNIN_COOKIE` | 夸克凭据与签到专用 Cookie |
| `PANSOU_API_URL` | PanSou 聚合搜索地址，留空则不启用聚合搜索 |
| `ARIA2_RPC_URL` / `ARIA2_SECRET` / `ARIA2_*_DIR` | Aria2 提交地址、密钥与电影/剧集/动画分类下载目录 |
| `TMDB_API_KEY` / `TMDB_LANGUAGE` | 元数据、海报与总集数来源 |
| `TELEGRAM_BOT_*` | 推送与 Bot 接入；`TELEGRAM_BOT_MODE` 取 `disabled`、`long_polling` 或 `webhook` |
| `BACKUP_INTERVAL_HOURS` / `BACKUP_RETENTION` | 自动备份间隔与保留份数，间隔设为 0 可关闭定时备份 |
| `SELF_UPDATE_ENABLED` / `SELF_UPDATE_BACKUP_RETENTION` | 在线更新开关与回滚副本数量 |
| `SELF_UPDATE_PUBLIC_KEY` | minisign 公钥。设置后**强制**校验 Release 的 `.minisig` 分离签名（推荐在编译期固化，见 [Docker 在线更新](docs/docker-online-update.md)） |
| `allowed_hosts`（设置页） | 可选的 Host 白名单。配置后拒绝 Host 不在列表内的请求，用于防御 DNS rebinding 绕过 CSRF 的 Origin/Host 比较；保存时必须包含当前访问的 Host |
| `hsts_enabled`（设置页） | 是否发送 HSTS 头，默认关闭。HSTS 按主机名生效、不分端口，只在该主机名所有端口都走 HTTPS 时开启 |
| `ALLOW_QUARANTINE_STARTUP` | 设为 `true` 时允许在 Store 文件损坏被隔离后以空数据启动；默认为 false（中止启动，避免静默丢数据） |
| `RUST_LOG` / `LOG_FORMAT` | 日志级别与输出格式，`LOG_FORMAT=json` 输出结构化日志 |

## 数据、备份与升级

业务数据位于 `DATA_DIR`，每个 Store 都是带 `schema_version` 的 JSON 信封：

```text
DATA_DIR/
  settings.json          subscriptions.json     notifications.json
  jobs.json              jobs.archive.json      automation_events.json
  automation-token.json  telegram_bot.json      backups/
```

写入采用临时文件加 `fsync` 再原子 rename，落盘成功后才替换内存状态，Unix 上权限为 `0600`。损坏文件会被隔离为 `*.json.corrupt-*` 并在诊断页持续报告，直到你核对备份后清理。

备份恢复需要精确确认文本 `RESTORE DATA`。归档先校验并暂存，在**下一次重启**时、加载任何 Store 与后台任务之前应用；应用失败会回滚并停止启动，保留暂存文件供排查后重试。提交恢复后请尽快重启，重启前的后续修改会被备份覆盖。

Docker 部署可以在系统设置的维护页直接切换 Release。更新器会校验 SHA256，把二进制与完整 WebUI **分别**以同目录 rename 原子切换（静态资源先切、二进制后切，二进制失败时回滚静态资源），并等待优雅停机后重启。

> 注意：两次 rename 之间存在一个短暂窗口，此时**旧二进制会配着新 WebUI 运行**，直到进程重启。因此新前端必须保持与上一个版本的 API 兼容；细节与回滚方式见 [Docker 在线更新](docs/docker-online-update.md)。

## 安全与部署

服务使用 HTTP Basic Auth，**生产环境必须由可信反向代理终止 HTTPS，不要把 56001 直接暴露到公网**。Nginx 示例与完整要求见 [HTTPS 反向代理](docs/https-reverse-proxy.md)。

- 密码需至少 12 位且非默认值；登录失败按来源 IP 限流，默认不信任 `X-Forwarded-For`，只有显式开启 `trust_proxy_headers` 后才按真实客户端计数。
- 自动化使用最小 scope 的 Bearer Token，明文只在创建时返回一次，服务端只保存 SHA-256 与前缀。
- 跨站状态修改按 `Origin` 与 `Sec-Fetch-Site` 校验并失败关闭；CSP、`nosniff`、拒绝 iframe 与 Referrer Policy 由应用统一返回。
- 备份包含 Cookie 与 Token，等同完整凭据集合，应加密保存并限制访问。
- 依赖由 CI 的 RustSec 审计把关，已知例外记录在[依赖审计策略](docs/security-audit.md)。

## API 与可观测性

| 用途 | 入口 |
|---|---|
| 存活与版本 | `GET /health`（免认证） |
| Prometheus 指标 | `GET /metrics` |
| 诊断快照与导出 | `GET /api/diagnostics`、`GET /api/diagnostics/export` |
| 在线 API 文档 | `GET /api-docs.html` |
| 任务实时事件 | `GET /api/jobs/events`（SSE） |
| 备份与恢复 | `GET /api/backups/export`、`POST /api/backups/restore` |
| 存储清理与门槛 | `GET`/`POST /api/storage/cleanup`、`GET /api/storage/decision` |

`static/openapi.json` 与路由表由 `scripts/check-openapi.py` 双向强制同步：缺失、未注册或破坏性变更都会让 CI 失败，规范版本号必须与 `Cargo.toml` 一致。当前契约覆盖 94 个路径、106 个操作，响应信封与错误码约定见 [API 契约](docs/api-contract.md)。

## 从源码构建

需要 Rust stable（edition 2021）与 Node.js。前端不使用打包器，`static/index.html` 由模板与 partials 拼装生成，**不要手改**。

```bash
cp .env.example .env
cargo run --release
```

与 CI 一致的完整检查：

```bash
node scripts/build-frontend.mjs --check          # 前端产物是否过期
find static -type f -name '*.js' -print0 | sort -z | xargs -0 -n1 node --check
node --test tests/frontend_*.test.js             # 144 项前端测试
npx --yes eslint@10.8.0 'static/**/*.js'         # no-undef 是原生 JS 的静态安全网

python3 scripts/check-openapi.py                 # 路由与规范双向契约

cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked                        # 718 项 Rust 测试
cargo build --release --locked
```

改动任何静态资源后要同步提升 `static/service-worker.js` 的 `CACHE_VERSION`，否则 PWA 客户端可能继续命中旧缓存。

## 文档

| 主题 | 内容 | 链接 |
|---|---|---|
| 开始使用 | 分层结构、请求与认证边界、扩展接入点 | [架构](docs/architecture.md) |
| 接口对接 | 响应信封、错误码、SSE 与例外登记 | [API 契约](docs/api-contract.md) |
| 自动化集成 | scoped Token、幂等合同与调用示例 | [自动化 API](docs/automation-api.md) |
| 事件流水线 | 结构化自动化事件的字段与投影规则 | [自动化事件](docs/automation-events.md) |
| 手机遥控 | Bot 命令、白名单、二次确认与审计 | [Telegram Bot](docs/telegram-bot.md) |
| 排期视图 | 日历的时区口径、推断规则与状态 | [媒体日历](docs/media-calendar.md) |
| 换源策略 | 候选评分、进度校验与自动切换 | [资源质量与换源](docs/source-quality.md) |
| 部署加固 | HTTPS 反代、安全要求与凭据保管 | [HTTPS 反向代理](docs/https-reverse-proxy.md) |
| 容器运维 | 运行载荷卷、镜像与在线更新的优先级 | [Docker 在线更新](docs/docker-online-update.md) |
| 容量规划 | JSON 性能基线与 SQLite 决策门槛 | [存储扩展](docs/storage-scaling.md) |
| 移动端 | PWA 壳层、缓存策略与安装 | [PWA](docs/pwa.md) |
| 发布流程 | 版本面门禁与发布检查清单 | [发布流程](docs/release-workflow.md) |
| 依赖安全 | RustSec 例外、已修 CVE 与锁文件策略 | [依赖审计](docs/security-audit.md) |
| 后续计划 | 唯一持久化计划入口与历史交接台账 | [路线图](docs/roadmap.md) |
| 代码评审 | 最近一次全量评审的结论与修复状态 | [评审 2026-09-21](docs/code-review-2026-09-21.md) |
| 历史评审 | 2026-07-26 的前端与代码评审（部分结论已修复，留档） | [评审 2026-07-26](docs/code-review-2026-07-26.md) · [前端评审 2026-07-26](docs/frontend-design-review-2026-07-26.md) |

## 版本说明

### 2.7.2

- **修复设置保存必然报错**：调度器 `reload()` 在首次启动后总是返回错误，导致修改检查间隔、夸克 Cookie 或签到设置时界面报错（配置其实已生效）。根因是 `tokio-cron-scheduler` 的 `JobScheduler::start()` 不幂等，而 `reload()` 每次无条件调用它；现在用一次性标志把 ticker 启动与任务增删解耦，并补了回归测试。
- **修复一处未认证即可触发的拒绝服务**：认证限流原先在凭据校验之前判定，失败计数饱和后连**正确密码**也会被 429。攻击者维持约 5 次/分钟的错误请求即可锁死整个界面（仅 `/health` 幸存）；反向代理后默认配置下所有请求共用同一限流键，一人打满即全员受影响。现在限流只惩罚凭据错误的请求。
- **修复 Token 越权**：scope `read` 此前被当作通配符，可满足任意以 `:read` 结尾的 scope，于是只授 `read` 的 Token 能读取 `/api/telegram/audits`——其中的 `target` 字段记录着用户粘贴的分享链接与提取码。scope 现在要求精确匹配。
- **修复已发布的样式缺陷**：编译产物 `static/styles.css` 停留在 v2.6.0，v2.7.0 新增的 `py-1.5` / `opacity-75` 从未生效；另有 9 个被引用但从未定义的类（含完全不可见的加载动画 `loading-spinner`、无效 token `bg-panel` 与 `text-text-muted`）。已补齐定义并重新编译，同时新增 CI 门禁防止再次漂移。
- **修复维护模式下队列无界增长**：`job_maintenance_mode` 原先只拦 worker 执行、不拦入队，而 `truncate_jobs` 又不淘汰排队任务，开启维护后 `jobs.json` 会无限增长且每次入队全量重写。
- **修复运行时阻塞**：备份创建/校验（整份归档读写 + 隔离恢复 + 全量 SHA-256）与 `GET /api/drive/aria2/browse` 的目录枚举原先都跑在 tokio worker 上，现在移入阻塞线程池。
- **新增单实例保护**：启动时对 `DATA_DIR` 取排他文件锁，避免两个进程共用同一数据目录时整份 JSON 互相覆盖、同一作业被重复执行。
- **转存与下载补齐幂等与对账**：转存前落盘「意图」并在重试时对账，避免云端已成功却重复转存；新增「已转存但未提交下载」记录与周期重试，避免该集永久不下载。
- **长驻后台任务不再静默死亡**：6 个关键循环（下载监控、Telegram 长轮询、自动化事件投影、定时备份与校验等）改用受监督的 spawn，panic 后记录并重启；同时安装全局 panic hook，把 panic 位置提升为 ERROR 日志。
- **修复前端状态失同步**：从 bfcache 返回后轮询、SSE、快捷键与路由全部失效；自动转存完成后订阅列表不刷新；下载轮询一旦空闲就彻底停止，导致别处新建的下载不再出现。
- **剧名匹配大幅增强**：日文标题内部的假名不再被误当作分隔符（`鬼滅の刃` 曾被截断成 `鬼滅`）；中文逗号之后的演员/描述整段丢弃（`交锋 4K … 完结，王凯` → `交锋`）；`DDP2 0`、`H.264`、`AAC2.0` 这类被拆开的音视频标记先粘合再剥离；书名号内视为标题本身；噪声词表补齐平台名、发布形容词、集数区间等；清洗结果附带年份与季号提示——新建订阅自动回填季号，元数据搜索按年份区分同名翻拍。
- **安全加固**：浏览器推送的 SSRF 过滤补上 IPv4-mapped IPv6、CGNAT 与保留段；`/api/*` 响应统一 `Cache-Control: no-store`；aria2 的 `out` 文件名强制清洗；Telegram webhook 加 64 KiB 体积上限；新增可选的 HSTS 开关与 Host 白名单（均默认关闭，白名单保存时防自锁）；compose 去掉多余 capability、禁止提权、根文件系统只读。
- **移除 `web-push`**：Web Push 改为自实现（`ece` 分组框架 + `p256`/`ring` 密码学后端），依赖树从 370 个 crate 精简到 **227** 个，并消除 CI 中长期保留的唯一 RustSec 例外（`RUSTSEC-2023-0071`，来自它引入的 `rsa`）；同时去掉第二套 C 实现的 TLS/HTTP 栈与 BoringSSL，全项目回到单一 rustls。
- **工程化**：固定 Rust 工具链并声明 `rust-version = "1.87"`；`Cargo.lock` 收敛 132 个 crate；接入 Dependabot；release 档开启 `overflow-checks` 并保留行号表；新增 CSS 类名覆盖、文档漂移与无障碍回归门禁；标识符派生从 MD5 换成截断 SHA-256，不再依赖 `md5`。

### 2.7.1

- 备份恢复改为「校验并暂存，重启时在加载数据前应用」，修掉旧进程内存覆盖恢复结果的问题；失败会回滚、停止启动并保留暂存归档。
- 检查、预览、任务载荷与持久化记录统一按「季 + 集」识别，不同季度目录下的同名文件不再互相顶掉或漏转存。
- 切换季度后按新季重算进度，不再沿用旧季完结记录；详情与日历按选中季度分别展示。
- 季度探测按链接与密码标识在途请求，快速切换时旧结果不会覆盖当前编辑器。

- 当前版本：[v2.7.2 升级指南](docs/upgrade-v2.7.2.md) · 完整变更见 [CHANGELOG.md](CHANGELOG.md)

各版本升级步骤在 `docs/upgrade-v*.md`。

## 许可证

本项目基于 [MIT 许可证](./LICENSE) 发布。
