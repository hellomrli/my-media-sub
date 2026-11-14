# 配置网盘、下载器和通知

[返回文档入口](index.md) · [首次使用](usage.md)

## 选择配置方式

优先在 WebUI 系统设置中填写可选集成，并使用对应的测试按钮验证连接。设置保存在 `DATA_DIR/settings.json`。

进程环境变量在启动时覆盖设置文件，完整名称和示例见 [`.env.example`](../.env.example)。Compose 读取 `.env` 用于替换 Compose 文件中的变量；**仅在 `.env` 增加一个变量不会自动把它传入容器**。需要环境配置时，将其加入 Compose 的 `environment`，或使用自己维护的配置文件。独立二进制与 `cargo run` 需要显式导出环境变量。

不要用示例文件整体覆盖已有部署配置。环境中的旧凭据会在每次启动覆盖 WebUI 设置，应同步更新来源。

## 接入资源和媒体库

| 任务 | 必需或可选配置 | 验证方式 |
|---|---|---|
| 探测分享、保存电影、转存新集 | 夸克 Cookie；也可用 `QUARK_COOKIE` | 设置页测试后对一份可访问的分享执行探测 |
| 自动签到 | 签到专用 Cookie 和签到开关/时间 | 先手工签到；`QUARK_SIGNIN_*` 独立于普通分享操作 |
| 聚合搜索资源 | PanSou 服务地址或 `PANSOU_API_URL` | 设置页测试，再在资源搜索中查询 |
| 标准标题、海报、年份及总集数 | TMDB API Key、语言或 `TMDB_API_KEY` / `TMDB_LANGUAGE` | 测试 Key，再匹配剧名；失败边界见 [剧名匹配](title-matching.md) |
| 下载到本地 | Aria2 RPC、密钥、分类下载目录 | 测试 RPC，再检查下载管理中的任务 |
| 刷新媒体库 | Jellyfin / Emby / Plex 的连接与认证设置 | 配置后使用设置页的连接测试 |

添加分享不要求先配置 PanSou、TMDB 或下载器。只有夸克 Cookie 是探测与转存的前提。

配置 Aria2 时使用 `ARIA2_RPC_URL`、`ARIA2_SECRET` 和 `ARIA2_MOVIE_DIR` / `ARIA2_SERIES_DIR` / `ARIA2_ANIME_DIR`。下载目录属于 **Aria2 所在机器或容器**，需要下载器可以写入，不能把应用容器内路径直接视为下载器路径。电影默认可同步下载，也可只保存到网盘；详见 [电影保存与画质](media-save-quality.md)。

## 配置通知和手机入口

通知支持企业微信、WxPusher、Telegram、Bark、Gotify、PushPlus、Server 酱、Browser Push 与签名 Webhook。先启用一个渠道并发送测试消息，再设置安静时段和摘要聚合。

`TZ` 影响基于本地时间的免打扰等功能。媒体日历自己的时间与推断口径见 [媒体日历](media-calendar.md)。

- Telegram 通知与控制 Bot 分开配置；控制模式取 `disabled`、`long_polling` 或 `webhook`。启用控制时设置用户/聊天白名单并完成授权，写操作有二次确认与脱敏审计。豆瓣链接处理流程见 [Telegram Bot](telegram-bot.md)。
- 浏览器安装与推送条件见 [PWA](pwa.md)。离线壳层不代表离线也能执行网盘或订阅操作。
- 接收签名 Webhook、使用脚本导出订阅的步骤见 [自动化 API](automation-api.md)。

## 配置访问入口

| 配置 | 行为 |
|---|---|
| `SERVER_HOST` / `SERVER_PORT` | 进程默认监听 `0.0.0.0:56001`；Compose 发布端口默认只绑定宿主机本地 |
| `SERVER_USERNAME` / `SERVER_PASSWORD` | 管理凭据；旧别名 `APP_USERNAME` / `APP_PASSWORD` 仍可用，优先级低于 `SERVER_*` |
| `trust_proxy_headers` | 仅在可信反代覆盖来源头、且客户端不能绕过代理时开启；默认不信任 X-Forwarded-For |
| `allowed_hosts` | WebUI 中设置 Host 白名单，保存时包含当前访问域名；无对应环境变量 |
| `hsts_enabled` | WebUI 中显式开启；按主机名生效且不区分端口，需要该主机所有相关入口均支持 HTTPS |

反代配置、请求头与认证要求见 [HTTPS 反向代理](https-reverse-proxy.md)。应用对带跨站 Origin / Sec-Fetch-Site 的修改请求执行 CSRF 检查；命令行客户端仍需通过认证。

## 调整运行和维护设置

| 配置 | 使用场景 |
|---|---|
| `DATA_DIR` | 指向持久化业务数据，默认 `./data` |
| `STATIC_DIR` / `APP_RUNTIME_DIR` | WebUI 路径和可写在线更新载荷；容器分别为 `/app/runtime/static` 与 `/app/runtime` |
| `SELF_UPDATE_ENABLED` / `SELF_UPDATE_BACKUP_RETENTION` | 在线更新开关与副本数量；不要在未核对目录的情况下改路径 |
| `SELF_UPDATE_PUBLIC_KEY` | 配置后强制要求 minisign 分离签名；信任与编译期固化说明见 [Docker 在线更新](docker-online-update.md#升级包的真实性校验minisign) |
| `BACKUP_INTERVAL_HOURS` / `BACKUP_RETENTION` | 定时备份与保留数量；间隔 0 关闭定时备份 |
| `ALLOW_QUARANTINE_STARTUP` | 明确允许损坏业务 Store 后以空数据启动，默认 false；设置存储不接受放行 |
| `RUST_LOG` / `LOG_FORMAT` | 日志等级及格式；`LOG_FORMAT=json` 输出结构化日志 |

调整备份、恢复、隔离和保留策略前，阅读 [数据维护](data-maintenance.md)。
