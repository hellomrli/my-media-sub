# My Media Sub

媒体订阅、资源搜索和夸克网盘转存管理工具。当前主分支为 Rust 版本，提供 WebUI、Basic Auth、订阅自动检查、自动转存、文件重命名、通知推送和基础网盘管理能力。

## 当前版本

- 版本：`0.7.10`
- 后端：Rust + Axum + Tokio
- 前端：单文件 WebUI，入口为 `static/index.html`
- 数据目录：默认 `./data`，可通过 `DATA_DIR` 修改
- 默认端口：`56001`
- 默认登录：`admin` / `change-me`

## 主要功能

- 资源搜索：通过 PanSou 搜索资源，支持链接检测、文件嗅探和失效链接过滤。
- 一次性转存：搜索结果可直接转存到夸克网盘指定目录。
- 订阅管理：创建订阅后可手动或定时检查更新。
- 自动转存：发现新文件后自动保存到电影、连续剧、动画或自定义目录。
- 智能重命名：按订阅模板识别 `S01E05`、`EP05`、`第05集` 等集数格式并重命名。
- 命名修复：订阅列表提供“修复命名”，可对已转存的现有视频重新按模板命名。
- 网盘管理：浏览夸克目录，支持新建文件夹、重命名、删除和批量删除。
- 通知中心：保存系统通知，支持已读和清空。
- 推送渠道：企业微信、Telegram、WxPusher、Bark、Gotify、PushPlus、Server 酱。
- 设置管理：支持运行时保存夸克 Cookie、推送配置、调度配置、NAS 同步、Aria2 等设置。

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
| `WECOM_BOT_URL` | 企业微信机器人地址 | 空 |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | 空 |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | 空 |

更多部署说明见 [DOCKER.md](DOCKER.md)。

## 使用流程

1. 登录 WebUI。
2. 在“系统设置”中配置夸克 Cookie、保存目录、推送渠道和自动检查间隔。
3. 在“资源搜索”中搜索资源，可选择“转存”或“订阅”。
4. 在“订阅管理”中检查订阅、删除订阅或对已有文件执行“修复命名”。
5. 在“我的网盘”中浏览、创建文件夹、重命名和删除文件。

## 重命名模板

订阅模板需要包含 `{}` 占位符，例如：

```text
庆余年.S01E{}
```

文件名中识别到 `EP05`、`S01E05` 或 `第05集` 后，会生成：

```text
庆余年.S01E05.mp4
```

如果文件已经转存但没有被命名，可在订阅卡片点击“修复命名”。

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
- `GET /api/jobs/events`：SSE 任务进度事件流

### 订阅

- `GET /api/subscriptions`
- `POST /api/subscriptions`
- `GET /api/subscriptions/{id}`
- `PUT /api/subscriptions/{id}`
- `DELETE /api/subscriptions/{id}`
- `POST /api/subscriptions/{id}/check`
- `POST /api/subscriptions/{id}/rename-existing`
- `POST /api/subscriptions/check`

### 通知

- `GET /api/notifications`
- `POST /api/notifications/{id}/read`
- `POST /api/notifications/read-all`
- `POST /api/notifications/clear`

### 夸克网盘

- `POST /api/quark/test`
- `GET /api/drive?fid={fid}`
- `GET /api/drive/find-path?path={path}`
- `POST /api/drive/mkdir`
- `POST /api/drive/delete`
- `POST /api/drive/rename`

### 推送

- `POST /api/push/test`

## 开发

```bash
cargo check
cargo test
cargo build
```

前端目前没有单独构建步骤，WebUI 直接由 `static/index.html` 提供。

## 项目结构

```text
src/
  api/       HTTP API 路由
  clients/   PanSou、夸克、转存客户端
  models/    数据模型
  services/  订阅检查、转存、重命名、推送等业务逻辑
  store/     JSON 数据存储
static/
  index.html WebUI
tests/
  real_data_compat.rs
```

## 文档

- [Docker 部署指南](DOCKER.md)
- [下一步计划](NEXT_STEPS.md)
- [架构文档](docs/architecture.md)
- [迁移记录](RUST_MIGRATION_V2.md)

## License

MIT
