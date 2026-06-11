# 升级指南 v0.4.0

本次升级包含以下主要改进：

## 🎯 主要改进

1. **异步化改造** - 所有网络请求改为 httpx.AsyncClient，大幅提升并发性能
2. **后台任务队列** - 引入 asyncio 任务队列，支持优先级和重试
3. **数据持久化** - 从 JSON 文件迁移到 SQLite + SQLAlchemy
4. **配置管理** - 统一使用 Pydantic BaseSettings
5. **智能去重** - 基于标题相似度和画质偏好的智能去重
6. **Telegram 通知** - 支持 Telegram Bot 推送订阅更新
7. **自动完结检测** - 连续 N 次无更新自动标记完结
8. **代码规范** - ruff + mypy + pre-commit hooks

## 📦 升级步骤

### 1. 安装新依赖

```bash
pip install -r requirements.txt
```

### 2. 数据迁移（重要！）

旧版本使用 JSON 文件存储，新版本使用 SQLite。需要运行迁移脚本：

```bash
python -m src.scripts.migrate_to_db
```

这会将以下数据迁移到数据库：
- 订阅列表 → `subscriptions` 表
- 通知记录 → `notifications` 表
- 设置 → 环境变量 / .env 文件

**注意**：迁移前请备份 `data/` 目录！

### 3. 更新配置

新版本使用 `.env` 文件统一配置。参考 `.env.example` 更新你的配置：

```bash
cp .env.example .env.new
# 将旧配置迁移到 .env.new
# 然后替换
mv .env.new .env
```

新增配置项：

```env
# 数据库
DATABASE_URL=sqlite+aiosqlite:///./data/app.db

# Telegram 通知
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
NOTIFICATION_LEVEL=info

# 画质偏好（逗号分隔）
PREFERRED_QUALITY_ORDER=4K,2160p,1080p,720p,480p

# 自动完结
AUTO_COMPLETE_AFTER_NO_UPDATES=5
```

### 4. 更新 Docker Compose（如果使用 Docker）

```yaml
services:
  app:
    build: .
    volumes:
      - ./data:/app/data
    environment:
      - DATABASE_URL=sqlite+aiosqlite:////app/data/app.db
      - TELEGRAM_BOT_TOKEN=${TELEGRAM_BOT_TOKEN}
      - TELEGRAM_CHAT_ID=${TELEGRAM_CHAT_ID}
```

### 5. 安装 pre-commit hooks（可选）

```bash
pip install pre-commit
pre-commit install
```

### 6. 重启服务

```bash
# 开发环境
uvicorn src.app_updated:app --reload

# Docker
docker compose down
docker compose up -d --build
```

## 🔄 API 变更

### 新增接口

- `GET /api/queue/status` - 查看任务队列状态

### 变更接口

- 所有搜索和订阅接口现在都是异步的，响应时间大幅减少
- 搜索结果新增 `quality` 和 `quality_rank` 字段

## ⚠️ 破坏性变更

1. **数据存储格式变更**
   - 从 JSON 文件迁移到 SQLite
   - 需要运行迁移脚本

2. **配置文件变更**
   - `settings.json` 废弃，改用 `.env`
   - 需要手动迁移配置

3. **Python 版本要求**
   - 最低要求 Python 3.11（因为使用了新的类型注解）

## 🐛 已知问题

1. 迁移脚本尚未完成 - 需要手动创建数据库和迁移数据
2. WebUI 部分功能尚未适配新的异步 API

## 📝 TODO

- [ ] 完成数据迁移脚本
- [ ] 更新 WebUI 以支持新功能（画质显示、批量操作）
- [ ] 补充单元测试
- [ ] 性能基准测试

## 🔙 回滚方案

如果升级后遇到问题，可以回滚到旧版本：

```bash
git checkout v0.3.0
pip install -r requirements.txt
docker compose down
docker compose up -d --build
```

数据库迁移暂时不支持回滚，请确保已备份 `data/` 目录。
