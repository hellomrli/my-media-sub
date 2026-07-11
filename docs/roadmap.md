# my-media-sub 持续开发路线与进度台账

> 本文件是项目后续工作的**唯一持久化计划入口**。每完成一个任务，必须同步更新复选框、证据和“当前执行指针”，以便跨会话、跨上下文继续。
>
> 最近更新：2026-07-11

## 状态说明

- `[x]`：已完成，并有代码/测试/文档证据。
- `[-]`：进行中，尚未满足全部验收条件。
- `[ ]`：待开始。
- `[!]`：需要产品决策、外部条件或明确授权。

## 当前执行指针

- **当前阶段**：P11 — v1.9.x 稳定性收口
- **当前任务**：`P11-03` 增加备份恢复端到端测试
- **下一任务**：实现备份恢复端到端测试、PWA 跨版本缓存测试和长运行验证
- **当前发布基线**：v1.9.0 已发布；P11–P20 作为后续开发快照推进，不自动创建新 tag
- **工作树状态**：P0 至 P10 已完成；P11-01 已完成并等待提交

---

## 跨窗口交接摘要（2026-07-11）

> 新窗口继续工作时，先阅读本节和 `P5`；如用户要求先发布已有里程碑，则改读 release checklist，不要仅依赖聊天历史。

### 仓库与提交状态

- 当前开发分支：`main`。
- P0 的稳定性收口、P1 日历、P2 安全自动换源、P3 结构化流水线、P4 前端模块化、P5 后端模块化与性能和 P6 CloudDriveProvider 抽象已组成当前开发基线。
- 正式发布仍需按对应 release checklist 核对版本、CHANGELOG、升级指南、tag、GitHub Release 和 GHCR 镜像。
- 不要执行 `git reset`、`git checkout -- .`、批量覆盖文件或清理未跟踪文件。
- 未经用户明确授权，不创建新的发布 tag 或 GitHub Release。
- `Cargo.toml` / `Cargo.lock` 已统一为 `1.3.0`；README、Release workflow、CHANGELOG、升级指南和架构图也已同步。

### v1.3.0 已完成能力

- 规则文档：`docs/media-calendar.md`。
- 后端模型：`src/models/calendar.rs`，订阅新增兼容字段 `manual_schedule`。
- 纯计算服务：`src/services/media_calendar.rs`。
- API：`src/api/calendar.rs`，路由为 `GET /api/calendar`。
- 前端模块：`static/js/features/calendar.js`。
- 页面：侧边栏“更新日历”，支持周、月、列表三种视图。
- 筛选：状态、媒体类型；API 还支持订阅 ID 和日期范围。
- 操作：订阅详情、立即检查、检查并补集。
- 订阅编辑器：支持开播日期、播出星期、上海时间、周期、首集编号和总集数；关闭手动排期会向 API 发送 `null` 并清除覆盖。
- 发布文档：`CHANGELOG-v1.3.0.md`、`docs/upgrade-v1.3.0.md`、`docs/v1.3.0-release-checklist.md`。
- 架构：`docs/architecture.md` 与 Graphviz SVG/PNG 已更新为 v1.3.0，并包含 Calendar model/service/API、手动排期和前端模块。

### P2 已完成能力

- Rust 后端权威资源质量评分，并通过共享 fixtures 与历史前端算法对齐；
- 搜索结果和 `SourceCandidate` 返回分数、等级、风险、剧集范围、更新时间和推荐理由；
- 自动换源默认关闭并支持仅搜索、最低分、最低分差、连续失效阈值和冷却时间；
- 候选必须通过探测、季度匹配、当前进度覆盖、历史链接和近期失败检查；
- 换源保留进度、已知和转存记录，并提供预览、审计历史和一键回滚；
- 前端新增候选对比、安全条件、策略设置、换源历史和回滚入口；
- 详细规则见 `docs/source-quality.md`。

### P3 已完成能力

- `AutomationEvent` 已覆盖 correlation、订阅、集数、Job、阶段、状态、尝试、消息、错误、metadata 和生命周期时间；
- 固化 source_check、file_filter、version_select、cloud_transfer、rename、strm、aria2、notification 八阶段和七状态状态机；
- `AutomationEventStore` 使用 schema v1、原子写入、`0600`、30/90 天分级保留、5,000 条上限及 subscription/correlation/job 内存索引；
- 同一执行阶段使用稳定事件 ID 原位更新，保留开始时间；摘要会折叠旧式历史状态，已完成任务不会因历史 running 事件被误判为卡住；
- Job 广播、订阅检查、版本选择、转存、重命名、STRM、Aria2 和通知路径均写入结构化事件，通知正文仅保留展示职责；
- API 支持事件筛选、自动化摘要、单订阅/单集/单 Job 流水线和失败/取消阶段安全重试；
- WebUI 已在工作台展示成功率、最近失败和卡住阶段，在订阅详情展示事件、耗时、尝试、错误和重试入口；
- 详细契约见 `docs/automation-events.md` 与 `docs/api-contract.md`。

### P5 已完成能力

- subscriptions 与 drive API 已按职责拆为目录模块，现有路由和响应契约保持兼容；
- 四类 Job 已拆为独立 handler，Worker 收缩为分发与生命周期控制；
- SubscriptionStore 支持原子快照变更和批量更新，批量检查真实存储只写一次；
- 新增检查/API 并发上限、同订阅互斥、同批分享探测去重、Job 幂等与安全重启恢复；
- 新增 Aria2 批量上限、HTTP 429/Retry-After 处理和对应一致性测试。

### 已固化的实现决策

- 日历业务时间统一为 `Asia/Shanghai`；当前 Rust 实现使用 UTC+08:00 固定偏移。
- 自然周为周一至周日，跨月和跨年不改变周边界。
- `check_weekdays` 只控制订阅检查任务，绝不用于推导媒体播出日。
- 排期优先级：手动排期 → 逐集元数据 → 下一集元数据 → 发布日期 → 稳定周期推断 → 排期未知。
- 手动排期覆盖展示结果，但不修改原始 `MediaMetadata`。
- API 默认查询当前自然周；`from`/`to` 为闭区间，最多覆盖 367 个自然日。
- 日历复用订阅详情聚合结果，不另外维护 known/transferred/STRM/Aria2 状态判定。
- `UpdateSubscriptionRequest` 对 `metadata` 和 `manual_schedule` 使用“字段存在感知”的反序列化，JSON `null` 表示显式清除，缺少字段表示保持不变。
- v1.3.0 不提升持久化 schema；回滚 v1.2.0 可以读取 schema v1，但旧程序重写订阅文件时可能丢弃 `manual_schedule`，完整回滚应恢复升级前备份。

### 最近完整验证证据

```text
scripts/build-css.sh
find static -type f -name '*.js' -print0 | sort -z | xargs -0 -n1 node --check
node --test tests/frontend_*.test.js
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
git diff --check
```

- 33 个前端纯函数测试通过，Node runner 的 7 个测试文件全部通过。
- 322 个 Rust 测试已登记：321 个通过，1 个真实网络测试按设计忽略。
- P3 针对性测试覆盖稳定生命周期更新、阶段/状态契约、重启索引、幂等、分级保留、Job 投影、当前状态摘要、单集 API 和安全重试。
- Release 构建通过；当前 Cargo 版本仍为 `1.3.0`。
- 临时 `DATA_DIR` HTTP 烟雾：健康检查、Basic Auth 401、Calendar 默认/筛选/错误参数、静态模块 MIME、手动排期保存/清除和元数据不变均通过。
- 无头 Chrome 1440×1000 与 390×844：周/月/列表成功渲染手动和元数据排期；Alpine 已初始化、`x-cloak` 已移除，无运行时异常、失败请求、HTTP 4xx/5xx 或页面级横向溢出。
- Graphviz SVG/PNG 可由 `architecture.dot` 字节级重现。
- 本地 Release 包 `my-media-sub-v1.3.0-linux-x86_64.tar.gz` 为 5,721,478 bytes，SHA256 为 `7b2de2e2bdeeb975e2ed464f9946aabc9befb9acbfea25030ca8d7b9b796c464`；归档包含二进制、static、README、v1.3.0 CHANGELOG 和完整 docs。

