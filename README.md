# My Media Sub

My Media Sub 是一个用 Rust 编写的媒体订阅、资源搜索和夸克网盘转存管理工具。它提供轻量 WebUI，用于搜索资源、创建追更订阅、自动转存、按规则重命名，并可把最终文件提交到 Aria2 下载或生成 STRM 文件供媒体服务器刮库。

## 当前版本

- 版本：`0.9.8`
- 后端：Rust + Axum + Tokio
- 前端：静态 WebUI，入口为 `static/index.html`
- 默认端口：`56001`
- 默认数据目录：`./data`
- 默认登录：`admin` / `change-me`（首次部署请立即修改密码）
- Docker 镜像：`ghcr.io/hellomrli/my-media-sub`

## 适合做什么

- 通过 PanSou 搜索资源，直接转存或创建订阅。
- 对连续剧、动画按季度和集数持续追更。
- 自动把夸克文件保存到电影、连续剧、动画目录，并按模板重命名。
- 订阅转存后同步提交 Aria2 下载，或生成 `.strm` 文件给 Emby/Jellyfin/Plex 扫库。
- 用 TMDB 补全海报、评分、年份、季度和集数信息。
- 通过后台日志跟踪转存、元数据刮削和推送派发，通过通知中心跟踪用户通知和推送记录。

## 主要功能

| 模块 | 能力 |
| --- | --- |
| 资源搜索 | PanSou 搜索、失效链接过滤、夸克分享探测、一键转存或订阅 |
| 订阅管理 | 手动/定时检查、从指定集数开始追更、完结判断、完结后自动恢复追更 |
| 自动转存 | 按媒体类型保存目录，递归定位视频文件，按集数去重和补转 |
| 重命名 | 规则中心、模板命名、正则替换、重命名预览、已转存文件命名修复 |
| 元数据 | TMDB 自动匹配、手动候选选择、批量刮削、集数自动同步 |
| 网盘管理 | 夸克账号健康、目录浏览、搜索、筛选、新建、重命名、删除、批量删除、每日签到 |
| 下载任务 | Aria2 RPC 测试、提交下载、暂停/继续/停止/删除、下载完成通知 |
| STRM | 订阅转存后生成本地 `.strm`，通过带 Token 的 HTTPStrm 代理播放 |
| 通知推送 | 企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱 |
| 系统维护 | Basic Auth、敏感配置脱敏、在线更新、GitHub Release、GHCR 镜像 |

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
  -e SERVER_PASSWORD="replace-with-a-strong-password" \
  -e QUARK_COOKIE="your_quark_cookie" \
  ghcr.io/hellomrli/my-media-sub:0.9.8
```

可用镜像标签：

- `latest`：main 分支最新构建
- `0.9.8`：当前稳定版本
- `0.9`：当前 0.9 系列版本
- `v0.9.8`：Git tag 对应版本

### 二进制部署

从 GitHub Release 下载 Linux x86_64 包：

```bash
curl -LO https://github.com/hellomrli/my-media-sub/releases/download/v0.9.8/my-media-sub-v0.9.8-linux-x86_64.tar.gz
tar -xzf my-media-sub-v0.9.8-linux-x86_64.tar.gz
cd my-media-sub-v0.9.8-linux-x86_64
SERVER_PORT=56001 DATA_DIR=./data ./my-media-sub
```

本地编译：

```bash
cargo build --release --locked
SERVER_PORT=56001 DATA_DIR=./data ./target/release/my-media-sub
```

## 配置

| 环境变量 | 说明 | 默认值 |
| --- | --- | --- |
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `56001` |
| `SERVER_USERNAME` | Basic Auth 用户名 | `admin` |
| `SERVER_PASSWORD` | Basic Auth 密码，首次部署请改为强密码 | `change-me` |
| `DATA_DIR` | 数据目录 | `./data` |
| `QUARK_COOKIE` | 夸克网盘 Cookie | 空 |
| `QUARK_SIGNIN_COOKIE` | 夸克签到参数，可填写移动端 Cookie 或抓包到的 `drive-m.quark.cn/.../act/growth/reward` 请求 URL | 空 |
| `QUARK_SIGNIN_ENABLED` | 是否启用夸克每日自动签到 | `false` |
| `QUARK_SIGNIN_HOUR` | 每日自动签到小时，范围 `0`-`23` | `8` |
| `PANSOU_API_URL` | PanSou API 地址，WebUI 中会脱敏显示 | 内置默认 |
| `TMDB_API_KEY` | TMDB API Key，用于元数据搜索和刮削 | 空 |
| `TMDB_LANGUAGE` | TMDB 返回语言 | `zh-CN` |
| `ARIA2_RPC_URL` | Aria2 JSON-RPC 地址 | 空 |
| `ARIA2_SECRET` | Aria2 RPC Secret | 空 |
| `ARIA2_MOVIE_DIR` | Aria2 电影下载目录 | 空 |
| `ARIA2_SERIES_DIR` | Aria2 连续剧下载目录 | 空 |
| `ARIA2_ANIME_DIR` | Aria2 动画下载目录 | 空 |
| `STRM_ENABLED` | 是否启用 STRM 文件生成 | `false` |
| `STRM_OUTPUT_DIR` | STRM 文件输出根目录 | 空 |
| `STRM_PUBLIC_BASE_URL` | HTTPStrm 对外访问地址，例如 `http://192.168.50.10:56001` | 空 |
| `STRM_ACCESS_TOKEN` | HTTPStrm 访问 Token，留空会自动生成并保存 | 自动生成 |
| `MY_MEDIA_SUB_ENABLE_SELF_UPDATE` | 是否允许在线更新替换服务二进制，需显式设为 `1` 才能执行更新/重启 | `false` |
| `WECOM_BOT_URL` | 企业微信机器人地址 | 空 |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | 空 |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | 空 |

