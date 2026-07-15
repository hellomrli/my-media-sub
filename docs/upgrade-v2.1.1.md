# v2.1.1 升级、验证与回滚指南

## 适用范围

本指南用于从 v2.1.0 升级到 v2.1.1。该版本修复转存目录、Aria2 下载状态、换季进度、并发导入和任务关闭问题。存储 `schema_version: 1` 与 OpenAPI 契约不变，无需手工迁移数据。

## 升级前

1. 通过 WebUI 导出完整备份，或停止实例后复制整个 `DATA_DIR`。
2. 保留 v2.1.0 二进制/镜像及其配套 `static/`，用于快速回滚。
3. 若当前有 Aria2 同步下载任务，建议保留通知历史直到升级后首次监控完成。新提交的任务会自动写入持久下载记录；旧任务继续兼容通知历史映射。

## Docker

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.1.1
docker compose pull
docker compose up -d
docker compose logs --tail=120 my-media-sub
```

也可以固定次版本标签 `ghcr.io/hellomrli/my-media-sub:2.1`。升级后使用以下命令确认实际 digest：

```bash
docker image inspect ghcr.io/hellomrli/my-media-sub:2.1.1 --format '{{.RepoDigests}}'
```

## Linux 二进制

```bash
sha256sum -c my-media-sub-v2.1.1-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v2.1.1-linux-x86_64.tar.gz
cd my-media-sub-v2.1.1-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

发布包中的二进制、`static/`、README、CHANGELOG 和 docs 应作为一个整体部署。即使本版本没有修改 WebUI，也建议保持二进制与发布包内静态资源一致。

## 升级后验证

1. `GET /health` 返回版本 `2.1.1`。
2. 创建或检查一个启用同步下载的订阅，确认订阅 JSON 中出现 `sync_downloads`，Aria2 完成后对应记录包含 `completed_at`。
3. 清理展示通知后，订阅详情中的下载完成状态仍应保留。
4. 修改测试订阅的季数，确认旧的当前集数、已知文件、转存记录、同步下载记录和完结状态已重置。
5. 对同一导入请求并发发送相同 `Idempotency-Key`，只应创建一份订阅并返回一致结果。
6. 停止服务时观察日志，Worker 应在宽限期内退出，超时任务会被中止并标记为可重试失败状态。

## 回滚

1. `docker compose down`，或停止二进制进程。
2. 部署 v2.1.0 的二进制与配套 `static/`，或拉取 `ghcr.io/hellomrli/my-media-sub:2.1.0`。
3. v2.1.0 会忽略新增的 `sync_downloads` 字段，因此可以读取 v2.1.1 数据；但旧版本再次保存订阅时可能移除该字段，下载完成关联会退回依赖通知历史。
4. 如果需要完整保留 v2.1.1 的持久下载跟踪状态，请恢复升级前备份或尽快重新升级到 v2.1.1。
