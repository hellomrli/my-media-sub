# v2.2.27 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.27 不修改 JSON Store schema，可直接复用 v2.2.26 的 `data/`。

本版本为安全加固与清理发布：在线升级解包增加归档成员路径校验；Gotify/PushPlus 凭据改走请求头与 HTTPS；订阅自动转存失败后不再自动重试（避免重复转存，改为人工确认）；修复设置页「版本更新」标签自动检查失效；工作台卡片布局压缩。同时移除了大量旧版遗留死代码与过期的 v1.x 升级文档、架构图。前端资源 URL 带有 `?v=2.2.27`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