WebUI 的系统设置会持久化到 `DATA_DIR/settings.json`。敏感配置默认显示为等长星号，保存星号不会覆盖已有真实值。

## 使用流程

1. 登录 WebUI。
2. 在“系统设置”中配置夸克 Cookie、保存目录、Aria2、STRM、TMDB 和推送渠道。
3. 在“资源搜索”中搜索资源，可选择“转存”或“订阅”。
4. 在“订阅管理”中检查订阅、编辑规则、补全元数据或修复已有文件命名。
5. 在“我的网盘”中浏览、管理夸克目录，或把文件发送到 Aria2 下载。
6. 在“后台日志”中跟踪转存、刮削和推送派发，在“下载任务”和“通知中心”中跟踪下载与用户通知。

创建或编辑连续剧/动画订阅时，可设置“从第几集开始转存”。低于起始集数的文件会记录为已知文件，但不会通知或自动转存，适合分享链接失效后更换链接继续追更。

订阅创建后的首次检查会直接提交本次新增文件的转存任务。后续自动转存需要同时满足：“自动下载新订阅项”已开启、夸克“启用自动转存”已开启、单个订阅没有勾选“仅通知不自动转存”。任一条件不满足时，订阅检查仍会记录新增并发送通知，但不会创建转存任务。

## 订阅完结和补转

0.9.0 改进了完结状态和补转行为：

- 已标记完结的订阅，如果后来手动修改总集数或 TMDB 元数据刮削得到更大的集数，会自动恢复为“追更中”。
- 定时检查会跳过真正完结的订阅，但会重新检查“总集数未达到”的已完结订阅。
- 已知文件中如果存在尚未转存的集数，自动转存会把它作为补转候选。
- 分享方改名后，转存不再只按原文件名匹配，也会按同季同集兜底匹配，减少补转漏文件。

## Aria2 下载

“我的网盘”发送文件到 Aria2 或订阅开启同步下载时，服务端会通过夸克 PC 下载接口获取临时直链，并把夸克 Cookie 与下载接口返回的临时 Cookie 一起写入 Aria2 任务 Header。

Aria2 RPC URL 可以填写完整地址，例如 `http://192.168.50.100:6800/jsonrpc`。如果只填写 `http://192.168.50.100:6800`，服务会自动补全 `/jsonrpc`。订阅同步下载未单独填写下载目录时，会按媒体类型使用电影、连续剧、动画目录；对应目录未配置时不向 Aria2 指定目录，由 Aria2 RPC 端自行决定保存位置。

如果下载失败并提示 `download file size limit[...]`、`require login [auth expired]` 或类似鉴权错误，优先更新夸克 Cookie 后重试，并确认 Aria2 所在机器可以访问夸克下载服务。