### 重启交接状态

- 用户将重启 Codex 以重置上下文；后续窗口必须从本文件继续，不依赖本轮聊天记录。
- P3 与 P4 已全部完成；P4 将原 5000+ 行 `static/app.js` 收缩为 Alpine 装配层，并完成路由、通知、轮询、stores 和页面模块拆分。
- 不要重复实现 P3，也不要把 P4 标记为完成。
- 未经用户明确授权，仍不得创建 commit、tag 或 push。

### 新窗口的精确下一步

1. 执行 `P5-01-01`：梳理 `src/api/subscriptions.rs` 的职责和现有测试覆盖。
2. 按 crud/status/actions/metadata/source 拆分实现，保持路由和响应结构不变。
3. 先补边界测试，再迁移 handler，避免继续扩大未提交工作树中的回归面。

---

# A. 已完成基线

- [x] `BASE-01` Media Deck 应用壳层、深浅主题和响应式布局。
  - 证据：`static/index.html`、`tailwind/input.css`、`static/styles.css`。
- [x] `BASE-02` 工作台、资源搜索、订阅、网盘、下载、日志、通知和设置页面统一视觉系统。
- [x] `BASE-03` 统一前端请求层 `apiFetch()`、`apiJson()`、`apiData()` 和 `ApiError`。
  - 证据：`static/js/core/api.js`、`tests/frontend_api.test.js`。
- [x] `BASE-04` 时间、容量、速度、时长和百分比格式化模块。
  - 证据：`static/js/core/formatters.js`、`tests/frontend_formatters.test.js`。
- [x] `BASE-05` 搜索结果质量分析、筛选、排序和海报/列表视图。
  - 证据：`static/js/features/search-results.js`、`tests/frontend_search_results.test.js`。
- [x] `BASE-06` 订阅详情、剧集网格、缺集检测、流水线和活动时间线。
  - 证据：`src/services/subscription_status.rs`、`static/js/features/subscription-detail.js`。
- [x] `BASE-07` 网盘面包屑、筛选排序、批量操作和 Aria2 提交。
- [x] `BASE-08` Aria2 任务关联订阅、集数、转存、重命名和 STRM 状态。
- [x] `BASE-09` 设置中心按连接、自动化、命名规则、通知和维护重组。
- [x] `BASE-10` JSON API 成功响应统一为 `{ok:true,data:...}`，应用错误包含 `{ok:false,error,message}`。
  - 证据：`src/api/response.rs`、`src/error.rs`。
- [x] `BASE-11` settings/subscriptions/notifications/jobs/automation_events 使用 `schema_version: 1` 信封并支持旧数据迁移。
  - 证据：`src/store/schema.rs` 及各 Store。
- [x] `BASE-12` CI 自动检查全部前端 JavaScript 并运行前端测试。
- [x] `BASE-13` 当前质量门通过：CSS 构建、JS 语法、前端测试、rustfmt、clippy、Rust 测试、Release 构建和浏览器烟雾测试。
  - 最近证据：33 个前端纯函数测试；322 个 Rust 测试已登记，321 个通过、1 个真实网络测试忽略。

---

# P0. v1.2.0 稳定性收口

## P0-01 持久化计划与进度机制

- [x] `P0-01-01` 创建本路线文件并使用稳定任务编号。
- [x] `P0-01-02` 记录已完成基线、当前执行指针和验证证据。
- [x] `P0-01-03` 本轮实现结束后已更新状态、验证结果和下一任务；后续每轮重复执行。

## P0-02 JSON Store 迁移与失败恢复

- [x] `P0-02-01` 通用 schema 解码、v0→v1 迁移和未来版本拒绝逻辑。
- [x] `P0-02-02` SubscriptionStore：旧裸数组迁移测试。
- [x] `P0-02-03` SubscriptionStore：未来 schema 保留测试。
- [x] `P0-02-04` SubscriptionStore：损坏文件隔离测试。
- [x] `P0-02-05` SubscriptionStore：保存失败不污染内存测试。
- [x] `P0-02-06` SettingsStore：补齐旧格式迁移和未来 schema 保留测试。
- [x] `P0-02-07` NotificationStore：补齐旧格式迁移、未来 schema 保留和损坏隔离测试。
- [x] `P0-02-08` JobStore：补齐旧格式迁移、未来 schema 保留和损坏隔离测试。
- [x] `P0-02-09` 所有版本化数据文件在加载时修复为 `0600`，原子写入由共享工具验证。
- [x] `P0-02-10` schema 迁移前创建一次性 `*.schema-v0.bak`，保留原始字节且不覆盖已有备份。
- [x] `P0-02-11` 四类 Store 均验证保存失败时不会提前污染内存状态。

## P0-03 API 响应一致性

- [x] `P0-03-01` 业务 API 成功响应统一信封。
- [x] `P0-03-02` AppError 统一错误信封。
- [x] `P0-03-03` WebUI 兼容当前、旧信封和历史裸响应。
- [x] `P0-03-04` Basic Auth 401 返回 JSON 错误信封并保留 `WWW-Authenticate`。
- [x] `P0-03-05` CSRF 403 返回 JSON 错误信封。
- [x] `P0-03-06` 认证、CSRF、已知/未知 404、malformed JSON、405 和验证错误均有内容类型与结构断言。
- [x] `P0-03-07` 已审计 API 路由，并在 `docs/api-contract.md` 登记 `/health`、`/strm/*`、SSE 和 204 例外。

## P0-04 架构和升级文档

- [x] `P0-04-01` `docs/architecture.md` 已更新为 v1.2.0 开发基线。
- [x] `P0-04-02` 已新增 Graphviz 源并重新生成 PNG/SVG，覆盖统一响应、schema、状态聚合和前端模块。
- [x] `P0-04-03` 已新增 `docs/upgrade-v1.2.0.md` 升级、验证和回滚说明。
- [x] `P0-04-04` 已在 API 契约和升级文档明确 `.data` 迁移方式与例外接口。
- [x] `P0-04-05` 已明确旧二进制回滚必须恢复 `.schema-v0.bak` 或完整 DATA_DIR 备份。
- [x] `P0-04-06` 已移除从未启用且未暴露的 `nas_sync_*` 占位字段；旧 JSON 字段可忽略并在保存时清理。

## P0-05 发布准备

- [x] `P0-05-01` 已在 `docs/v1.2.0-release-checklist.md` 整理 6 个逻辑提交边界和交叉文件 staging 规则。
- [!] `P0-05-02` 经用户确认后创建提交；不擅自提交或推送。
- [x] `P0-05-03` 版本已升级到 1.2.0，并同步 Cargo、README、镜像标签示例、升级文档和 Release 工作流。
- [x] `P0-05-04` 已新增 `CHANGELOG-v1.2.0.md` 和 README 1.2.0 Release 正文。
- [x] `P0-05-05` 已完成手工二进制/static 与 Docker 同数据卷的 v1.1.3→v1.2.0→v1.1.3 升级回滚演练。
- [x] `P0-05-06` 已完成 1440/768/390px、深浅主题、8 个主页面共 48 组无头浏览器回归。
- [x] `P0-05-07` 完整质量门、Release 归档、SHA256、内容和工作流校验均通过。

