# v2.7.5 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.7.5 不修改 JSON Store schema，可直接复用 v2.7.4 的 `data/`。

本版本是搜索性能修复 + 依赖安全补丁：

- 搜索探测改为最多 8 路并发，并增加 60 秒服务端总预算；到点返回已完成的部分结果。默认“嗅探文件 + 过滤失效”搜索不再撞前端 120 秒超时。
- 修复 4 条 RustSec 公告，`p256` 升级到 0.14.0；依赖审计继续保持零例外。
- Docker 构建阶段固定为 Rust 1.97.0，与 `rust-toolchain.toml` 一致。

前端资源 URL 带有 `?v=2.7.5`。升级后如果浏览器仍加载旧资源，刷新页面或等待 Service Worker 更新。

真实浏览器推送链路无法在 CI 中端到端验证；升级后建议对至少一个订阅手动发送一次，并确认设备收到。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