## STRM 与 HTTPStrm

在“系统设置”中启用 STRM 后，配置输出目录和 HTTPStrm 访问地址；创建或编辑订阅时再开启“转存后生成 STRM 文件”。订阅转存并重命名完成后，会在输出目录下按夸克保存目录结构生成同名 `.strm` 文件，内容为本服务的 HTTP 链接。

HTTPStrm 支持通过 `Authorization: Bearer <token>` 或 `X-HTTPStrm-Token: <token>` 传递 Token；为兼容已有 `.strm` 文件，链接中的 `?token=...` 仍可使用。媒体服务器访问该链接时，服务会用当前夸克 Cookie 实时换取临时下载地址并代理 Range 请求。`/strm/` 路由不走 Basic Auth，访问控制依赖 STRM Token，请不要把 Token 暴露到不可信环境。

## 保存目录规则

- 电影：保存到电影分类目录下的 `片名（年份）`，例如 `/电影/沙丘（2021）`。
- 连续剧：保存到连续剧分类目录下的 `剧名（年份）/Season X`，例如 `/连续剧/庆余年（2019）/Season 1`。
- 动画：保存到动画分类目录下的 `动画名（年份）/Season X`，例如 `/动画/孤独摇滚！（2022）/Season 1`。
- 自定义目录：电影直接保存到该目录；连续剧和动画会在该目录下追加 `Season X`，如果目录末尾已经是 `Season X` 则不会重复追加。

## 重命名模板

订阅模板可使用 `{}` 作为集数占位符，也支持具名变量：

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

文件名中识别到 `EP05`、`S01E05`、`第05集`、`178重置版` 等集数格式后，会生成类似：

```text
庆余年.S01E05.mp4
```

连续剧和动画订阅会结合文件名与分享内父目录判断季别。合集源中如果同时包含 `第1季`、`S03E01`、`番外篇`、`剧场版` 或 `前五季` 这类目录，非当前订阅季的文件会在检查、预览和自动转存阶段跳过；没有季别标记但能识别集数的普通文件名，例如 `178重置版.mp4`，仍会按当前订阅季处理。

## 在线更新

在线更新通过 GitHub Release 完成。该功能会替换当前服务二进制，默认关闭；需要先设置 `MY_MEDIA_SUB_ENABLE_SELF_UPDATE=1` 并重启服务。服务会要求 Release 同时提供 Linux x86_64 包和对应 `.sha256` 文件，校验通过后才会解压和替换。

- `GET /api/update/check`：检查最新版本和当前版本改动
- `GET /api/update/releases`：列出最近 Release，可用于选择指定版本升级或回退
- `GET /api/update/progress`：查询升级进度、阶段和百分比
- `POST /api/update/apply`：下载 Linux x86_64 Release 包，替换 `my-media-sub` 二进制和 WebUI 静态资源；可传入 `{"tag":"v0.9.1"}` 切换到指定版本
- `POST /api/update/restart`：替换完成后重启服务，前端等待 `/health` 恢复后刷新页面

## 后续规划

- 下一步更新建议报告：[docs/next-steps-0.9.3.md](docs/next-steps-0.9.3.md)
- 近期方向：保持 Rust 单体和夸克专精，优先完善追剧任务、规则中心、后台日志、工作台和追剧日历。

## 版本更新

### 0.9.8

- 工作台统计块和夸克健康卡补充状态图标，提升信息区辨识度，顶部不再显示工作台说明文案。
- 夸克健康检测会在连接测试时尝试读取移动端容量/连签信息，容量展示不再依赖手动签到结果。
- 设置页移除“夸克网盘默认根目录”输入项，设置字段定义也不再暴露该配置；历史配置仍兼容读取。
- 夸克健康配置检查不再把默认根目录作为必填项，自动转存只保留电影、连续剧、动画和自定义分类目录配置入口。

### 0.9.7

