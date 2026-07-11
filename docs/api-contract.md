# HTTP API 响应契约

> 适用于 v1.2.0 起的统一响应契约，并包含 v1.3.0 日历接口。本文用于约束后端路由、WebUI 请求层和外部脚本，新增接口不得绕过本契约。

## 1. JSON 成功响应

除下文登记的例外外，成功响应统一为：

```json
{
  "ok": true,
  "data": {}
}
```

可选的人类可读提示位于顶层 `message`；业务对象自身也可以包含业务 `message` 字段。

## 2. JSON 错误响应

业务错误、中间件拒绝和 Axum 框架请求拒绝统一为：

```json
{
  "ok": false,
  "error": "validation_error",
  "message": "可安全展示给用户的错误信息"
}
```

要求：

- `Content-Type` 必须是 `application/json`；
- `error` 是稳定、机器可读的 snake_case 代码；
- `message` 不得包含 Cookie、Token、密码、内部路径或上游完整响应；
- 5xx 的内部细节只写服务端日志；
- 401 必须保留 `WWW-Authenticate: Basic realm="my-media-sub"`；
- 405 等框架响应应保留 `Allow` 等有效响应头。

当前通用错误代码：

| HTTP | error | 说明 |
|---|---|---|
| 400 | `bad_request` / `validation_error` | 参数或业务校验失败 |
| 401 | `unauthorized` | Basic Auth 失败 |
| 403 | `csrf_forbidden` / `forbidden` | 跨站修改或访问被拒绝 |
| 404 | `not_found` | 资源或 API 不存在 |
| 405 | `method_not_allowed` | HTTP 方法不支持 |
| 413 | `payload_too_large` | 请求体过大 |
| 415 | `unsupported_media_type` | Content-Type 不支持 |
| 422 | `invalid_request` | JSON 等请求内容无法解析 |
| 429 | `rate_limited` | 请求频率受限 |
| 500 | `internal_error` / `database_error` / `config_error` | 服务内部错误 |
| 502 | `http_error` | 上游服务失败 |

## 3. 响应例外登记

以下响应不使用普通 JSON 信封：

| 接口 | 类型 | 原因 |
|---|---|---|
| `GET /health` | 裸 JSON `{status, version}` | 免鉴权健康检查，供容器和探针使用 |
| `GET /strm/quark/{fid}/{file_name}` | 媒体流或文本错误 | 直接代理媒体内容，依赖 STRM Token |
| `GET /api/jobs/events` | `text/event-stream` | Job SSE 实时事件流 |
| `DELETE /api/subscriptions/{id}` | `204 No Content` | 删除成功无需响应体 |
| `POST /api/notifications/{id}/read` | `204 No Content` | 状态操作成功无需响应体 |
| `POST /api/notifications/read-all` | `204 No Content` | 状态操作成功无需响应体 |
| `POST /api/notifications/clear` | `204 No Content` | 状态操作成功无需响应体 |

新增例外必须同时更新本文、README、前端请求处理和集成测试。

### Job 队列接口

- `GET /api/jobs` 返回的每个 Job 包含 `priority: high|normal|low`；历史数据缺失该字段时按 `normal` 读取。
- `POST /api/jobs/{id}/priority` 请求体为 `{ "priority": "high|normal|low" }`，只允许调整 `queued` 任务；响应为更新后的 Job 标准成功信封。
- 新任务默认优先级：手动转存和推送为 `high`，订阅转存为 `normal`，元数据刮削为 `low`；重试保留原任务优先级。
- `job_max_concurrency` 控制全局 Job 并发，`job_transfer_max_concurrency`、`job_metadata_max_concurrency`、`job_push_max_concurrency` 控制类别并发，所有值范围为 1–32；同一订阅无论类别始终互斥。
- Job 还返回 `attempt`、`next_attempt_at` 和 `error_class`；错误分类为 `rate_limited|transient|authentication|validation|not_found|permanent|internal|timed_out`。
- `GET /api/jobs/archive?offset=0&limit=100` 分页返回已归档终态任务，单次 limit 最大 500。
- `job_maintenance_mode=true` 时暂停认领新任务但不取消运行中任务；诊断 API 的 `queue` 同时返回延迟重试、重试、超时、归档、维护和积压状态。

### 结构化自动化事件接口

以下接口使用标准 JSON 信封：

- `GET /api/automation/events`；
- `GET /api/automation/summary`；
- `GET /api/subscriptions/{id}/pipeline`，可选 `episode` 查询参数；
- `GET /api/jobs/{id}/pipeline`；
- `POST /api/automation/events/{id}/retry`。

