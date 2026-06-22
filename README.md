# My Media Sub

媒体订阅、资源搜索和夸克网盘转存管理工具。项目提供一个轻量 WebUI，用于搜索夸克资源、创建追更订阅、自动转存、按规则重命名，并可把重命名后的文件提交到 Aria2 下载。

适合的使用方式：

- 用 PanSou 搜索资源，直接转存或创建追更订阅。
- 对连续剧、动画按季度和集数持续检查更新。
- 将夸克网盘中的文件规范保存到电影、连续剧、动画目录。
- 通过 Aria2 把转存后的最终文件同步下载到指定目录。
- 用 TMDB 补全海报、评分、年份和季度集数信息。

## 当前版本

- 版本：`0.8.14`
- 后端：Rust + Axum + Tokio
- 前端：静态 WebUI，入口为 `static/index.html`，交互逻辑在 `static/app.js`
- 数据目录：默认 `./data`，可通过 `DATA_DIR` 修改
- 默认端口：`56001`
- 默认登录：`admin` / `change-me`

## 主要功能

| 模块 | 能力 |
| --- | --- |
| 资源搜索 | PanSou 搜索、失效链接过滤、夸克文件嗅探、搜索结果一键转存或订阅 |
| 订阅管理 | 手动/定时检查更新、从指定集数开始追更、首次创建后自动检查并提交转存任务 |
| 自动转存 | 电影保存到 `片名（年份）`，连续剧和动画保存到 `剧名（年份）/Season X` |
| 重命名 | 模板命名、正则替换、变量识别、重命名预览、已转存文件命名修复 |
| 元数据 | TMDB 自动匹配、手动候选选择、批量刮削、海报/评分/年份/集数补全 |
| 网盘管理 | 文件管理器式浏览夸克目录，支持面包屑、搜索、筛选、列表/网格、新建、重命名、删除、批量删除、每日签到、发送到 Aria2 和短时目录缓存 |
| 下载任务 | 查看 Aria2 活动、排队和最近结束任务，支持暂停、停止、删除、RPC 连接测试和下载完成通知 |
| STRM | 订阅转存后生成本地 `.strm` 文件，提供带 Token 的 HTTPStrm 播放链接 |
| 通知推送 | 企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱 |
| 系统维护 | Basic Auth、敏感配置星号显示、在线更新、Docker 镜像发布 |

## 快速开始

### Docker Compose

```bash
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub
docker compose up -d
```

访问：

```text
http://localhost:56001/
```

### Docker Run

```bash
docker run -d \
  --name my-media-sub \
  -p 56001:56001 \
  -v "$(pwd)/data:/app/data" \
  -e SERVER_USERNAME=admin \
  -e SERVER_PASSWORD=change-me \
  -e QUARK_COOKIE="your_quark_cookie" \
  ghcr.io/hellomrli/my-media-sub:latest
```

### 本地运行

```bash
cargo build --release
SERVER_PORT=56001 DATA_DIR=./data ./target/release/my-media-sub
```

开发调试：

```bash
cargo run
```

## 配置

