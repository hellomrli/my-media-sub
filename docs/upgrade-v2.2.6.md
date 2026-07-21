# v2.2.6 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.6 不修改 JSON Store schema，可直接复用 v2.2.5 的 `data/`。

本版本包含多季订阅、剧名清洗下沉、Telegram 搜索/订阅/换源菜单，以及若干功能修复。PWA 缓存代次已提升；前端资源 URL 带有 `?v=2.2.6`。

Telegram 启用后会自动 `setMyCommands`，也可在 BotFather 同步命令列表。搜索/换源会话会写入 `telegram_bot.json`，进程重启后 15 分钟内仍可 `/subscribe` 或 `/switch_apply`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
