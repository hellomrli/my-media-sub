# v2.1.2 升级、验证与回滚指南

## 适用范围

本指南用于从 v2.1.1 升级到 v2.1.2。该版本只调整 Aria2 前台轮询策略，不改变存储格式、后台下载监控或 OpenAPI 契约。

## 升级前

1. 通过 WebUI 导出完整备份，或停止实例后复制整个 `DATA_DIR`。
2. 保留 v2.1.1 二进制/镜像及其配套 `static/`，用于快速回滚。

## Docker

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.1.2
docker compose pull
docker compose up -d
docker compose logs --tail=120 my-media-sub
```

升级后可确认实际 digest：

```bash
docker image inspect ghcr.io/hellomrli/my-media-sub:2.1.2 --format '{{.RepoDigests}}'
```

## 升级后验证

1. `GET /health` 返回版本 `2.1.2`。
2. 没有活动或排队任务时，打开工作台或下载页只发生一次 Aria2 状态请求，不再持续每 2 秒轮询。
3. 提交一个下载任务后，页面立即刷新并恢复高频轮询；任务完成或停止后轮询自动停止。
4. 后台下载监控仍能处理完成状态、订阅进度和通知。
5. 已安装 PWA 客户端激活新 Service Worker 后，静态资源缓存版本应更新为 v2.1.2。

## 回滚

1. `docker compose down`，或停止二进制进程。
2. 部署 v2.1.1 的二进制与配套 `static/`，或拉取 `ghcr.io/hellomrli/my-media-sub:2.1.1`。
3. 保留同一 `DATA_DIR`；v2.1.2 未改变存储格式。