- 新增工作台首页，集中展示追更订阅、失效订阅、后台任务、下载速度、最近通知和夸克健康状态。
- 规则中心升级为可持久化的规则预设中心，支持保存、应用、删除预设；创建和编辑订阅时可直接选择预设并记录来源。
- 订阅检查历史增加扫描、新增、已知、目录跳过、非当前季跳过、起始集数前跳过和同集重复跳过计数，订阅列表可直接查看最近检查拆分。
- 夸克健康检测返回 Cookie、自动转存、签到、保存根目录、STRM 配置完整性和问题列表，工作台与设置页同步展示。
- WebUI 默认入口改为工作台，整体主色调调整为更活力的蓝色，并继续保持静态资源预编译部署。
- 通知中心推送结果与业务通知合并记录，不再把正常系统日志和推送派发结果拆成独立用户通知。
- 加固设置文件读取、STRM 写入和在线更新校验：损坏配置会隔离并阻止静默回退，STRM 使用原子写入，Release 校验匹配目标资产。

### 0.9.6

- WebUI 迁移到 Tailwind standalone CLI 预编译 CSS，去除运行时 Tailwind CDN，静态产物继续随 `static/` 一起部署。
- 新增深色/浅色双主题，顶栏可即时切换并持久化到浏览器本地存储，默认跟随系统偏好。
- 清理旧的 `blue-*` 颜色覆盖层，HTML 和 Alpine 动态 class 改为 `primary`、`surface`、`success`、`warning`、`danger` 等语义色。
- 优化移动端主导航、网盘列表、检查详情和规则预览表格，减少小屏横向滚动。
- 弹窗补充 `role="dialog"`、`aria-modal`、Escape 关闭和基础焦点循环，图标按钮补齐可访问名称。

### 0.9.5

- 系统设置页内部标签改为顶部横向胶囊导航，桌面端不再占用左侧栏宽度，移动端可横向滑动。
- WebUI 视觉风格改为更年轻的青绿、珊瑚、琥珀强调色，统一面板、按钮、输入框、标签和焦点态。
- 统一覆盖旧蓝色状态样式，减少深色蓝灰的单调感，同时保持工具型后台的紧凑信息密度。
- 优化移动端设置页标签显示，隐藏原生横向滚动条，减少窄屏拥挤感。

### 0.9.4

- 订阅设置进一步简化，创建向导默认使用系统“规则中心”的重命名模板，不再把重命名规则重复塞进每个订阅。
- 自动转存、手动命名修复和重命名预览统一使用有效规则；连续剧和动画在订阅未单独配置模板时会继承系统默认模板，电影不会误用剧集模板。
- 移除订阅级“按星期自动检查”设置，定时任务恢复为按系统设置的检查间隔自动检查全部启用订阅。
- WebUI 去掉订阅列表和高级规则里的星期调度控件，降低订阅创建和换源编辑时的配置复杂度。
- 保留历史配置中的星期字段兼容读取，旧数据可继续加载，但不再参与自动检查调度。

### 0.9.3

- “转存历史”重构为“后台日志”，以后台任务为数据源展示转存、元数据刮削和推送派发记录。
- 通知中心聚焦用户通知、系统通知和推送记录，不再混入转存、转存失败和元数据刮削这类后台执行结果。
- 后台日志新增类型、状态、关键词筛选，以及任务详情弹窗，可查看 Payload、Result、错误信息、耗时并复制详情。
- 订阅创建弹窗升级为三步向导，支持内容、规则、下载/STRM 分步配置，并提供常用命名规则预设。
- 系统设置新增“规则中心”，可集中调试命名模板、过滤条件、同集保留策略和样例文件，并写入默认重命名模板。
- 夸克设置页新增账号健康面板，显示 Cookie 检测状态、自动转存开关、签到结果、连签进度和容量信息。
- 明确产品方向：继续以 Rust + Axum 实现，专精夸克网盘追剧自动化；工作台和追剧日历列为后续规划。

### 0.9.2

- 在线更新支持列出历史 Release，并可选择指定版本进行升级或回退；完成替换后仍通过原有重启流程生效。
- 在线更新页新增“指定版本切换”区域，展示目标版本、发布时间、包大小和升级/回退说明。
- 搜索资源新增阶段式进度条，显示提交搜索、检测/嗅探、整理结果和完成/失败状态。
- 设置接口新增 `/api/settings/schema`，输出字段类型、默认值、密钥标记和可选项，为前后端设置统一打基础。
- PanSou API 地址保存后会被新的搜索请求即时使用，不再需要重启服务。
- 设置页默认值、推送开关和密钥配置标记与后端默认值对齐，并补充夸克默认根目录入口。
- 换源编辑订阅时展示当前进度、保存后起始集、已知集数和已转存数量，降低重复下载风险。
- 文件名识别增加真实样本回归，覆盖 `001v2.mp4`、`第178话 重置版.mp4`、`179 V2 1080p.mp4` 等数字变体。
- WebUI 统一卡片、统计块、按钮、焦点态和移动端布局，改善窄屏设置页、搜索页和表格预览体验。

