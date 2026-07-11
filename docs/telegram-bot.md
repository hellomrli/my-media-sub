# Telegram 主动控制 Bot

P21-01 提供单实例、单管理员场景的 Telegram **只读**控制入口。它与现有 Telegram 通知推送共用 `TELEGRAM_BOT_TOKEN`，但控制面默认关闭；启用前必须配置数字 user/chat ID 白名单。

## 安全边界

- 授权只比较 Telegram `from.id` 与 `chat.id` 数字值，不读取或信任 username、昵称。
- `telegram_bot_private_only` 默认为 `true`。即使群组 chat ID 位于白名单中，也会拒绝群聊 Update；确有需要时才显式关闭。
- Webhook 同时校验随机 URL 路径和 `X-Telegram-Bot-Api-Secret-Token`，任一不符均返回 404。
- 未授权 Update 不回复任何内容，只写入按 user/chat 每分钟最多一次的安全日志；诊断仅累计数量。
- Bot Token、Webhook 路径密钥和 Header Secret 均按设置密钥脱敏，不写入日志。Telegram API 错误会清理 Token 并截断到 300 字符。
- 当前命令均为预定义只读查询；不执行 Shell、不读取任意文件、不回显 Cookie/Token/密码，也不直接修改 Store。

## BotFather 准备

1. 在 Telegram 中联系 `@BotFather`，执行 `/newbot` 创建 Bot。
2. 保存 Bot Token，并通过 WebUI“设置 → 推送渠道 → Telegram”填写；不要把 Token 提交到仓库或粘贴到日志。
3. 可通过可信 Bot/API 获取自己的数字 user ID 和私聊 chat ID。不要把 username 写入白名单。
4. 建议通过 BotFather 的 `/setcommands` 设置：

```text
start - 连接说明
help - 命令列表
status - 系统概况
subscriptions - 订阅列表
calendar - 本周排期
jobs - 最近任务
notifications - 未读通知
diagnostics - Bot 诊断
check - 检查订阅（需要确认）
retry - 重试允许的任务（需要确认）
cancel - 取消允许的任务（需要确认）
signin - 执行夸克签到（需要确认）
read - 标记通知已读（需要确认）
```

## Long polling 部署

Long polling 不要求公网回调地址，适合 NAT、家庭网络和仅出站访问环境。

WebUI 配置：

1. 填写 Bot Token。
2. “主动控制接入模式”选择 `Long polling`。
3. 填写允许的 User ID 和 Chat ID；私聊场景通常两者相同。
4. 保持“仅允许私聊”开启并保存。

服务会先调用 `deleteWebhook`，再使用最长 25 秒的 `getUpdates`。网络或 Telegram 429/5xx 不会终止应用，失败按 2–60 秒退避并展示在诊断中。配置从 WebUI 修改后后台任务会自动重新协调，不需要重启。

等价环境变量：

```dotenv
TELEGRAM_BOT_TOKEN=123456:replace-me
TELEGRAM_CHAT_ID=123456789
TELEGRAM_BOT_MODE=long_polling
TELEGRAM_BOT_ALLOWED_USER_IDS=123456789
TELEGRAM_BOT_ALLOWED_CHAT_IDS=123456789
TELEGRAM_BOT_PRIVATE_ONLY=true
```

## Webhook 部署

Webhook 必须具备 Telegram 可访问的公网 HTTPS 根地址。反向代理应将 `/api/telegram/webhook/` 原样转发，不要记录完整 URL，也不要移除 `X-Telegram-Bot-Api-Secret-Token`。

WebUI 配置：

1. 填写 Token、User ID 和 Chat ID 白名单。
2. 选择 `Webhook`，填写公网根地址，例如 `https://media.example.com`，不要附加 `/api/telegram/...`。
3. 系统首次创建设置文件时会生成随机路径 Secret 和 Header Secret；保持系统生成值即可，也可替换为至少 24 字符的强随机值。
4. 保存后服务会调用 Telegram `setWebhook`，完整回调形如：
   `https://media.example.com/api/telegram/webhook/<随机路径>`。

环境变量部署时需自行提供两个高熵 Secret：

```dotenv
TELEGRAM_BOT_MODE=webhook
TELEGRAM_BOT_ALLOWED_USER_IDS=123456789
TELEGRAM_BOT_ALLOWED_CHAT_IDS=123456789
TELEGRAM_BOT_PRIVATE_ONLY=true
TELEGRAM_BOT_WEBHOOK_PUBLIC_URL=https://media.example.com
TELEGRAM_BOT_WEBHOOK_PATH_SECRET=<至少24字符随机值>
TELEGRAM_BOT_WEBHOOK_SECRET=<至少24字符随机值>
```

反向代理不要给此路径追加 Basic Auth；应用内部的随机路径和 Header Secret 是回调认证。其他 `/api/*` 管理接口仍受 Basic Auth/Bearer scope 保护。

## 只读命令与分页

| 命令 | 说明 |
|---|---|
| `/start`、`/help` | 连接说明和允许的命令 |
| `/status` | 版本、订阅、任务和未读通知概况 |
| `/subscriptions [页码]` | 每页 8 条订阅及状态/进度 |
| `/calendar [页码]` | 当前上海时区自然周排期，每页 8 条 |
| `/jobs [页码]` | 最近更新任务，每页 8 条 |
| `/notifications [页码]` | 未读通知，每页 8 条 |
| `/diagnostics` | 接入模式、运行状态、最近 Update/成功时间和脱敏错误 |

回复按 Unicode 字符边界切分，每条最多 3,500 字符，一次命令最多发送 4 条消息。未知命令只返回帮助，不会转发为系统命令。

