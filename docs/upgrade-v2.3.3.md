# v2.3.3 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.3.3 不修改 JSON Store schema，可直接复用 v2.3.2 的 `data/`。

本版本为日历 UI 微调：周视图订阅卡片缩略图放大到默认 56×84（保持 2:3），卡片四边内边距收窄到 3px。前端资源 URL 带有 `?v=2.3.3`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
