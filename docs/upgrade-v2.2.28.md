# v2.2.28 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.28 不修改 JSON Store schema，可直接复用 v2.2.27 的 `data/`。

本版本为日历订阅卡片的布局修正：周视图把「该日期更新的集」与「目前已转存」合并为一行统一字号，卡片与缩略图收窄、留白收紧，同一天排期更紧凑。前端资源 URL 带有 `?v=2.2.28`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
