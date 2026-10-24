# v2.7.6 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.7.6 不修改 JSON Store schema，可直接复用 v2.7.5 的 `data/`。

本版本包含 Store 恢复安全、取消竞态和资源搜索部分结果修复：

- Store 写盘开始后，即使发起请求或元数据任务被取消，提交仍会持有保存锁直到磁盘和内存状态一致；后续写入会等待，不会用过时快照覆盖数据。
- 损坏的 JSON Store 被隔离且原文件缺失时，后续启动会持续失败关闭，避免容器重启后误以空数据运行。将有效备份恢复到原文件路径后即可启动，隔离副本无需删除。`ALLOW_QUARANTINE_STARTUP=true` 仅允许业务 Store 显式降级启动，不适用于设置文件。
- 搜索 API 保留原有 `data` 数组，并增加 `partial`、`timed_out`、`probed_count`、`total_count`。50 秒预算内未完成的探测会在前端明确标记；关闭失效过滤时会保留有效性未知的资源，开启时只展示已确认有效项。
- Nginx 推荐读取超时 90 秒（应用预算 50 秒、前端总超时 120 秒）；其他反向代理也应留出类似余量。

前端资源 URL 带有 `?v=2.7.6`，Service Worker 缓存代次同步更新。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
