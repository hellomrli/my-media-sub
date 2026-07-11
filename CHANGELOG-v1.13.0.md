# v1.13.0

v1.13.0 完成 P20–P21，在 v1.12.0 的诊断与数据生命周期基线上增加稳定自动化 API 和 Telegram 主动控制机器人。本版本继续服务单实例、单管理员场景，保持 `schema_version: 1` 和 JSON 单写，不引入多用户、任意远程 Shell 或 SQLite 双写。

## API 与自动化集成

- OpenAPI 与 Axum literal route 自动双向核对，当前登记 91 条路径、103 个操作。
- `docs/openapi-baseline-v1.12.0.json` 继续作为兼容基线，删除稳定路径/方法或修改 Success/Error 信封会阻断 CI 和 Release。
- 单实例自动化 Token 只保存 SHA-256 哈希，支持轮换、撤销、过期、最后使用时间和最小 scope。
- Token scope 覆盖订阅读取/写入/检查、Job 读取/写入、通知读取/写入、诊断读取和 `quark:signin`；设置、备份恢复、清理和升级接口不向 Token 开放。
- 订阅导出使用版本化信封；导入支持冲突预览、skip/new_id 策略、确认短语、原子批量 Store 写入和 Idempotency-Key 重放保护。
- 推送 Webhook 使用版本化事件信封，包含 event/request/correlation/subscription/job 上下文，并保留 HMAC 双签名轮换能力。

## Telegram 安全接入与只读控制

- 支持 `disabled`、long polling 和 webhook 三种模式，单实例同一时间只启用一种接入。
- Long polling 自动删除旧 Webhook，使用最长 25 秒 `getUpdates` 并在失败时 2–60 秒退避。
- Webhook 使用随机 URL 路径和 `X-Telegram-Bot-Api-Secret-Token` 双重常量时间校验；两个 Secret 首次创建设置时自动生成并按密钥脱敏。
- 授权只使用 Telegram 数字 user ID 和 chat ID，不信任 username；默认仅允许 private chat。
- 提供 `/start`、`/help`、`/status`、`/subscriptions`、`/calendar`、`/jobs`、`/notifications`、`/diagnostics`，列表分页且消息按 Unicode 边界限制长度。
- 诊断 API 和 WebUI 展示接入模式、运行状态、最近 Update/成功、脱敏错误、审计、待确认、去重和限流统计。

## 受控写命令与交互确认

- 增加 `/check <订阅ID|all>`、`/retry <Job ID>`、`/cancel <Job ID>`、`/signin`、`/read <通知ID|all>`。
- 每个写动作映射到 P20 最小 scope，并复用 `SubscriptionCheckService`、`JobQueue`、`QuarkSigninService` 和 `NotificationStore`，不复制业务规则。
- 所有写命令使用 120 秒一次性 Inline Keyboard 确认，绑定 user/chat/action/resource/scope；跨用户、跨 chat、过期、重启后和重复确认均拒绝。
- `update_id`、Callback Query ID 和业务 Idempotency-Key 三层持久化去重；结果返回 request/correlation，Job 操作同时返回 Job ID。
- 明确不开放删除订阅、恢复备份、清理 Store、直接 Store 编辑、任意路径读取或任意命令执行。

## 主动通知、审计与限流

- Telegram 通知可附加 HMAC-SHA256 签名的查看详情、标记已读和重新检查按钮；签名绑定动作、资源、有效期、user/chat，Callback 数据不超过 64 字节。
- 追更、失效、完结、转存、下载和队列积压继续经过事件开关、渠道路由、最低级别、上海时区安静时段、错误绕过、重复限频和摘要策略。
- 新增私有权限 `telegram_bot.json`，持久化最近 2,000 条 Update、Callback、业务幂等键和脱敏命令审计，并纳入完整 DATA_DIR 备份/恢复校验。
- `GET /api/telegram/audits` 通过 Basic Auth 或 `diagnostics:read` Token 返回有界审计列表。
- 命令按 user、chat、command 三层限流；连续三次写动作失败后冷却 60 秒，状态不与普通 Telegram 推送共享。
- 错误和审计清理 Bot Token、Webhook Secret、Cookie、Token、Password、Key、Authorization/Bearer，并限制输出长度。

## 测试、文档与运维

- 无网络测试覆盖 Webhook 双 Secret、白名单、伪造 username、重复 Update/Callback、重启恢复、过期/跨用户 Callback、并发确认、HMAC 按钮、429/5xx 和核心流水线隔离。
- 增加 `scripts/smoke-telegram.sh`；CI 未配置 Secret 时安全跳过，配置沙箱 Token 后执行真实 `getMe`，发送测试消息仍需显式开启。
- 增加 BotFather、long polling、Webhook 反向代理、命令、Token 吊销和应急停用完整指南。
- 426 个 Rust 测试登记，425 个通过、1 个真实 PanSou 网络测试按设计忽略；14 个前端 Node 测试通过。

## 兼容性与升级

- `schema_version` 保持 1；Settings 新字段均有默认值，旧实例数据可直接加载。
- 首次启动会创建 `telegram_bot.json` 并生成 Webhook Secret；控制模式默认 `disabled`，不会因升级自动接收命令。
- v1.12.0 自动化 API 基线保持兼容，新增路由和 `quark:signin` scope 为向后兼容扩展。
- 必须同时替换二进制和完整 `static/`；不要让不同版本同时写入同一个 DATA_DIR。
