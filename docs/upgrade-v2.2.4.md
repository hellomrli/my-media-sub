# v2.2.4 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.4 不修改 JSON Store schema，可直接复用 v2.2.3 的 `data/`。

本版本将 TMDB 图片加载改为同源 VPS 代理：浏览器只请求当前 my-media-sub 服务，VPS 负责拉取、校验和缓存 TMDB 图片。前端资源 URL 也已带上 `?v=2.2.4`，用于绕过旧 Service Worker 和 HTTP 缓存。

升级后只需普通刷新或重新打开页面，无需使用 `Ctrl+Shift+F5`。如果使用二进制部署，必须同时替换整个 `static/` 目录。
