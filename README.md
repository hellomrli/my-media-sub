# My Media Sub

My Media Sub 是一个用 Rust 编写的媒体订阅、资源搜索和夸克网盘自动化工具。它提供轻量 WebUI，用于搜索资源、创建追更订阅、自动转存、按规则重命名、提交 Aria2 下载、生成 STRM 文件，并通过通知中心和推送渠道跟踪关键结果。

## 当前版本

- 版本：`1.0.4`
- 后端：Rust + Axum + Tokio
- 前端：静态 WebUI，入口为 `static/index.html`
- 默认端口：`56001`
- 默认数据目录：`./data`
- 默认登录：`admin` / `change-me`，首次部署必须修改密码
- Docker 镜像：`ghcr.io/hellomrli/my-media-sub`

## 功能概览

| 模块 | 能力 |
| --- | --- |
| 资源搜索 | PanSou 搜索、云盘类型过滤、失效链接过滤、夸克分享探测、一键转存或创建订阅 |
| 订阅追更 | 手动检查、定时检查、起始集数过滤、同集多版本去重、完结判断、完结后恢复追更 |
| 自动转存 | 按媒体类型保存到电影/连续剧/动画/自定义目录，递归定位视频文件，按同季同集兜底匹配 |
| 重命名规则 | 模板变量、正则替换、预设规则、转存前预览、已转存文件命名修复 |
| 元数据 | TMDB 搜索、候选选择、批量刮削、年份/海报/评分/总集数同步 |
| 夸克网盘 | Cookie 健康检测、容量读取、目录浏览、搜索、创建目录、重命名、删除、批量删除、每日签到 |
| Aria2 下载 | RPC 测试、夸克直链换取、Header 注入、提交/暂停/继续/停止/删除任务、下载完成通知 |
| STRM | 转存后生成本地 `.strm`，HTTPStrm 代理播放，支持 Range 请求和 Token 校验 |
| 通知推送 | 企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱 |
| 后台任务 | 转存、刮削、推送派发统一进入任务队列，支持取消、重试、SSE 实时状态 |
| 运维维护 | Basic Auth、敏感配置脱敏、在线更新、Release 校验、轻量指标接口、GHCR 镜像 |

## 快速开始

### Docker Compose

```bash
mkdir -p my-media-sub/data
cd my-media-sub
curl -LO https://raw.githubusercontent.com/hellomrli/my-media-sub/main/docker-compose.yml
docker compose up -d
```

访问：

```text
http://localhost:56001/
```

`docker-compose.yml` 默认使用：

```text
ghcr.io/hellomrli/my-media-sub:1.0.4
```

### Docker Run

```bash
docker run -d \
  --name my-media-sub \
  --restart unless-stopped \
  -p 56001:56001 \
  -v "$(pwd)/data:/app/data" \
  -e SERVER_USERNAME=admin \
  -e SERVER_PASSWORD="replace-with-a-strong-password" \
  -e TZ=Asia/Shanghai \
  ghcr.io/hellomrli/my-media-sub:1.0.4
```

常用镜像标签：

- `latest`：`main` 分支最新稳定构建
- `1.0.4`：当前正式版本
- `1.0`：当前 1.0 系列版本
- `v1.0.4`：Git tag 对应版本

### 二进制部署

从 GitHub Release 下载 Linux x86_64 包：

```bash
curl -LO https://github.com/hellomrli/my-media-sub/releases/download/v1.0.4/my-media-sub-v1.0.4-linux-x86_64.tar.gz
curl -LO https://github.com/hellomrli/my-media-sub/releases/download/v1.0.4/my-media-sub-v1.0.4-linux-x86_64.tar.gz.sha256
sha256sum -c my-media-sub-v1.0.4-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.0.4-linux-x86_64.tar.gz
cd my-media-sub-v1.0.4-linux-x86_64
SERVER_PORT=56001 DATA_DIR=./data ./my-media-sub
```

本地编译：

```bash
cargo build --release --locked
SERVER_PORT=56001 DATA_DIR=./data ./target/release/my-media-sub
```

## 配置

WebUI 的系统设置会持久化到 `DATA_DIR/settings.json`。敏感字段在前端默认脱敏显示，保存脱敏占位符不会覆盖已有真实值。