---

# P1. 媒体更新日历（建议 v1.3.0）

## P1-01 规则定义

- [x] `P1-01-01` 定义今日更新、本周待更新、已播未发现、已发现待转存、已转存待下载、完结缺集和排期未知。
- [x] `P1-01-02` 明确上海时区、自然周和跨月/跨年规则。
- [x] `P1-01-03` 明确 `check_weekdays` 仅代表检查计划，不代表播出计划。
- [x] `P1-01-04` 定义元数据、手动排期和推断数据的优先级与可信度。
  - 证据：`docs/media-calendar.md`。

## P1-02 后端模型与服务

- [x] `P1-02-01` 新增 `src/models/calendar.rs`。
- [x] `P1-02-02` 新增纯计算服务 `src/services/media_calendar.rs`。
- [x] `P1-02-03` 从 `MediaMetadata.episodes/next_episode_to_air/release_date` 派生日历。
- [x] `P1-02-04` 合并 known/transferred/STRM/Aria2 状态。
- [x] `P1-02-05` 增加手动播出星期、时间、开播日期、周期和总集数覆盖字段。
- [x] `P1-02-06` 保证所有新增字段使用兼容默认值。
  - 证据：`src/models/calendar.rs`、`src/services/media_calendar.rs`；7 个日历服务单元测试。

## P1-03 API

- [x] `P1-03-01` 新增 `GET /api/calendar`。
- [x] `P1-03-02` 支持 from/to/status/media_type/subscription 查询参数。
- [x] `P1-03-03` 返回日历摘要、状态、来源可信度和快捷操作信息。
- [x] `P1-03-04` 增加 API 集成测试和错误参数测试。
  - 证据：`src/api/calendar.rs`、`tests/api_integration.rs`；2 个查询解析测试和 2 个 HTTP 集成测试。

## P1-04 前端

- [x] `P1-04-01` 新增日历导航和 `static/js/features/calendar.js`。
- [x] `P1-04-02` 实现周视图、月视图和紧凑列表。
- [x] `P1-04-03` 实现状态和媒体类型筛选。
- [x] `P1-04-04` 支持跳转订阅详情、立即检查和检查并补集。
- [x] `P1-04-05` 增加日历空状态、未知排期和错误状态。
- [x] `P1-04-06` 增加桌面与移动端浏览器测试。
  - 证据：`static/js/features/calendar.js`、`tests/frontend_calendar.test.js`、`static/index.html`。
  - 浏览器证据：1440×1000 与 390×844 均正确渲染 3 个当周项目（2 个订阅，含手动与元数据排期），Alpine 已初始化，无运行时异常、失败请求或页面级横向溢出。

## P1-05 验收

- [x] `P1-05-01` 覆盖跨时区、跨周、同日多集、未来集数和元数据缺失测试。
- [x] `P1-05-02` 验证手动排期可覆盖元数据但不修改原始元数据。
- [x] `P1-05-03` 发布 v1.3.0 文档和迁移说明。

## P1 最近验证

- 24 个前端单元测试通过。
- 296 个 Rust 测试已登记：295 个通过，1 个真实网络测试按设计忽略。
- CSS 构建、全部 JavaScript 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 通过。
- Calendar HTTP 烟雾覆盖认证、默认/筛选/错误查询、静态模块 MIME、手动排期保存/清除和元数据不变。
- 1440×1000 与 390×844 无头 Chrome 周/月/列表回归通过，无异常、失败请求或页面级横向溢出。
- v1.3.0 Release notes 提取、Graphviz 重现、归档内容和 SHA256 校验通过。

---

# P2. 资源质量评分与安全自动换源（建议 v1.4.0）

## P2-01 后端权威评分

- [x] `P2-01-01` 新增 `src/services/source_quality.rs`。
- [x] `P2-01-02` 迁移清晰度、编码、HDR、音轨、文件数、剧集覆盖、更新时间和风险评分。
- [x] `P2-01-03` 扩展 SourceCandidate：分数、等级、风险、集数范围、更新时间和推荐原因。
- [x] `P2-01-04` 前端改为展示后端评分，仅保留兼容降级分析。
- [x] `P2-01-05` 用共享 fixtures 保证 Rust 与历史前端评分迁移一致。

## P2-02 自动换源策略

- [x] `P2-02-01` 增加自动换源开关、最低分、最低分差、失效次数和冷却时间。
- [x] `P2-02-02` 强制候选探测成功、季度匹配和当前进度覆盖。
- [x] `P2-02-03` 避免历史失效链接和近期失败候选。
- [x] `P2-02-04` 保留进度、已知文件和已转存记录。
- [x] `P2-02-05` 换源后立即检查；失败不清空原状态。

## P2-03 安全与回滚

- [x] `P2-03-01` 默认关闭自动应用，支持“仅搜索”。
- [x] `P2-03-02` 增加换源预览和差异对比。
- [x] `P2-03-03` 增加换源历史与一键回滚上一来源。
- [x] `P2-03-04` 记录审计事件和失败原因。
- [x] `P2-03-05` 增加幂等、冷却和重复换源测试。

## P2-04 前端

- [x] `P2-04-01` 展示当前来源评分和候选对比表。
- [x] `P2-04-02` 展示风险标签、剧集覆盖和推荐理由。
- [x] `P2-04-03` 增加自动换源策略配置、历史和回滚入口。

---

# P3. 结构化领域事件与完整自动化流水线（建议 v1.5.0）

## P3-01 事件模型

- [x] `P3-01-01` 定义 AutomationEvent：correlation/subscription/episode/job/stage/status/attempt/message/error/metadata/time。
- [x] `P3-01-02` 定义 source_check、file_filter、version_select、cloud_transfer、rename、strm、aria2、notification 阶段。
- [x] `P3-01-03` 定义 pending/running/succeeded/skipped/failed/retrying/canceled 状态机。

## P3-02 事件职责与存储

- [x] `P3-02-01` Job 只负责执行，事件负责业务轨迹，Notification 负责用户消息，Metrics 负责统计。
- [x] `P3-02-02` 不再依赖通知文本推断 STRM、重命名和下载关联。
- [x] `P3-02-03` 实现 JSON 事件存储、条数/时间保留和失败事件延长保留。
- [x] `P3-02-04` 为单订阅和 correlation_id 建立高效内存索引。
- [x] `P3-02-05` 增加重启恢复、重复事件和幂等测试。

## P3-03 流水线 API 与 UI

- [x] `P3-03-01` 新增单订阅、单集和单 Job 流水线接口。
- [x] `P3-03-02` 展示每集当前阶段、耗时、重试和跳过原因。
- [x] `P3-03-03` 支持单步骤安全重试。
- [x] `P3-03-04` 工作台展示最近失败、卡住任务、重试热点和成功率。

证据：`src/models/automation_event.rs`、`src/store/automation_event.rs`、`src/services/automation_events.rs`、`src/api/automation.rs`、`src/services/subscription_check.rs`、`src/services/subscription_status.rs`、`static/js/features/automation-events.js`、`static/app.js`、`static/index.html`、`tests/api_integration.rs`、`tests/frontend_automation_events.test.js`、`docs/automation-events.md`。

最近验证：33 个前端纯函数测试通过；322 个 Rust 测试登记，321 个通过、1 个真实网络测试按设计忽略；CSS、全部 JavaScript 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 通过。

---

# P4. 前端渐进模块化