### 0.9.1

- 集数识别支持 `178重置版.mp4` 这类“数字前缀 + 中文后缀”的文件名，重命名模板可正确生成对应集数。
- 集数兜底识别继续排除 `4K.mp4`、`1080p.mp4` 等纯清晰度文件名，避免误判为第 4 集或第 1080 集。
- 换源编辑订阅时，后端默认保留进度并从当前集下一集继续追更，避免未传前端选项或直接调用 API 时从头转存。
- 前端通知 Toast 改为安全文本渲染，并移除设置对象的生产控制台输出。

### 0.9.0

- 已完结订阅在总集数增加或元数据刮削更新集数后，会自动恢复为追更中，避免后续定时检查被跳过。
- 批量检查、单订阅检查、手动编辑和后台元数据任务统一使用同一套完结恢复逻辑。
- 自动转存补转已知但未转存的集数，并按已转存集数去重，避免分享改名后漏补或重复提交。
- 转存匹配新增同季同集兜底，分享方更换文件名后仍可定位需要转存的视频。
- 前端订阅状态展示兼容“已完结但当前集数小于总集数”的历史数据，自动显示为追更中。

### 0.8.14

- 夸克签到参数兼容整段移动端 `reward` 请求 URL，可直接粘贴抓包链接解析 `kps`、`sign`、`vcode`。
- 系统设置中的签到参数说明补充抓包 URL 填写方式，减少手动拆参数出错。

### 0.8.13

- 检查周期保存和调度启动统一使用后端归一化逻辑，覆盖页面预设周期和异常 API 输入，避免生成非法调度配置。
- 完善默认重命名模板变量说明，逐项解释变量含义并增加示例。

### 0.8.12

- 在线更新改为弹窗展示升级进度，替换完成后由用户点击“重启服务并刷新页面”生效。
- 新增 `POST /api/update/restart` 接口，服务恢复后前端自动检测并刷新当前页面。
- 订阅定时检查改用重复任务调度，避免长周期生成非法 cron 表达式导致服务启动失败。

### 0.8.11

- 订阅检查和自动转存新增同集重复视频保留策略，默认保留清晰度最高版本，也可选择上传时间最新、文件最大或最先出现。
- 同一集的多版本视频会在检查结果中标记为同集跳过，并按集数写入转存去重键。
- 系统设置新增默认重命名模板，新建连续剧/动画订阅时可直接套用。

### 0.8.10

- 新增独立夸克签到 Cookie 配置，可填写移动端 Cookie；留空时继续使用夸克网盘 Cookie。
- 订阅定时检查完成后会发送本轮检查汇总推送。
- 定时夸克签到失败会写入通知中心，并按“夸克签到”推送开关发送失败推送。

### 0.8.9

- 新增夸克网盘自动签到，可每日定时签到，也可手动立即签到。
- 签到成功后写入通知中心，并可按开关发送推送通知。
- 新增 `QUARK_SIGNIN_ENABLED`、`QUARK_SIGNIN_HOUR` 环境变量和 `POST /api/quark/signin` 手动签到接口。

### 0.8.8

- 修复 Docker 环境中 `SERVER_USERNAME`/`SERVER_PASSWORD` 每次启动都会覆盖 WebUI 已保存账号密码的问题。

### 0.8.7

- Release 二进制改为在 Ubuntu 22.04 环境构建，避免 Docker/bookworm 环境中 glibc 版本不匹配。
- 在线更新会同步替换 WebUI 静态资源目录。

### 0.8.6

- 优化“我的网盘”界面，新增面包屑导航、搜索、筛选、排序、列表/网格视图和批量操作。
- 提升夸克目录加载性能，目录创建、删除、重命名后自动清理缓存。
- 新增 Aria2 下载任务总体暂停/停止，以及单任务暂停、继续、停止、删除操作。
- 消息推送新增“下载完成”事件，Aria2 完成任务会记录通知并按开关推送。

### 0.8.5

