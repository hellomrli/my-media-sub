# My Media Sub

> Rust 编写的媒体订阅与夸克网盘自动化工具，提供轻量 WebUI。

[![Release](https://img.shields.io/github/v/release/hellomrli/my-media-sub)](https://github.com/hellomrli/my-media-sub/releases)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue)](https://github.com/hellomrli/my-media-sub/pkgs/container/my-media-sub)
[![License](https://img.shields.io/github/license/hellomrli/my-media-sub)](LICENSE)
[![CI](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml/badge.svg)](https://github.com/hellomrli/my-media-sub/actions/workflows/ci.yml)

搜索资源、创建追更订阅、自动转存到夸克指定目录、按规则重命名、提交 Aria2 下载、生成 STRM 文件，并在分享失效时辅助换源，通过通知中心和7个推送渠道跟踪关键结果。

---

## 功能一览

| 模块 | 能力 |
|---|---|
| 资源搜索 | PanSou 搜索、云盘类型过滤、失效链接过滤、夸克分享探测、一键转存或创建订阅 |
| 订阅追更 | 手动/定时检查、起始集数过滤、同集多版本去重、完结判断与自动恢复、失效链接自动搜索换源 |
| 自动转存 | 按媒体类型保存到电影/连续剧/动画/自定义目录，递归定位视频文件 |
| 重命名规则 | 模板变量、正则替换、预设规则、转存前预览、已转存文件命名修复 |
| 元数据 | TMDB 搜索、候选选择、批量刮削、年份/海报/评分/总集数同步 |
| 夸克网盘 | Cookie 健康检测、容量读取、目录浏览/搜索/创建/重命名/删除、每日自动签到 |
| Aria2 下载 | RPC 测试、夸克直链换取、Header 注入、提交/暂停/继续/停止/删除、后台下载完成通知 |
| STRM | 转存后生成本地 `.strm`，HTTPStrm 代理播放，支持 Range 请求与 Token 校验 |
| 通知推送 | 企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱 |
| 后台任务 | 转存/刮削/推送统一进入任务队列，支持取消、重试、SSE 实时状态 |

---

## 快速部署

### Docker Compose（推荐）

```bash
mkdir -p my-media-sub/data && cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml
docker compose up -d
```

访问 `http://localhost:56001`，默认账号 `admin` / `change-me`，**首次登录后立即修改密码**。

### Docker Run

```bash
docker run -d \
  --name my-media-sub \
  --restart unless-stopped \
  -p 56001:56001 \
  -v "$(pwd)/data:/app/data" \
  -e SERVER_PASSWORD="replace-with-a-strong-password" \
  -e TZ=Asia/Shanghai \
  ghcr.io/hellomrli/my-media-sub:latest
```

常用镜像标签：`latest`（主分支最新）、`1.1.1`（当前稳定版）、`1.1`（1.1 系列）。

### 二进制部署

从 [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases/latest) 下载 Linux x86\_64 包：

```bash
# 替换 VERSION 为实际版本号，例如 v1.1.1
VERSION=v1.1.1
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-${VERSION}-linux-x86_64.tar.gz"
cd "my-media-sub-${VERSION}-linux-x86_64"
SERVER_PORT=56001 DATA_DIR=./data ./my-media-sub
```

本地编译：

```bash
cargo build --release --locked
SERVER_PORT=56001 DATA_DIR=./data ./target/release/my-media-sub
```

---

## 配置

业务配置（夸克 Cookie、Aria2、STRM、TMDB、推送 Token 等）建议部署后在 WebUI 的**系统设置**中填写，这些值会持久化到 `DATA_DIR/settings.json`，支持脱敏显示和安全回填。

以下环境变量用于服务启动参数，不在 WebUI 中管理：

| 环境变量 | 说明 | 默认值 |
|---|---|---|
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `56001` |
| `SERVER_USERNAME` / `APP_USERNAME` | Basic Auth 用户名 | `admin` |
| `SERVER_PASSWORD` / `APP_PASSWORD` | Basic Auth 密码 | `change-me` |
| `DATA_DIR` | JSON 数据目录 | `./data` |
| `STRM_TOKEN_IN_URL` | 把 HTTPStrm Token 写入 `.strm` URL（兼容模式） | `false` |
| `TZ` | 容器时区 | `Asia/Shanghai` |

数据文件：

```
DATA_DIR/
  settings.json       # 应用设置
  subscriptions.json  # 订阅列表
  notifications.json  # 通知历史
  jobs.json           # 后台任务
```

读取到损坏 JSON 时，服务会把原文件隔离为 `.corrupt-<timestamp>`，不会静默覆盖数据。

---

## 使用流程

1. 登录 WebUI → **系统设置** → 修改默认密码。
2. 填写夸克 Cookie、媒体保存目录，按需配置 PanSou、TMDB、Aria2、STRM 和推送渠道。
3. **资源搜索** → 选择资源 → 一键转存，或直接创建订阅。
4. **订阅管理** → 检查更新、编辑规则、预览转存计划、修复命名、补全元数据。
5. **我的网盘** → 管理夸克目录，或把文件提交到 Aria2 下载。
6. **后台任务** 查看任务状态；**通知中心** 查看业务通知和推送结果。

---

## 订阅与自动转存

连续剧和动画按季别、集数处理：

- 支持 `S01E05`、`EP05`、`第05集`、`[05]`、`178重置版` 等常见格式。
- 父目录含 `第2季`、`Season 2`、`S02` 等信息时，用于识别当前订阅季。
- `番外`、`剧场版`、`special`、`ova`、`前五季` 等合集提示会避免误转非当前季内容。
- 同集出现多个视频时，可按清晰度、上传时间、文件大小或最先出现策略保留一个版本。
- 设置起始集数后，低于起始集数的文件记录为已知，但不触发通知或自动转存。
- 已完结订阅如总集数调大或元数据刮削得到更大目标集数，会自动恢复为追更中。

**自动转存需同时满足：**

- 全局"自动下载新订阅项"已开启
- 全局夸克自动转存已开启
- 单个订阅未设为"仅通知不自动转存"
- 配置的夸克 Cookie 可用

任一不满足时，检查仍会记录新增并按规则通知，但不创建转存任务。

---

## 重命名模板

| 变量 | 说明 | 示例输出 |
|---|---|---|
| `{}` / `{episode}` | 两位集数 | `05` |
| `{episode_number}` | 原始集数数字 | `5` |
| `{title}` | 订阅标题 | `庆余年` |
| `{season}` | 两位季号 | `01` |
| `{season_number}` | 原始季号数字 | `1` |
| `{original}` | 原始文件名（去扩展名） | `Show.S01E05` |
| `{name}` | 正则替换后文件名（去扩展名） | `Show.05` |
| `{ext}` | 原扩展名，不含点 | `mp4` |

示例：模板 `{title}.S{season}E{episode}.{ext}`，输入 `第05集.mkv`，标题 `庆余年`，第1季 → `庆余年.S01E05.mkv`

---

## Aria2 下载

Aria2 RPC URL 填写完整地址，例如 `http://192.168.50.100:6800/jsonrpc`，只填到端口时会自动补全 `/jsonrpc`。提交下载时，服务先通过夸克接口换取临时直链，并把必要 Cookie 写入 Aria2 任务 Header。

---

## STRM 与 HTTPStrm

启用 STRM 后，订阅转存并重命名时会在 `STRM_OUTPUT_DIR` 下生成同名 `.strm` 文件。媒体服务器访问 HTTPStrm 链接时，服务实时换取夸克临时下载地址并代理 Range 请求。

Token 支持三种传递方式：

- `Authorization: Bearer <token>`
- `X-HTTPStrm-Token: <token>`
- `.strm` URL 中的 `?token=...`（需设置 `STRM_TOKEN_IN_URL=true`）

`/strm/` 路由不走 Basic Auth，访问控制依赖 STRM Token。

---

## 通知与推送

通知中心记录业务通知，推送结果合并到同一条通知中，便于查看哪些渠道成功、哪些渠道失败。

支持渠道：**企业微信机器人** · **Telegram Bot** · **WxPusher** · **Bark** · **Gotify** · **PushPlus** · **Server 酱**

---

## 后台任务与 API

任务状态支持 SSE 实时更新：

```
GET /api/jobs/events
```

常用接口：

| 接口 | 说明 |
|---|---|
| `GET /health` | 健康检查（免鉴权） |
| `GET /api/jobs` | 后台任务列表，支持 `offset` / `limit` |
| `GET /api/metrics` | 内存指标快照 |
| `GET /api/update/check` | 检查最新 GitHub Release |
| `POST /api/update/apply` | 下载并应用指定 Release（默认最新） |
| `POST /api/update/restart` | 在线更新完成后重启服务 |

在线更新默认启用。Release 包必须同时提供 `.tar.gz` 和 `.sha256`，校验通过后才会替换二进制和静态资源。

---

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --locked
```

WebUI 是纯静态资源（`static/`），无需 Node.js。修改 `index.html`、`app.js` 或 `styles.css` 后直接刷新即可。

本地调试订阅检查可使用 mock fixture，避免访问真实网盘：

```bash
MOCK_QUARK_SHARE_FIXTURE=tests/fixtures/mock_quark_share.json cargo run
```

使用 `https://pan.quark.cn/s/mock-show` 模拟正常分享，`https://pan.quark.cn/s/mock-invalid` 模拟失效分享。

---

## 发布流程

合入 `main` 后，Docker 工作流自动构建并推送 `latest` 和 `<sha>` 镜像。

正式发布：更新 `Cargo.toml` 版本和 README 版本说明，提交后打 tag：

```bash
git tag v1.1.1
git push origin main
git push origin v1.1.1
```

`v*` tag 触发：

- **Release 工作流**：构建 Linux x86\_64 二进制包，打包 `static/` 和 README，生成 `.sha256`，运行测试，发布 GitHub Release。
- **Docker 工作流**：构建并推送 `v1.1.1`、`1.1.1`、`1.1`、SHA 和 `latest` 标签镜像。

Release 正文从本 README 的"版本更新"中自动提取对应版本小节。

---

## 项目结构

```
src/
  api/        HTTP API 路由（订阅/设置/驱动/任务/通知/更新等）
  clients/    PanSou、夸克、Aria2、共享 HTTP client 池
  jobs/       后台任务队列、任务状态、worker
  models/     数据模型（订阅/设置/通知/规则/元数据）
  services/   订阅检查、转存、规则、推送、STRM、元数据、夸克签到
  store/      SettingsStore/SubscriptionStore/NotificationStore，原子写入
  utils/      通用工具和轻量指标
static/
  index.html  WebUI 结构
  app.js      WebUI 交互逻辑（Alpine.js）
  styles.css  WebUI 样式（Tailwind CSS）
tests/
  api_integration.rs       HTTP 层集成测试（鉴权/CRUD/CSRF）
  subscription_flow.rs     订阅规则集成测试
  real_data_compat.rs      序列化兼容性测试
  fixtures/                测试 fixture 和本地模拟数据
docs/
  architecture.md          架构说明
```

---

## 版本更新

### 1.1.1

- 完善失败链接自动换源闭环：订阅失效时自动保存换源候选，WebUI 可手动搜索候选并一键应用新分享链接。
- 应用换源后服务端会立即触发一次订阅检查，不再等待 6 小时定时任务；检查仍遵守订阅和系统自动转存设置，不会强制转存。
- 修复 Aria2 下载完成通知依赖打开网页的问题：新增后台下载完成监控服务，每 15 秒扫描 Aria2 stopped 任务并发送通知/推送。
- Aria2 下载完成通知、订阅同步下载完结判断和下载页刷新共用同一套去重逻辑，避免后台监控和 WebUI 轮询重复通知。
- 修复换源 API 在 Axum 0.8 下的路由参数格式，`/api/subscriptions/{id}/source-candidates/*` 可正常注册。
- WebUI 订阅管理增加“换源”入口和候选弹窗，支持刷新候选、搜索候选、应用候选，并展示换源后立即检查结果。

### 1.1.0

- 新增订阅失效自动换源：失效分享会通过 PanSou 搜索替代源并保存候选。
- 订阅模型新增 `source_candidates`、`last_source_search_time`、`previous_share_links` 字段，兼容旧数据自动补默认值。
- 修复同集重复下载问题，已知集数和已转存集数会参与后续新增文件过滤。

### 1.0.5

- 在线更新功能始终启用，移除 `ONLINE_UPDATE_ENABLED` 环境变量开关。
- 用 `std::sync::LazyLock`（Rust 标准库）替换 `once_cell` 依赖，减少第三方依赖。
- 修复 `src/error.rs` 中冗余双 match，`IntoResponse` 实现合并为单次映射，逻辑更清晰。
- 删除 `src/store/json_store.rs`（从未实例化的泛型基础结构体），消除死代码。
- 新增 `src/lib.rs`，将 binary crate 同时暴露为 library，使 `tests/` 可直接引用应用类型。
- 新增 `tests/api_integration.rs`：12 个 HTTP 层集成测试，覆盖健康检查、Basic Auth（401/200）、CSRF 防护（403）、订阅 CRUD（201/200/404/400/204）、设置读取。
- 为 `real_data_compat.rs` 的三类测试（订阅/设置/通知）补充 `tests/fixtures/` fixture 文件，确保 CI 每次实际执行而非静默跳过。
- 新增 `SETTINGS_ENV_KEYS` 同步性测试，防止 `apply_env_overrides` 新增 env var 时漏更新前置检查数组。
- Release 流水线在构建二进制前加入 `cargo test --locked`，防止 `workflow_dispatch` 手动触发时绕过测试发布。
- 修复 release notes 提取 awk 脚本，兼容 `##` 和 `###` 两种 Markdown 标题格式。
- `docker-compose.yml` 镜像 tag 改为 `latest`，不再硬编码版本号。
- 删除根目录残留的本地构建产物（`.tar.gz` / `.sha256`）。

### 1.0.4

- 工作台夸克健康卡片显示今日签到状态，区分今日已签到、今日签到失败、今日待签到和未开启自动，并展示自动签到执行时间。
- 工作台停留期间定时刷新通知和下载任务，自动签到完成后状态不再长期停留在旧值，下载速度也能持续更新。
- 手动签到失败会写入通知中心，失败后工作台卡片可显示"今日签到失败"；只配置签到 Cookie 时也可以直接执行手动签到。
- 我的网盘页面首次进入时自动加载目录列表，减少需要手动刷新才能看到内容的情况。
- 调整 WebUI 明暗主题配色，使工作台和常用状态色更清晰。

### 1.0.3

- 增加跨站状态修改请求拦截，降低已登录浏览器被第三方页面触发后台操作的风险。
- 调整 JSON 存储写入流程，写盘成功后再提交内存状态，避免磁盘写入失败造成运行态与持久化数据不一致。
- 优化后台任务取消语义，已开始执行的转存和推送任务不再被标记为已取消。
- HTTPStrm 默认不再把 Token 写入生成的 `.strm` URL，新增兼容模式 `STRM_TOKEN_IN_URL=true`。

### 1.0.2

- 修复夸克自动签到调度按容器默认 UTC 执行的问题，改为北京时间调度，并在服务启动或配置重载后自动补签当天错过的签到。
- 网盘健康状态新增已用容量读取，前端显示"已用 / 总容量"和使用百分比。
- 精简 Docker Compose 和快速部署中的环境变量，只保留基础服务、数据目录和时区配置。

### 1.0.1

- 修复升级或服务重启后未完成的推送派发任务被恢复执行，导致旧通知再次发送的问题。
- 优化 Aria2 下载完成通知去重逻辑：额外使用文件名、下载目录和文件大小识别同一下载结果。
- 调整订阅定时检查摘要推送策略：只有发现更新、链接失效或订阅完结时才发送摘要。

### 1.0.0

- 正式发布 1.0 版本。
- 完成订阅检查、订阅转存和推送服务大文件拆分，统一推送渠道派发逻辑。
- 新增共享 HTTP client 池、`/api/metrics` 指标接口、后台任务 SSE 实时更新。
- 新增订阅规则集成测试，覆盖重命名模板、同集去重、季别过滤和起始集数过滤。