| 环境变量 | 说明 | 默认值 |
| --- | --- | --- |
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `56001` |
| `SERVER_USERNAME` / `APP_USERNAME` | Basic Auth 用户名 | `admin` |
| `SERVER_PASSWORD` / `APP_PASSWORD` | Basic Auth 密码 | `change-me` |
| `DATA_DIR` | JSON 数据目录 | `./data` |
| `ONLINE_UPDATE_ENABLED` | 是否允许在线替换二进制并重启 | `true` |
| `STRM_TOKEN_IN_URL` | 是否把 HTTPStrm Token 写入生成的 `.strm` URL | `false` |
| `TZ` | 容器时区，影响日志和定时任务显示 | `Asia/Shanghai` |

夸克 Cookie、签到 Cookie、Aria2、STRM、TMDB、PanSou 和推送 token 等业务配置建议部署后在 WebUI 的系统设置中填写。程序仍兼容这些字段的环境变量覆盖，便于自动化部署，但默认部署模板不再暴露大量 secret 变量。

数据文件默认包括：

```text
DATA_DIR/
  settings.json
  subscriptions.json
  notifications.json
  jobs.json
```

现有 JSON 数据无需迁移。读取到损坏 JSON 时，服务会把原文件隔离为 `.corrupt-<timestamp>`，避免静默覆盖。

## 使用流程

1. 登录 WebUI，先在系统设置中修改默认密码。
2. 配置夸克 Cookie、媒体保存目录、PanSou、TMDB、Aria2、STRM 和推送渠道。
3. 在资源搜索中搜索资源，选择直接转存或创建订阅。
4. 在订阅管理中检查更新、编辑规则、预览转存计划、修复命名或补全元数据。
5. 在我的网盘中管理夸克目录，或把文件提交到 Aria2 下载。
6. 在后台任务中查看转存、刮削、推送派发状态；在通知中心查看业务通知和推送结果。

## 订阅与自动转存

连续剧和动画订阅会按季别和集数处理文件：

- 支持 `S01E05`、`EP05`、`第05集`、`[05]`、`178重置版` 等常见集数格式。
- 分享内父目录包含 `第2季`、`Season 2`、`S02` 等信息时，会用于判断当前订阅季。
- `番外`、`剧场版`、`special`、`ova`、`前五季` 等合集提示会避免误转非当前季内容。
- 同一集出现多个视频时，可按清晰度、上传时间、文件大小或最先出现策略保留一个版本。
- 设置起始集数后，低于起始集数的文件会记录为已知文件，但不会通知或自动转存。
- 已完结订阅如果总集数被调大，或元数据刮削得到更大的目标集数，会恢复为追更中。

自动转存需要同时满足：

- 全局“自动下载新订阅项”已开启。
- 全局夸克自动转存已开启。
- 单个订阅没有设置为“仅通知不自动转存”。
- WebUI 中配置的夸克 Cookie 可用。

任一条件不满足时，订阅检查仍会记录新增并按规则通知，但不会创建转存任务。

## 重命名模板

模板支持 `{}` 作为集数占位符，也支持具名变量：

| 变量 | 说明 | 示例 |
| --- | --- | --- |
| `{}` / `{episode}` | 两位集数 | `05` |
| `{episode_number}` | 原始集数数字 | `5` |
| `{title}` | 订阅标题 | `庆余年` |
| `{season}` | 两位季号 | `01` |
| `{season_number}` | 原始季号数字 | `1` |
| `{original}` | 原始文件名去扩展名 | `Show.S01E05` |
| `{name}` | 正则替换后的文件名去扩展名 | `Show.05` |
| `{ext}` | 原扩展名，不含点 | `mp4` |

示例：

```text
{title}.S{season}E{episode}.{ext}
```

输入 `第05集.mkv`，订阅标题为 `庆余年`，第 1 季时，会生成：

```text
庆余年.S01E05.mkv
```

## Aria2 下载

Aria2 RPC URL 可以填写完整地址，例如：

```text
http://192.168.50.100:6800/jsonrpc
```

如果只填写到端口，例如 `http://192.168.50.100:6800`，服务会自动补全 `/jsonrpc`。提交下载时，服务会先通过夸克接口换取临时下载直链，并把必要 Cookie 写入 Aria2 任务 Header。

订阅同步下载未单独填写下载目录时，会按媒体类型使用电影、连续剧、动画或自定义分类目录。对应目录未配置时，不向 Aria2 指定目录，由 Aria2 RPC 端自行决定保存位置。

## STRM 与 HTTPStrm

