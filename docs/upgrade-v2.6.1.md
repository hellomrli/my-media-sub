# v2.6.1 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.6.1 不修改 JSON Store schema，可直接复用 v2.6.0 的 `data/`。

本版本为纯修复发布：不修改 JSON Store schema，不新增 API 端点，历史 `data/` 可直接复用；`SourceSwitchHistoryItem` 新增 `previous_start_episode_number` 可选字段，旧版本回滚读取时自动忽略。前端资源 URL 带有 `?v=2.6.1`。

主要修复：多季订阅第二季停更、Telegram 转存确认可能错位、批次合并下载通知丢失、`trust_proxy_headers` 限流键可被伪造、长转存被看门狗误杀、队列恢复在极端积压下挂死等，完整清单见 CHANGELOG.md。

注意：v2.6.0 引入的豆瓣链接转存能力在本版本修复了确认错位与 long_polling 按钮不渲染的问题，无需额外配置。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