- [x] `P4-01` `core/api.js`。
- [x] `P4-02` `core/formatters.js`。
- [x] `P4-03` `features/search-results.js`。
- [x] `P4-04` `features/subscription-detail.js`。
- [x] `P4-05` 抽离 `core/router.js`。
- [x] `P4-06` 抽离 `core/notifications.js` 和 Toast。
- [x] `P4-07` 抽离轮询生命周期 `core/polling.js`。
- [x] `P4-08` 抽离 `stores/downloads.js`。
- [x] `P4-09` 抽离 `stores/drive.js`。
- [x] `P4-10` 抽离 `features/updates.js`。
- [x] `P4-11` 抽离设置、工作台和订阅列表模块。
- [x] `P4-12` 将 `app.js` 收缩为 Alpine 装配层，并为纯函数增加 Node 测试。
- [x] `P4-13` 确保页面切换不会遗留轮询器和事件监听器。

证据：`static/js/core/{router,notifications,polling,shell}.js`、`static/js/stores/{downloads,drive,jobs,subscriptions}.js`、`static/js/features/{updates,settings,dashboard,search-page,calendar-page}.js`、`static/app.js`、`tests/frontend_{router,polling,p4_stores,app_modules}.test.js`。

最近验证：43 个前端 Node 测试通过；`app.js` 从 5000+ 行收缩为 40 行装配层；路由保留 tab/settings/subscription URL、历史前进后退和旧设置 tab 别名；页面切换会停止搜索、在线更新、下载和通知轮询，应用销毁会统一停止轮询、移除 `popstate`/快捷键监听并关闭 Job EventSource。

---

# P5. 后端模块化与性能

## P5-01 API 拆分

- [x] `P5-01-01` 拆分 subscriptions：crud/status/actions/metadata/source。
- [x] `P5-01-02` 拆分 drive：browse/actions/aria2/automation。
- [x] `P5-01-03` 保持现有路由、响应结构和测试不变。

证据：`src/api/subscriptions/{crud,status,actions,metadata,source}.rs`、`src/api/drive/{browse,actions,aria2,automation}.rs`；原 `subscriptions.rs`、`drive.rs` 已由目录模块替代，API 集成测试保持通过。

## P5-02 Job Handler

- [x] `P5-02-01` 将 ManualTransfer 拆为独立 handler。
- [x] `P5-02-02` 将 SubscriptionTransfer 拆为独立 handler。
- [x] `P5-02-03` 将 MetadataScrape 拆为独立 handler。
- [x] `P5-02-04` 将 PushDispatch 拆为独立 handler。
- [x] `P5-02-05` Worker 只保留分发、状态、取消、错误和事件记录。

证据：`src/jobs/worker/{manual_transfer,subscription_transfer,metadata_scrape,push_dispatch}.rs`；`src/jobs/worker.rs` 负责队列消费、类型分发、运行状态、取消判断、panic/错误收口和通用通知记录。

## P5-03 批量持久化

- [x] `P5-03-01` 为 SubscriptionStore 增加 update_many/mutate_snapshot 能力。
- [x] `P5-03-02` 批量检查只落盘一次。
- [x] `P5-03-03` 写入成功后才更新内存，失败不产生部分状态。
- [x] `P5-03-04` 增加落盘次数和并发一致性测试。

证据：`SubscriptionStore::from_snapshot/mutate_snapshot/update_many/save_count`；批量检查先在内存快照并发执行，再以一次 `update_many` 提交；覆盖单次落盘、保存失败不污染内存/磁盘和并发不丢失互不相交更新的测试。

## P5-04 并发与幂等

- [x] `P5-04-01` 增加订阅检查和外部 API 最大并发配置。
- [x] `P5-04-02` 同一订阅互斥、同一分享链接去重。
- [x] `P5-04-03` 增加 Job 幂等键和重启重复任务保护。
- [x] `P5-04-04` 增加 Aria2 批量提交限制和上游限速处理。

证据：设置新增 `subscription_check_max_concurrency`、`external_api_max_concurrency` 和 `aria2_batch_submit_limit`；批量检查以两项并发上限的较小值执行，同一订阅使用弱引用命名锁互斥，同批相同分享链接共享探测缓存；Job 持久化幂等键并只允许一个同键 queued/running 任务，重启按创建顺序恢复、跳过重复 queued 任务和 interrupted 任务的重复副本；Aria2 API/自动提交均限制批量，夸克探测和转存客户端统一将 HTTP 429 映射为 `rate_limited` 并保留 `Retry-After` 提示。

最近验证：43 个前端 Node 测试通过；334 个 Rust 测试登记，333 个通过，1 个真实 PanSou 网络测试按设计忽略；全部 JavaScript 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 通过。

---

# P6. CloudDriveProvider 抽象

- [x] `P6-01` 定义 probe/list/find/ensure/transfer/rename/delete/download_info/health 能力。
- [x] `P6-02` 新增 Quark Provider 适配器，保持现有行为。
- [x] `P6-03` 业务 Service 不再直接依赖 QuarkSaveClient。
- [x] `P6-04` 新增 Mock Provider，用于检查、转存和失败测试。
- [x] `P6-05` 让 `cloud_type` 真正选择 Provider。
- [x] `P6-06` 将签到保留为夸克专属扩展能力。
- [!] `P6-07` 只有出现明确需求后才增加阿里云盘、115 等第二 Provider。

---

# P7. 备份、诊断、可观测性与安全

## P7-01 备份恢复

- [x] `P7-01-01` 下载完整 DATA_DIR 业务数据备份。
- [x] `P7-01-02` 定时备份、保留最近 N 份和磁盘空间保护。
- [x] `P7-01-03` 恢复前预览、schema 校验和当前快照。
- [x] `P7-01-04` 恢复后安全重启及路径穿越防护。

## P7-02 诊断与指标

- [x] `P7-02-01` 增加脱敏诊断页和诊断包导出。
- [x] `P7-02-02` 展示版本、schema、数据大小、队列、调度器和外部服务状态。
- [x] `P7-02-03` 增加 request_id/correlation_id。
- [x] `P7-02-04` 扩展检查耗时、转存耗时、失败阶段、队列长度、自动换源和备份指标。

## P7-03 安全

- [x] `P7-03-01` 登录失败频率限制。
- [x] `P7-03-02` 密码强度和默认密码风险提示。
- [x] `P7-03-03` 全链路敏感字段日志脱敏。
- [x] `P7-03-04` 引入 CSP，并把首页内联主题脚本移到外部文件。
- [x] `P7-03-05` 增加依赖安全检查和 HTTPS 反向代理文档。
- [x] `P7-03-06` 恢复、删除和批量危险操作强化确认。

---

# P8. PWA 与移动端（建议 v1.7.0）

- [x] `P8-01` 新增 manifest、图标和 service worker。
- [x] `P8-02` HTML network-first，静态资源 stale-while-revalidate。
- [x] `P8-03` `/api/*` 与 `/strm/*` network-only，不缓存敏感业务数据。
- [x] `P8-04` 增加安装入口和新版本缓存更新提示。
- [x] `P8-05` 增加今日更新、缺集、失败任务、检查全部、下载进度和签到快捷入口。
- [x] `P8-06` 覆盖 Basic Auth、离线壳层、缓存升级和 390px 测试。

---

# P9. JSON 优化与 SQLite 决策

- [x] `P9-01` 增加数据文件大小、解析耗时和写入耗时监控。
- [x] `P9-02` 增加历史保留、压缩/清理和内存索引。
- [x] `P9-03` 构造 500 订阅、10,000 Job、10,000 通知/事件性能基线。
- [x] `P9-04` 仅在数百订阅、数万历史、数十 MB 文件或复杂查询出现时启动 SQLite 迁移。
- [x] `P9-05` SQLite 迁移必须保留 JSON、可重复执行、可校验、失败可回滚且不长期双写。

