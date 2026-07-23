# My Media Sub

> 夸克网盘上的媒体追更工作台：搜索、订阅、转存、重命名、下载与通知，一条链路跑通。

[![CI](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg)](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hellomrli/my-media-sub?display_name=tag)](https://github.com/hellomrli/my-media-sub/releases)
[![Container](https://img.shields.io/badge/GHCR-my--media--sub-blue?logo=docker)](https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

追剧不必反复点开分享链接。My Media Sub 把「发现资源 → 盯住更新 → 自动转存 → 整理命名 → 可选下载 → 及时通知」收成一个可自托管的小服务：Rust + Axum 后端，Cinema Slate 风格的无打包器 WebUI，数据落在本地 JSON，不依赖外部数据库。

单二进制即可运行；Docker 一键拉起。STRM 相关能力在 v2.2.0 起暂时下线，旧字段仍保留，方便以后以独立模块接回。

![架构图](docs/architecture.png)

---

## 它能做什么

| 你想解决的事 | 它怎么做 |
|---|---|
| 自动追更 | 定时或手动检查分享；按季/起始集过滤；同集多版本择优；识别缺集与完结，支持自动恢复 |
| 自动转存 | 幂等写入电影 / 剧集 / 动画目录；规则过滤、模板重命名、批量修复命名 |
| 分享失效 | 失效计数、候选评分、进度校验、冷却与自动换源，全程可回滚审计 |
| 找资源 | PanSou 搜索、夸克分享探测与质量评分、TMDB 元数据（海报、年份、评分、总集数） |
| 看排期 | 上海时区周 / 月 / 列表日历；手动排期或元数据推断；逐集处理状态一目了然 |
| 下到本地 | Aria2 幂等提交与退避重试；可触发 Jellyfin / Emby / Plex / Webhook 刷新媒体库 |
| 心里有数 | 持久化任务队列、真实取消、心跳看门狗、优雅停机、SSE 实时状态 |
| 消息触达 | 企业微信 / Telegram / Bark / Gotify 等；Browser Push；签名 Webhook；安静时段与摘要聚合 |
| 手机遥控 | Telegram Bot：白名单、写操作二次确认、限流与审计 |
| 随时打开 | 深浅主题 WebUI，可安装为 PWA |
| 敢上生产 | 原子 JSON Store、自动备份与隔离验证、关联日志、Prometheus 指标 |

---

## 五分钟跑起来

### 方式一：Docker Compose（推荐）

```bash
mkdir -p my-media-sub/data && cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml

# 同目录写入管理员密码（必需，勿使用 change-me）
printf 'SERVER_PASSWORD=replace-with-a-strong-password\nTZ=Asia/Shanghai\n' > .env
docker compose up -d
```

浏览器打开 `http://服务器地址:56001`，用户名默认 `admin`。

**从 v2.0.0 起，默认密码不可登录。** 必须通过 `SERVER_PASSWORD` / `APP_PASSWORD` 或系统设置配置真实密码。

容器以 uid/gid `1000` 运行；入口脚本会自动修正挂载数据目录属主，旧版本升级一般无需手动 `chown`。

```bash
docker compose ps
docker compose logs -f
docker compose pull && docker compose up -d
docker compose down
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

生产环境请钉死版本标签。每个发布会同时打补丁与次版本标签（例如 `2.2.10` 与 `2.2`）：

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.2.10
docker image inspect ghcr.io/hellomrli/my-media-sub:2.2.10 --format '{{.RepoDigests}}'
```

### 方式三：Linux 二进制

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases) 下载并校验：

```bash
VERSION=v2.2.10
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
  → 同订阅互斥 + 批量并发限制
  → 分享探测 → 规则过滤 → 季度匹配 → 同集择优
  → 检查结果按字段合并回写（不踩并发转存与用户编辑）
  → 幂等 SubscriptionTransfer Job（高 / 中 / 低加权公平）
  → 夸克转存（成功即落盘）→ 重命名 → 可选 Aria2 → 通知
  → AutomationEvent 流水线审计
```

设计上偏「宁可慢一点，也不要悄悄写错」：

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
- STRM 自 v2.2.0 暂时下线，旧字段保留。

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

存储是带 `schema_version` 的 JSON 信封：临时文件写入 → `fsync` → 原子 rename；写盘成功后才替换内存；Unix 上文件权限 `0600`；损坏文件会被隔离。任务裁剪只淘汰终态任务，排队与运行中的不会被顺手清掉。

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

需要：Rust stable（edition 2021）、Node.js（前端测试）。可选 Docker、Tailwind standalone CLI（改样式）、Graphviz（重绘架构图）。

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
docker build -t my-media-sub:2.2.5 -t my-media-sub:latest .
```

前端产物由脚本生成，**不要手改** `static/index.html` / `static/styles.css`：

```bash
node scripts/build-frontend.mjs          # index.html ← 模板 + partials
node scripts/build-frontend.mjs --check
TAILWIND_BIN=/path/to/tailwindcss scripts/build-css.sh
```

---

## 仓库结构（精简）

```text
src/
├── api/           HTTP 路由与响应契约
├── clients/       PanSou / 夸克 / Aria2
├── jobs/          持久化队列与 Handler
├── models/        领域模型
├── services/      检查、转存、换源、日历、通知…
├── store/         原子 JSON Store
└── utils/         时间、文件、指标、脱敏

static/
├── index.tmpl.html + partials/   源模板
├── index.html / styles.css       生成物
└── js/                           core · stores · features
```

---

## 文档索引

- [架构](docs/architecture.md) · [API 契约](docs/api-contract.md) · [自动化事件](docs/automation-events.md)  
- [自动化 API / Token / 导入导出](docs/automation-api.md) · [Telegram Bot](docs/telegram-bot.md)  
- [媒体日历](docs/media-calendar.md) · [资源质量与换源](docs/source-quality.md)  
- [HTTPS 与安全部署](docs/https-reverse-proxy.md) · [PWA](docs/pwa.md)  
- [存储扩展与 SQLite 决策](docs/storage-scaling.md)  
- 当前版本：[v2.2.10 升级指南](docs/upgrade-v2.2.10.md) · 完整变更见 [CHANGELOG.md](CHANGELOG.md)

各版本升级步骤仍在 `docs/upgrade-v*.md`；变更历史统一写在 [CHANGELOG.md](CHANGELOG.md)。

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

### 2.2.10

- 订阅「高级规则」界面整理：去掉重复 Tab、修正表单嵌套、合并默认规则与预设卡片、样例输入折叠，仅保留季度预览。
- PWA 缓存代次升至 2.2.10；存储 schema 保持兼容，可从 v2.2.9 直接升级。

### 2.2.9

- 开启「启用自动转存」后，订阅检查发现新文件会自动转存；手动检查默认允许转存。
- Aria2：转存后网盘枚举失败时回退用转存文件 ID 提交下载。
- 魔法匹配去除标题前 emoji/装饰符号；重命名预览仅保留季度折叠列表。
- PWA 缓存代次升至 2.2.9；存储 schema 保持兼容，可从 v2.2.8 直接升级。

### 2.2.8

- Aria2 同步下载与手动提交：超过单批提交大小时自动分批提交全部文件，不再截断丢弃。
- 设置文案改为「Aria2 单批提交大小」；PWA 缓存代次升至 2.2.8。
- 存储 schema 保持兼容，可从 v2.2.7 直接升级。

### 2.2.7

- Job Worker store 写失败可观测（日志 + `job_store_update_failures`），不再静默丢弃超时/重试状态更新。
- 运行中任务句柄 registry 在 mutex poison 时恢复，避免取消/中止路径 panic。
- Telegram 搜索结果上限 5 → 10；规划与架构文档对齐当前基线。
- PWA 缓存代次与静态资源版本升至 2.2.7。
- 存储 schema 保持兼容，可从 v2.2.6 直接升级。

### 2.2.6

- 多季订阅（`1-4` / `season_spec`）：剧名目录 + 按文件季号自动 `Season N` 转存与 Aria2 下载。
- 剧名清洗、季号解析、预览分组与列表展示字段下沉到 Rust；搜索结果带 `display_title`。
- Telegram 主菜单与 `/search`、`/subscribe`、`/switch`、`/switch_apply`；会话持久化与内联按钮。
- 换源强制不可越过季度不匹配；若干订阅/网盘/预览功能修复。
- 存储 schema 保持兼容，可从 v2.2.5 直接升级。

### 2.2.5

- 修复编辑保存订阅误清空手动排期、网盘批量删除确认失败、一键转存丢失分享密码。
- 网盘列表失败返回真实错误；路径查找只读，不再副作用创建目录。
- 定时调度尊重订阅级检查间隔与星期；手动检查全部仍检查全部启用订阅。
- 订阅目标目录可浏览网盘选择；重命名预览在探测失败时明确提示。
- 搜索 busy 状态、订阅加载失败提示、设置完成度自定义目录识别等前端反馈修复。
- 移除已下线 STRM 入口；OpenAPI 与前端资源版本对齐到 2.2.5。
- 存储 schema 保持兼容，可从 v2.2.4 直接升级。

### 2.2.4

- TMDB 海报、搜索结果和日历缩略图改为由 VPS 同源代理并缓存，浏览器不再直接连接 `image.tmdb.org`。
- 代理路径限定 TMDB 尺寸和图片文件名，并限制响应类型与大小，避免成为通用 SSRF 代理。
- 前端 JS/CSS 资源 URL 与应用版本绑定，旧 Service Worker 或浏览器缓存也无法继续命中旧脚本。
- 存储 schema 保持兼容，可从 v2.2.3 直接升级。

### 2.2.3

- 修复「检查全部订阅」后 Alpine 复用同 ID 图片节点，导致旧失败状态一直把海报藏起来的问题。
- 订阅刷新后清理失败 / 重试 / `hidden` 残留，并为未成功加载的缩略图重新请求。
- PWA 缓存代次提升；schema 兼容，可从 v2.2.2 直接升级。

### 2.2.2

- 修复升级或重新登录后 Service Worker 仍返回旧 JS/CSS、页面与脚本版本错配的问题。
- 远程海报与日历缩略图加载失败时自动退避重试，不再被原生 `hidden` 永久藏起。
- PWA 缓存与应用版本绑定，并补充跨版本静态资源策略测试。
- schema 兼容，可从 v2.2.1 直接升级。

### 2.2.1

- 日历不再根据「未播出占位集」误判为仍在更新。
- 已达目标集数但历史文件名无法解析时，自动转存订阅也能正确完结。
- schema 兼容，可从 v2.2.0 直接升级。

### 2.2.0

- 暂时下线 STRM HTTP 代理、生成 / 审计 API 与 WebUI 配置；保留旧数据字段。
- 转存后处理改为 Rust trait 注册表，媒体库刷新不再硬编码在转存服务里。
- Telegram Bot 支持单订阅与单 Job 详情查询。
- 首屏请求并发化；服务端启用 Brotli / Gzip，并加强静态资源缓存。
- schema 兼容，可从 v2.1.2 直接升级。

### 2.1.2

- WebUI 仅在存在活动或排队中的 Aria2 任务时做 2 秒高频轮询；空闲时不再空转刷 RPC。
- 任务归零后自动停止前台轮询；后台下载监控仍作低频兜底。
- PWA 缓存版本提升；schema 与 OpenAPI 不变，可从 v2.1.1 直接升级。

### 2.1.1

- 目标目录创建失败会终止转存，不再静默落到网盘根目录却报成功。
- Aria2 GID / 文件名 / 完成状态持久化到订阅，通知被清理后状态仍可闭环。
- 修改季数或媒体类型时清理上一季进度，并重算总集数。
- 修复订阅导入 `Idempotency-Key` 并发竞态；关闭队列时同步中止内层业务任务。
- schema 与 OpenAPI 不变，可从 v2.1.0 直接升级。

### 2.1.0

- WebUI「Cinema Slate」视觉重设计：深色炭黑 + 琥珀金，浅色暖纸白 + 深琥珀。
- 卡片扁平化，统一圆角与发丝线边框；纯 CSS token 重写，Alpine 绑定与功能不变。
- 后端与存储契约无变化，可从 v2.0.0 无缝升级。

### 2.0.0

- 安全：拒绝默认密码；限流与 `trust_proxy_headers`；Token 读白名单；固定长度密钥掩码。
- 任务：终态裁剪、SIGTERM 优雅停机、心跳看门狗、取消真正中止。
- 订阅：批量检查按字段合并、跳过中途删除、转存成功立即持久化。
- 通知：摘要跨重启、推送端点容错清理、Telegram 并发、安静时段用主机时区。
- 部署：容器非 root；compose 从 `.env` 读口令；WebUI 模板化组装。
- 保持 `schema_version: 1`，可从 v1.13.x 直接升级。公网实例请先设强密码。

更早的 v1.x 说明见 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases)、[CHANGELOG.md](CHANGELOG.md) 与 `docs/upgrade-v1.*.md`。

---

## License

[MIT](LICENSE)
