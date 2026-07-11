# v1.9.0 升级、验证与回滚指南

## 升级前

1. 停止会写入数据的旧实例。
2. 完整备份 `DATA_DIR`，并记录当前二进制或 Docker 镜像标签。
3. 确认数据盘有足够空间容纳备份；本版本保持 `schema_version: 1`。

## Docker 升级

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 my-media-sub
```

建议生产环境固定使用 `ghcr.io/hellomrli/my-media-sub:1.9.0`，确认稳定后再使用 `latest`。

## 二进制升级

下载 `my-media-sub-v1.9.0-linux-x86_64.tar.gz` 及 SHA256 文件，校验后同时替换二进制和整个 `static/` 目录，保留原 `DATA_DIR`：

```bash
sha256sum -c my-media-sub-v1.9.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.9.0-linux-x86_64.tar.gz
```

不要混用旧版静态资源与新版二进制。

## 升级后验证

- `/health` 返回版本 `1.9.0`。
- 登录、订阅列表、检查、转存和任务队列正常。
- “系统诊断”可读取脱敏状态并导出诊断包。
- 可创建并下载备份，恢复操作可先完成预览。
- `/api-docs.html` 可打开，PWA manifest 与 service worker 可加载。
- Browser Push/Webhook 仅在 HTTPS 或受信任本机环境中启用。
- 数据目录没有意外的 `.corrupt-*` 文件。

## 回滚

停止 v1.9.0，恢复升级前二进制与完整 `static/`，并在确有数据异常时恢复升级前 `DATA_DIR` 备份。不要让新旧版本同时写同一个数据目录。
