# v2.2.2

## 修复

- PWA 关键 JS、CSS 和 Worker 改为 network-first；在线时不再先执行旧缓存，离线时仍可回退到当前版本缓存。
- 提升 Service Worker 缓存版本，升级后会激活新 Worker 并清理 v2.1.2 遗留的应用壳层与静态缓存。
- 海报、搜索结果和日历缩略图发生临时网络错误时自动退避重试两次，不再设置会跨后续状态持续生效的原生 `hidden`。
- 发布测试从 `Cargo.toml` 动态校验 PWA 缓存版本，避免后续版本再次漏升缓存键。

## 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.1 升级，保留现有 `data/`。
