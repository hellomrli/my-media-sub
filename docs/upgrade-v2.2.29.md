# v2.2.29 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.29 不修改 JSON Store schema，可直接复用 v2.2.28 的 `data/`。

本版本为日历订阅卡片布局调整：缩略图撑满卡片高度并紧贴边缘，右侧为加粗订阅名称、当天会上映的集、目前已存的集三行文字，卡片宽度收敛到刚好容纳内容。前端资源 URL 带有 `?v=2.2.29`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
