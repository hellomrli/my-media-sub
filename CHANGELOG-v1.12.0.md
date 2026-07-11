# v1.12.0

v1.12.0 完成 P17–P19，重点提升移动端 WebUI、可观测性、故障排查、备份可恢复性和 JSON Store 生命周期治理。本版本继续面向单用户夸克自动化，保持 `schema_version: 1`，不引入第二网盘、多用户或 SQLite 双写。

## WebUI 与移动端

- 移动端订阅详情使用安全区、粘性导航和操作栏、44px 触控目标及单列弹窗。
- 订阅支持当前窗口全选、受限并发检查和短语确认批量删除；列表筛选与视图偏好持久化。
- 订阅、Job、通知和网盘大列表使用分段可见窗口与加载更多，避免一次渲染全部记录。
- 统一 loading、空状态、错误边界、危险确认、键盘焦点与诊断复制体验。
- 自动化详情提供有界时间线和原始诊断 JSON；真实浏览器持续验证 390×844 与 1440×1000。

## 可观测性与故障诊断

- HTTP、订阅检查和后台 Job 串联 `request_id`、`correlation_id`、`subscription_id` 与 `job_id`。
- Job 持久化关联上下文并保持旧数据兼容；自动换源、转存和推送沿用同一 correlation。
- `LOG_FORMAT=json` 输出结构化 JSON；`GET|PUT /api/observability/log-filter` 可热更新 EnvFilter。
- `GET /metrics` 提供受认证的 Prometheus 0.0.4 指标；JSON 指标继续由 `/api/metrics` 提供。
- 所有外部 HTTP 请求记录服务级次数、非 2xx/传输失败、累计和最大延迟；HTTP、订阅、转存及 Job 支持慢操作告警。
- 诊断快照增加 DATA_DIR 容量/权限、上海时区偏移、已配置服务 DNS、五类 Store 一致性和只读处理建议。

## 备份与数据生命周期

- 备份预览返回格式、Schema、安全路径、Base64、大小、SHA-256、业务模型和 settings 完整性清单。
- 每次服务器备份在隔离目录完整恢复并逐文件复核；成功和失败报告持久化，可定期或手动重验。
- `BACKUP_EXTERNAL_DIR` 支持将已验证备份以 0600 权限原子复制到 DATA_DIR 外部，并独立清理历史副本。
- `/api/storage/cleanup` 提供 Store 记录数、独立保留上限、预计处理量、文件大小和增长预警的只读预览。
- 清理要求 `CLEANUP DATA`，在变更前创建并验证 `pre-cleanup` 备份，再原子应用订阅、通知、Job 和自动化事件保留策略。
- SQLite 决策门继续使用 500 订阅、10,000 历史记录、32 MiB Store 或复杂查询需求；达到门槛也只进入决策，不自动建库或长期双写。

## API 与运维

- 新增 `/metrics`、`/api/observability/log-filter`、`/api/backups/verification`、`/api/storage/cleanup` 和 `/api/storage/decision`。
- OpenAPI、README、环境变量示例、架构与 API 合同同步更新。
- Linux x86_64 发布包包含二进制、完整 `static/`、文档、README 和本变更记录。
- GHCR 发布 `1.12.0`、`1.12`，同时维护 `latest` 和 main/sha 开发标签。

## 兼容性

- `schema_version` 仍为 1，无需离线数据迁移。
- Job 新关联字段均有默认值，旧 Job 可直接读取。
- 新环境变量均有安全默认值；未配置外部备份目录时保持原行为。
- 不要让不同版本同时写入同一个 DATA_DIR；升级与回滚必须同时替换二进制和完整 `static/`。
