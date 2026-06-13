# Rust Migration Plan V2 - 可执行方案

> **基于现状核实的真实迁移方案**  
> 创建时间：2026-06-13  
> 分支：`rust-rewrite-v2`

---

## 🎯 核心决策（已拍板）

1. **重新设计 API + 重写前端**：不做 Python 接口的精确替身，而是设计更清晰的 REST API
2. **搜索先做 Remote 服务**：只移植 `RemotePanSouClient`，调用 `pansou.lxf87.com.cn/api/search`
3. **对等后完整切换**：功能对等并验证后，改 Dockerfile/compose/Actions 指向 Rust，移除 Python

---

## 📊 Python 真实架构分析

### 数据模型（真相源：JSON 文件）

#### Subscription（订阅）
```json
{
  "id": "abc123def456",
  "title": "某动画 第一季",
  "source_title": "【某字幕组】某动画",
  "media_type": "anime",  // "movie" | "series" | "anime" | "custom_*"
  "season": 1,
  "current_episode_number": 12,
  "total_episode_number": 24,
  "source_group": "某字幕组",
  "cloud_type": "quark",
  "url": "https://pan.quark.cn/s/...",
  "password": "",
  "known_files": ["第01集.mkv", "第02集.mkv"],
  "known_file_keys": ["file_key_1", "file_key_2"],
  "known_episodes": [1, 2, 3],
  "transferred_files": [],
  "transferred_file_keys": [],
  "last_probe": { "ok": true, "files": [...] },
  "last_plan_summary": "匹配 3 个文件...",
  "notify_only": false,
  "enabled": true,
  "completed": false,
  "rules": {
    "target_dir": "/某动画",
    "rename_template": "某动画.S01E{}",
    "season_pattern": "S(\\d{2})",
    "episode_pattern": "E(\\d{2,3})",
    "filter_pattern": "",
    "finish_after_episode": null
  },
  "created_at": 1718236800,
  "updated_at": 1718323200,
  "last_checked_at": 1718323200,
  "last_new_files": ["第03集.mkv"],
  "last_new_episodes": [3],
  "last_check_summary": "匹配 3 个文件，规划新增 1 个...",
  "check_history": [
    {
      "time": 1718323200,
      "state": "active",
      "matched_count": 3,
      "transfer_count": 1,
      "new_files": ["第03集.mkv"],
      "new_episodes": [3],
      "summary": "..."
    }
  ],
  "status": "active",  // "active" | "completed" | "invalid"
  "invalid_since": null,
  "last_error": ""
}
```

**核心字段说明：**
- `known_files` / `known_file_keys`：所有见过的文件（去重用）
- `transferred_files` / `transferred_file_keys`：已转存的文件
- `rules`：转存规则（目标目录、重命名模板、正则）
- `last_probe`：最近一次网盘探测结果
- `check_history`：最近 30 次检查历史

#### Settings（设置）
```json
{
  "app_username": "admin",
  "app_password": "***",
  "cloud_types": ["quark"],
  "check_links": true,
  "probe_quark_files": true,
  "filter_bad_links": true,
  "quark_cookie": "***",
  "quark_save_enabled": true,
  "quark_save_root": "",
  "quark_save_movie_dir": "/电影",
  "quark_save_series_dir": "/连续剧",
  "quark_save_anime_dir": "/动画",
  "custom_categories": [],
  "subscription_scheduler_enabled": true,
  "subscription_check_interval_minutes": 60,
  "auto_download_new_subscription_items": false,
  "aria2_rpc_url": "",
  "aria2_secret": "",
  "aria2_dir": "",
  "nas_sync_enabled": false,
  "nas_sync_source": "",
  "nas_sync_target": "",
  "telegram_bot_token": "",
  "telegram_chat_id": "",
  "bark_url": "",
  "gotify_url": "",
  "gotify_token": "",
  "pushplus_token": "",
  "serverchan_key": "",
  "wecom_bot_url": "",
  "wxpusher_app_token": "",
  "wxpusher_uids": "",
  "push_on_update": true,
  "push_on_failed": true,
  "push_on_completed": true,
  "push_on_save": true,
  "push_silent": false
}
```

#### SearchSession（搜索会话，内存态）
```python
{
  "chat_id": "web-session-xyz",
  "keyword": "某动画",
  "results": [
    {
      "title": "【某字幕组】某动画 全24集",
      "url": "https://pan.quark.cn/s/...",
      "password": "",
      "source": "某字幕组",
      "cloud_type": "quark",
      "probe": {
        "ok": true,
        "state": "ok",
        "files": [
          {"name": "第01集.mkv", "size": 123456789, "file_key": "..."}
        ]
      }
    }
  ],
  "created_at": 1718323200
}
```

