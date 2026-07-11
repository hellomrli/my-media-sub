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
| 媒体库刷新 | 转存后自动刷新 Jellyfin、Emby、Plex 或通用 Webhook |

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

### Linux 二进制

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 下载 Linux x86_64 压缩包并校验 SHA256：

```bash
VERSION=v1.9.0
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
  → 夸克转存
  → 重命名
  → STRM 生成
  → Aria2 提交
  → 通知与推送
  → AutomationEvent 流水线审计
```

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
| `BACKUP_RETENTION` | `7` | 保留最近的服务器备份数 |
| `BACKUP_MAX_ARCHIVE_MB` | `256` | 单个备份解码内容上限 |
| `BACKUP_MAX_STORAGE_MB` | `512` | 服务器备份目录总预算 |
| `RUST_LOG` | `info` | Rust 日志过滤规则 |
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

完整示例见 [`.env.example`](.env.example)。环境变量只覆盖非空值，已经保存在 WebUI 中的密钥不会因为空环境变量被清除。

## 数据、兼容性与安全

`DATA_DIR` 中主要包含：

```text
data/
├── settings.json
├── subscriptions.json
├── notifications.json
├── jobs.json
├── automation_events.json
└── backups/
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
- 运行指标：`GET /api/metrics`（包含逐 Store 大小、解析和写入耗时）
- 脱敏诊断：`GET /api/diagnostics`、`GET /api/diagnostics/export`
- 备份：`GET /api/backups/export`、`POST /api/backups/preview`、`POST /api/backups/restore`
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

当前基线包含 43 个前端测试和 334 个 Rust 测试；真实 PanSou 网络测试默认忽略。

### 编译二进制

```bash
cargo build --release --locked
file target/release/my-media-sub
sha256sum target/release/my-media-sub
```

### 构建 Docker 镜像

```bash
docker build \
  -t my-media-sub:1.3.0 \
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
- [媒体日历规则](docs/media-calendar.md)
- [资源质量与安全换源](docs/source-quality.md)
- [HTTPS 反向代理与安全部署](docs/https-reverse-proxy.md)
- [PWA、离线壳层与缓存安全](docs/pwa.md)
- [JSON Store 性能基线与 SQLite 决策](docs/storage-scaling.md)
- [OpenAPI 3.1 文档](/api-docs.html)
- [v1.9.0 升级指南](docs/upgrade-v1.9.0.md)
- [v1.9.0 完整变更记录](CHANGELOG-v1.9.0.md)

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
