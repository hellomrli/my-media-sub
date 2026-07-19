# v2.2.3 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.3 不修改 JSON Store schema，可直接复用 v2.2.2 的 `data/`。

本版本会提升 PWA 缓存代次。订阅检查完成后，前端会主动清理 Alpine 复用图片节点上遗留的加载失败状态，并重新加载未成功显示的海报和缩略图。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