| 环境变量 | 说明 | 默认值 |
| --- | --- | --- |
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `56001` |
| `SERVER_USERNAME` | Basic Auth 用户名 | `admin` |
| `SERVER_PASSWORD` | Basic Auth 密码 | `change-me` |
| `DATA_DIR` | 数据目录 | `./data` |
| `QUARK_COOKIE` | 夸克网盘 Cookie | 空 |
| `QUARK_SIGNIN_COOKIE` | 夸克签到参数，可填写移动端 Cookie，或直接填写抓包到的 `drive-m.quark.cn/.../act/growth/reward` 请求 URL；留空时使用 `QUARK_COOKIE` | 空 |
| `QUARK_SIGNIN_ENABLED` | 是否启用夸克每日自动签到 | `false` |
| `QUARK_SIGNIN_HOUR` | 夸克每日自动签到小时，范围 `0`-`23` | `8` |
| `WECOM_BOT_URL` | 企业微信机器人地址 | 空 |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | 空 |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | 空 |
| `ARIA2_RPC_URL` | Aria2 JSON-RPC 地址 | 空 |
| `ARIA2_SECRET` | Aria2 RPC Secret | 空 |
| `ARIA2_MOVIE_DIR` | Aria2 电影下载目录，订阅未单独填写下载目录时使用 | 空 |
| `ARIA2_SERIES_DIR` | Aria2 连续剧下载目录，订阅未单独填写下载目录时使用 | 空 |
| `ARIA2_ANIME_DIR` | Aria2 动画下载目录，订阅未单独填写下载目录时使用 | 空 |
| `STRM_ENABLED` | 是否启用 STRM 文件生成 | `false` |
| `STRM_OUTPUT_DIR` | STRM 文件输出根目录，需挂载到媒体服务器可扫库的位置 | 空 |
| `STRM_PUBLIC_BASE_URL` | HTTPStrm 对外访问地址，例如 `http://192.168.50.10:56001` | 空 |
| `STRM_ACCESS_TOKEN` | HTTPStrm 访问 Token，留空会自动生成并保存 | 自动生成 |
| `PANSOU_API_URL` | PanSou API 地址，WebUI 中会脱敏显示 | 内置默认 |
| `TMDB_API_KEY` | TMDB API Key，用于元数据搜索和刮削 | 空 |
| `TMDB_LANGUAGE` | TMDB 返回语言 | `zh-CN` |

## Aria2 下载说明

“我的网盘”发送文件到 Aria2 或订阅开启同步下载时，服务端会通过夸克 PC 下载接口获取临时直链，并把夸克 Cookie 与下载接口返回的临时 Cookie 一起写入 Aria2 任务 Header。

Aria2 RPC URL 可以填写完整地址，例如 `http://192.168.50.100:6800/jsonrpc`。如果只填写 `http://192.168.50.100:6800`，服务会自动补全 `/jsonrpc`。订阅同步下载未单独填写下载目录时，会按媒体类型使用电影、连续剧、动画或自定义类别对应的 Aria2 目录；对应目录未配置时不向 Aria2 指定目录，由 Aria2 RPC 端自行决定保存位置。

如果下载失败并提示 `download file size limit[...]`、`require login [auth expired]` 或类似鉴权错误，优先在“系统设置”中更新夸克 Cookie 后重试，并确认 Aria2 所在机器可以访问夸克下载服务。

## STRM 与 HTTPStrm

在“系统设置”中启用 STRM 后，配置本地输出目录和 HTTPStrm 访问地址；创建或编辑订阅时再开启“转存后生成 STRM 文件”。订阅转存并重命名完成后，会在输出目录下按夸克保存目录结构生成同名 `.strm` 文件，内容为本服务的 HTTP 链接。

HTTPStrm 链接形如 `/strm/quark/{fid}/{file_name}?token=...`。媒体服务器访问该链接时，服务会用当前夸克 Cookie 实时换取临时下载地址并代理 Range 请求，因此客户端不需要持有夸克 Cookie。`/strm/` 路由不走 Basic Auth，访问控制依赖 STRM Token，请不要把 Token 暴露到不可信环境。

## 使用流程

1. 登录 WebUI。
2. 在“系统设置”中配置夸克 Cookie、保存目录、Aria2、STRM、推送渠道和自动检查周期。
3. 在“资源搜索”中搜索资源，可选择“转存”或“订阅”。
4. 在“订阅管理”中检查订阅、编辑订阅规则、补全元数据或对已有文件执行“修复命名”。
5. 在“我的网盘”中浏览、创建文件夹、重命名、删除文件，或将文件发送到 Aria2 下载。
6. 在“下载任务”中查看 Aria2 实时下载进度、速度和保存目录。

创建或编辑连续剧/动画订阅时，可设置“从第几集开始转存”。低于起始集数的新增文件会记录为已知文件，但不会通知或自动转存，适合原订阅失效后更换分享链接继续追更。

