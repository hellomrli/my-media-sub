# 部署与使用参考

本页保留原 README 的详细安装、配置、API 示例与运维说明。首次安装可先读 [README](../README.md)。命令未特别说明时在项目根目录执行。

## 快速安装

推荐 Docker Compose。需要 Docker、Compose v2、`curl` 与 `openssl`，容器以 uid/gid `1000` 运行。以下命令使用新的安装目录，不要在已有部署目录覆盖 `.env`。

```bash
set -eu
mkdir my-media-sub
cd my-media-sub
curl -fsSLo compose.yaml https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml
(umask 077; printf 'SERVER_PASSWORD=%s\n' "$(openssl rand -hex 24)" > .env)
docker compose up -d
```

`SERVER_PASSWORD` 是必填项：未在 `.env` 里设置时 compose 会直接拒绝启动。随机密码保存在 `.env` 中，可在本机用 `cat .env` 查看，不要上传或分享。数据写在 `./data`，可在线更新的二进制与 WebUI 写在 `./runtime`，两者都不受容器重建影响。

不使用 Docker 时可以直接跑发布二进制。当前发布产物只提供 **linux-x86_64**：

```bash
VERSION=2.8.0
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/v$VERSION/my-media-sub-v$VERSION-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/v$VERSION/my-media-sub-v$VERSION-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-v$VERSION-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-v$VERSION-linux-x86_64.tar.gz"
cd "my-media-sub-v$VERSION-linux-x86_64"

export SERVER_PASSWORD="$(openssl rand -hex 24)"
printf '管理员密码：%s\n' "$SERVER_PASSWORD"
SERVER_HOST=127.0.0.1 DATA_DIR=./data SERVER_PORT=56001 ./my-media-sub
```

手工升级时必须同时替换二进制与整个 `static/` 目录，只换二进制会继续运行旧的 WebUI。


## 快速开始

默认 Compose 只绑定本机，先在部署机器打开 `http://127.0.0.1:56001`；远程访问使用 HTTPS 反向代理或 SSH 端口转发，用 `admin`（或你设置的 `SERVER_USERNAME`）与 `SERVER_PASSWORD` 登录。默认密码 `change-me` 会被服务端拒绝，必须配置真实密码。

1. 进入系统设置，填入**夸克 Cookie**——没有它无法探测分享或执行转存，检查接口会直接返回「未配置夸克 Cookie」。
2. 按需填入 PanSou 地址、TMDB API Key、Aria2 RPC 与至少一个通知渠道；每个集成在设置页都有对应的测试按钮。
3. 到资源搜索或订阅管理新增订阅：粘贴夸克分享链接后，编辑器会探测该分享包含哪些季度，勾选要追的季即可。
4. 保存后点「立即检查」验证链路：命中新集会出现 Job 与通知，转存结果出现在你的夸克网盘目标目录。

想先确认服务活着，可以不登录直接探测健康检查，它返回状态与当前版本：

```bash
curl http://127.0.0.1:56001/health
# {"status":"ok","version":"2.8.0"}
```


## 使用示例

下面这条链路对应「订阅一部剧，检查后看到转存结果」。先将真实管理员密码导出为 `SERVER_PASSWORD`，并替换示例中的分享链接与订阅 ID；不要原样执行占位值。所有请求使用 HTTP Basic Auth，响应统一为 `{"ok": true, "data": ...}` 信封。

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


## 配置与集成

电影使用一次性保存流程，剧集与动画可以固定 1080P / 4K 后持续追更。保存步骤、命名识别边界及 API 字段见 [电影保存与画质选择](media-save-quality.md)。

配置有两个来源：进程环境变量，以及 WebUI 里写入 `settings.json` 的设置。环境变量在启动时覆盖设置文件，适合容器化部署。Compose 会读取 `.env`；独立二进制和 `cargo run` 不会自动加载 `.env`，需通过 shell 导出变量。完整清单见 [`.env.example`](../.env.example)。

