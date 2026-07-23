# v2.2.10 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.10 不修改 JSON Store schema，可直接复用 v2.2.9 的 `data/`。

本版本整理订阅「高级规则」界面布局（去重导航、修正表单嵌套、精简预览区）。前端资源 URL 带有 `?v=2.2.10`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