启用订阅定时检查后，每轮自动检查完成会按“订阅更新”推送开关发送汇总推送，包含本次有更新、无更新、失效和完结的订阅概况。

订阅创建后的首次检查会直接提交本次新增文件的转存任务。后续定时或手动检查的自动转存需要同时满足三个条件：“自动下载新订阅项”已开启、夸克“启用自动转存”已开启、单个订阅没有勾选“仅通知不自动转存”。任一条件不满足时，订阅检查仍会记录新增文件并发送更新通知，但不会创建自动转存任务。

创建或编辑订阅时可开启“同步用Aria2下载”，并指定同步下载目录。留空时按媒体类型使用系统设置中的 Aria2 分类目录，未配置分类目录时不指定目录；订阅检查发现更新并完成转存、重命名后，会把最终文件提交到 Aria2。

创建或编辑订阅时可开启“转存后生成 STRM 文件”。已有订阅可在订阅卡片点击“生成 STRM”补齐当前目标目录中的视频文件。

高级设置中的 PanSou API URL、夸克 Cookie、夸克签到 Cookie 和推送 Token 会按敏感配置处理：WebUI 默认显示等长星号，点击显示按钮后才读取明文；保留星号保存不会覆盖已有真实配置。修改 PanSou API URL 后需要重启服务才会切换搜索客户端。

## 版本更新

### 0.8.14

- 夸克签到参数兼容整段移动端 `reward` 请求 URL，可直接粘贴抓包链接解析 `kps`、`sign`、`vcode`。
- 系统设置中的签到参数说明补充抓包 URL 填写方式，减少手动拆参数出错。

### 0.8.13

- 检查周期保存和调度启动统一使用后端归一化逻辑，覆盖页面预设周期和异常 API 输入，避免再次生成非法调度配置。
- 完善默认重命名模板变量说明，逐项解释变量含义并增加示例。

### 0.8.12

- 在线更新改为弹窗展示升级进度，替换完成后由用户点击“重启服务并刷新页面”生效。
- 新增 `POST /api/update/restart` 接口，在线升级完成后服务恢复可由前端自动检测并刷新当前页面。
- 订阅定时检查改用重复任务调度，避免 60 分钟以上周期生成非法 cron 表达式导致服务启动失败；后台调度启动失败会记录日志但不再阻断 Web 服务启动。

### 0.8.11

- 订阅检查和自动转存新增同集重复视频保留策略，默认保留清晰度最高版本，也可选择上传时间最新、文件最大或最先出现。
- 同一集的多版本视频会在检查结果中标记为同集跳过，并按集数写入转存去重键，避免后续重复提交转存和 Aria2 下载。
- 系统设置新增默认重命名模板，新建连续剧/动画订阅时可直接套用，并在界面展示支持的重命名变量。

### 0.8.10

- 新增独立夸克签到 Cookie 配置，可在“系统设置 > 夸克网盘 > 自动签到”中填写移动端 Cookie；留空时继续使用夸克网盘 Cookie。
- 订阅定时检查完成后会发送本轮检查汇总推送，列出有更新、无更新、失效和完结的订阅概况。
- 定时夸克签到失败会写入通知中心，并按“夸克签到”推送开关发送失败推送。

### 0.8.9

- 新增夸克网盘自动签到，可在“系统设置 > 夸克网盘”中启用每日定时签到，也可手动立即签到。
- 签到成功后写入通知中心，并可通过“消息推送 > 夸克签到”开关发送推送通知。
- 新增 `QUARK_SIGNIN_ENABLED`、`QUARK_SIGNIN_HOUR` 环境变量和 `POST /api/quark/signin` 手动签到接口。

### 0.8.8

- 修复 Docker 环境中 `SERVER_USERNAME`/`SERVER_PASSWORD` 每次启动都会覆盖 WebUI 已保存账号密码的问题。环境变量现在只在本地配置仍为空或默认值时用于初始化登录凭据。

