# v1.2.0

v1.2.0 将 WebUI 升级为 Media Deck，并完成 API、数据存储和订阅状态体系的第一轮结构化收口。由于 JSON API 响应和持久化文件格式发生变化，本版本按次版本发布。

## 新增

- Media Deck 应用壳层、深浅主题、响应式工作台和全局快捷搜索。
- 资源搜索海报/列表双视图、质量评分、有效性/风险标签、筛选和排序。
- 独立订阅详情路由、逐集状态网格、缺集检测、自动化流水线和活动时间线。
- `GET /api/subscriptions/{id}/status` 聚合接口。
- 网盘面包屑、搜索筛选、批量选择、批量删除和批量提交 Aria2。
- Aria2 任务的订阅、集数、目标目录、转存、重命名和 STRM 关联状态。
- 按连接、自动化、命名规则、通知和维护组织的任务型设置中心。
- `static/js/core/api.js`、`formatters.js` 和可测试的搜索/订阅详情功能模块。
- 前端 Node 单元测试和 CI JavaScript 语法检查。
- 持久化开发路线 `docs/roadmap.md`、API 契约和升级回滚指南。

## API 变化

- JSON 成功响应统一为 `{"ok":true,"data":...}`。
- 应用错误统一为 `{"ok":false,"error":"...","message":"..."}`。
- Basic Auth、CSRF、已知/未知 404、405 和请求解析错误统一返回 JSON。
- 401 保留 `WWW-Authenticate`，405 保留 `Allow` 等有效响应头。
- WebUI `apiData()` 兼容当前信封、旧 `{data:...}` 信封和历史裸响应。
- `/health`、STRM、Job SSE 和成功的 204 操作为登记例外。

## 数据安全与兼容

- `settings.json`、`subscriptions.json`、`notifications.json` 和 `jobs.json` 使用 `schema_version: 1` 信封。
- 旧裸 JSON 首次加载时自动迁移。
- 迁移前创建一次性 `*.schema-v0.bak` 原始备份，不覆盖已有备份。
- 业务数据和迁移备份在 Unix 上自动修复为 `0600`。
- 未来 schema 会拒绝读取，但不会被当作损坏文件隔离或覆盖。
- 真正损坏的 JSON 继续隔离为 `.corrupt-<timestamp>`。
- Store 写盘成功后才更新内存，失败不会产生运行态和磁盘状态分叉。
- 删除从未启用、也从未公开的 `nas_sync_*` 占位字段；旧 JSON 字段可安全忽略。

## 行为优化

- 未配置 Aria2 时停止下载任务轮询，避免全新安装持续产生 400 请求。
- 下载、签到、换源、转存和推送页面适配统一 API 响应。
- 搜索结果和网盘时间统一使用共享格式化工具。
- API 未知路由不再落入静态资源 404。

## 测试和发布

- 19 个前端单元测试。
- 284 个 Rust 测试通过，1 个真实网络测试按设计忽略。
- HTTP 集成测试覆盖成功信封、验证错误、401、403、404、405、malformed JSON、204、SSE、STRM 和静态资源。
- Release 工作流执行前端检查、rustfmt、clippy、完整测试、tag/version 校验和 Release 构建。
- Release 归档包含二进制、静态资源、README、CHANGELOG 和 docs。

## 升级注意事项

- 外部 API 脚本需要从 `.data` 读取业务对象。
- v1.1.x 二进制不能直接读取 schema v1 数据文件。
- 回滚时必须同时恢复旧二进制、旧静态资源和 `.schema-v0.bak` 或完整 DATA_DIR 备份。
- 完整步骤见 `docs/upgrade-v1.2.0.md`。
