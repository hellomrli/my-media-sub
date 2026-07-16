# v2.2.0

## 变更

- 暂时下线 STRM HTTP 代理、生成/审计 API 和 WebUI 配置；保留旧数据字段，后续以独立 Rust 模块重新接入。
- 新增 Rust 原生 `PostTransferModule` 转存后处理模块注册表。
- Telegram Bot 新增 `/subscription <ID>` 与 `/job <ID>` 详情查询。
- 首屏请求并发化，Rust 服务端启用 Brotli/Gzip 压缩和静态资源缓存。

## 兼容性

- JSON Store schema 未变化。
- v2.1.2 的 `strm_*` 字段可以继续读取，但 v2.2.0 不会执行 Strm 生成或提供 Strm HTTP 代理。