### 0.8.7

- 修复 GitHub Release 在线更新包在 Docker/bookworm 环境中可能要求 `GLIBC_2.39` 导致服务重启失败的问题，Release 二进制改为在 Ubuntu 22.04 环境构建。
- 在线更新现在会同步替换 WebUI 静态资源目录，避免只更新后端二进制而前端页面仍停留在旧版本。

### 0.8.6

- 优化“我的网盘”界面为文件管理器式体验，新增面包屑导航、搜索、分类筛选、排序、列表/网格视图、渐进渲染和更清晰的批量操作。
- 提升夸克目录加载性能：后端目录列表改为分页拉取并加入短时缓存，目录创建、删除、重命名后自动清理缓存。
- 新增 Aria2 下载任务总体暂停/停止，以及单任务暂停、继续、停止、删除操作。
- 消息推送新增“下载完成”事件，Aria2 完成任务会记录通知并按开关推送。
- 订阅管理改为追更中、已失效、已完结三个标签页，并把批量刮削入口改为批量检查。
- 改进集数识别，支持 `129 4K.mp4`、`23(1).mp4`、`S01E144...` 等文件名，并在下载完成后根据已下载集数自动标记订阅完结。

### 0.8.5

- 修复 Docker 镜像运行阶段 glibc 版本不匹配导致容器启动失败的问题，构建阶段固定使用 bookworm Rust 镜像。

### 0.8.4

- 新增 STRM/HTTPStrm：订阅转存后可生成 `.strm` 文件，并通过带 Token 的 HTTP 链接实时代理夸克文件播放，支持 Range 请求。
- Aria2 下载目录改为按媒体类型配置电影、连续剧、动画和自定义分类目录；订阅表单支持填写下载地址并从 Aria2 目录浏览选择。
- 订阅弹窗按功能拆分为“订阅内容”“重命名”“下载/STRM”三个标签页，TMDB Key 未配置时隐藏刮削匹配区域。
- 在线更新升级完成后自动重启服务，并新增升级进度百分比、阶段文案和下载进度查询接口。
- 清理设置页旧的夸克根目录和默认 Aria2 下载目录入口，README 与示例环境变量同步为当前配置项。

### 0.8.3

- 美化 README 项目说明、功能分组和使用流程，让部署、配置、订阅和 Aria2 同步下载说明更清晰。
- 清理历史 GitHub Release 正文，改为使用 README 中对应版本的更新内容，移除自动生成的比较链接。
- 修复 Release 工作流提取版本说明时可能带入后续 README 章节的问题。

### 0.8.2

- 设置页敏感配置改为等长星号显示，支持点击按钮临时查看明文，并避免保存时用星号覆盖真实配置。
- 浏览器后退键改为在 WebUI 内部页面间后退，避免直接离开应用。
- 订阅创建时新增资源名称智能识别，自动去除年份、清晰度、字幕和全集等资源后缀，提升元数据刮削和目录命名准确度。
- 新增 Aria2 连接测试按钮，并自动兼容裸端口 RPC URL。
- 精简资源搜索高级筛选，移除重复的链接检测选项。
- 订阅管理页只保留订阅卡片，检查结果移动到转存历史页。
- 修复创建订阅后的首次检查没有触发转存的问题，首次检查会提交本次新增文件的转存任务。
- Release 工作流改为从 README 对应版本小节生成发布说明，不再使用 GitHub 自动比较链接。

### 0.8.1

- 修复订阅自动转存遇到分享外层目录时可能转存父目录的问题，现在会递归定位本次新增的视频文件，并按订阅重命名规则保存到指定目录。
- 优化订阅创建后的提示文案，明确创建后先检查更新，是否自动转存由全局自动化设置和订阅规则共同决定。

### 0.8.0

