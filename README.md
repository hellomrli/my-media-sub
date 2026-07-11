# My Media Sub

> 面向夸克网盘的媒体订阅、追更、转存与下载自动化服务。后端使用 Rust + Axum，前端为无需打包器的 Media Deck WebUI。

[![CI](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg)](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hellomrli/my-media-sub?display_name=tag)](https://github.com/hellomrli/my-media-sub/releases)
[![Container](https://img.shields.io/badge/GHCR-my--media--sub-blue?logo=docker)](https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

My Media Sub 将资源搜索、订阅检查、更新日历、自动转存、重命名、STRM、Aria2 下载、失效换源和通知推送组合为一个可自托管的媒体自动化工作台。

![架构图](docs/architecture.png)

## 核心能力

| 模块 | 能力 |
|---|---|
| Media Deck | 深色/浅色主题、响应式布局、工作台、订阅、日历、搜索、网盘、下载、任务和通知页面 |
| 资源搜索 | PanSou 搜索、云盘类型过滤、夸克分享探测、资源质量评分和风险提示 |
| 订阅追更 | 手动或定时检查、季与起始集过滤、同集多版本去重、缺集识别、完结与自动恢复 |
| 更新日历 | 上海时区周/月/列表视图、手动排期、元数据推断、逐集处理状态和快捷操作 |
| Browser Push / Webhook | 标准 VAPID 后台通知、签名 Webhook 和统一推送报告 |
| PWA | 可安装应用、离线壳层、安全缓存更新与今日/缺集/任务/下载/签到快捷入口 |
| 自动转存 | 保存到电影/连续剧/动画或自定义目录，支持递归文件定位、规则过滤和批量处理 |
| 安全换源 | 失效计数、候选评分、季度/进度校验、冷却时间、历史审计、自动应用和回滚 |
| 重命名 | 模板变量、正则替换、规则预设、转存前预览、已有文件批量修复 |
| 元数据 | TMDB 搜索与刮削、海报/年份/评分/总集数同步、单订阅和批量任务 |
| 夸克网盘 | Cookie 检测、容量、目录浏览、创建、重命名、删除和每日签到 |
| Aria2 | RPC 检测、幂等查重与退避重试、直链/Header、批量上限、任务控制和完成通知 |
| STRM | 自动生成 `.strm`、HTTPStrm 代理、Range 请求、独立 Token 与缺失/孤立/重复审计 |
| 自动化流水线 | source_check → file_filter → version_select → cloud_transfer → rename → strm → aria2 → notification |
| 后台任务 | 持久化队列、独立 Job Handler、幂等提交、取消、重试、重启恢复和 SSE 实时状态 |
| 通知推送 | 企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱 |
| Telegram 控制 Bot | Long polling/Webhook、数字 ID 白名单、只读/受控写命令、一次性确认、主动通知按钮、审计与限流 |
| 媒体库刷新 | 转存后自动刷新 Jellyfin、Emby、Plex 或通用 Webhook |
| 可观测性 | request/correlation/subscription/job 关联日志、JSON 日志、Prometheus、慢操作和外部依赖延迟 |
| 数据可靠性 | 备份逐项清单、隔离恢复验证、外部副本、Store 增长预警、独立保留策略和安全清理预览 |

## 快速开始

### Docker Compose（推荐）

```bash
mkdir -p my-media-sub/data
cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml

# 请先修改 docker-compose.yml 中的 SERVER_PASSWORD
docker compose up -d
```

访问：`http://服务器地址:56001`

默认账号为 `admin`，默认密码为 `change-me`。如果没有通过环境变量覆盖，**首次登录后必须立即修改密码**。

常用维护命令：

```bash
docker compose ps
docker compose logs -f
docker compose pull
docker compose up -d
docker compose down
```

### Docker Run

```bash
docker run -d \
  --name my-media-sub \
  --restart unless-stopped \
  -p 56001:56001 \
  -v "$(pwd)/data:/app/data" \
  -e SERVER_USERNAME=admin \
  -e SERVER_PASSWORD='replace-with-a-strong-password' \
  -e TZ=Asia/Shanghai \
  ghcr.io/hellomrli/my-media-sub:latest
```

生产环境建议固定版本标签；v1.13.1 同时发布 `1.13.1` 和 `1.13`：

```bash
docker pull ghcr.io/hellomrli/my-media-sub:1.13.1
docker image inspect ghcr.io/hellomrli/my-media-sub:1.13.1 --format '{{.RepoDigests}}'
```

### Linux 二进制

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 下载 Linux x86_64 压缩包并校验 SHA256：

```bash
VERSION=v1.13.1
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-${VERSION}-linux-x86_64.tar.gz"
cd "my-media-sub-${VERSION}-linux-x86_64"

SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

运行目录需要保留 `static/`。业务数据默认写入 `./data`，可通过 `DATA_DIR` 修改。

## 首次配置

1. 打开“系统设置”，先修改管理员密码。
2. 配置夸克 Cookie，并使用连接测试确认有效。
3. 配置电影、连续剧和动画的夸克目标目录。
4. 按需配置 PanSou、TMDB、Aria2、STRM 和推送渠道。
5. 在资源搜索中创建订阅，或直接手动添加分享链接。
6. 设置检查周期、星期、最大并发、自动转存和安全换源策略。
7. 在更新日历和订阅详情中确认已知、缺失、已转存、STRM 与下载状态。

## 自动化流程

```text
定时器 / 手动检查
  → 同订阅互斥与批量并发限制
  → 分享链接探测与同批结果去重
  → 文件过滤、季度匹配和同集版本选择
  → 更新订阅快照（批量检查只提交一次真实存储）
  → 创建幂等 SubscriptionTransfer Job
  → 按 high/normal/low 加权公平调度
  → 全局、任务类别和同订阅三层并发限制
  → 夸克转存
  → 重命名
  → STRM 生成
  → Aria2 提交
  → 通知与推送
  → AutomationEvent 流水线审计
```

后台任务页可查看并调整排队任务优先级。系统设置可分别配置 Job 全局、转存、元数据和推送并发；类别上限同时受全局上限约束，同一订阅始终串行。

可重试故障会按错误类别执行最多 3 次带确定性抖动的指数退避；同类别连续临时故障会打开熔断器，冷却后仅放行一个恢复探测。任务超过 30 分钟会被卡死检测终止；维护模式暂停新任务，队列达到 100 条时生成限频告警，旧终态任务自动归档到 `jobs.archive.json`。

通知中心支持事件到渠道路由、最低级别、上海时区安静时段、错误绕过、重复限频、延迟摘要和模板预览。Webhook 按目标独立重试并支持双签名重叠轮换；所有推送工作均与订阅检查、转存、下载监控和签到调用栈隔离。

分享失效时，可按配置进入仅搜索或自动应用模式。自动换源会检查候选质量、当前进度覆盖、季别、历史链接、近期失败和冷却时间，并保留可回滚审计记录。

## 配置方式

推荐在 WebUI 中管理业务配置。环境变量适合容器启动参数、初始账号或自动化部署。

### 基础环境变量

| 变量 | 默认值 | 说明 |
|---|---:|---|
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `56001` | HTTP 端口 |
| `SERVER_USERNAME` | `admin` | 初始管理员账号 |
| `SERVER_PASSWORD` | `change-me` | 初始管理员密码，生产环境必须覆盖 |
| `DATA_DIR` | `./data` | JSON 数据、备份与运行状态目录 |
| `BACKUP_INTERVAL_HOURS` | `24` | 自动备份间隔，设为 `0` 关闭 |
| `BACKUP_VERIFY_INTERVAL_HOURS` | `24` | 对最近备份执行隔离恢复验证的间隔，设为 `0` 关闭 |
| `BACKUP_EXTERNAL_DIR` | 空 | 经校验后原子复制备份的 DATA_DIR 外部目录 |
| `BACKUP_RETENTION` | `7` | 保留最近的服务器备份数 |
| `BACKUP_MAX_ARCHIVE_MB` | `256` | 单个备份解码内容上限 |
| `BACKUP_MAX_STORAGE_MB` | `512` | 服务器备份目录总预算 |
| `RUST_LOG` | `info` | Rust 日志过滤规则 |
| `LOG_FORMAT` | `text` | 日志输出格式；设为 `json` 输出含关联上下文的 JSON 日志 |
| `SLOW_OPERATION_MS` | `1000` | 慢操作告警与指标阈值，范围 100–300000 毫秒 |
| `RETENTION_SUBSCRIPTION_CHECKS` | `30` | 每个订阅保留的检查历史 |
| `RETENTION_SOURCE_SWITCHES` | `50` | 每个订阅保留的换源历史 |
| `RETENTION_PREVIOUS_LINKS` | `50` | 每个订阅保留的历史分享链接 |
| `RETENTION_NOTIFICATIONS` | `300` | 通知 Store 独立保留条数 |
| `RETENTION_ACTIVE_JOBS` | `300` | 活跃 Job Store 保留的终态任务数 |
| `RETENTION_ARCHIVED_JOBS` | `5000` | Job 归档独立保留条数 |
| `RETENTION_AUTOMATION_EVENTS` | `5000` | 自动化事件数量上限 |
| `RETENTION_AUTOMATION_DAYS` | `30` | 普通自动化事件保留天数 |
| `RETENTION_FAILED_AUTOMATION_DAYS` | `90` | 失败自动化事件保留天数 |
| `STORE_GROWTH_WARNING_MB` | `24` | 单个 Store 大小增长预警线 |
| `TZ` | 系统时区 | 容器建议设置为 `Asia/Shanghai` |

### 集成环境变量

| 类型 | 变量 |
|---|---|
| 夸克 | `QUARK_COOKIE`、`QUARK_SIGNIN_COOKIE`、`QUARK_SIGNIN_ENABLED`、`QUARK_SIGNIN_HOUR` |
| 搜索 | `PANSOU_API_URL` |
| Aria2 | `ARIA2_RPC_URL`、`ARIA2_SECRET`、`ARIA2_MOVIE_DIR`、`ARIA2_SERIES_DIR`、`ARIA2_ANIME_DIR` |
| STRM | `STRM_ENABLED`、`STRM_OUTPUT_DIR`、`STRM_PUBLIC_BASE_URL`、`STRM_ACCESS_TOKEN`、`STRM_TOKEN_IN_URL` |
| TMDB | `TMDB_API_KEY`、`TMDB_LANGUAGE` |
| 推送 | `WECOM_BOT_URL`、`TELEGRAM_BOT_TOKEN`、`TELEGRAM_CHAT_ID`、`WXPUSHER_APP_TOKEN`、`WXPUSHER_UIDS`、`BARK_URL`、`GOTIFY_URL`、`GOTIFY_TOKEN`、`PUSHPLUS_TOKEN`、`SERVERCHAN_KEY` |
| Telegram 控制 Bot | `TELEGRAM_BOT_MODE`、`TELEGRAM_BOT_ALLOWED_USER_IDS`、`TELEGRAM_BOT_ALLOWED_CHAT_IDS`、`TELEGRAM_BOT_PRIVATE_ONLY`、`TELEGRAM_BOT_WEBHOOK_PUBLIC_URL`、`TELEGRAM_BOT_WEBHOOK_PATH_SECRET`、`TELEGRAM_BOT_WEBHOOK_SECRET` |

完整示例见 [`.env.example`](.env.example)。环境变量只覆盖非空值，已经保存在 WebUI 中的密钥不会因为空环境变量被清除。

## 数据、兼容性与安全

`DATA_DIR` 中主要包含：

```text
data/
├── settings.json
├── subscriptions.json
├── notifications.json
├── jobs.json
├── jobs.archive.json
├── automation_events.json
├── telegram_bot.json      Telegram Update/Callback 幂等状态与脱敏命令审计
└── backups/
    └── verification.json
```

存储采用带 `schema_version` 的 JSON 信封，并具备：

- 临时文件、`fsync` 和原子 rename；
- 写盘成功后才替换内存状态；
- v0 → v1 自动迁移与一次性迁移备份；
- Unix `0600` 权限修复；
- 损坏文件隔离；
- 未来 schema 保护，避免旧程序覆盖新格式数据。

安全建议：

- 不要把 `data/`、Cookie、Token、数据库或 `.env` 提交到 Git；
- 不要使用默认密码暴露公网；
- 优先放在反向代理、HTTPS、VPN 或可信局域网后；
- `/strm/*` 使用独立访问 Token，不依赖管理端 Basic Auth；
- 浏览器状态修改请求带有同源/CSRF 防护；
- 登录失败频率限制、CSP 和通用安全响应头默认启用；
- WebUI“系统诊断”页可下载完整备份、预览恢复并导出脱敏诊断包；
- 生产部署参见 [HTTPS 反向代理指南](docs/https-reverse-proxy.md)。

## API 与可观测性

- 健康检查：`GET /health`
- 运行指标：`GET /api/metrics`（JSON）、`GET /metrics`（Prometheus）
- 运行时日志：`GET|PUT /api/observability/log-filter`
- 脱敏诊断：`GET /api/diagnostics`、`GET /api/diagnostics/export`
- 备份：`GET /api/backups/export`、`POST /api/backups/preview`、`GET|POST /api/backups/verification`、`POST /api/backups/restore`
- Store 生命周期：`GET|POST /api/storage/cleanup`、`GET /api/storage/decision`
- 自动化 Token：`GET|POST|DELETE /api/automation-token`
- 订阅交换：`GET /api/subscriptions/export`、`POST /api/subscriptions/import/preview|import`
- Job SSE：`GET /api/jobs/events`
- 更新日历：`GET /api/calendar`
- 自动化事件：`GET /api/automation/events`
- 自动化摘要：`GET /api/automation/summary`

API 成功响应统一为：

```json
{"ok": true, "data": {}}
```

错误响应统一为：

```json
{"ok": false, "error": "validation_error", "message": "..."}
```

详细约定见 [`docs/api-contract.md`](docs/api-contract.md)，流水线模型见 [`docs/automation-events.md`](docs/automation-events.md)。

## 从源码构建

### 依赖

- Rust stable（edition 2021）
- Node.js，仅用于前端语法和纯函数测试
- Docker / Docker Buildx，可选
- Tailwind standalone CLI，仅在修改 Tailwind 输入时需要
- Graphviz，仅在重新生成架构图时需要

### 开发运行

```bash
cp .env.example .env
cargo run --release
```

### 质量检查

```bash
find static -type f -name '*.js' -print0 | sort -z | xargs -0 -n1 node --check
node --test tests/frontend_*.test.js
cargo fmt --all -- --check
cargo check --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
```

CI 会执行全部前端模块测试、Rust 单元/集成测试和 OpenAPI 路由契约检查；真实 PanSou 网络测试默认忽略。

### 编译二进制

```bash
cargo build --release --locked
file target/release/my-media-sub
sha256sum target/release/my-media-sub
```

### 构建 Docker 镜像

```bash
docker build \
  -t my-media-sub:1.13.1 \
  -t my-media-sub:latest \
  .
```

### 重新生成前端 CSS

```bash
TAILWIND_BIN=/path/to/tailwindcss scripts/build-css.sh
```

### 重新生成架构图

```bash
dot -Tsvg docs/architecture.dot -o docs/architecture.svg
dot -Tpng -Gdpi=160 docs/architecture.dot -o docs/architecture.png
```

## 项目结构

```text
src/
├── api/                  HTTP 路由与稳定响应契约
│   ├── subscriptions/   CRUD、状态、动作、元数据、换源
│   └── drive/           浏览、操作、Aria2、自动化投影
├── clients/              PanSou、夸克、Aria2 和共享 HTTP 处理
├── jobs/
│   └── worker/           四类独立 Job Handler
├── models/               Settings、Subscription、Calendar、AutomationEvent 等
├── services/             检查、转存、换源、日历、元数据、通知和 STRM
├── store/                schema、原子 JSON Store 和索引
└── utils/                时间、文件、指标和安全工具

static/
├── app.js                Alpine 装配层
├── index.html
├── styles.css
└── js/
    ├── core/             API、路由、通知、轮询和 Shell
    ├── stores/           subscriptions、jobs、downloads、drive
    └── features/         dashboard、calendar、search、settings 等
```

## 文档

- [架构说明](docs/architecture.md)
- [持续开发路线](docs/roadmap.md)
- [API 契约](docs/api-contract.md)
- [自动化事件](docs/automation-events.md)
- [自动化 API、Token、导入导出和 Webhook 示例](docs/automation-api.md)
- [Telegram 主动控制 Bot 部署与安全指南](docs/telegram-bot.md)
- [媒体日历规则](docs/media-calendar.md)
- [资源质量与安全换源](docs/source-quality.md)
- [HTTPS 反向代理与安全部署](docs/https-reverse-proxy.md)
- [PWA、离线壳层与缓存安全](docs/pwa.md)
- [JSON Store 性能基线与 SQLite 决策](docs/storage-scaling.md)
- [OpenAPI 3.1 文档](/api-docs.html)
- [v1.13.1 升级指南](docs/upgrade-v1.13.1.md)
- [v1.13.1 完整变更记录](CHANGELOG-v1.13.1.md)
- [v1.13.0 升级指南](docs/upgrade-v1.13.0.md)
- [v1.13.0 完整变更记录](CHANGELOG-v1.13.0.md)
- [v1.12.0 升级指南](docs/upgrade-v1.12.0.md)
- [v1.12.0 完整变更记录](CHANGELOG-v1.12.0.md)
- [v1.11.0 升级指南](docs/upgrade-v1.11.0.md)
- [v1.10.0 升级指南](docs/upgrade-v1.10.0.md)

## 升级

Docker：

```bash
docker compose pull
docker compose up -d
```

二进制：

1. 备份 `DATA_DIR`；
2. 下载并校验新压缩包；
3. 同时替换二进制和整个 `static/`；
4. 保留原 `data/`；
5. 启动后检查 `/health`、系统设置和后台任务。

不要只替换二进制而继续使用旧版 `static/`。详细步骤见对应版本的升级指南。

## 版本说明

### 1.13.1

- 修复目标集已经完成但前端因展示进度滞后而未归入“已完结”的问题，并在元数据、总集数或规则变化后重新协调权威完结状态；
- 修复 TMDB 部分季详情请求失败或图片首次加载失败后，剧集缩略图偶发永久丢失的问题；
- 修复轮询列表遇到重复/缺失业务 ID 时 Alpine 偶发 `Cannot read properties of undefined (reading 'after')`，并对 Aria2 状态快照去重；
- 工作台改为直接展示失效订阅、失败任务和未读通知，移除含义不明的广告式文案和装饰信息；
- 重写并重新生成当前架构文档与架构图，移除过时的 v1.3.0 架构描述；
- 保持 `schema_version: 1`、JSON 单写和单实例管理员模型，可从 v1.13.0 直接升级。

### 1.13.0

- 完成 P20–P21：OpenAPI 自动化契约、单实例 scoped Token、幂等订阅交换，以及 Telegram 主动控制机器人；
- OpenAPI 与 Axum 路由自动同步并兼容校验 v1.12.0 基线，当前覆盖 91 条路径、103 个操作；自动化 Token 支持轮换、撤销、过期和最小 scope；
- 订阅导入/导出使用版本化信封、冲突预览、原子批量写入和 Idempotency-Key，版本化 Webhook 贯通 request/correlation/job 标识；
- Telegram Bot 支持 long polling 或双 Secret Webhook、数字 user/chat ID 白名单、默认私聊限制、分页只读命令和脱敏运行诊断；
- 受控 `/check`、`/retry`、`/cancel`、`/signin`、`/read` 写命令使用绑定 user/chat/action/resource/scope 的 120 秒一次性确认，Update、Callback 和业务键三层去重；
- Telegram 主动通知可附带 HMAC 签名的查看详情、标记已读和重新检查按钮，并继续遵守渠道路由、安静时段、摘要与重复限频；
- 新增持久化 `telegram_bot.json`、脱敏命令审计、user/chat/command 分层限流、失败冷却、诊断页 Bot 卡片及可选真实 Telegram 沙箱 smoke；
- 保持 `schema_version: 1` 和 JSON 单写；升级必须同时替换二进制与完整 `static/`，启用 Bot 前请阅读 Telegram 部署与应急指南。

### 1.12.0

- 完成 P17–P19：移动端与大列表体验、全链路可观测性、故障诊断以及备份与 Store 生命周期治理；
- HTTP、订阅和 Job 日志贯通 request/correlation/subscription/job 标识，支持 JSON 日志、Prometheus、慢操作、外部依赖延迟和运行时日志过滤；
- 诊断页增加磁盘、权限、时区、DNS、Store 一致性和分级建议，所有环境与数据检查保持只读；
- 备份增加逐项清单、隔离恢复验证、定期重验和 DATA_DIR 外部原子副本；清理前强制创建并验证 `pre-cleanup` 备份；
- Store 支持独立保留策略、增长预警和只读清理预览；SQLite 仅在记录阈值达到后进入决策阶段，当前继续 JSON 单写且不引入数据库依赖；
- 保持 `schema_version: 1`，旧 Job、Settings 和业务 Store 均兼容读取；升级必须同时替换二进制和完整 `static/`。

### 1.11.0

- 完成 P15–P16：优先级公平队列、分层并发、错误恢复、熔断、维护与历史归档，以及通知策略中心；
- Job 支持 high/normal/low、3:2:1 公平轮转、同订阅互斥、延迟重试、卡死检测和 `jobs.archive.json`；
- 通知支持事件渠道路由、最低级别、安静时段、重复限频、摘要聚合和模板预览；
- Webhook 支持逐目标退避重试与双签名重叠轮换，所有推送失败与核心自动化调用栈隔离；
- 保持 `schema_version: 1`，旧 Job 和 Settings 数据通过默认字段兼容读取。

### 1.10.0

- 完成 P11–P14：发布与升级烟雾测试、追更识别准确性、搜索与安全换源、转存和媒体库工作流；
- 日历增加媒体缩略图，支持剧照、季度海报和媒体海报逐级回退；
- 支持多集范围、SP/OVA/OAD、订阅级集数正则，以及缺集和重复集预览；
- 增加换源搜索偏好、排除词、稳定排序、PanSou 退避和候选集数覆盖预览；
- 增加目标冲突策略、安全文件名、STRM 一致性审计、Aria2 幂等重试及 Jellyfin/Emby/Plex/Webhook 刷新；
- 保持 `schema_version: 1`，历史配置和数据可兼容读取。

### 1.9.0

- 完成 P0–P10 路线图：Provider 抽象、备份恢复、诊断指标、安全加固、PWA、JSON 性能治理和 OpenAPI 文档；
- 新增标准 Browser Push、签名 Webhook、统一推送报告和可安装离线应用；
- 新增完整数据备份、恢复预览、自动备份保留策略及脱敏诊断包；
- 新增 CloudDriveProvider 能力边界与 Quark/Mock Provider，降低业务层对夸克客户端的直接耦合；
- 增强请求关联 ID、登录限流、CSP、安全响应头、敏感日志脱敏和危险操作确认；
- 保持 `schema_version: 1`，历史数据可兼容读取；升级时必须同时替换二进制和 `static/`。

### 1.3.0

- 新增上海时区媒体更新日历，提供周、月和列表视图；
- 支持手动排期、元数据排期和周期推断；
- 日历聚合缺集、已知、转存、STRM、Aria2 和任务状态；
- 完成 API/前端模块化、结构化自动化事件、安全换源和后端并发/幂等基础设施；
- JSON Store 增加 schema 迁移、原子批量提交和失败一致性保护。

### 1.2.0

- 统一成功/错误 API 信封和 WebUI 请求处理；
- 加固 JSON Store、在线更新、任务取消和设置密钥回填；
- 增加稳定性测试、发布检查与升级文档。

更早版本请查看 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 和仓库中的历史 CHANGELOG。

## License

[MIT](LICENSE)