---

# P10. 可选功能池

- [!] `P10-01` Telegram 主动控制：status/today/missing/check/search/subscribe/jobs/retry。
- [!] `P10-02` NAS 同步：若保留则实现独立 Job、预览、防误删和下载完成关联；否则移除死配置。
- [!] `P10-03` 第二云盘 Provider。
- [x] `P10-04` 浏览器 Push。
- [x] `P10-05` Webhook。
- [x] `P10-06` OpenAPI 文档。
- [x] `P10-07` 数据导入导出、订阅标签和自定义首页组件。
- [!] `P10-08` 多用户权限，仅在单用户自用定位发生变化后评估。

---

# Q. 全程质量门

每个任务完成前，根据影响范围执行对应检查；每个发布里程碑必须全部执行：

- [ ] `Q-01` `scripts/build-css.sh`
- [ ] `Q-02` 所有 `static/**/*.js` 执行 `node --check`
- [ ] `Q-03` `node --test tests/frontend_*.test.js`
- [ ] `Q-04` `cargo fmt --all -- --check`
- [ ] `Q-05` `cargo clippy --all-targets --all-features --locked -- -D warnings`
- [ ] `Q-06` `cargo test --all --locked`
- [ ] `Q-07` `cargo build --release --locked`
- [ ] `Q-08` `git diff --check`
- [ ] `Q-09` HTTP API 与静态资源烟雾测试
- [ ] `Q-10` 无头 Chrome：Alpine 初始化、无异常、无控制台错误、无意外失败请求
- [ ] `Q-11` 1440/768/390px 和深浅主题回归
- [ ] `Q-12` 真实网络测试只做手动检查，不进入默认 CI

## 每次进度回写模板

```text
完成任务：P?-??-??
代码证据：文件/接口
测试证据：命令与结果
文档更新：文件
遗留风险：无 / 描述
下一任务：P?-??-??
```

---

# 进度记录

## 2026-07-10 — P0 第一批稳定性收口

完成任务：`P0-01`、`P0-02`、`P0-03`

代码证据：

- `docs/roadmap.md`：完整持久化计划和执行指针；
- `src/store/schema.rs`：迁移前一次性原始备份；
- `src/store/settings.rs`、`subscription.rs`、`notification.rs`、`src/jobs/store.rs`：权限修复、迁移/未来版本/损坏/保存失败测试；
- `src/error.rs`、`src/api/mod.rs`：认证、CSRF、未知 API 和框架拒绝统一 JSON；
- `docs/api-contract.md`：响应契约、错误代码和例外登记；
- `tests/api_integration.rs`：401、403、404、405、malformed JSON、SSE、STRM、静态资源和 204 测试。

测试证据：

- 全部 JavaScript 语法检查和 19 个前端单元测试通过；
- rustfmt、clippy `-D warnings` 和 Release 构建通过；
- 281 个 Rust 测试通过，1 个真实网络测试按设计忽略；
- API 集成测试 20/20 通过；
- 迁移备份保留原始字节并使用 `0600`；
- 未来 schema 内容不变且不被隔离；
- 保存失败后内存状态保持不变。

遗留风险：

- 当前工作树仍较大且未拆分提交；
- v1.2.0 架构图、升级说明、版本号和发布验证尚未完成。

下一任务：`P0-04-01` 更新 `docs/architecture.md`。

## 2026-07-10 — P0 架构与升级文档收口

完成任务：`P0-04`

代码和文档证据：

- `docs/architecture.md`：更新为 v1.2.0 开发基线，补充 HTTP、前端、Store、Job、状态聚合和在线更新流程；
- `docs/architecture.dot`：可重复生成的 Graphviz 源文件；
- `docs/architecture.svg`、`docs/architecture.png`：重新生成的当前架构图；
- `docs/upgrade-v1.2.0.md`：升级前备份、数据/API 迁移、三种升级方式、验证和 v1.1.x 回滚步骤；
- `src/models/settings.rs`、`src/api/settings.rs`：删除未使用的 `nas_sync_*` 占位配置；
- 兼容测试证明旧 JSON 中的 NAS 字段会被忽略且不会再次持久化。

测试证据：

- 架构 SVG/PNG 可由 `architecture.dot` 字节级重现；
- rustfmt、clippy `-D warnings`、完整 Rust 测试和 Release 构建通过；
- 284 个 Rust 测试通过，1 个真实网络测试按设计忽略；
- `git diff --check` 通过。

遗留风险：

- v1.2.0 版本号、CHANGELOG、README 版本正文和镜像标签尚未更新；
- 当前大工作树尚未整理为逻辑提交；
- 二进制/Docker 的真实升级和回滚演练仍待执行。

下一任务：`P0-05-01` 整理逻辑提交清单。

## 2026-07-10 — v1.2.0 Release Candidate 准备完成

完成任务：`P0-05-01`、`P0-05-03` 至 `P0-05-07`

代码和发布证据：

- `docs/v1.2.0-release-checklist.md`：6 个逻辑提交边界、版本同步、工作流和发布验证清单；
- `Cargo.toml`、`Cargo.lock`：版本 1.2.0；
- `CHANGELOG-v1.2.0.md`、README `### 1.2.0`：完整更新内容和可提取 Release notes；
- `.github/workflows/ci.yml`：all-targets/all-features check、clippy 和完整测试；
- `.github/workflows/release.yml`：tag/Cargo 版本校验、前端测试、rustfmt、clippy、完整测试及 docs/CHANGELOG 归档；
- 本地 Release 包：`my-media-sub-v1.2.0-linux-x86_64.tar.gz`，约 5.2 MiB；
- SHA256：`1cd9b4465355f765b34d6e4542d88446232fcb43f902192e3d8b0a221c24f9bd`。

验证证据：

- CSS、全部 JS 语法、19 个前端测试、rustfmt、all-targets/all-features check、clippy 和完整 Rust 测试通过；
- 284 个 Rust 测试通过，1 个真实网络测试按设计忽略；
- Release 归档包含二进制、static、README、CHANGELOG 和 docs，SHA256 校验通过；
- v1.1.3 裸 JSON → v1.2.0 schema v1：4 个迁移备份内容和 `0600` 权限验证通过；
- 手工二进制/static 回滚到 v1.1.3 后版本、订阅数量和文件哈希一致；
- v1.1.3 Docker → v1.2.0 Docker → v1.1.3 Docker 使用同一数据卷回滚通过；
- v1.2.0 新镜像健康检查、认证 API 和数据卷权限通过；
- 48 组桌面/平板/移动端、深浅主题和主页面检查无横向溢出、异常、控制台错误或失败请求。

待用户授权：

- `P0-05-02` 创建逻辑提交、tag 和 push；
- 未获授权前不会执行这些 Git 写操作。

下一任务：`P1-01-01` 开始媒体更新日历规则定义。

## 2026-07-10 — v1.3.0 Release Candidate 准备完成

完成任务：`P1-05-03`，P1 媒体更新日历全部完成

代码、文档和发布证据：

