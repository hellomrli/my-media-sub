# v2.2.2 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.2 不修改 JSON Store schema，可直接复用 v2.2.1 的 `data/`。

本版本会提升 PWA 缓存代次。升级后浏览器会安装新的 Service Worker、清理旧静态缓存，并优先从网络加载当前版本的 JS/CSS。远程海报和日历缩略图发生临时加载错误时会自动重试，无需再使用 `Ctrl+Shift+F5`。
