# v2.6.0 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.6.0 新增 Telegram 豆瓣链接转存 + 可选下载能力；同时移除已下线（自 v2.2.0）的 STRM 残留字段。历史 `data/` 可直接复用，旧数据中的 STRM 字段会被 serde 自动忽略，无需手动迁移。前端资源 URL 带有 `?v=2.6.0`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
