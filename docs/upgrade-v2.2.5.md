# v2.2.5 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.5 不修改 JSON Store schema，可直接复用 v2.2.4 的 `data/`。

本版本主要修复订阅保存、网盘操作、转存密码、调度间隔和前端反馈等功能问题。PWA 缓存代次已提升；前端资源 URL 带有 `?v=2.2.5`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
