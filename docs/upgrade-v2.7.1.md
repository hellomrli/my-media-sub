# v2.7.1 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.7.1 不修改 JSON Store schema 版本，可直接复用 v2.7.0 的 `data/`。本次修复跨季进度、同名文件转存、详情与日历状态、季度探测竞态以及备份恢复覆盖问题。

## 备份恢复行为

`POST /api/backups/restore` 校验并暂存备份，返回 `staged_files`、`restored_files: 0` 和 `restart_required: true`。备份在下一次启动时、加载任何 Store 和后台任务之前应用。提交恢复后请及时重启；重启前的后续修改会被备份覆盖。

恢复前快照会包含旧进程停止前的最终数据。应用失败时回滚并停止启动，待恢复归档保留在 `data/backups/restore-pending.json`，修复磁盘空间或权限等故障后再次启动即可重试。正常升级不需要执行备份恢复。

## 跨季记录与 API

- 旧订阅记录继续可读。编辑季度时绑定旧进度的季度归属，再计算新季进度；切回旧季仍可复用保留的转存记录。
- 文件记录和任务载荷可以含目录，如 `Season 3/01.mkv`；新增转存键为 `s:3:ep:1`。旧 `ep:1` 键仅归属主季。
- 详情响应的 `episodes` 每项包含 `season`，客户端应使用「季＋集」作为唯一标识；整数集号数组继续保留用于兼容。
- 如需回滚旧版，请同时恢复升级前的数据备份，避免旧程序错误解释新增的跨季记录。

## 前端资源

前端资源 URL 带有 `?v=2.7.1`，Service Worker 缓存版本同步更新。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