### API 接口（真实 Python 版本）

#### 订阅 API
```
GET  /api/subscriptions              # 列出所有订阅
POST /api/subscriptions              # 创建订阅（从搜索结果）
POST /api/subscriptions/update       # 更新订阅
POST /api/subscriptions/check        # 检查单个订阅
POST /api/subscriptions/check-all    # 检查所有订阅
POST /api/subscriptions/plan         # 规划转存
POST /api/subscriptions/delete       # 删除订阅
```

#### 搜索 API
```
POST /api/search                     # 搜索资源
POST /api/select                     # 选择搜索结果（保存到会话）
```

#### 设置 API
```
GET  /api/settings                   # 获取设置
POST /api/settings                   # 更新设置
POST /api/settings/test/quark        # 测试夸克 Cookie
POST /api/settings/test/mount-paths  # 测试挂载路径
POST /api/settings/test/nas-sync     # 测试 NAS 同步
GET  /api/cloud-types                # 获取云盘类型列表
```

#### 推送 API
```
POST /api/push/test                  # 测试所有推送渠道
POST /api/push/test/{channel}        # 测试单个渠道
GET  /api/push/history               # 推送历史
GET  /api/push/stats                 # 推送统计
GET  /api/push/daily-summary         # 获取每日摘要数据
POST /api/push/daily-summary         # 发送每日摘要
```

#### 夸克网盘 API（略）
```
POST /api/quark/*                    # 各种夸克操作
```

#### 通知 API
```
GET  /api/notifications              # 获取通知列表
POST /api/notifications/mark-read    # 标记已读
POST /api/notifications/clear        # 清空通知
```

---

## 🦀 新 Rust 架构设计

### 技术栈

- **Web 框架**：Axum 0.7（已在用）
- **异步运行时**：Tokio（已在用）
- **HTTP 客户端**：reqwest（已在用）
- **JSON**：serde + serde_json（已在用）
- **日志**：tracing + tracing-subscriber（已在用）
- **存储**：JSON 文件（保持兼容）
- **认证**：HTTP Basic Auth

### 目录结构（新设计）

```
my-media-sub/
├── src/
│   ├── main.rs                      # 入口 + Axum 服务器
│   ├── config.rs                    # 配置加载（环境变量）
│   ├── error.rs                     # 统一错误处理
│   │
│   ├── models/                      # 数据模型（与 Python JSON 兼容）
│   │   ├── mod.rs
│   │   ├── subscription.rs          # Subscription 完整结构
│   │   ├── settings.rs              # Settings 完整结构
│   │   ├── search.rs                # SearchSession, SearchResult
│   │   ├── transfer.rs              # TransferPlan, TransferItem
│   │   └── notification.rs          # Notification
│   │
│   ├── store/                       # JSON 存储层
│   │   ├── mod.rs
│   │   ├── json_store.rs            # 通用 JSON 原子写入
│   │   ├── subscription.rs          # SubscriptionStore
│   │   ├── settings.rs              # SettingsStore
│   │   ├── session.rs               # SessionStore（内存）
│   │   └── notification.rs          # NotificationStore
│   │
│   ├── services/                    # 业务逻辑层
│   │   ├── mod.rs
│   │   ├── subscription.rs          # 订阅管理逻辑
│   │   ├── transfer_rule.rs         # 转存规则引擎（核心）
│   │   ├── episode.rs               # 集数解析（移植 utils/episode.py）
│   │   ├── search.rs                # 搜索服务
│   │   ├── push.rs                  # 推送编排
│   │   └── scheduler.rs             # 定时任务
│   │
│   ├── clients/                     # 外部客户端
│   │   ├── mod.rs
│   │   ├── quark/                   # 夸克网盘客户端
│   │   │   ├── mod.rs
│   │   │   ├── client.rs            # 主客户端
│   │   │   ├── save.rs              # 转存操作
│   │   │   └── types.rs             # 夸克 API 类型
│   │   ├── pansou.rs                # PanSou Remote 客户端
│   │   └── push/                    # 推送客户端
│   │       ├── mod.rs
│   │       ├── telegram.rs
│   │       ├── bark.rs
│   │       ├── wxpusher.rs
│   │       ├── gotify.rs
│   │       ├── pushplus.rs
│   │       ├── serverchan.rs
│   │       └── wecom.rs
│   │
│   ├── api/                         # HTTP API 层
│   │   ├── mod.rs
│   │   ├── auth.rs                  # HTTP Basic Auth 中间件
│   │   ├── subscriptions.rs         # 订阅路由
│   │   ├── search.rs                # 搜索路由
│   │   ├── settings.rs              # 设置路由
│   │   ├── push.rs                  # 推送路由
│   │   ├── quark.rs                 # 夸克路由
│   │   └── notifications.rs         # 通知路由
│   │
│   └── utils/                       # 工具函数
│       ├── mod.rs
│       └── time.rs                  # 时间工具
│
├── static/                          # 前端（需重写）
│   ├── index.html
│   ├── app.js                       # 按新 API 重写
│   └── style.css
│
├── data/                            # 运行时数据（保持兼容）
│   ├── subscriptions.json
│   ├── settings.json
│   └── notifications.json
│
├── Cargo.toml
├── Dockerfile                       # 改为编译 Rust
└── docker-compose.yml               # 端口保持 8787
```

