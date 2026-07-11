# v1.10.0 升级、验证与回滚指南

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

生产环境可固定 `ghcr.io/hellomrli/my-media-sub:1.10.0`。

## 二进制

```bash
sha256sum -c my-media-sub-v1.10.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.10.0-linux-x86_64.tar.gz
```

同时替换二进制和完整 `static/`，保留原数据目录。

## 升级后验证

- `/health` 返回 `1.10.0`。
- 订阅、日历缩略图、检查和转存正常。
- 规则预览能返回多集、缺集和重复集。
- 换源候选可显示质量和集数覆盖。
- STRM 审计接口可用。
- 使用 Aria2 或媒体库刷新时先执行测试任务。
- 数据目录没有意外 `.corrupt-*` 文件。

## 回滚

停止 v1.10.0，恢复旧二进制和完整旧版 `static/`；仅在数据异常时恢复升级前 `DATA_DIR`。不要让两个版本同时写同一数据目录。