- `CHANGELOG-v1.3.0.md`：日历能力、API、排期规则、兼容性和升级注意事项；
- `docs/upgrade-v1.3.0.md`：v1.2.0/v1.1.x 升级差异、备份、验证和回滚；
- `docs/v1.3.0-release-checklist.md`：P1 逻辑提交边界、版本同步、工作流和发布验证；
- `Cargo.toml`、`Cargo.lock`、README：版本、镜像/tag 示例和 `### 1.3.0` Release 正文统一；
- `.github/workflows/release.yml`：校验精确 README 版本小节、CHANGELOG 和升级文档，精确提取 Release notes；
- `docs/architecture.md`、`.dot`、`.svg`、`.png`：v1.3.0 Calendar model/service/API、手动排期和前端模块架构；
- 本地 Release 包：`my-media-sub-v1.3.0-linux-x86_64.tar.gz`，5,721,478 bytes；
- SHA256：`7b2de2e2bdeeb975e2ed464f9946aabc9befb9acbfea25030ca8d7b9b796c464`。

验证证据：

- 24 个前端测试通过；296 个 Rust 测试已登记，295 个通过、1 个真实网络测试按设计忽略；
- CSS、全部 JS 语法、rustfmt、cargo check、clippy、完整测试、Release 构建和 `git diff --check` 通过；
- 临时服务健康检查、认证、Calendar API、静态模块、手动排期清除与元数据保持不变通过；
- 1440×1000 和 390×844 周/月/列表浏览器回归通过，无异常、失败请求、4xx/5xx 或页面级横向溢出；
- Graphviz 产物字节级可重现；Release notes、归档内容和 SHA256 校验通过。

待用户授权：

- 按 v1.2.0 与 v1.3.0 release checklist 整理当前大工作树的逻辑提交；
- 创建 v1.3.0 release commit、tag、push 和 GitHub Release；
- 未获授权前不会执行这些 Git 写操作。

下一任务：`P2-01-01` 新增 `src/services/source_quality.rs`。

## 2026-07-10 — P2 资源质量评分与安全自动换源完成

完成任务：`P2-01` 至 `P2-04`

代码和文档证据：

- `src/models/source_quality.rs`、`src/services/source_quality.rs`：0–100 后端权威评分、等级、标签、风险、剧集范围、更新时间和推荐理由；
- `tests/fixtures/source_quality.json`：Rust 与历史前端共用评分 fixtures；
- `SourceCandidate.quality`、搜索 API `quality`：新增字段使用兼容默认值；
- `Settings`：自动换源总开关、仅搜索/自动应用模式、最低分、最低分差、连续失效阈值和冷却时间；
- `SubscriptionSourceSwitchService`：候选探测评分、安全预览、季度/进度覆盖、历史与近期失败过滤、最高分候选选择、审计和回滚；
- `SubscriptionCheckService`：连续失效计数、冷却期候选复用、默认仅搜索和满足策略后的自动应用与立即检查；
- `src/api/subscription_source.rs`：候选预览、应用、历史和回滚 API；
- `static/js/features/source-switch.js` 与换源弹窗：候选评分对比、风险、覆盖、预览、策略设置、历史和回滚入口；
- `docs/source-quality.md`、`docs/api-contract.md`：评分和安全换源契约。

验证证据：

- 30 个前端纯函数测试通过；
- 303 个 Rust 测试登记时全部业务测试通过，真实 PanSou 网络测试按设计忽略；
- Rust/Node 共享 fixtures 的分数、等级、清晰度、视频/剧集数和风险完全一致；
- API 集成测试覆盖预览、手动应用、进度保持、历史、回滚和策略参数钳制；
- 单元测试覆盖历史/近期失败过滤、冷却、重复应用、最高分自动选择和回滚；
- 1440×1000 与 390×844 无头 Chrome 验证候选评分、预览、安全条件、自动换源设置，无异常、失败请求或页面级横向溢出；
- JavaScript 语法、rustfmt、clippy `-D warnings`、完整 Rust 测试和 `git diff --check` 通过。

下一任务：`P3-01-01` 定义 `AutomationEvent`。



## 2026-07-10 — P3 结构化领域事件与完整自动化流水线完成

完成任务：`P3-01` 至 `P3-03`

代码和文档证据：

- `src/models/automation_event.rs`：八阶段、七状态状态机和完整 `AutomationEvent` 契约；
- `src/store/automation_event.rs`：schema v1、原子持久化、权限修复、分级保留、幂等和三类内存索引；
- `src/services/automation_events.rs`：稳定生命周期更新、Job 广播投影、转存后 rename/STRM/Aria2/notification 结果投影；
- `src/services/subscription_check.rs`：source_check、file_filter、version_select 结构化事件和 correlation 传递；
- `src/api/automation.rs`：事件查询、当前状态摘要、单订阅/单集/单 Job 流水线及安全重试；
- `src/services/subscription_status.rs`：结构化事件优先覆盖旧通知 metadata 降级，不解析通知正文；
- `static/js/features/automation-events.js`、`static/app.js`、`static/index.html`：工作台健康摘要、订阅事件视图、耗时和重试入口；
- `docs/automation-events.md`、`docs/api-contract.md`：职责、状态机、保留、API 和安全重试契约。

本轮补强：

- Job 和订阅阶段改用稳定事件 ID 原位更新，避免历史 running 事件造成永久“卡住”；
- Store 状态更新保留 `created_at`/`started_at`，可正确计算执行耗时；
- 自动化摘要按 correlation/订阅/集数/阶段折叠当前结果，同时兼容旧式多状态事件；
- Job 广播接收 Lagged 后继续投影，不因一次拥塞永久退出；
- 单订阅流水线新增 `episode` 筛选；同步重试保证最终落到 succeeded/failed，Job 重试保留新任务关联；
- PushDispatch payload 继承 correlation/subscription 上下文，转存结果投影用户通知阶段。

验证证据：

- 33 个前端纯函数测试通过，Node runner 7/7 测试文件通过；
- 322 个 Rust 测试登记，321 个通过，1 个真实 PanSou 网络测试按设计忽略；
- CSS 构建、全部 JavaScript 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 全部通过。

当时下一任务：`P4-05`；现已由后续 P4 完成记录取代。


## 2026-07-10 — Codex 重启交接（已被后续完成记录取代）

- 该恢复点已完成；当前以文首执行指针和后续“P4 前端渐进模块化完成”记录为准。
- 仍需保留当前未提交工作树，不执行 reset、批量覆盖、清理未跟踪文件、commit、tag 或 push。

## 2026-07-10 — P4 前端渐进模块化完成

完成任务：`P4-05` 至 `P4-13`

代码和文档证据：

- `static/js/core/router.js`：纯路由规范化、URL 序列化、History state、旧设置 tab 别名和页面副作用装配；
- `static/js/core/notifications.js`：通知中心状态、Toast、安全类型归一化和轮询入口；
- `static/js/core/polling.js`：命名轮询器、事件监听器和外部资源的统一生命周期注册表；
- `static/js/stores/downloads.js`、`drive.js`、`jobs.js`、`subscriptions.js`：按领域抽离 Alpine state/getter/action；
- `static/js/features/updates.js`、`settings.js`、`dashboard.js`、`search-page.js`、`calendar-page.js`：页面功能模块；
- `static/js/core/shell.js`：初始化、主题、全局快捷键和刷新装配；
- `static/app.js`：仅保留 descriptor-safe store 组合和 `app()` 暴露；
- `static/index.html`：按依赖顺序加载模块，移除重复的显式 `x-init`，依赖 Alpine `init()/destroy()` 生命周期。

验证证据：

