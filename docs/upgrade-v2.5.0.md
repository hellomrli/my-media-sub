# v2.5.0 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.5.0 不修改 JSON Store schema，可直接复用 v2.4.0 的 `data/`。

本版本主要变更：修复同批下载合并通知重复发送；新增「下载完成后写入 NFO/海报到 Aria2 下载目录」（实验性，设置页「媒体元数据」开关默认关闭，供 Jellyfin/Emby/Kodi 刮削）。前端资源 URL 带有 `?v=2.5.0`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