- 新增订阅手动元数据刮削，可在候选结果中手动选择 TMDB 匹配项，避免自动刮削选错。
- 新增订阅级 Aria2 同步下载，自动转存并重命名后可将最终文件提交到指定下载目录。
- 设置页移除 NAS 同步配置，保留旧配置文件的兼容读取。
- 在线更新页简化为版本状态、当前版本改动和一键升级；升级会替换当前运行环境中的 `my-media-sub` 二进制文件并自动重启服务。
- 修复 CI clippy 门禁问题，补充环境变量覆盖测试，支持 `PANSOU_API_URL` 独立覆盖。

### 0.7.15

- 引入 GitHub Release 在线更新检查，展示最新版本、Release 文件和部署信息。
- 完善订阅、任务、推送和真实数据兼容性测试。
- 拆分 WebUI 静态资源和发布工作流，支持 GitHub Actions 自动构建二进制包与 Docker 镜像。

## 保存目录规则

- 电影：保存到电影分类目录下的 `片名（年份）`，例如 `/电影/沙丘（2021）`。
- 连续剧：保存到连续剧分类目录下的 `剧名（年份）/Season X`，例如 `/连续剧/庆余年（2019）/Season 1`。
- 动画：保存到动画分类目录下的 `动画名（年份）/Season X`，例如 `/动画/孤独摇滚！（2022）/Season 1`。
- 自定义目录：电影直接保存到该目录；连续剧和动画会在该目录下追加 `Season X`，如果目录末尾已经是 `Season X` 则不会重复追加。

## 重命名模板

订阅模板可使用 `{}` 占位符。连续剧和动画季号默认是 `1`，可在订阅表单手动修改；例如：

```text
庆余年.S01E{}
```

文件名中识别到 `EP05`、`S01E05` 或 `第05集` 后，会生成：

```text
庆余年.S01E05.mp4
```

如果需要随订阅季号变化，可使用：

```text
庆余年.S{season}E{}
```

季号为 `2` 时会生成 `庆余年.S02E05.mp4`。

电影不使用季号目录，默认重命名模板为片名本身。

如果文件已经转存但没有被命名，可在订阅卡片点击“修复命名”。

高级重命名支持正则替换和变量：

- `{}`：补零集数，例如 `05`
- `{title}`：订阅标题
- `{season}`：两位季号，例如 `02`
- `{season_number}`：原始季号，例如 `2`
- `{episode}`：两位集数，例如 `05`
- `{episode_number}`：原始集数，例如 `5`
- `{original}`：原始文件名（不含扩展名）
- `{name}`：正则替换后的文件名（不含扩展名）
- `{ext}`：扩展名（不含点）

## API 概览

### 健康检查

- `GET /health`

### 设置

- `GET /api/settings`
- `POST /api/settings`

### 搜索与转存

- `POST /api/search`
- `POST /api/transfer`：创建后台转存任务并返回 `job_id`

### 任务

- `GET /api/jobs`
- `GET /api/jobs/{id}`
- `POST /api/jobs/{id}/cancel`
- `POST /api/jobs/{id}/retry`
- `GET /api/jobs/events`：SSE 任务进度事件流
- 任务类型：`manual_transfer`、`subscription_transfer`、`metadata_scrape`、`push_dispatch`

### 元数据

- `GET /api/metadata/search?query={title}&media_type={type}`

### 订阅

- `GET /api/subscriptions`
- `POST /api/subscriptions`
- `POST /api/subscriptions/rename-preview`：预览订阅规则的匹配、跳过和重命名结果
- `GET /api/subscriptions/{id}`
- `PUT /api/subscriptions/{id}`
- `DELETE /api/subscriptions/{id}`
- `POST /api/subscriptions/{id}/check`
- `POST /api/subscriptions/{id}/rename-existing`
- `POST /api/subscriptions/{id}/strm`：按订阅目标目录中的已有视频补齐 STRM 文件
- `POST /api/subscriptions/{id}/metadata/scrape`：后台刮削单个订阅元数据
- `POST /api/subscriptions/check`
- `POST /api/subscriptions/metadata/scrape`：后台批量刮削订阅元数据