启用 STRM 后，订阅转存并完成重命名时会在 `STRM_OUTPUT_DIR` 下生成同名 `.strm` 文件。文件内容是本服务的 HTTPStrm 链接，媒体服务器访问该链接时，服务会用当前夸克 Cookie 实时换取临时下载地址并代理 Range 请求。

HTTPStrm Token 支持三种传递方式：

- `Authorization: Bearer <token>`
- `X-HTTPStrm-Token: <token>`
- `.strm` 链接中的 `?token=...`（需要开启兼容模式或设置 `STRM_TOKEN_IN_URL=true`）

`/strm/` 路由不走 Basic Auth，访问控制依赖 STRM Token。默认生成的 `.strm` 链接不携带 token，避免 token 进入代理、播放器或服务访问日志；如播放端无法附加 Header，可显式开启兼容模式。

## 通知与推送

通知中心记录业务通知，并把推送结果合并到同一条通知中，便于查看哪些渠道成功、哪些渠道失败。推送派发会进入后台任务队列，失败渠道会记录脱敏后的错误信息。

支持的推送渠道：

- 企业微信机器人
- Telegram Bot
- WxPusher
- Bark
- Gotify
- PushPlus
- Server 酱

## 后台任务与运维接口

后台任务覆盖转存、批量元数据刮削和推送派发。任务状态支持 SSE 实时更新，前端会自动订阅：

```text
GET /api/jobs/events
```

常用运维接口：

| 接口 | 说明 |
| --- | --- |
| `GET /health` | 健康检查，免 Basic Auth |
| `GET /api/jobs` | 后台任务列表，支持 `offset` / `limit` |
| `GET /api/metrics` | 内存指标快照 |
| `GET /api/update/check` | 检查最新 GitHub Release |
| `GET /api/update/releases` | 列出最近 Release |
| `GET /api/update/progress` | 查询在线更新进度 |
| `POST /api/update/apply` | 下载并应用指定 Release，默认最新版本 |
| `POST /api/update/restart` | 在线更新完成后重启服务 |

在线更新默认启用；如需在受限环境禁用，可设置 `ONLINE_UPDATE_ENABLED=false` 并重启服务。Release 包必须同时提供 `.tar.gz` 和 `.sha256`，校验通过后才会替换二进制和静态资源。

## 开发

```bash
cargo fmt --all -- --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

WebUI 是纯静态资源，部署时需要随二进制一起提供 `static/` 目录。修改 `static/index.html`、`static/app.js` 或 `static/styles.css` 后，不需要 Node 运行时。

本地调试订阅检查时可以使用模拟夸克分享 fixture，避免访问真实网盘：

```bash
MOCK_QUARK_SHARE_FIXTURE=tests/fixtures/mock_quark_share.json cargo run
```

创建订阅时使用 `https://pan.quark.cn/s/mock-show` 可模拟正常分享，使用 `https://pan.quark.cn/s/mock-invalid` 可模拟失效分享。

## 发布流程

普通功能合入 `main` 后，Docker 工作流会构建并推送：

- `ghcr.io/hellomrli/my-media-sub:latest`
- `ghcr.io/hellomrli/my-media-sub:<sha>`

正式发布时更新 `Cargo.toml`、`Cargo.lock` 和 README 版本说明，提交后打 tag：

```bash
git tag v1.0.4
git push origin main
git push origin v1.0.4
```

`v*` 标签会触发：

- Release 工作流：构建 Linux x86_64 二进制包、打包 `static/` 和 README、生成 `.sha256`、发布 GitHub Release。
- Docker 工作流：构建并推送 `v1.0.4`、`1.0.4`、`1.0` 和 SHA 标签镜像。

Release 正文会从 README 的“版本更新”中提取对应版本小节。

## 项目结构

```text
src/
  api/       HTTP API 路由
  clients/   PanSou、夸克、Aria2、共享 HTTP client
  jobs/      后台任务队列、任务状态、worker
  models/    数据模型
  services/  订阅检查、转存、规则、推送、STRM、元数据、签到
  store/     JSON 存储、原子写入、损坏文件隔离
  utils/     通用工具和轻量指标
static/
  index.html WebUI 结构
  app.js     WebUI 交互逻辑
  styles.css WebUI 样式
docs/
  architecture.md 架构说明
tests/
  fixtures/             测试和本地模拟数据
  real_data_compat.rs   真实数据兼容性测试
  subscription_flow.rs  订阅规则集成测试
```

