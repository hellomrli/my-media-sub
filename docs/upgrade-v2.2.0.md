# v2.2.0 升级指南

## Docker

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

升级前建议备份 `data/`。容器会继续复用原有 JSON 数据，不需要迁移数据库。

## Linux 二进制

1. 备份 `DATA_DIR`。
2. 下载 `my-media-sub-v2.2.0-linux-x86_64.tar.gz` 和对应 `.sha256`。
3. 使用 `sha256sum -c` 校验压缩包。
4. 同时替换二进制和 `static/` 目录，保留原 `data/`。
5. 启动后检查 `/health` 和 WebUI 登录。

## STRM 说明

v2.2.0 暂时移除了 Strm HTTP 代理、生成/审计 API 和配置入口。旧订阅和设置中的 `strm_*` 字段不会被删除，但不会再触发 Strm 操作。后续恢复时将作为独立模块发布。

## 回滚

停止服务后恢复旧版本二进制、`static/` 和升级前的 `data/` 备份即可。v2.2.0 未修改 Store schema。