### 通知

- `GET /api/notifications`
- `POST /api/notifications/{id}/read`
- `POST /api/notifications/read-all`
- `POST /api/notifications/clear`

### 夸克网盘

- `POST /api/quark/test`
- `POST /api/quark/signin`：立即执行夸克签到，成功后写入通知中心并按推送设置发送通知
- `GET /api/drive?fid={fid}`
- `GET /api/drive/find-path?path={path}`
- `POST /api/drive/mkdir`
- `POST /api/drive/delete`
- `POST /api/drive/rename`
- `POST /api/drive/aria2`：获取夸克文件临时直链并提交到 Aria2
- `GET /api/drive/aria2/tasks`：查询 Aria2 当前、排队和最近结束任务
- `POST /api/drive/aria2/tasks/pause-all`：暂停全部下载任务
- `POST /api/drive/aria2/tasks/stop-all`：停止全部活动和排队下载任务
- `POST /api/drive/aria2/tasks/{gid}/pause|resume|stop|delete`：控制单个 Aria2 下载任务

### HTTPStrm

- `GET /strm/quark/{fid}/{file_name}?token={token}`：获取夸克文件的 HTTPStrm 播放/下载流，支持 Range 请求

### 推送

- `POST /api/push/test`
- 支持按事件开关控制推送：订阅更新、订阅失效、订阅完结、转存完成、下载完成。

### 在线更新

- `GET /api/update/check`：检查 GitHub Release 最新版本和版本改动
- `GET /api/update/progress`：查询当前在线升级进度、阶段和百分比
- `POST /api/update/apply`：下载最新 Linux x86_64 Release 包，替换当前运行环境中的 `my-media-sub` 二进制文件和 WebUI 静态资源
- `POST /api/update/restart`：在线升级替换完成后重启服务，前端会等待 `/health` 恢复后刷新当前页面

## 开发

```bash
cargo check
cargo test
cargo build
```

前端目前没有单独构建步骤，WebUI 直接由 `static/index.html` 提供。

本地调试订阅检查时可以使用模拟夸克分享 fixture，避免访问真实网盘：

```bash
MOCK_QUARK_SHARE_FIXTURE=tests/fixtures/mock_quark_share.json cargo run
```

创建订阅时使用 `https://pan.quark.cn/s/mock-show` 可模拟正常分享，使用 `https://pan.quark.cn/s/mock-invalid` 可模拟失效分享。启用该环境变量后，未在 fixture 中声明的分享链接会按失效处理。

## 发布流程

普通功能提交直接推送到 `main`，GitHub Actions 会构建并推送 `ghcr.io/hellomrli/my-media-sub:latest` 镜像。需要发布新版本时按下面顺序执行：

```bash
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked -- -D warnings
cargo test --locked
node --check static/app.js
```

确认通过后更新 `Cargo.toml` 和 README 中的版本号，并把本次变更整理到 README“版本更新”的对应新版本小节。提交到 `main` 后创建并推送版本标签：

```bash
git tag v0.8.0
git push origin main
git push origin v0.8.0
```

`v*` 标签会触发 Release 工作流，自动编译 Linux x86_64 二进制包、打包 `static/` 和 README，并上传 `.tar.gz` 与 `.sha256` 到 GitHub Release。Release 正文会从 README“版本更新”中匹配当前标签对应的小节；如果没有找到对应版本小节，工作流会失败，避免生成自动比较链接。Docker 工作流会同时构建并推送 `latest`、版本号和 SHA 标签镜像。

## 项目结构

```text
src/
  api/       HTTP API 路由
  clients/   PanSou、夸克、转存客户端
  models/    数据模型
  services/  订阅检查、转存、重命名、推送等业务逻辑
  store/     JSON 数据存储
static/
  index.html WebUI 结构
  app.js     WebUI 交互逻辑
  styles.css WebUI 样式
tests/
  fixtures/             测试和本地模拟数据
  real_data_compat.rs
```

## License

MIT