- 修复 Docker 镜像运行阶段 glibc 版本不匹配导致容器启动失败的问题。

### 0.8.4

- 新增 STRM/HTTPStrm，支持订阅转存后生成 `.strm` 文件和 Range 代理播放。
- Aria2 下载目录改为按媒体类型配置电影、连续剧、动画和自定义分类目录。
- 订阅弹窗按功能拆分为“订阅内容”“重命名”“下载/STRM”三个标签页。

### 0.8.3

- 美化 README 项目说明、功能分组和使用流程。
- Release 工作流改为从 README 对应版本小节生成发布说明。

### 0.8.2

- 设置页敏感配置改为等长星号显示，支持点击按钮临时查看明文。
- 浏览器后退键改为在 WebUI 内部页面间后退。
- 订阅创建时新增资源名称智能识别。
- 订阅创建后的首次检查会提交本次新增文件的转存任务。

### 0.8.1

- 修复订阅自动转存遇到分享外层目录时可能转存父目录的问题。
- 优化订阅创建后的提示文案。

### 0.8.0

- 新增订阅手动元数据刮削，可在候选结果中手动选择 TMDB 匹配项。
- 新增订阅级 Aria2 同步下载。
- 在线更新页简化为版本状态、当前版本改动和一键升级。
- 支持 `PANSOU_API_URL` 独立覆盖。

## 开发

```bash
cargo check
cargo test
cargo build
node --check static/app.js
scripts/build-css.sh
```

WebUI 仍是纯静态资源，部署时只需要 `static/` 目录。修改 `static/index.html`、`static/app.js` 或 `tailwind/input.css` 后，需要重新生成 `static/styles.css`：

```bash
scripts/build-css.sh
scripts/build-css.sh --watch
```

CSS 构建使用 Tailwind standalone CLI，不依赖 npm 或 `node_modules`，二进制不入库。首次使用请从 <https://github.com/tailwindlabs/tailwindcss/releases> 下载对应平台的 `tailwindcss` 可执行文件，放入 `PATH`，或通过 `TAILWIND_BIN` 指定：

```bash
TAILWIND_BIN=~/.local/bin/tailwindcss scripts/build-css.sh
```

本地调试订阅检查时可以使用模拟夸克分享 fixture，避免访问真实网盘：

```bash
MOCK_QUARK_SHARE_FIXTURE=tests/fixtures/mock_quark_share.json cargo run
```

创建订阅时使用 `https://pan.quark.cn/s/mock-show` 可模拟正常分享，使用 `https://pan.quark.cn/s/mock-invalid` 可模拟失效分享。启用该环境变量后，未在 fixture 中声明的分享链接会按失效处理。

## 发布流程

普通功能提交直接推送到 `main`，GitHub Actions 会构建并推送 `ghcr.io/hellomrli/my-media-sub:latest` 镜像。发布版本时按下面顺序执行：

```bash
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked -- -D warnings
cargo test --locked
node --check static/app.js
scripts/build-css.sh
```

确认通过后更新 `Cargo.toml` 和 README 中的版本号，并把本次变更整理到 README“版本更新”的对应新版本小节。提交到 `main` 后创建并推送版本标签：

```bash
git tag v0.9.8
git push origin main
git push origin v0.9.8
```

`v*` 标签会触发 Release 工作流，自动编译 Linux x86_64 二进制包、打包 `static/` 和 README，并上传 `.tar.gz` 与 `.sha256` 到 GitHub Release。Release 正文会从 README“版本更新”中匹配当前标签对应的小节；Docker 工作流会同时构建并推送 `latest`、版本号、`major.minor` 和 SHA 标签镜像。

## 项目结构

```text
src/
  api/       HTTP API 路由
  clients/   PanSou、夸克、Aria2 客户端
  jobs/      后台任务队列
  models/    数据模型
  services/  订阅检查、转存、重命名、推送、STRM 等业务逻辑
  store/     JSON 数据存储
static/
  index.html WebUI 结构
  app.js     WebUI 交互逻辑
  styles.css WebUI 样式
tailwind/
  input.css  Tailwind 输入 CSS 和设计 token
tailwind.config.js
scripts/
  build-css.sh Tailwind standalone CLI 构建脚本
tests/
  fixtures/             测试和本地模拟数据
  real_data_compat.rs   真实数据兼容性测试
```

## License

MIT