- 43 个前端 Node 测试通过，其中新增路由、轮询注册表、下载/网盘/更新/通知纯函数、装配层和页面切换清理测试；
- 322 个 Rust 测试登记，321 个通过，1 个真实 PanSou 网络测试按设计忽略；
- CSS 构建、全部 `static/**/*.js` 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 全部通过；
- 真实服务静态模块均返回 `text/javascript`；无头 Chrome 中 Alpine 初始化完成、`x-cloak` 移除、History route 为 dashboard、无运行时异常或控制台错误；
- dashboard→search→dashboard 切换时，通知轮询从注册表移除后只恢复一个实例，`popstate`、全局快捷键和 Job EventSource 始终各保留一个；
- 页面切换先停止离开页面的搜索和在线更新轮询，再启动目标页面副作用；重复注册同名 timer/listener/resource 会先清理旧实例，应用销毁统一关闭全部资源。

下一任务：`P5-01-01` 拆分 subscriptions API。



## 2026-07-11 — P5 后端模块化与性能完成

完成任务：`P5-01` 至 `P5-04`

代码和测试证据：

- API：`src/api/subscriptions/`、`src/api/drive/`；
- Job：`src/jobs/worker/`、`src/jobs/{model,store,queue}.rs`；
- 批量持久化：`src/store/subscription.rs`、`src/services/subscription_check.rs`；
- 并发与限速：`src/models/settings.rs`、`src/api/settings.rs`、`src/api/drive/aria2.rs`、`src/clients/{mod,quark,quark_save}.rs`；
- 测试覆盖原子批量提交、保存失败回滚、并发更新、同订阅互斥、同链接探测去重、Job 活跃幂等、重启去重、Aria2 上限和上游 429。

验证证据：

- 43 个前端 Node 测试通过；334 个 Rust 测试登记，333 个通过，1 个真实 PanSou 网络测试按设计忽略；
- 全部 JavaScript 语法、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 通过。

下一任务：`P6-01` 定义 CloudDriveProvider 能力边界。

## 2026-07-11 — P6 CloudDriveProvider 抽象完成

完成任务：`P6-01` 至 `P6-06`；`P6-07` 按约束继续保持待产品需求状态。

代码和测试证据：

- `src/providers/mod.rs`：定义 `probe/list/find/ensure/transfer/rename/delete/download_info/health` 对象安全能力边界、通用文件/下载/健康数据和 `CloudDriveProviderRegistry`；
- `src/providers/quark.rs`：用现有 `QuarkShareProbe` / `QuarkSaveClient` 实现 Quark Provider，保留递归探测、手动整分享转存、订阅选择性转存、目录、重命名、删除和下载直链行为；
- `src/services/{subscription_check,subscription_transfer,strm}.rs` 与 `src/jobs/worker/manual_transfer.rs`：业务检查、自动转存、重命名、同步下载、STRM 和手动转存通过 Provider 能力调用，不再直接依赖 `QuarkSaveClient`；
- `src/providers/mock.rs`：提供可注入探测结果、目录项、转存记录和按操作失败注入；检查与转存 Service 测试证明 `cloud_type=mock` 会选择注入 Provider；
- `src/api/subscriptions/crud.rs`：创建和更新时规范化并校验 `cloud_type`，空值兼容为 `quark`，未知 Provider 提前返回稳定校验错误；
- `src/services/quark_signin.rs`：签到继续作为夸克专属服务直接使用夸克扩展 API，不进入通用 CloudDriveProvider 能力面；
- 未新增阿里云盘、115 等第二 Provider，遵守 `P6-07` 的需求门槛。

验证证据：

- 343 个 Rust 测试登记，342 个通过，1 个真实 PanSou 网络测试按设计忽略；
- `cargo check --all-targets --all-features`、`cargo clippy --all-targets --all-features -- -D warnings`、完整 Rust 测试、rustfmt 和 `git diff --check` 通过。

下一任务：`P7-01-01` 定义并实现完整 DATA_DIR 业务数据备份。

## 2026-07-11 — P7 备份、诊断、可观测性与安全完成

完成任务：`P7-01` 至 `P7-03`。

代码和文档证据：

- `src/services/backup.rs`、`src/api/backup.rs`：完整 DATA_DIR 自描述 JSON 归档、Base64 内容、逐文件 SHA-256、格式/schema/大小校验、定时备份、保留策略、存储预算、恢复前快照、原子写入、保留路径与符号链接防护以及恢复后重启标记；
- `src/api/diagnostics.rs`、`static/js/features/diagnostics.js`：脱敏诊断 API、诊断包导出和 WebUI 系统诊断页，展示版本、schema、数据大小、队列、调度器、外部服务配置状态、备份、指标和密码风险；
- `src/api/mod.rs`：请求/关联 ID、安全日志、五次失败/60 秒认证限速、`Retry-After`、CSP、nosniff、iframe/referrer/permissions 安全头；
- `src/utils/metrics.rs`：检查/转存耗时、失败阶段、队列深度、换源和备份/恢复指标；
- `src/utils/mod.rs`、`src/error.rs`：错误和已识别 URL/query/header 敏感字段统一脱敏；
- `static/js/theme-init.js`：首页主题初始化移出内联脚本，CSP 下不再需要 `script-src unsafe-inline`；
- 网盘单项/批量删除要求匹配 ID/数量的确认文本，订阅删除要求确认参数匹配订阅 ID，恢复要求精确输入 `RESTORE DATA`；
- `.github/workflows/ci.yml` 增加 RustSec 依赖审计；`docs/https-reverse-proxy.md` 增加 HTTPS、可信代理、备份敏感性和安全部署说明；README、环境变量和 API 契约同步更新。

验证证据：

- 351 个 Rust 测试登记，350 个通过，1 个真实 PanSou 网络测试按设计忽略；新增备份往返/保留/篡改/路径穿越、密码风险、认证限速、安全头、request/correlation ID、诊断和备份 API 集成测试；
- 11 个前端 Node 测试通过，全部 `static/**/*.js` 语法检查通过；
- rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试和 `git diff --check` 通过。

下一任务：`P8-01` 新增 manifest、图标和 service worker。

## 2026-07-11 — P8 PWA 与移动端完成

完成任务：`P8-01` 至 `P8-06`。

代码和文档证据：

- `static/manifest.webmanifest`、`static/icons/`：standalone Manifest、192/512/maskable PNG 图标以及今日更新、缺集、失败任务、检查全部、下载进度和夸克签到六类快捷入口；
- `static/service-worker.js`、`static/js/pwa-policy.js`：版本化应用壳层、HTML network-first、静态资源 stale-while-revalidate、旧缓存清理和可独立测试的请求分类/响应缓存策略；
- `/api/*`、`/strm/*`、`/health`、跨域请求和非 GET 请求始终 network-only；401/403、非 200、`private` 和 `no-store` 响应永不缓存；
- `static/js/features/pwa.js`：Service Worker 注册、认证后缓存预热、安装提示、在线/离线状态、新 Worker 更新提示、`SKIP_WAITING` 和快捷动作复用；
- `static/index.html`、`tailwind/input.css`：安装入口、更新/离线横幅、六项快捷面板和 390px 专用布局；
- `src/api/mod.rs`：Service Worker 返回 `no-cache` 和根 scope 许可；Manifest、Worker 和图标继续受 Basic Auth 保护；
- `tests/frontend_pwa.test.js`、`tests/api_integration.rs`：覆盖缓存分类、敏感路由、Basic Auth 401 不缓存、离线壳层、版本清理、Manifest 快捷入口、Service Worker 响应头和 390px 合同；
- `docs/pwa.md`：安装、缓存安全、更新、移动端和故障排查说明。

验证证据：

- 352 个 Rust 测试登记，351 个通过，1 个真实 PanSou 网络测试按设计忽略；
- 12 个前端 Node 测试通过，全部 `static/**/*.js` 语法检查通过；
- Tailwind CSS 重建、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试和 `git diff --check` 通过。