事件阶段、状态机、保留和安全重试规则见 [`automation-events.md`](automation-events.md)。

### 来源质量与安全换源接口

搜索结果和 `SourceCandidate` 可以返回后端权威 `quality` 对象。换源预览、应用、历史和回滚接口均使用标准 JSON 信封：

- `POST /api/subscriptions/{id}/source-candidates/preview`；
- `POST /api/subscriptions/{id}/source-candidates/apply`；
- `GET /api/subscriptions/{id}/source-history`；
- `POST /api/subscriptions/{id}/source-history/rollback`。

手动应用也必须通过候选探测、季度匹配、进度覆盖和历史失败检查。详细规则见 [`source-quality.md`](source-quality.md)。

### 日历接口

`GET /api/calendar` 使用标准成功信封，支持 `from`、`to`、`status`、
`media_type` 和 `subscription` 查询参数。日期统一使用 `YYYY-MM-DD`，
排期时间按 `Asia/Shanghai` 返回，默认范围是当前自然周，闭区间最长 367 个自然日。状态、数据优先级和可信度规则见
`docs/media-calendar.md`。

## 4. WebUI 兼容策略

`static/js/core/api.js` 提供：

- `apiFetch()`：保留原生 Response，统一 HTTP/网络错误；
- `apiJson()`：读取完整 JSON；
- `apiData()`：读取业务数据。

`apiData()` 在升级过渡期兼容：

1. 当前 `{ok:true,data:...}`；
2. 历史 `{data:...}`；
3. 更早的裸 JSON 对象或数组。

新前端代码优先使用 `apiData()`；只有需要状态码、响应头、顶层 `message` 或 204 语义时才使用 `apiFetch()`/`apiJson()`。

## 5. 路由兜底和框架拒绝

- 已认证的未知 `/api/*` 路由返回 JSON 404；
- Basic Auth 和 CSRF 在进入业务路由前返回统一 JSON；
- 非 JSON 的 Axum extractor、405 和其他框架错误由 API 错误规范化中间件转换；
- 非 `/api` 静态资源错误不被转换；
- 已经是 JSON 的 AppError 响应不会被二次包装。

## 6. 测试要求

`tests/api_integration.rs` 至少覆盖：

- 成功信封；
- 业务验证错误；
- 未登录和错误密码；
- CSRF 403；
- 已知资源 404；
- 未知 API 404；
- malformed JSON；
- 405 及 `Allow` 响应头；
- 204 例外；
- 静态资源和健康检查不被错误转换。

## P7 运维与安全接口

- `GET /api/diagnostics`：返回脱敏版本、schema、数据大小、队列、调度器、外部服务配置状态、备份和指标快照。
- `GET /api/diagnostics/export`：下载不含 Cookie、Token 或密码的 JSON 诊断包。
- `GET /api/backups/export`：下载完整 DATA_DIR 自描述 JSON 归档；归档含敏感配置，应加密保管。
- `GET|POST /api/backups`：列出或立即创建服务器备份。
- `POST /api/backups/preview`：返回格式版本、Store schema、安全路径、Base64、大小和 SHA-256 的逐项校验清单。
- `GET|POST /api/backups/verification`：读取最近验证报告，或立即对最新服务器备份执行隔离目录恢复与逐文件哈希复核。
- `POST /api/backups/restore`：请求体必须含 `confirmation: "RESTORE DATA"`；恢复前创建当前快照，响应要求安全重启。

所有响应包含 `X-Request-ID` 和 `X-Correlation-ID`；客户端可提供格式安全的同名请求头。连续五次认证失败后，同一来源在 60 秒窗口内收到 `429` 和 `Retry-After: 60`。

## P18 可观测性与只读诊断接口

- `GET /api/metrics`：返回 JSON 指标快照，包括 HTTP、外部依赖、慢操作与 Store I/O。
- `GET /metrics`：返回 Prometheus 0.0.4 文本格式；与管理 API 一样受 Basic Auth 保护。
- `GET|PUT /api/observability/log-filter`：读取或热更新 tracing `EnvFilter`；PUT 请求为 `{ "filter": "info,my_media_sub=debug" }`，无需重启。
- `GET /api/diagnostics` 的 `environment` 返回 DATA_DIR 容量/权限提示、上海时区偏移、已配置外部服务 DNS 结果及五类 JSON Store 只读一致性检查。
- `recommendations` 仅提供分级建议，不创建文件、不修改权限、不修复数据，也不回显 Cookie、Token、密码或完整外部 URL。

## P9 存储维护

