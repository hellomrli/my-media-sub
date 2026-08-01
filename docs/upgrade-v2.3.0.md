# v2.3.0 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.3.0 不修改 JSON Store schema，可直接复用 v2.2.30 的 `data/`。

本版本聚焦 Telegram Bot 体验：消息改为 HTML 排版并统一转义用户内容；订阅/任务/通知/日历列表支持翻页按钮；支持中文指令与完整中文主菜单；直接粘贴豆瓣电影链接即可自动解析片名并搜索对应资源。推送通道（Telegram/PushPlus）同步加固 HTML 转义。前端资源 URL 带有 `?v=2.3.0`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