## 架构说明

当前架构仍保持单体服务：

- `AppContext` 统一装配 Store、Service、JobQueue 和后台调度器。
- API 层只负责 HTTP 请求/响应和鉴权后的业务调用。
- 长耗时操作进入 `JobQueue`，由 `JobWorker` 更新任务状态。
- Store 使用 JSON 文件持久化，写入采用原子保存和独立写锁。
- 外部 HTTP 请求复用连接池；夸克动态 Cookie 客户端保留独立实例。

更多结构说明见 [docs/architecture.md](docs/architecture.md)。

## 版本更新

### 1.0.4

- 工作台夸克健康卡片显示今日签到状态，区分今日已签到、今日签到失败、今日待签到和未开启自动，并展示自动签到执行时间。
- 工作台停留期间定时刷新通知和下载任务，自动签到完成后状态不再长期停留在旧值，下载速度也能持续更新。
- 手动签到失败会写入通知中心，失败后工作台卡片可显示“今日签到失败”；只配置签到 Cookie 时也可以直接执行手动签到。
- 我的网盘页面首次进入时自动加载目录列表，减少需要手动刷新才能看到内容的情况。
- 调整 WebUI 明暗主题配色，使工作台和常用状态色更清晰。

### 1.0.3

- 增加跨站状态修改请求拦截，降低已登录浏览器被第三方页面触发后台操作的风险。
- 调整 JSON 存储写入流程，写盘成功后再提交内存状态，避免磁盘写入失败造成运行态与持久化数据不一致。
- 优化后台任务取消语义，已开始执行的转存和推送任务不再被标记为已取消，避免实际副作用与任务状态不一致。
- HTTPStrm 默认不再把 Token 写入生成的 `.strm` URL，新增兼容模式 `STRM_TOKEN_IN_URL=true`。
- 在线更新保留环境变量开关，默认启用；需要受限部署时可设置 `ONLINE_UPDATE_ENABLED=false`。

### 1.0.2

- 修复夸克自动签到调度按容器默认 UTC 执行的问题，改为北京时间调度，并在服务启动或配置重载后自动补签当天错过的签到。
- 网盘健康状态新增已用容量读取，前端显示“已用 / 总容量”和使用百分比。
- 精简 Docker Compose 和 README 快速部署中的环境变量，只保留基础服务、数据目录和时区配置，业务 token 改为推荐部署后在 WebUI 设置。

### 1.0.1

- 修复升级或服务重启后未完成的推送派发任务被恢复执行，导致旧通知再次发送的问题。
- 优化 Aria2 下载完成通知去重逻辑：除 GID 外，额外使用文件名、下载目录和文件大小识别同一下载结果，并参考历史成功推送任务，避免通知中心清空后重复推送。
- 调整订阅定时检查摘要推送策略：只有发现更新、链接失效或订阅完结时才发送摘要，全部无更新时只保留日志。
- 增加队列恢复、下载完成去重和订阅摘要推送策略测试，保持 `cargo test --locked` 与 `cargo clippy --all-targets --locked -- -D warnings` 通过。

### 1.0.0

- 正式发布 1.0 版本，版本号、Docker Compose 示例、Release 下载地址和在线更新说明统一更新到 `1.0.0`。
- 重构 README，移除已删除或过期的工作流描述，重新整理部署、配置、订阅、转存、STRM、推送、后台任务和发布流程。
- 完成 `docs/refactor-plan.md` 中除 Telegram 控制外的重构计划：拆分订阅检查、订阅转存和推送服务大文件，统一推送渠道派发逻辑。
- 优化 JSON Store 写锁粒度，减少列表接口整表克隆，订阅和任务列表支持分页参数。
- 新增共享 HTTP client 池，复用 PanSou、Aria2、TMDB、推送、在线更新和 STRM 代理相关请求连接。
- 新增 `/api/metrics` 轻量指标接口，统计订阅检查、检查失败、转存任务、推送成功和推送失败。
- 后台任务 SSE 已接入 WebUI，任务队列状态变化可实时更新，减少前端轮询。
- 增加订阅规则集成测试，覆盖重命名模板、同集去重、季别过滤和起始集数过滤。
- 清理非测试代码中的裸 `unwrap` / `expect`，并保持 `cargo clippy --all-targets --locked -- -D warnings` 通过。