- `GET /api/diagnostics` 的 `metrics.store_io` 返回逐 Store 当前大小、读写次数/字节数、解析/写入累计微秒和失败数。
- `storage_decision` 返回当前规模、显式 SQLite 门槛、建议和迁移合同。
- `GET /api/storage/cleanup`：只读返回各 Store 当前记录、独立保留上限、预计处理数、文件大小、增长预警和 SQLite 阈值决策。
- `POST /api/storage/cleanup` 要求 `confirmation: "CLEANUP DATA"`；执行前创建并验证 `pre-cleanup` 备份，再应用订阅历史、通知、活跃/归档 Job 和自动化事件的独立保留策略。
- `POST /api/storage/compact` 保留兼容入口并要求 `confirmation: "COMPACT JSON"`，内部执行相同的备份优先生命周期清理。
- `GET /api/storage/decision`：返回当前 JSON/SQLite 门控状态；任一记录阈值未达到时 `migration_phase=not_started`，达到后仅变为 `decision_required`，始终报告 `dual_write_active=false`。

## P10 扩展接口

- `GET|POST|DELETE /api/push/browser`：获取 VAPID 公钥、注册或取消浏览器 PushSubscription。
- `POST /api/push/test` 支持 `browser` 和 `webhook` 渠道；Webhook 可使用 HMAC-SHA256 签名。
- `/openapi.json` 为 OpenAPI 3.1 机器契约，`/api-docs.html` 为同源受保护查看页。
- `scripts/check-openapi.py` 从 `src/api/**/*.rs` 的字面量 Axum `.route()` 注册提取路径与方法，要求与 OpenAPI 双向完全一致；非字面量路由和未显式展开的 `any()` 会直接失败。
- `docs/openapi-baseline-v1.12.0.json` 固化 v1.12.0 的 84 条路径、94 个操作及稳定 Success/Error 信封；删除路径/方法或修改稳定信封会作为破坏性变更阻断 CI。
- 新增路由后先运行 `scripts/check-openapi.py --update` 生成基础操作，再补充有意义的 summary、参数、请求体和响应说明，最后运行 `scripts/check-openapi.py` 验证；不得只改代码或只改 JSON。
- 完整数据导入导出使用 `/api/backups/export|preview|restore`；订阅 create/update 支持 `tags`。

## P16 通知策略接口

- `GET /api/push/diagnostics` 返回已配置渠道、事件路由、最低级别、安静时段、摘要/限频和 Webhook 轮换状态；实际连通性继续使用 `POST /api/push/test`。
- `POST /api/push/template/preview` 接收 `{event,title,message,level}`，返回模板渲染结果及最终路由渠道，不执行发送。
- `POST /api/push/webhook/rotate-secret` 接收可选 `overlap_hours`（1–168），响应只显示一次新密钥；重叠期同时发送当前和 previous HMAC-SHA256 签名头。
- 设置支持 `push_event_routes`、`push_min_level`、安静时段、错误绕过、重复窗口、摘要窗口及 `{{title}}/{{message}}/{{event}}/{{level}}` 模板。
- 推送调度使用 detached task 和后台 Job；任何渠道、DNS、Webhook、Store 或模板失败均不得改变核心订阅检查、转存、下载监控和签到结果。


## P20 API 与自动化集成

- `scripts/check-openapi.py` 在 CI 和 Release 中双向核对 91 条 Axum 路径、103 个操作与 OpenAPI；v1.12.0 基线禁止删除既有路径/方法或修改稳定 Success/Error 信封。
- `GET|POST|DELETE /api/automation-token` 仅允许管理员 Basic Auth，用于读取脱敏状态、轮换和撤销单实例 Token；明文只在轮换响应显示一次，磁盘只保存 SHA-256、前缀、scope、有效期和审计时间。
- Bearer Token 按 subscriptions/jobs/notifications/diagnostics 的 read/write/check 最小作用域鉴权；设置、Token 管理、备份恢复、Store 清理和在线升级始终拒绝 Bearer Token。
- `GET /api/subscriptions/export` 返回版本化订阅信封；`POST /api/subscriptions/import/preview` 只读报告冲突，执行接口支持 skip/update/new_id、确认短语及 24 小时有界 Idempotency-Key 去重。
- Webhook v1.0 提供 `version`、`event_id`、`occurred_at`、correlation/subscription/job 标识和 `data`，并发送版本头；签名继续覆盖原始正文，接收端应按 event_id 去重。
- 可运行 curl、Python 和 GitHub Actions 示例见 `docs/automation-api.md`。
