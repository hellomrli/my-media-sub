# v1.12.0 升级、验证与回滚指南

## 升级前

1. 停止会写入 DATA_DIR 的旧实例，确认没有并行容器或二进制进程。
2. 通过旧版 `/api/backups/export` 或复制目录保存完整 DATA_DIR，并保留当前二进制/镜像标签。
3. 本版本继续使用 `schema_version: 1`，无需离线迁移，也不会创建 SQLite 数据库。
4. 如计划配置 `BACKUP_EXTERNAL_DIR`，先创建 DATA_DIR 之外的专用挂载目录并确认运行用户可写。

## Docker

固定版本升级：

```bash
docker pull ghcr.io/hellomrli/my-media-sub:1.12.0
docker compose pull
docker compose up -d
docker compose logs --tail=150 my-media-sub
```

可选环境变量：

```bash
LOG_FORMAT=json
SLOW_OPERATION_MS=1000
BACKUP_VERIFY_INTERVAL_HOURS=24
BACKUP_EXTERNAL_DIR=/mnt/offsite/my-media-sub
STORE_GROWTH_WARNING_MB=24
```

若使用外部备份目录，需额外挂载，例如：

```yaml
volumes:
  - ./data:/app/data
  - /mnt/offsite/my-media-sub:/app/offsite-backups

environment:
  BACKUP_EXTERNAL_DIR: /app/offsite-backups
```

## Linux 二进制

```bash
sha256sum -c my-media-sub-v1.12.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.12.0-linux-x86_64.tar.gz
cd my-media-sub-v1.12.0-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

必须同时替换二进制和完整 `static/`；运行目录需要保留发布包中的 `static/`、`docs/` 和 README。

## 升级后验证

1. `GET /health` 返回版本 `1.12.0`。
2. 响应包含 `X-Request-ID` 和 `X-Correlation-ID`。
3. `GET /metrics` 在 Basic Auth 下返回 Prometheus 文本。
4. 系统诊断页显示磁盘、权限、时区、DNS、Store 一致性和处理建议。
5. 创建服务器备份后，`GET /api/backups/verification` 返回 `status: "passed"`。
6. `GET /api/storage/cleanup` 返回 `mutates_data: false`，不会在预览时修改数据。
7. `GET /api/storage/decision` 应显示 `runtime_backend: "json"` 和 `dual_write_active: false`。
8. 如果配置外部目录，确认出现与本地同名的 `backup-*.json` 且权限为 0600。

## 清理安全说明

- `/api/storage/cleanup` 的执行请求必须包含 `confirmation: "CLEANUP DATA"`。
- 系统会先创建并隔离验证 `pre-cleanup` 备份，备份失败时不得继续清理。
- SQLite 阈值达到后仅显示 `decision_required`，不会自动创建数据库或开启双写。

## 回滚

1. 停止 v1.12.0，确保没有进程继续写 DATA_DIR。
2. 恢复旧二进制或旧 Docker 镜像及其完整 `static/`。
3. 优先恢复升级前备份；若已执行生命周期清理，可使用响应中的 `snapshot_backup` 或最近 `pre-cleanup` 备份恢复。
4. 删除或停用仅新版本使用的环境变量不会破坏旧版数据。
5. 不要让新旧版本同时运行在同一 DATA_DIR。