通知支持企业微信、WxPusher、Telegram、Bark、Gotify、PushPlus、Server 酱、Browser Push 与签名 Webhook，可配置安静时段和摘要聚合。Telegram Bot 另提供白名单、写操作二次确认与脱敏审计；粘贴豆瓣链接可进入搜索、订阅或转存流程，见 [Telegram Bot](telegram-bot.md)。

| 变量 | 作用 |
|---|---|
| `SERVER_HOST` / `SERVER_PORT` | 监听地址与端口，默认 `0.0.0.0:56001` |
| `SERVER_USERNAME` / `SERVER_PASSWORD` | 管理凭据；密码为空的部署会拒绝所有请求。旧别名 `APP_USERNAME` / `APP_PASSWORD` 仍被接受，但优先级低于 `SERVER_*` |
| `STATIC_DIR` | WebUI 静态资源目录，默认 `./static`；容器内指向运行时目录（`/app/runtime/static`）。**它与 `APP_RUNTIME_DIR` 共同决定在线更新是否可用** |
| `APP_RUNTIME_DIR` | 在线更新写入的持久化运行时目录（容器内为 `/app/runtime`），需可写。改动前请先读 [Docker 在线更新](docker-online-update.md) |
| `DATA_DIR` | 业务数据目录，默认 `./data` |
| `TZ` | 进程时区，影响推送免打扰时段等基于本地时间的功能 |
| `QUARK_COOKIE` / `QUARK_SIGNIN_COOKIE` | 夸克凭据与签到专用 Cookie |
| `PANSOU_API_URL` | PanSou 聚合搜索地址，留空则不启用聚合搜索 |
| `ARIA2_RPC_URL` / `ARIA2_SECRET` / `ARIA2_*_DIR` | Aria2 提交地址、密钥与电影/剧集/动画分类下载目录 |
| `TMDB_API_KEY` / `TMDB_LANGUAGE` | 元数据、海报与总集数来源 |
| `TELEGRAM_BOT_*` | 推送与 Bot 接入；`TELEGRAM_BOT_MODE` 取 `disabled`、`long_polling` 或 `webhook` |
| `BACKUP_INTERVAL_HOURS` / `BACKUP_RETENTION` | 自动备份间隔与保留份数，间隔设为 0 可关闭定时备份 |
| `SELF_UPDATE_ENABLED` / `SELF_UPDATE_BACKUP_RETENTION` | 在线更新开关与回滚副本数量 |
| `SELF_UPDATE_PUBLIC_KEY` | minisign 公钥。设置后**强制**校验 Release 的 `.minisig` 分离签名（推荐在编译期固化，见 [Docker 在线更新](docker-online-update.md)） |
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

提交开始后由独立任务持有 Store 保存锁直至写盘与内存更新结束；取消请求或元数据任务不会让旧快照覆盖后续写入。文件被隔离后，即使容器自动重启，也会持续拒绝以空数据启动，直到恢复原路径的有效文件。业务 Store 可用 `ALLOW_QUARANTINE_STARTUP=true` 显式允许空数据启动；设置存储不接受此放行，避免重新生成凭据和密钥。已恢复数据可保留隔离副本，不会阻止启动。

备份恢复需要精确确认文本 `RESTORE DATA`。归档先校验并暂存，在**下一次重启**时、加载任何 Store 与后台任务之前应用；应用失败会回滚并停止启动，保留暂存文件供排查后重试。提交恢复后请尽快重启，重启前的后续修改会被备份覆盖。

Docker 部署可以在系统设置的维护页直接切换 Release。更新器会校验 SHA256，把二进制与完整 WebUI **分别**以同目录 rename 原子切换（静态资源先切、二进制后切，二进制失败时回滚静态资源），并等待优雅停机后重启。