---

## 📋 Phase 划分（可执行）

### Phase 0: 清理现有 Rust 代码 ✅ **[1-2小时]**

**目标**：移除错误的原型代码，保留可复用的基础设施。

**任务清单：**
- [ ] 备份当前 `src/` 到 `src.backup/`
- [ ] 保留：
  - `src/main.rs`（Axum 骨架）
  - `src/store/json_store.rs`（JSON 原子写入原语）
  - `src/clients/quark.rs` / `pansou.rs`（HTTP 客户端基础）
- [ ] 删除：
  - `src/models/*`（字段全错）
  - `src/api/*`（接口不匹配）
  - `src/services/*`（逻辑不对）
- [ ] 验证：`cargo build` 通过

---

### Phase 1: 数据模型 ✅ **[4-6小时]**

**目标**：定义与 Python JSON 完全兼容的数据结构。

**任务清单：**
- [ ] `models/subscription.rs`：完整 `Subscription` 结构（45+ 字段）
  - [ ] 序列化/反序列化测试（读取真实 `data/subscriptions.json`）
- [ ] `models/settings.rs`：完整 `Settings` 结构（40+ 字段）
  - [ ] 序列化/反序列化测试（读取真实 `data/settings.json`）
- [ ] `models/search.rs`：`SearchResult`, `SearchSession`, `FileProbe`
- [ ] `models/transfer.rs`：`TransferPlan`, `TransferItem`, `TransferRules`
- [ ] `models/notification.rs`：`Notification`
- [ ] 验证：所有模型能正确读写 Python 生成的 JSON

---

### Phase 2: 存储层 ✅ **[3-4小时]**

**目标**：实现 Store 层，保持 Python 数据兼容。

**任务清单：**
- [ ] `store/json_store.rs`：通用 JSON 原子写入（已有，验证）
- [ ] `store/subscription.rs`：
  - [ ] `list()` / `get(id)` / `create()` / `update()` / `delete()`
  - [ ] `update_check()` - 更新检查结果
  - [ ] `mark_transferred()` - 标记已转存
- [ ] `store/settings.rs`：
  - [ ] `get()` / `update()`
  - [ ] 默认值填充（环境变量优先）
- [ ] `store/session.rs`（内存）：
  - [ ] `save_search()` / `get_search()`
  - [ ] 会话超时清理（1小时）
- [ ] `store/notification.rs`：
  - [ ] `add()` / `list()` / `mark_read()` / `clear()`
- [ ] 验证：能读写 Python 生成的 `data/*.json`

---

### Phase 3: 核心业务逻辑 ✅ **[8-12小时]**

**目标**：移植核心引擎（转存规则、集数解析）。

**任务清单：**
- [ ] `services/episode.rs`：
  - [ ] 移植 `utils/episode.py`（集数提取正则）
  - [ ] `extract_episode()` / `extract_season_episode()`
  - [ ] 单元测试（覆盖 Python 测试用例）
- [ ] `services/transfer_rule.rs`：
  - [ ] 移植 `transfer_rule_service.py`（255行核心逻辑）
  - [ ] `build_transfer_plan()` - 规划转存
  - [ ] `match_files()` - 文件匹配
  - [ ] `rename_files()` - 重命名逻辑
  - [ ] `filter_transferred()` - 过滤已转存
  - [ ] 单元测试（关键场景）
- [ ] `services/subscription.rs`：
  - [ ] `create_subscription()` - 从搜索结果创建
  - [ ] `update_subscription()` - 更新订阅
  - [ ] `check_subscription()` - 检查单个订阅
  - [ ] `check_all_subscriptions()` - 检查所有
- [ ] 验证：规则引擎输出与 Python 一致

---

### Phase 4: 搜索服务（Remote only）✅ **[3-4小时]**

