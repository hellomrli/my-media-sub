# My Media Sub

> 夸克网盘上的媒体追更工作台：搜索、订阅、转存、重命名、下载与通知，一条链路跑通。

[![CI](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg)](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hellomrli/my-media-sub?display_name=tag)](https://github.com/hellomrli/my-media-sub/releases)
[![Container](https://img.shields.io/badge/GHCR-my--media--sub-blue?logo=docker)](https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

追剧不必反复点开分享链接。My Media Sub 把「发现资源 → 盯住更新 → 自动转存 → 整理命名 → 可选下载 → 及时通知」收成一个可自托管的小服务：

- **Rust + Axum** 单二进制后端，无外部数据库，数据落在本地 JSON（原子写入 + 自动备份）；
- **无打包器 WebUI**（Alpine.js + Tailwind，Cinema Slate 视觉），可安装为 PWA；
- **Docker 一键拉起**，容器非 root，默认拒绝弱密码。

![架构图](docs/architecture.png)

---

## 功能总览

| 你想解决的事 | 它怎么做 |
|---|---|
| 自动追更 | 定时/手动检查分享；按季与起始集过滤；同集多版本择优；识别缺集与完结；分享探测分页拉全并显式标记截断 |
| 自动转存 | 业务级幂等（重试不重复转存）；电影/剧集/动画目录归类；规则过滤、模板重命名、批量修复命名 |
| 分享失效 | 失效计数、候选评分、进度校验、冷却与自动换源，全程可回滚审计 |
| 找资源 | PanSou 聚合搜索、夸克分享探测与质量评分、TMDB 元数据（海报、年份、评分、总集数） |
| 看排期 | 上海时区周/月/列表日历；手动排期或元数据推断；逐集处理状态一目了然 |
| 下到本地 | Aria2 幂等提交、批量分批、退避重试；可触发 Jellyfin / Emby / Plex / Webhook 刷新媒体库 |
| 心里有数 | 持久化任务队列、真实取消、心跳看门狗、优雅停机、SSE 实时状态、结构化自动化事件流水线 |
| 消息触达 | 企业微信 / Telegram / Bark / Gotify / WxPusher / PushPlus / Server 酱；Browser Push；签名 Webhook；安静时段与摘要聚合 |
| 手机遥控 | Telegram Bot：白名单、写操作二次确认、限流与审计 |
| 敢上生产 | 原子 JSON Store、损坏隔离 + 显式告警、自动备份与恢复验证、关联日志、Prometheus 指标 |

STRM 相关能力自 v2.2.0 起暂时下线，旧字段保留，方便以后以独立模块接回。

---

## 快速开始

### 方式一：Docker Compose（推荐）

```bash
mkdir -p my-media-sub/data && cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml

# 同目录写入管理员密码（必需，勿使用 change-me）
printf 'SERVER_PASSWORD=replace-with-a-strong-password\nTZ=Asia/Shanghai\n' > .env
docker compose up -d
```

浏览器打开 `http://服务器地址:56001`，用户名默认 `admin`。

> **从 v2.0.0 起，默认密码不可登录。** 必须通过 `SERVER_PASSWORD` / `APP_PASSWORD` 或系统设置配置真实密码。

容器以 uid/gid `1000` 运行；入口脚本会自动修正挂载数据目录属主，旧版本升级一般无需手动 `chown`。

常用运维命令：

```bash
docker compose ps            # 状态
docker compose logs -f       # 日志
docker compose pull && docker compose up -d   # 升级
docker compose down          # 停止
```

### 方式二：Docker Run

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

生产环境请钉死版本标签。每个发布会同时打补丁与次版本标签（例如 `2.2.11` 与 `2.2`）：

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.2.11
docker image inspect ghcr.io/hellomrli/my-media-sub:2.2.11 --format '{{.RepoDigests}}'
```

### 方式三：Linux 二进制

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 下载并校验：

```bash
VERSION=v2.2.11
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-${VERSION}-linux-x86_64.tar.gz"
cd "my-media-sub-${VERSION}-linux-x86_64"

SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

运行目录需保留完整 `static/`。业务数据默认写在 `./data`，可用 `DATA_DIR` 改路径。

---

## 第一次打开时

1. 进入「系统设置」，确认管理员密码足够强。
2. 填入夸克 Cookie，用连接测试确认可用。
3. 配置电影、剧集、动画（以及你需要的自定义分类）目标目录。
4. 按需打开 PanSou、TMDB、Aria2 与推送渠道。
5. 在资源搜索里创建订阅，或直接粘贴分享链接。
6. 设定检查周期、并发、自动转存与换源策略。
7. 若前面有可信反向代理，再开启 `trust_proxy_headers`，登录限流才会按 `X-Forwarded-For` 计真实客户端；直连部署请保持关闭（默认）。

---

## 一条检查会经历什么

```text
定时器 / 手动检查
  → 批量互斥 + 同订阅互斥 + 并发限制
  → 分享探测（分页拉全，截断显式标记 partial）
  → 规则过滤 → 季度匹配 → 同集择优
  → 检查结果按字段合并回写（不踩并发转存、完结状态与用户编辑）
  → 幂等 SubscriptionTransfer Job（高 / 中 / 低加权公平）
  → 夸克转存（执行前按已转存状态过滤，成功即落盘）
  → 重命名 → 可选 Aria2 → 通知
  → AutomationEvent 流水线审计
```

设计上偏「宁可慢一点，也不要悄悄写错」：

- 转存具备业务级幂等：任务重试/重放会跳过已转存文件，不在网盘制造重复内容。
- 可重试错误最多 3 次指数退避（带抖动）；连续临时故障会熔断，冷却后再探测恢复。
- 心跳看门狗：超过 30 分钟毫无进度才判卡死；取消会真正中止任务并立刻释放并发槽。
- SIGTERM / Ctrl+C：拒收新任务，给运行中任务约 30 秒落盘，残留收敛为可手动重试的中断态。
- 通知支持路由、最低级别、安静时段（主机时区）、限频与延迟摘要；摘要可跨重启；失效的浏览器推送端点会自动清理。

---

## 配置

日常配置优先走 WebUI。环境变量更适合容器启动参数与初始账号：只覆盖非空值，不会用空字符串把已保存的密钥冲掉。

### 基础环境变量

| 变量 | 默认 | 说明 |
|---|---:|---|
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `56001` | HTTP 端口 |
| `SERVER_USERNAME` | `admin` | 初始管理员账号 |
| `SERVER_PASSWORD` | 无（必填） | 管理员密码；未设置或仍为默认值时拒绝登录 |
| `DATA_DIR` | `./data` | JSON 数据、备份与运行状态 |
| `BACKUP_INTERVAL_HOURS` | `24` | 自动备份间隔；`0` 关闭 |
| `BACKUP_VERIFY_INTERVAL_HOURS` | `24` | 备份隔离恢复验证间隔；`0` 关闭 |
| `BACKUP_EXTERNAL_DIR` | 空 | 校验后原子复制到外部目录 |
| `BACKUP_RETENTION` | `7` | 服务器侧保留份数 |
| `RUST_LOG` | `info` | 日志过滤 |
| `LOG_FORMAT` | `text` | `json` 时输出带关联上下文的 JSON 日志 |
| `SLOW_OPERATION_MS` | `1000` | 慢操作阈值（100–300000 ms） |
| `TZ` | 系统时区 | 容器建议 `Asia/Shanghai` |

保留策略（`RETENTION_*`）与备份容量（`BACKUP_MAX_*`、`STORE_GROWTH_WARNING_MB`）见 [`.env.example`](.env.example)。

### 常见集成变量

| 类型 | 变量 |
|---|---|
| 夸克 | `QUARK_COOKIE`、`QUARK_SIGNIN_COOKIE`、`QUARK_SIGNIN_ENABLED`、`QUARK_SIGNIN_HOUR` |
| 搜索 | `PANSOU_API_URL` |
| Aria2 | `ARIA2_RPC_URL`、`ARIA2_SECRET`、`ARIA2_MOVIE_DIR`、`ARIA2_SERIES_DIR`、`ARIA2_ANIME_DIR` |
| TMDB | `TMDB_API_KEY`、`TMDB_LANGUAGE` |
| 推送 | `WECOM_BOT_URL`、`TELEGRAM_BOT_TOKEN`、`TELEGRAM_CHAT_ID`、`WXPUSHER_APP_TOKEN`、`WXPUSHER_UIDS`、`BARK_URL`、`GOTIFY_URL`、`GOTIFY_TOKEN`、`PUSHPLUS_TOKEN`、`SERVERCHAN_KEY` |
| Telegram Bot | `TELEGRAM_BOT_MODE`、`TELEGRAM_BOT_ALLOWED_USER_IDS`、`TELEGRAM_BOT_ALLOWED_CHAT_IDS`、`TELEGRAM_BOT_PRIVATE_ONLY`、`TELEGRAM_BOT_WEBHOOK_PUBLIC_URL`、`TELEGRAM_BOT_WEBHOOK_PATH_SECRET`、`TELEGRAM_BOT_WEBHOOK_SECRET` |

---

## 安全与部署建议

v2.x 默认基线：

- 拒绝默认密码；未配置密码无法登录。
- 登录限流默认按连接对端 IP；`X-Forwarded-For` 仅在显式开启 `trust_proxy_headers` 后信任。自动化 Token 认证失败同样计入限流。
- 自动化 Token 最小 scope + 读路径白名单，未列出的接口默认拒绝。
- 容器非 root；密钥在 UI 中固定长度掩码。
- 浏览器写请求有同源 / CSRF 防护；默认启用 CSP 与安全响应头。

请务必：

- 不要把 `data/`、Cookie、Token、`.env` 提交进 Git。
- 公网访问放在反向代理 + HTTPS 之后，参见 [HTTPS 反向代理指南](docs/https-reverse-proxy.md)。
- 出事前先在「系统诊断」里下载完整备份；恢复前可用预览与脱敏诊断包。

---

## 数据放在哪里

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

存储是带 `schema_version` 的 JSON 信封：临时文件写入 → `fsync` → 原子 rename；写盘成功后才替换内存；Unix 上文件权限 `0600`。损坏文件会被隔离为 `*.json.corrupt-*` 并**显式告警**：启动时发站内通知，`/api/diagnostics` 数据一致性检查会持续报告，直到你核对备份并清理隔离文件。任务裁剪只淘汰终态任务，排队与运行中的不会被顺手清掉。

---

## API 与可观测性

| 用途 | 入口 |
|---|---|
| 存活 | `GET /health` |
| 指标 | `GET /api/metrics`（JSON）、`GET /metrics`（Prometheus） |
| 日志过滤 | `GET\|PUT /api/observability/log-filter` |
| 诊断 | `GET /api/diagnostics`、`GET /api/diagnostics/export` |
| 备份 | `GET /api/backups/export`、`POST /api/backups/preview`、`GET\|POST /api/backups/verification`、`POST /api/backups/restore` |
| 存储清理 | `GET\|POST /api/storage/cleanup`、`GET /api/storage/decision` |
| 自动化 Token | `GET\|POST\|DELETE /api/automation-token` |
| 订阅交换 | `GET /api/subscriptions/export`、`POST /api/subscriptions/import/preview\|import` |
| 实时任务 | `GET /api/jobs/events`（SSE） |
| 日历 / 流水线 | `GET /api/calendar`、`GET /api/automation/events` |

响应统一信封：

```json
{"ok": true, "data": {}}
{"ok": false, "error": "validation_error", "message": "..."}
```

完整契约见 [`docs/api-contract.md`](docs/api-contract.md)，以及运行中的 OpenAPI 页面 `/api-docs.html`。

---

## 从源码构建

需要：Rust stable（edition 2021）、Node.js（前端测试与模板组装）。可选 Docker、Tailwind standalone CLI（改样式）、Graphviz（重绘架构图）。

```bash
cp .env.example .env
cargo run --release

# 与 CI 一致的检查
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all --locked
node --test tests/frontend_*.test.js
python3 scripts/check-openapi.py

cargo build --release --locked
docker build -t my-media-sub:dev .
```

前端产物由脚本生成，**不要手改** `static/index.html` / `static/styles.css`：

```bash
node scripts/build-frontend.mjs          # index.html ← 模板 + partials
node scripts/build-frontend.mjs --check  # 校验产物是否过期
TAILWIND_BIN=/path/to/tailwindcss scripts/build-css.sh
```

改动任何静态资源后，同步 bump `static/service-worker.js` 的 `CACHE_VERSION`，否则 PWA 客户端可能继续命中旧缓存。

### 仓库结构（精简）

```text
src/
├── api/           HTTP 路由与响应契约
├── clients/       PanSou / 夸克 / Aria2
├── jobs/          持久化队列、调度与 Handler
├── models/        领域模型
├── services/      检查、转存、换源、日历、通知…
├── store/         原子 JSON Store
└── utils/         时间、文件、指标、正则缓存、脱敏

static/
├── index.tmpl.html + partials/   源模板
├── index.html / styles.css       生成物（勿手改）
└── js/                           core · stores · features
```

---

## 文档索引

- [架构](docs/architecture.md) · [API 契约](docs/api-contract.md) · [自动化事件](docs/automation-events.md)
- [自动化 API / Token / 导入导出](docs/automation-api.md) · [Telegram Bot](docs/telegram-bot.md)
- [媒体日历](docs/media-calendar.md) · [资源质量与换源](docs/source-quality.md)
- [HTTPS 与安全部署](docs/https-reverse-proxy.md) · [PWA](docs/pwa.md)
- [存储扩展与 SQLite 决策](docs/storage-scaling.md)
- 当前版本：[v2.2.11 升级指南](docs/upgrade-v2.2.11.md) · 完整变更见 [CHANGELOG.md](CHANGELOG.md)

各版本升级步骤在 `docs/upgrade-v*.md`；变更历史统一写在 [CHANGELOG.md](CHANGELOG.md)。

---

## 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包
#        → 同时替换二进制与整个 static/ → 保留 data/ → 检查 /health
```

**不要只换二进制却留着旧的 `static/`。** 细节与回滚见对应版本的升级指南。

---

## 版本说明

### 2.2.11

- 转存业务级幂等：执行前按已转存文件名与状态键过滤候选，任务重试/重放不再把已成功的季重复转存到网盘（`ep:` 集数键不含季号，仅单季订阅启用，避免多季误判）。
- 消除三处「整条覆盖」写回（检查服务自动换源、API 手动换源与回滚、Telegram 换源）：改为在 Store update 闭包内基于最新记录做字段级修改，不再丢失并发的转存进度与用户编辑。
- 批量检查加服务级互斥；批量写回不再把并发期间刚判定的完结/失效状态回退成追更中。已有批量在跑时再次触发返回 400「批量检查已在进行中」。
- 电影订阅补转：known 但未转存的电影文件重新入选转存候选，转存链路失败一次后不再永久漏转。
- 夸克分享探测：目录列表分页拉全（此前每目录只取前 100 项）；达到全局上限标记 `partial` 并提示截断；上限从 200 提升到 500。
- JobStore 归档读写移入阻塞线程池并批量化触发；规则正则进程级缓存；`build_check_details` 每次检查只算一次。
- 订阅列表界面去重：表格/海报视图共用数据驱动操作菜单，顶部三层面板合并，批量操作栏仅在有选中时出现。
- 数据文件损坏隔离后显式告警：启动站内通知 + 诊断接口报告 `quarantined_backup_present`。
- PWA 缓存代次升至 2.2.11；存储 schema 保持兼容，可从 v2.2.10 直接升级。

### 2.2.10

- 订阅「高级规则」界面整理：去掉重复 Tab、修正表单嵌套、合并默认规则与预设卡片、样例输入折叠，仅保留季度预览。
- PWA 缓存代次升至 2.2.10；存储 schema 保持兼容，可从 v2.2.9 直接升级。

### 2.2.9

- 开启「启用自动转存」后，订阅检查发现新文件会自动转存；手动检查默认允许转存。
- Aria2：转存后网盘枚举失败时回退用转存文件 ID 提交下载。
- 魔法匹配去除标题前 emoji/装饰符号；重命名预览仅保留季度折叠列表。
- PWA 缓存代次升至 2.2.9；存储 schema 保持兼容，可从 v2.2.8 直接升级。

更早版本的详细说明见 [CHANGELOG.md](CHANGELOG.md)、[GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 与 `docs/upgrade-v*.md`。

---

## License

[MIT](LICENSE)