## 诊断与应急停用

管理诊断 `GET /api/diagnostics` 的 `telegram_bot` 字段包含模式、状态、最近 Update、最近成功、脱敏错误和未授权计数。Bot 内 `/diagnostics` 提供相同的安全子集。

应急处理顺序：

1. 将模式改为 `disabled` 或设置 `TELEGRAM_BOT_MODE=disabled` 并重启；普通 Telegram 推送仍可继续使用。
2. 怀疑 Token 泄露时立即在 BotFather 吊销并生成新 Token，然后更新设置。
3. Webhook Secret 泄露时轮换随机路径和 Header Secret；服务会重新调用 `setWebhook`。
4. 检查反向代理访问日志是否意外记录完整随机路径，并按需要清理/轮换。

## 受控写命令与二次确认

P21-02 只开放下列白名单动作。每个动作复用与 P20 自动化 API 相同的最小 scope 和现有 Service/JobQueue，不直接修改业务 Store：

| 命令 | 最小 scope | 行为 |
|---|---|---|
| `/check <订阅ID>` | `subscriptions:check` | 检查一个订阅 |
| `/check all` | `subscriptions:check` | 检查全部符合条件的订阅 |
| `/retry <Job ID>` | `jobs:write` | 仅重试现有 Queue 允许重试的失败/取消任务 |
| `/cancel <Job ID>` | `jobs:write` | 仅取消 JobQueue 允许取消的任务 |
| `/signin` | `quark:signin` | 执行既有夸克签到服务 |
| `/read <通知ID>`、`/read all` | `notifications:write` | 标记单条/全部通知已读 |

订阅、Job 和通知列表会显示短 ID；命令接受唯一 ID 前缀，前缀不唯一时拒绝执行。

所有写命令先返回 Inline Keyboard：

1. 服务生成 120 秒有效的一次性 nonce。
2. nonce 绑定 Telegram 数字 user ID、chat ID、动作、资源和最小 scope。
3. “确认”和“取消” Callback 只能由原 user/chat 使用；跨用户、跨 chat、过期、重启后或重复 Callback 均拒绝。
4. 确认时原子移除 nonce，再以 `update_id`、Callback Query ID 和业务 idempotency key 三层去重。
5. 结果返回 `request`、`correlation`，Job 操作同时返回 `job` ID，可回到 WebUI 自动化时间线排查。

确认状态故意只保存在内存中，因此重启会安全地使所有待确认按钮失效。已处理的 Update、Callback 和业务幂等键则保存在私有权限 `telegram_bot.json` 中，重启后仍可阻止重放。

不会开放删除订阅、恢复备份、Store 清理、直接 Store 编辑、任意路径读取或任意命令执行。现有高风险管理接口仍只能通过 Basic Auth 管理面完成。

## 主动通知按钮

Telegram 仍经过通知中心原有的事件开关、渠道路由、最低级别、上海时区安静时段、错误绕过、重复限频和摘要策略。仅当通知最终选择 Telegram 渠道、控制 Bot 已启用、且恰好配置一个允许 user ID 时，消息才附加受限按钮：

- `查看详情`：显示对应通知的安全字段；
- `标记已读`：再次进入一次性确认；
- 订阅更新、失效或完结时的 `重新检查`：再次进入一次性确认。

主动通知 Callback 使用 15 分钟有效的 HMAC-SHA256 签名，签名材料绑定动作、资源、有效期、允许 user ID 和推送 chat ID；Callback 数据限制在 Telegram 的 64 字节以内。签名错误、跨用户、跨 chat 和过期按钮均不会创建确认。

普通 Telegram 推送不依赖 Bot 命令处理状态。命令限流、连续失败冷却或审计写入失败不会停用普通推送渠道。

## 审计、限流与持久化

每条授权命令和已确认动作记录以下脱敏字段：

- Update ID、Callback Query ID；
- 数字 user/chat ID；
- 命令、目标、结果与耗时；
- request/correlation；
- 截断并清理 Cookie、Token、Password、Secret、Key、Authorization/Bearer 的结果摘要。

管理面可通过下列只读接口查看最近审计：

```bash
curl -u admin:password 'http://localhost:56001/api/telegram/audits?limit=100'
```

自动化 Token 需要 `diagnostics:read`。审计、Update、Callback 和业务幂等记录各自最多保留 2,000 条，并随完整 DATA_DIR 备份/恢复校验。

命令限流按 user、chat、command 三层执行：user 每分钟 20 次、chat 每分钟 30 次、单个写命令每分钟 6 次、单个只读命令每分钟 15 次。连续三次写动作失败后对应 user/chat 冷却 60 秒。未授权安全日志仍按 user/chat 每分钟最多一条；这些限制不共享普通推送的状态。

## 确定性测试与真实沙箱 Smoke

无网络 CI 覆盖随机路径/Header Secret、数字 ID 白名单、私聊限制、重复 Update/Callback、持久化重启去重、过期/跨用户确认、并发一次性领取、签名按钮、分层限流、连续失败冷却、Telegram 429/5xx 脱敏和核心 Store 故障隔离。

可选真实沙箱检查：

```bash
export TELEGRAM_BOT_TOKEN='123456:replace-me'
scripts/smoke-telegram.sh

# 明确允许发送一条静默测试消息时：
export TELEGRAM_CHAT_ID='123456789'
export TELEGRAM_SMOKE_SEND=true
scripts/smoke-telegram.sh
```

CI 会调用同一脚本；未配置仓库 Secret 时安全跳过，配置 `TELEGRAM_BOT_TOKEN` 后执行 `getMe`。默认不会发送消息。