**目标**：只移植 `RemotePanSouClient`，调用已部署的 Go 服务。

**任务清单：**
- [ ] `clients/pansou.rs`：
  - [ ] 调用 `pansou.lxf87.com.cn/api/search?kw=...&res=merge&src=all`
  - [ ] 解析 `merged_by_type.quark`
  - [ ] 错误处理 + 超时重试
- [ ] `services/search.rs`：
  - [ ] `search_media()` - 搜索资源
  - [ ] `select_result()` - 选择结果到会话
  - [ ] 可选：`check_links` / `probe_files`（调用夸克 API）
- [ ] 验证：搜索结果与 Python 一致

---

### Phase 5: 夸克网盘客户端 ✅ **[6-8小时]**

**目标**：实现夸克操作（Cookie 认证 + 文件管理）。

**任务清单：**
- [ ] `clients/quark/client.rs`：
  - [ ] Cookie 管理（从 Settings 读取）
  - [ ] 用户信息获取
  - [ ] 文件列表（分享链接解析）
  - [ ] 文件搜索
- [ ] `clients/quark/save.rs`：
  - [ ] 转存文件到夸克网盘
  - [ ] 批量转存
  - [ ] 错误重试
- [ ] 验证：能正常转存文件

---

### Phase 6: 推送服务 ✅ **[6-8小时]**

**目标**：实现 7 种推送渠道。

**任务清单：**
- [ ] `clients/push/telegram.rs` - Telegram Bot 推送
- [ ] `clients/push/bark.rs` - Bark 推送
- [ ] `clients/push/wxpusher.rs` - WxPusher 推送
- [ ] `clients/push/gotify.rs` - Gotify 推送
- [ ] `clients/push/pushplus.rs` - PushPlus 推送
- [ ] `clients/push/serverchan.rs` - Server酱 推送
- [ ] `clients/push/wecom.rs` - 企业微信推送
- [ ] `services/push.rs`：
  - [ ] 推送编排（根据设置选择渠道）
  - [ ] 场景模板（订阅更新/失败/完成/转存）
  - [ ] 批量推送 + 重试
- [ ] 验证：测试推送到真实渠道

---

### Phase 7: HTTP API 层 ✅ **[4-6小时]**

**目标**：实现 HTTP 路由，对接前端。

**任务清单：**
- [ ] `api/auth.rs` - HTTP Basic Auth 中间件
- [ ] `api/subscriptions.rs`：
  - [ ] `GET /api/subscriptions`
  - [ ] `POST /api/subscriptions`
  - [ ] `POST /api/subscriptions/update`
  - [ ] `POST /api/subscriptions/check`
  - [ ] `POST /api/subscriptions/check-all`
  - [ ] `POST /api/subscriptions/plan`
  - [ ] `POST /api/subscriptions/delete`
- [ ] `api/search.rs`：
  - [ ] `POST /api/search`
  - [ ] `POST /api/select`
- [ ] `api/settings.rs`：
  - [ ] `GET /api/settings`
  - [ ] `POST /api/settings`
  - [ ] `POST /api/settings/test/*`
  - [ ] `GET /api/cloud-types`
- [ ] `api/push.rs`：
  - [ ] `POST /api/push/test`
  - [ ] `POST /api/push/test/{channel}`
  - [ ] `GET /api/push/history`
  - [ ] `GET /api/push/stats`
  - [ ] `GET /api/push/daily-summary`
  - [ ] `POST /api/push/daily-summary`
- [ ] `api/notifications.rs`：
  - [ ] `GET /api/notifications`
  - [ ] `POST /api/notifications/mark-read`
  - [ ] `POST /api/notifications/clear`
- [ ] `api/quark.rs`（按需实现，不影响主流程）
- [ ] 验证：Postman/curl 测试所有接口

---

### Phase 8: 前端重写 ✅ **[6-8小时]**

**目标**：重写 `static/app.js`，对接新 API。

**任务清单：**
- [ ] 分析现有 `static/app.js` 的功能点
- [ ] 按新 API 重写：
  - [ ] 搜索资源 → 订阅创建
  - [ ] 订阅列表展示
  - [ ] 订阅编辑（规则、状态）
  - [ ] 手动检查订阅
  - [ ] 设置页面
  - [ ] 推送测试
- [ ] 保持 `static/style.css` 不变（视觉一致）
- [ ] 验证：完整流程走通

---

### Phase 9: 定时任务 ✅ **[2-3小时]**

**目标**：订阅自动检查 + 每日摘要。

