# v2.2.8 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.8 不修改 JSON Store schema，可直接复用 v2.2.7 的 `data/`。

本版本修复 Aria2 批量提交截断问题：超过「单批提交大小」的文件会自动分批继续提交，而不是只提交前 N 个后丢弃。前端资源 URL 带有 `?v=2.2.8`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
