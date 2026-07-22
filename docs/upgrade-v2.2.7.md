# v2.2.7 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.7 不修改 JSON Store schema，可直接复用 v2.2.6 的 `data/`。

本版本加固 Job Worker 状态写盘可观测性（失败日志 + `job_store_update_failures` 指标）、运行句柄 registry 的 poison 恢复，并同步规划文档与 PWA 缓存代次。前端资源 URL 带有 `?v=2.2.7`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
