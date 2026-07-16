# My Media Sub

> 面向夸克网盘的媒体订阅、追更、转存与下载自动化服务。Rust + Axum 后端,无打包器的 Media Deck WebUI(Cinema Slate 设计语言)。

[![CI](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg)](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hellomrli/my-media-sub?display_name=tag)](https://github.com/hellomrli/my-media-sub/releases)
[![Container](https://img.shields.io/badge/GHCR-my--media--sub-blue?logo=docker)](https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

My Media Sub 把资源搜索、订阅检查、更新日历、自动转存、重命名、Aria2 下载、失效换源和通知推送组合成一个可自托管的媒体自动化工作台。单二进制 + JSON 存储,无外部数据库。STRM 暂时作为独立模块下线，旧数据字段保留以便后续迁移。

![架构图](docs/architecture.png)

## 核心能力

| 领域 | 能力 |
|---|---|
| 追更自动化 | 定时/手动检查、季与起始集过滤、同集多版本去重、缺集识别、完结判定与自动恢复 |
| 自动转存 | 幂等转存到电影/剧集/动画目录、递归文件定位、规则过滤、模板化重命名与批量修复 |
| 安全换源 | 失效计数、候选评分、季度/进度校验、冷却时间、自动应用与可回滚审计 |
| 资源与元数据 | PanSou 搜索、夸克分享探测与质量评分、TMDB 刮削(海报/年份/评分/总集数) |
| 更新日历 | 上海时区周/月/列表视图、手动排期、元数据推断、逐集处理状态 |
| 下载与媒体库 | Aria2 幂等提交与退避重试、Jellyfin/Emby/Plex/Webhook 刷新 |
| 后台任务 | 持久化优先级队列、三层并发限制、心跳看门狗、真实取消、优雅停机、重启恢复、SSE 实时状态 |
| 通知推送 | 企业微信/Telegram/WxPusher/Bark/Gotify/PushPlus/Server 酱、Browser Push、签名 Webhook、安静时段与摘要聚合 |
| Telegram Bot | Long polling/Webhook、数字 ID 白名单、受控写命令一次性确认、审计与限流、并发处理更新 |
| WebUI / PWA | Cinema Slate 深浅双主题、响应式布局、可安装离线应用、快捷入口 |
| 可观测性 | request/correlation/subscription/job 关联日志、JSON 日志、Prometheus、慢操作与外部依赖延迟 |
| 数据可靠性 | 原子 JSON Store、自动备份与隔离恢复验证、外部副本、保留策略与安全清理预览 |

## 快速开始

### Docker Compose(推荐)

```bash
mkdir -p my-media-sub/data
cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml

# 在同目录 .env 中设置管理员密码(必需)
printf 'SERVER_PASSWORD=replace-with-a-strong-password\nTZ=Asia/Shanghai\n' > .env
docker compose up -d
```

访问 `http://服务器地址:56001`,账号 `admin`。

**自 v2.0.0 起登录不再接受默认密码**:必须通过 `SERVER_PASSWORD`/`APP_PASSWORD` 或系统设置先设置真实密码,否则无法登录。

容器以非 root 用户(uid/gid 1000)运行;入口脚本会自动修正挂载数据目录的属主,从旧版本升级无需手动 chown。

常用维护命令:

```bash
docker compose ps
docker compose logs -f
docker compose pull && docker compose up -d
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

生产环境建议固定版本标签;每个版本同时发布补丁与次版本标签(如 `2.1.2` 和 `2.1`):

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.1.2
docker image inspect ghcr.io/hellomrli/my-media-sub:2.1.2 --format '{{.RepoDigests}}'
```

### Linux 二进制

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 下载并校验:

```bash
VERSION=v2.1.2
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-${VERSION}-linux-x86_64.tar.gz"
cd "my-media-sub-${VERSION}-linux-x86_64"

SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

运行目录需保留 `static/`。业务数据默认写入 `./data`,可用 `DATA_DIR` 修改。

## 首次配置

1. 打开「系统设置」,确认管理员密码已设置为强密码。
2. 配置夸克 Cookie,用连接测试确认有效。
3. 配置电影、剧集、动画的夸克目标目录。
4. 按需配置 PanSou、TMDB、Aria2 和推送渠道。
5. 在资源搜索中创建订阅,或手动添加分享链接。
6. 设置检查周期、并发、自动转存与安全换源策略。
7. 部署在可信反向代理之后时,在系统设置中开启 `trust_proxy_headers`,登录限流将按 `X-Forwarded-For` 的真实客户端 IP 计数;直连部署保持关闭(默认)。

## 自动化流程

```text
定时器 / 手动检查
  → 同订阅互斥与批量并发限制
  → 分享探测、文件过滤、季度匹配、同集版本选择
  → 检查结果按字段合并回写(不覆盖并发转存与用户编辑)
  → 幂等 SubscriptionTransfer Job(high/normal/low 加权公平调度)
  → 夸克转存(成功即持久化) → 重命名 → Aria2 → 通知推送
  → AutomationEvent 流水线审计
```

- 可重试故障按错误类别做最多 3 次带抖动的指数退避;连续临时故障触发熔断,冷却后放行恢复探测。
- 卡死看门狗基于心跳:仅任务超过 30 分钟无任何进度更新才终止;取消运行中任务会真正中止并立即释放并发槽。
- 收到 SIGTERM/Ctrl+C 时优雅停机:拒绝新任务、给运行中任务 30 秒宽限落盘,残留任务收敛为可手动重试的中断态。
- 通知中心支持事件路由、最低级别、安静时段(主机时区)、重复限频与延迟摘要;摘要跨重启恢复;失效的浏览器推送端点自动清理。

## 配置

推荐在 WebUI 管理业务配置;环境变量适合容器启动参数与初始账号。环境变量只覆盖非空值,已保存的密钥不会被空值清除。

### 基础环境变量

| 变量 | 默认值 | 说明 |
|---|---:|---|
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `56001` | HTTP 端口 |
| `SERVER_USERNAME` | `admin` | 初始管理员账号 |
| `SERVER_PASSWORD` | 无(必填) | 管理员密码;未设置或仍为默认值时登录被拒绝 |
| `DATA_DIR` | `./data` | JSON 数据、备份与运行状态目录 |
| `BACKUP_INTERVAL_HOURS` | `24` | 自动备份间隔,`0` 关闭 |
| `BACKUP_VERIFY_INTERVAL_HOURS` | `24` | 备份隔离恢复验证间隔,`0` 关闭 |
| `BACKUP_EXTERNAL_DIR` | 空 | 校验后原子复制备份的外部目录 |
| `BACKUP_RETENTION` | `7` | 保留的服务器备份数 |
| `RUST_LOG` | `info` | 日志过滤规则 |
| `LOG_FORMAT` | `text` | `json` 输出含关联上下文的 JSON 日志 |
| `SLOW_OPERATION_MS` | `1000` | 慢操作告警阈值(100–300000 毫秒) |
| `TZ` | 系统时区 | 容器建议 `Asia/Shanghai` |

保留策略类变量(`RETENTION_*`)与备份容量类变量(`BACKUP_MAX_*`、`STORE_GROWTH_WARNING_MB`)见 [`.env.example`](.env.example)。

### 集成环境变量

| 类型 | 变量 |
|---|---|
| 夸克 | `QUARK_COOKIE`、`QUARK_SIGNIN_COOKIE`、`QUARK_SIGNIN_ENABLED`、`QUARK_SIGNIN_HOUR` |
| 搜索 | `PANSOU_API_URL` |
| Aria2 | `ARIA2_RPC_URL`、`ARIA2_SECRET`、`ARIA2_MOVIE_DIR`、`ARIA2_SERIES_DIR`、`ARIA2_ANIME_DIR` |
| TMDB | `TMDB_API_KEY`、`TMDB_LANGUAGE` |
| 推送 | `WECOM_BOT_URL`、`TELEGRAM_BOT_TOKEN`、`TELEGRAM_CHAT_ID`、`WXPUSHER_APP_TOKEN`、`WXPUSHER_UIDS`、`BARK_URL`、`GOTIFY_URL`、`GOTIFY_TOKEN`、`PUSHPLUS_TOKEN`、`SERVERCHAN_KEY` |
| Telegram Bot | `TELEGRAM_BOT_MODE`、`TELEGRAM_BOT_ALLOWED_USER_IDS`、`TELEGRAM_BOT_ALLOWED_CHAT_IDS`、`TELEGRAM_BOT_PRIVATE_ONLY`、`TELEGRAM_BOT_WEBHOOK_PUBLIC_URL`、`TELEGRAM_BOT_WEBHOOK_PATH_SECRET`、`TELEGRAM_BOT_WEBHOOK_SECRET` |

## 安全

v2.x 的默认安全基线:

- 登录拒绝默认密码;未设置密码无法登录。
- 登录限流按连接对端 IP 计数,`X-Forwarded-For` 仅在 `trust_proxy_headers` 显式开启时被信任;失败的自动化 Token 认证同样计入限流。
- 自动化 Token 采用最小 scope 与显式读路径白名单,未列出的接口默认拒绝。
- 容器以非 root 用户运行;设置密钥以固定长度掩码展示。
- 浏览器状态修改请求带同源/CSRF 防护,CSP 与安全响应头默认启用。
- STRM 已在 v2.2.0 暂时下线，旧数据字段保留；后续将以独立模块重新接入。

部署建议:

- 不要把 `data/`、Cookie、Token 或 `.env` 提交到 Git;
- 公网访问放在反向代理 + HTTPS 之后,参见 [HTTPS 反向代理指南](docs/https-reverse-proxy.md);
- WebUI「系统诊断」页可下载完整备份、预览恢复并导出脱敏诊断包。

## 数据与备份

`DATA_DIR` 主要内容:

```text
data/
├── settings.json
├── subscriptions.json
├── notifications.json
├── jobs.json
├── jobs.archive.json
├── automation_events.json
├── telegram_bot.json
└── backups/
    └── verification.json
```

存储为带 `schema_version` 的 JSON 信封:临时文件 + `fsync` + 原子 rename、写盘成功才替换内存、Unix `0600` 权限、损坏文件隔离、未来 schema 保护。任务存储裁剪只淘汰终态任务,排队/运行中的任务不会被清理。

## API 与可观测性

- 健康检查:`GET /health`
- 运行指标:`GET /api/metrics`(JSON)、`GET /metrics`(Prometheus)
- 运行时日志:`GET|PUT /api/observability/log-filter`
- 脱敏诊断:`GET /api/diagnostics`、`GET /api/diagnostics/export`
- 备份:`GET /api/backups/export`、`POST /api/backups/preview`、`GET|POST /api/backups/verification`、`POST /api/backups/restore`
- Store 生命周期:`GET|POST /api/storage/cleanup`、`GET /api/storage/decision`
- 自动化 Token:`GET|POST|DELETE /api/automation-token`
- 订阅交换:`GET /api/subscriptions/export`、`POST /api/subscriptions/import/preview|import`
- Job SSE:`GET /api/jobs/events`;日历:`GET /api/calendar`;自动化事件:`GET /api/automation/events`

成功与错误响应统一信封:

```json
{"ok": true, "data": {}}
{"ok": false, "error": "validation_error", "message": "..."}
```

完整契约见 [`docs/api-contract.md`](docs/api-contract.md)与在线 OpenAPI 文档(`/api-docs.html`,91 条路径、103 个操作)。

## 从源码构建

依赖:Rust stable(edition 2021);Node.js(前端测试);可选 Docker、Tailwind standalone CLI(改样式时)、Graphviz(重新生成架构图)。

```bash
# 开发运行
cp .env.example .env
cargo run --release

# 质量检查(CI 同款)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
node --test tests/frontend_*.test.js
python3 scripts/check-openapi.py

# 发布二进制
cargo build --release --locked

# Docker 镜像
docker build -t my-media-sub:2.1.2 -t my-media-sub:latest .
```

前端产物由脚本生成,请勿直接编辑:

```bash
node scripts/build-frontend.mjs          # static/index.html ← index.tmpl.html + partials/
node scripts/build-frontend.mjs --check  # 校验一致性
TAILWIND_BIN=/path/to/tailwindcss scripts/build-css.sh   # static/styles.css ← tailwind/input.css
```

## 项目结构

```text
src/
├── api/                  HTTP 路由与稳定响应契约
│   ├── subscriptions/    CRUD、状态、动作、元数据、换源
│   └── drive/            浏览、操作、Aria2、自动化投影
├── clients/              PanSou、夸克、Aria2 和共享 HTTP 处理
├── jobs/                 持久化队列、调度器、看门狗与四类 Job Handler
├── models/               Settings、Subscription、Calendar、AutomationEvent 等
├── services/             检查、转存、换源、日历、元数据和通知
│   └── post_transfer.rs  Rust trait 驱动的转存后处理模块注册表
├── store/                schema、原子 JSON Store 和索引
└── utils/                时间、文件、指标和安全工具

static/
├── index.tmpl.html       页面拼装模板(@include 标记)
├── partials/             各页面/弹窗 HTML 分片
├── index.html            生成产物(scripts/build-frontend.mjs)
├── styles.css            生成产物(scripts/build-css.sh)
└── js/                   core(API/路由/轮询) + stores + features
```

## 文档

- [架构说明](docs/architecture.md) · [API 契约](docs/api-contract.md) · [自动化事件](docs/automation-events.md)
- [自动化 API、Token、导入导出和 Webhook 示例](docs/automation-api.md)
- [Telegram Bot 部署与安全指南](docs/telegram-bot.md)
- [媒体日历规则](docs/media-calendar.md) · [资源质量与安全换源](docs/source-quality.md)
- [HTTPS 反向代理与安全部署](docs/https-reverse-proxy.md) · [PWA 与缓存安全](docs/pwa.md)
- [JSON Store 性能基线与 SQLite 决策](docs/storage-scaling.md)
- [v2.2.0 升级指南](docs/upgrade-v2.2.0.md) · [v2.2.0 变更记录](CHANGELOG-v2.2.0.md)
- [v2.1.2 升级指南](docs/upgrade-v2.1.2.md) · [v2.1.2 变更记录](CHANGELOG-v2.1.2.md)
- [v2.1.1 升级指南](docs/upgrade-v2.1.1.md) · [v2.1.1 变更记录](CHANGELOG-v2.1.1.md)
- [v2.1.0 升级指南](docs/upgrade-v2.1.0.md) · [v2.1.0 变更记录](CHANGELOG-v2.1.0.md)
- [v2.0.0 升级指南](docs/upgrade-v2.0.0.md) · [v2.0.0 变更记录](CHANGELOG-v2.0.0.md)

## 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制:备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。详细步骤与回滚见对应版本的升级指南。

## 版本说明

### 2.2.0

- 暂时下线 STRM HTTP 代理、生成/审计 API 和 WebUI 配置；保留旧数据字段，后续以独立 Rust 模块重新接入。
- 新增 Rust 原生转存后处理模块注册表，媒体库刷新不再硬编码在转存服务中。
- Telegram Bot 新增单订阅和单 Job 详情查询，继续复用权限、确认按钮、限流和审计。
- 首屏请求并发化，Rust 服务端启用 Brotli/Gzip 压缩并增加静态资源缓存。
- 存储 schema 保持兼容，可从 v2.1.2 直接升级。

### 2.1.2

- WebUI 仅在存在活动或排队中的 Aria2 任务时进行 2 秒高频轮询；无任务时保留进入页面和手动操作后的单次刷新，避免空闲状态持续请求远程 Aria2。
- Aria2 任务归零后自动停止前台轮询；后台下载监控仍保留，用于完成状态和异常恢复的低频兜底。
- 提升 PWA 静态缓存版本，确保已安装客户端及时获取新的轮询逻辑。
- 存储 `schema_version` 与 OpenAPI 契约保持不变，可从 v2.1.1 直接升级。

### 2.1.1

- 目标目录创建失败时终止转存,不再静默把文件保存到网盘根目录并误报成功;
- Aria2 GID、文件名和完成状态持久化到订阅,通知被清理后仍可完成订阅状态流转,瞬时失败可重试;
- 修改季数或媒体类型时清理上一季的集数、转存、同步下载和完结状态,并重新计算总集数;
- 修复订阅导入 `Idempotency-Key` 并发竞态,相同 Key 的并发请求只执行一次;
- 关闭任务队列时同步中止内层业务任务,终态任务不会被迟到的进度更新改回运行或成功;
- 存储 `schema_version` 与 OpenAPI 契约保持不变,可从 v2.1.0 直接升级。

### 2.1.0

- WebUI 视觉重设计「Cinema Slate」:深色主题为中性炭黑底 + 琥珀金强调,浅色主题为暖纸白 + 深琥珀;
- 卡片扁平化,移除渐变光晕与网格纹理,改用发丝线边框;圆角统一为卡片 12px / 控件 8px;
- 纯 CSS token 与组件重写,未改动 Alpine 绑定与 DOM 结构,功能不变;
- 后端、存储与 OpenAPI 契约无变化,可从 v2.0.0 无缝升级;PWA 缓存版本已更新。

### 2.0.0

- 安全加固:登录拒绝默认密码;限流默认按对端 IP,`trust_proxy_headers` 控制 XFF 信任;Token 读权限白名单;固定长度密钥掩码;
- 后台任务:裁剪只淘汰终态任务、SIGTERM 优雅停机、心跳看门狗、取消真正中止任务;
- 订阅:批量检查按字段合并回写、跳过中途删除的订阅、转存成功立即持久化,消除重复转存与丢更新;
- 通知:摘要跨重启恢复、浏览器推送逐订阅容错并清理失效端点、Telegram 并发处理、安静时段用主机时区;
- 部署:容器非 root 运行(入口自动修正数据属主)、compose 从 `.env` 读取口令;
- 工程化:WebUI 拆分为模板 + 分片,由零依赖脚本组装;
- 保持 `schema_version: 1`,可从 v1.13.x 直接升级,公网实例请先设置强密码。

更早版本(v1.x)的说明与升级指南见 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 与仓库内历史 `CHANGELOG-v1.*.md`、`docs/upgrade-v1.*.md`。

## License

[MIT](LICENSE)
