# v2.3.1 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.3.1 不修改 JSON Store schema，可直接复用 v2.3.0 的 `data/`。

本版本为日历 UI 微调：周视图订阅卡片缩略图放大、上下缝隙与左侧对齐，卡片宽度收窄并微调文字位置，减少右侧留白。前端资源 URL 带有 `?v=2.3.1`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
