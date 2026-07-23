# v2.2.9 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.9 不修改 JSON Store schema，可直接复用 v2.2.8 的 `data/`。

本版本修复订阅自动转存门槛与 Aria2 提交回退，并改进剧名魔法匹配与重命名预览 UI。前端资源 URL 带有 `?v=2.2.9`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
