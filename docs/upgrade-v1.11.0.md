# v1.11.0 升级、验证与回滚指南

## 升级前

1. 停止会写数据的旧实例并备份完整 `DATA_DIR`。
2. 记录当前二进制或 Docker 镜像标签。
3. 本版本继续使用 `schema_version: 1`，无需离线迁移。

## Docker

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 my-media-sub
```

生产环境可固定 `ghcr.io/hellomrli/my-media-sub:1.11.0`。

## 二进制

```bash
sha256sum -c my-media-sub-v1.11.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.11.0-linux-x86_64.tar.gz
```

同时替换二进制和完整 `static/`，保留原数据目录。

## 升级后验证

- `/health` 返回 `1.11.0`。
- 后台任务能显示优先级、执行次数和错误分类。
- 系统设置可保存 Job 分层并发、维护模式及通知策略。
- `/api/push/diagnostics` 和模板预览接口可用。
- Webhook 轮换后接收端可在重叠期验证当前或上一签名。
- 诊断页显示延迟重试、超时、归档和积压状态。
- 数据目录没有意外 `.corrupt-*` 文件；`jobs.archive.json` 出现时权限应为 0600。

## 回滚

停止 v1.11.0，恢复旧二进制和完整旧版 `static/`。旧版本会忽略 Settings 中未知字段，但不理解新的 Job 字段时应优先恢复升级前 `DATA_DIR`，不要让两个版本同时写同一数据目录。