**任务清单：**
- [ ] `services/scheduler.rs`：
  - [ ] Tokio 定时器
  - [ ] 订阅检查调度（间隔可配置）
  - [ ] 每日摘要发送（可配置时间）
- [ ] 在 `main.rs` 中启动调度器
- [ ] 验证：定时任务正常运行

---

### Phase 10: 测试 + 优化 ✅ **[4-6小时]**

**目标**：确保功能对等。

**任务清单：**
- [ ] 集成测试：
  - [ ] 完整搜索 → 订阅 → 检查 → 转存流程
  - [ ] 推送到所有渠道
  - [ ] 设置读写
- [ ] 错误处理验证：
  - [ ] 网络错误
  - [ ] 无效数据
  - [ ] 并发安全
- [ ] 性能测试（与 Python 对比）
- [ ] 日志优化
- [ ] 文档更新（README）

---

### Phase 11: 部署切换 ✅ **[2-3小时]**

**目标**：切换到 Rust 运行时。

**任务清单：**
- [ ] 更新 `Dockerfile`：
  ```dockerfile
  FROM rust:1.75 as builder
  WORKDIR /app
  COPY Cargo.* ./
  COPY src ./src
  RUN cargo build --release
  
  FROM debian:bookworm-slim
  RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
  WORKDIR /app
  COPY --from=builder /app/target/release/my-media-sub ./
  COPY static ./static
  EXPOSE 8787
  CMD ["./my-media-sub"]
  ```
- [ ] 更新 `docker-compose.yml`（端口保持 8787）
- [ ] 更新 `.github/workflows/docker-publish.yml`
- [ ] 数据迁移验证（读取现有 JSON）
- [ ] 灰度发布：
  - [ ] 本地 Docker 测试
  - [ ] 推送到 GHCR
  - [ ] 生产环境部署
- [ ] 回滚方案：保留 Python 代码到 `legacy-python/` 分支

---

## ⏱️ 时间估算

| Phase | 工作量 | 说明 |
|-------|--------|------|
| Phase 0: 清理代码 | 1-2h | 移除错误原型 |
| Phase 1: 数据模型 | 4-6h | 45+ 字段的结构体 |
| Phase 2: 存储层 | 3-4h | JSON 读写 |
| Phase 3: 核心逻辑 | 8-12h | 转存规则引擎（核心） |
| Phase 4: 搜索服务 | 3-4h | Remote only |
| Phase 5: 夸克客户端 | 6-8h | Cookie + API |
| Phase 6: 推送服务 | 6-8h | 7 种渠道 |
| Phase 7: HTTP API | 4-6h | 路由层 |
| Phase 8: 前端重写 | 6-8h | 按新 API 改 |
| Phase 9: 定时任务 | 2-3h | 调度器 |
| Phase 10: 测试优化 | 4-6h | 集成测试 |
| Phase 11: 部署切换 | 2-3h | Docker + CI |

**总计：50-70 小时**（按每天 6 小时，约 **9-12 个工作日**）

---

## 🎯 里程碑

### M1: 数据层完成（Phase 0-2）✅ **[~2天]**
- 数据模型定义
- JSON 读写正常
- 验证：能读取 Python 生成的数据

### M2: 核心功能（Phase 3-5）✅ **[~4天]**
- 转存规则引擎
- 搜索服务
- 夸克客户端
- 验证：核心业务逻辑正确

### M3: 功能对等（Phase 6-8）✅ **[~3天]**
- 推送服务
- HTTP API
- 前端重写
- 验证：所有功能可用

### M4: 生产就绪（Phase 9-11）✅ **[~2天]**
- 定时任务
- 测试验证
- 部署切换
- 验证：生产环境稳定

---

## 📝 注意事项

1. **Python 保留为对照组**：在 Rust 完全稳定前，保留 Python 代码在独立分支
2. **数据兼容优先**：Rust 必须能读写 Python 的 JSON，确保无缝切换
3. **增量测试**：每个 Phase 完成后立即测试，不要堆积到最后
4. **错误处理完善**：网络请求、文件 I/O、JSON 解析都要有 fallback
5. **日志清晰**：用 tracing 记录关键操作，方便排查问题

---

## 🚀 立即开始

### 下一步行动（Phase 0）

```bash
# 1. 备份现有代码
cd ~/my-media-sub
mv src src.backup

# 2. 创建新目录结构
mkdir -p src/{models,store,services,clients/{quark,push},api,utils}

# 3. 保留可复用部分
cp src.backup/main.rs src/
# ... 逐步复制可用模块

# 4. 验证编译
cargo build
```

---

**准备好开始了吗？我们从 Phase 0 开始清理！**
