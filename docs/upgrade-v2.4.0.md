# v2.4.0 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.4.0 不修改 JSON Store schema，可直接复用 v2.3.3 的 `data/`。

本版本主要变更：下载目录年份括号改为半角（`聪明镇 (2026)`）；打开项目自动检测夸克账号状态；系统设置新增 TMDB API 测试；Telegram 推送支持缩略图；同批下载文件全部完成后合并通知、下载失败立即通知。前端资源 URL 带有 `?v=2.4.0`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