> 注意：两次 rename 之间存在一个短暂窗口，此时**旧二进制会配着新 WebUI 运行**，直到进程重启。因此新前端必须保持与上一个版本的 API 兼容；细节与回滚方式见 [Docker 在线更新](docker-online-update.md)。


## 安全与部署

服务使用 HTTP Basic Auth，**生产环境必须由可信反向代理终止 HTTPS，不要把 56001 直接暴露到公网**。Nginx 示例与完整要求见 [HTTPS 反向代理](https-reverse-proxy.md)。

- 密码需至少 12 位且非默认值；登录失败按来源 IP 限流，默认不信任 `X-Forwarded-For`，只有显式开启 `trust_proxy_headers` 后才按真实客户端计数。
- 自动化使用最小 scope 的 Bearer Token，明文只在创建时返回一次，服务端只保存 SHA-256 与前缀。
- 带跨站 `Origin` 或 `Sec-Fetch-Site` 的状态修改请求会被拒绝；无这些头的命令行客户端仍需通过鉴权。CSP、`nosniff`、拒绝 iframe 与 Referrer Policy 由应用统一返回。
- 备份包含 Cookie 与 Token，等同完整凭据集合，应加密保存并限制访问。
- 依赖由 CI 的 RustSec 审计把关，已知例外记录在[依赖审计策略](security-audit.md)。


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

`static/openapi.json` 与路由表由 `scripts/check-openapi.py` 双向强制同步：缺失、未注册或破坏性变更都会让 CI 失败，规范版本号必须与 `Cargo.toml` 一致。当前契约覆盖 94 个路径、106 个操作，响应信封与错误码约定见 [API 契约](api-contract.md)。


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

HTTP、调度器、Job Worker 与 Telegram 命令复用同一套 Service 与 Store 合同，不在适配层复制业务规则。云盘能力由 `CloudDriveProvider` 抽象，生产只注册夸克，Mock 实现用于确定性测试。完整分层说明见[架构文档](architecture.md)。


## 文档

| 主题 | 内容 | 链接 |
|---|---|---|
| 开始使用 | 分层结构、请求与认证边界、扩展接入点 | [架构](architecture.md) |
| 接口对接 | 响应信封、错误码、SSE 与例外登记 | [API 契约](api-contract.md) |
| 自动化集成 | scoped Token、幂等合同与调用示例 | [自动化 API](automation-api.md) |
| 事件流水线 | 结构化自动化事件的字段与投影规则 | [自动化事件](automation-events.md) |
| 手机遥控 | Bot 命令、白名单、二次确认与审计 | [Telegram Bot](telegram-bot.md) |
| 排期视图 | 日历的时区口径、推断规则与状态 | [媒体日历](media-calendar.md) |
| 换源策略 | 候选评分、进度校验与自动切换 | [资源质量与换源](source-quality.md) |
| 部署加固 | HTTPS 反代、安全要求与凭据保管 | [HTTPS 反向代理](https-reverse-proxy.md) |
| 容器运维 | 运行载荷卷、镜像与在线更新的优先级 | [Docker 在线更新](docker-online-update.md) |
| 容量规划 | JSON 性能基线与 SQLite 决策门槛 | [存储扩展](storage-scaling.md) |
| 移动端 | PWA 壳层、缓存策略与安装 | [PWA](pwa.md) |
| 发布流程 | 版本面门禁与发布检查清单 | [发布流程](release-workflow.md) |
| 依赖安全 | RustSec 例外、已修 CVE 与锁文件策略 | [依赖审计](security-audit.md) |
| 后续计划 | 唯一持久化计划入口与历史交接台账 | [路线图](roadmap.md) |
| 代码评审 | 最近一次全量评审的结论与修复状态 | [评审 2026-09-21](code-review-2026-09-21.md) |
| 历史评审 | 2026-07-26 的前端与代码评审（部分结论已修复，留档） | [评审 2026-07-26](code-review-2026-07-26.md) · [前端评审 2026-07-26](frontend-design-review-2026-07-26.md) |