下一任务：`P9-01` 增加 JSON Store 大小、解析耗时和写入耗时监控。

## 2026-07-11 — P9 JSON 优化与 SQLite 决策完成

完成任务：`P9-01` 至 `P9-05`；当前规模未触发 SQLite，按门槛继续使用 JSON。

代码和文档证据：

- `src/store/schema.rs`、`src/utils/mod.rs`、`src/utils/metrics.rs`：统一采集 Store 当前大小、读写次数/字节数、解析/写入耗时和失败；业务 Store 改为紧凑 JSON；
- `src/store/{subscription,notification,automation_event,settings}.rs`、`src/jobs/store.rs`：显式历史保留、手动整理接口以及紧凑重写；订阅检查 30、换源/历史链接 50、通知 300、Job 500、自动化事件 30/90 天且最多 5,000；
- `SubscriptionStore` 和 `JobStore` 新增 ID 内存索引；AutomationEventStore 继续维护订阅、correlation ID 和 Job ID 索引；
- `src/api/storage.rs`：`POST /api/storage/compact` 重新应用保留策略并改写所有业务 Store；
- `src/services/storage.rs`、`src/api/diagnostics.rs`：500 订阅、10,000 历史、32 MiB 文件和复杂查询门槛；诊断页展示逐 Store I/O 与 SQLite 决策；
- `tests/json_performance.rs`：500 订阅和各 10,000 条 Job/通知/自动化事件的序列化、解析、大小和索引基线；
- 压力样本最大单 Store 约 2.68 MiB，紧凑序列化约 2.63–103.03 ms，解析约 2.30–96.58 ms；
- `SqliteMigrationContract` 固化保留 JSON、可重复导入、计数/校验和验证、切换前回滚和禁止长期双写；实际规模未过门槛，因此不创建 SQLite 文件；
- `docs/storage-scaling.md`：记录基线、生产保留上限、迁移门槛和未来迁移状态机。

验证证据：

- 357 个 Rust 测试登记，356 个通过，1 个真实 PanSou 网络测试按设计忽略；
- 12 个前端 Node 测试通过，全部 `static/**/*.js` 语法检查通过；
- Tailwind CSS 重建、rustfmt、all-targets/all-features check、clippy `-D warnings`、完整 Rust 测试、Release 构建和 `git diff --check` 通过。

下一任务：`P10-04` 评估并实现浏览器 Push。

## 2026-07-11 — P10 可直接实施功能完成

完成任务：`P10-04` 至 `P10-07`。`P10-01/02/03/08` 继续保持产品决策门禁。

代码和文档证据：

- Browser Push：标准 VAPID P-256 密钥自动生成、PushSubscription 持久化、RFC Web Push 加密发送、Service Worker `push`/`notificationclick`、浏览器权限与订阅开关；
- Webhook：最多 5 个 HTTP(S) 目标、统一事件 JSON、可选 HMAC-SHA256 `X-Media-Sub-Signature-256` 签名，并纳入现有重试/推送报告；
- OpenAPI：`static/openapi.json` 提供 OpenAPI 3.1 契约，`/api-docs.html` 提供无第三方脚本的受保护查看页；
- 数据导入导出复用完整 DATA_DIR 备份、预览和恢复闭环；订阅新增最多 20 个规范化标签；`dashboard_widgets` 支持 API 配置首页快捷区、Hero、KPI、媒体库和运维组件；
- 测试覆盖 VAPID 密钥兼容、OpenAPI、Browser Push 状态和标签去重。

验证：359 个 Rust 测试登记，358 个通过，1 个真实 PanSou 网络测试按设计忽略；12 个前端 Node 测试、CSS 构建、JS 语法、rustfmt、clippy `-D warnings` 和 `git diff --check` 通过。

当前不实施：Telegram 主动控制、NAS 同步、第二 Provider 和多用户权限，均需产品定位或外部需求确认。

下一任务：执行 Q 质量门和发布准备；未经授权不创建 tag 或 Release。

---

# P11–P20 单用户夸克深耕计划

> 范围约束：暂不增加第二网盘，不建设多用户、多租户、RBAC、SSO 或用户级数据隔离。现有 Provider 边界继续用于隔离夸克实现和测试替身。

## P11. v1.9.x 稳定性收口

- [x] `P11-01` 增加真实 release 二进制启动烟雾测试，覆盖健康检查、Basic Auth、首页、PWA 资源和诊断 API，并接入 CI/Release。
- [x] `P11-02` 建立 v1.3.0 → 当前版本的二进制与 Docker 数据卷升级烟雾测试。
- [ ] `P11-03` 增加备份导出、恢复预览、恢复执行及失败回滚端到端测试。
- [ ] `P11-04` 增加 PWA 跨版本缓存升级测试和真实浏览器 390px 测试。
- [ ] `P11-05` 增加长时间运行、Store 增长、调度器隔离和异常任务恢复验证。

## P12. 订阅追更准确性

- [ ] 扩充真实文件名语料，覆盖上下集、合集、SP/OVA、季目录及中英文数字变体。
- [ ] 提供订阅级解析覆盖规则和文件接受/拒绝/替换原因。
- [ ] 增加缺集、重复集、跳集诊断及转存前最终文件计划预览。

## P13. 搜索与安全换源

- [ ] 增加搜索关键词偏好、排除词、质量/字幕/编码偏好和稳定去重排序。
- [ ] 增加候选探测缓存、退避降级及换源覆盖范围预览。
- [ ] 完善仅建议模式、通知摘要、历史回滚和冷却策略。

## P14. 转存与媒体库工作流

- [ ] 增加目录冲突策略、重命名预览和跨平台安全文件名。
- [ ] 增加 STRM 失效/孤立扫描及 Aria2 幂等重试。
- [ ] 增加 Jellyfin、Emby、Plex 媒体库刷新 Webhook 和统一进度。

## P15. 任务队列与调度可靠性

- [ ] 增加任务优先级、公平调度及分层并发限制。
- [ ] 增加错误分类、指数退避、抖动、熔断和恢复探测。
- [ ] 增加卡死检测、维护模式、队列积压告警和历史归档。

## P16. 通知中心

- [ ] 增加事件级别、渠道路由、安静时段、聚合摘要和重复限频。
- [ ] 增加渠道状态诊断、模板预览、Webhook 重试和签名轮换。
- [ ] 保证所有推送失败均不阻塞核心自动化流水线。

## P17. WebUI 与移动端

- [ ] 优化移动端详情、批量操作、筛选持久化和大列表性能。
- [ ] 统一加载、空状态、错误边界、危险确认和键盘无障碍体验。
- [ ] 增加自动化时间线、诊断复制和真实浏览器端到端测试。

## P18. 可观测性与故障排查

- [ ] 串联 request/correlation/subscription/job 标识并统一结构化日志。
- [ ] 增加 Prometheus 指标、慢操作、外部依赖延迟和动态日志级别。
- [ ] 增加只读数据一致性、磁盘、权限、时区和 DNS 诊断建议。

## P19. 备份与数据生命周期

- [ ] 增加备份校验清单、定期可恢复性验证和外部目录复制。
- [ ] 增加 Store 增长预警、独立保留周期及清理预览。
- [ ] 达到已记录阈值后再启动 SQLite 决策，不提前长期双写。

## P20. API 与自动化集成

- [ ] 保持 OpenAPI 与路由同步并建立契约兼容回归测试。
- [ ] 增加单实例自动化 Token 的轮换、撤销和最小作用域；不扩展为多用户认证。
- [ ] 增加幂等键、版本化 Webhook、订阅导入导出和自动化示例。
