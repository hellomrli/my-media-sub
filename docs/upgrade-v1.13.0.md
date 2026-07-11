# v1.13.0 升级、验证与回滚指南

## 升级前

1. 停止所有会写入 DATA_DIR 的旧实例，确认没有并行容器或二进制。
2. 通过 `/api/backups/export` 或离线复制保存完整 DATA_DIR，并保留 v1.12.0 二进制/镜像与完整 `static/`。
3. 本版本保持 `schema_version: 1`，无需离线迁移，也不会创建 SQLite。
4. Telegram 控制模式升级后默认 `disabled`；如果计划启用，请先准备 BotFather Token、数字 user/chat ID 白名单和停用方案。

## Docker

```bash
docker pull ghcr.io/hellomrli/my-media-sub:1.13.0
docker compose pull
docker compose up -d
docker compose logs --tail=150 my-media-sub
```

固定 minor 标签可使用 `ghcr.io/hellomrli/my-media-sub:1.13`。

Long polling 可选环境变量：

```dotenv
TELEGRAM_BOT_TOKEN=123456:replace-me
TELEGRAM_CHAT_ID=123456789
TELEGRAM_BOT_MODE=long_polling
TELEGRAM_BOT_ALLOWED_USER_IDS=123456789
TELEGRAM_BOT_ALLOWED_CHAT_IDS=123456789
TELEGRAM_BOT_PRIVATE_ONLY=true
```

Webhook 模式还需公网 HTTPS 根地址及两个至少 24 字符的高熵 Secret；完整说明见 [`telegram-bot.md`](telegram-bot.md)。建议先通过 WebUI 保存自动生成的 Secret，再启用 webhook。

## Linux 二进制

```bash
sha256sum -c my-media-sub-v1.13.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.13.0-linux-x86_64.tar.gz
cd my-media-sub-v1.13.0-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

必须同时替换二进制和整个 `static/`；运行目录需保留发布包中的 `static/`、`docs/`、README 和 CHANGELOG。

## 升级后验证

1. `GET /health` 返回版本 `1.13.0`。
2. Basic Auth 下 `GET /api/diagnostics` 包含 `telegram_bot`，初始模式应为 `disabled`。
3. `GET /api/automation-token/scopes` 包含 `quark:signin`。
4. OpenAPI 包含 `/api/telegram/webhook/{path_secret}` 和 `/api/telegram/audits`。
5. DATA_DIR 中出现权限为 0600 的 `telegram_bot.json`；创建并预览完整备份时该文件通过 schema/哈希校验。
6. Long polling 模式下诊断状态变为 `polling`；Webhook 模式下状态变为 `webhook_active`。
7. 使用非白名单 user/chat 或群聊发送命令时 Bot 不回复，诊断中的未授权计数增加。
8. `/status` 和分页只读命令正常；写命令必须先出现一次性确认按钮，取消或过期后不得执行。
9. 管理面 `GET /api/telegram/audits?limit=20` 返回脱敏审计，不包含 Token、Cookie 或密码。
10. 如配置仓库外的真实沙箱，可运行 `scripts/smoke-telegram.sh` 验证 Bot Token。

## Webhook 与反向代理注意事项

- 只转发 `/api/telegram/webhook/` 到应用，不要移除 `X-Telegram-Bot-Api-Secret-Token`。
- 不要在代理访问日志中长期记录完整随机路径。
- Webhook 路由不使用 Basic Auth；随机路径与 Header Secret 是专用认证，其他管理 API 仍受 Basic/Bearer 保护。
- 普通 Telegram 推送与控制命令限流相互隔离；停用控制可将模式改为 `disabled`，无需删除推送 Chat ID。

## 回滚到 v1.12.0

1. 先将 `TELEGRAM_BOT_MODE=disabled`，或在 WebUI 停用控制；Webhook 模式建议同时通过 BotFather/API 删除 Webhook。
2. 停止 v1.13.0，确保没有进程继续写 DATA_DIR。
3. 恢复 v1.12.0 二进制/镜像及其完整 `static/`，优先恢复升级前备份。
4. v1.12.0 会忽略 Settings 中新增字段；独立 `telegram_bot.json` 不参与旧版业务，但回滚期间不要让新旧版本并行写入。
5. 怀疑 Token 或 Secret 泄露时应在 BotFather 吊销 Token，并轮换随机路径和 Header Secret，而不是只回滚程序。
