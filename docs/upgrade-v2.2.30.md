# v2.2.30 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.30 不修改 JSON Store schema，可直接复用 v2.2.29 的 `data/`。

本版本为稳定性与 UI 修复：设置页不再允许保存会导致登录失效的用户名/密码；网盘目录缓存加入过期清理与上限；发版脚本覆盖全部版本面；日历订阅卡缩略图留边、「已存」显示季度。前端资源 URL 带有 `?v=2.2.30`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
