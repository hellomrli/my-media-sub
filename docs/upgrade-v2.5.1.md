# v2.5.1 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.5.1 不修改 JSON Store schema，可直接复用 v2.5.0 的 `data/`。

本版本修复夸克分享返回 404/410 时未触发来源失效通知的问题；升级后会按订阅的失效通知开关发送 `subscription_invalid` 推送。前端资源 URL 带有 `?v=2.5.1`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
