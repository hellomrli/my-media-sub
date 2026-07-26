# 界面布局与功能设计评估 2026-07-26

范围：`main` 分支 v2.2.14 工作树。评估对象是**信息架构与功能设计的合理性**，以及前后端的冗余内容。与同日的 `code-review-2026-07-26.md`（工程质量）互补，不重复其结论。

## 总体判断

底层架构是健康的：模块化 UMD + 摊平 store 的组合让 8k 行原生 JS 保持了可读性，活动中心（v2.2.12）把后台任务和通知合并成一条时间线是正确的收敛，订阅列表→详情的 master-detail 走 URL 路由也是对的。

问题集中在**三个方向**：工作台把同一批数字重复展示了三遍；三个后端功能完整实现却没有任何界面入口；以及积累了约 1100 行前后端死代码。

---

## 一、信息架构

### 1.1 工作台：三个控件区展示同一批数字

工作台自上而下有五个区块，其中三个在重复表达同样的状态：

| 区块 | 内容 |
|---|---|
| 快捷入口 | 今日更新、缺集、**失败任务**、检查全部、**下载进度**、夸克签到 |
| 待处理事项 | 失效订阅、**失败任务**、**未读通知** |
| KPI 卡片 | 活跃订阅、后台任务（含失败数）、**实时下载**、**通知收件箱** |

「失败任务」出现三次，「未读通知」和「下载」各两次。合计工作台上有 **6 个入口通向同一个活动中心 tab**（快捷入口 1 个、待处理事项 2 个、KPI 2 个、最近活动面板 1 个），再加侧边栏一处。

这三个区块各自都合理，问题在于并存。建议保留「待处理事项」（唯一带异常语义、能直接下钻到筛选后视图的），KPI 降为纯展示不可点，快捷入口只留下钻不到的动作（检查全部、夸克签到）。

### 1.2 导航顺序与代码里写的意图不符

`static/partials/nav.html:19` 的过滤数组写作 `['dashboard','calendar','search','subscriptions','drive','downloads']`，但 `.filter()` 保持的是 `tabs` 数组的顺序，实际渲染为 **dashboard → calendar → search → drive → downloads → subscriptions**。

订阅管理是这个产品的核心功能，却被排在网盘和下载之后。过滤数组里的顺序说明作者本意并非如此。改 `core/router.js:77` 的 `tabs` 数组顺序即可对齐。

### 1.3 工作台缺少描述文案

`core/router.js:78` 里 dashboard 的 `description` 是空字符串，其余 8 个 tab 都有。结果是停在工作台时，页头的 `page-description` 和侧栏的 `nav-description` 都是空白，与其它页面的视觉节奏不一致。

### 1.4 移动端：9 个 tab 挤在横向滚动条里

`nav.html:106` 的移动端 tab 栏渲染 `tabs` 全部 9 项，包含「系统设置」——而桌面端特意把设置放在侧栏 footer 的独立位置，与其它 tab 分层。移动端丢掉了这个层次。

### 1.5 移动端表格策略不统一

全部 `.data-table` 在 `tailwind/input.css:375` 硬性 `min-width: 760px`，390px 屏上每个列表都要横向滚动近两屏。

只有 `page-drive.html:100` 做了响应式（`hidden lg:grid` 表头 + 行内标签），订阅、活动中心、下载三个页面都没有。要么统一采用 drive 的模式，要么统一接受横向滚动，目前是两种策略并存。

### 1.6 设置页：7 个保存按钮全都是全量保存

`page-settings.html` 有 1 个「保存全部设置」加 6 个分区「保存设置」，7 个都调用同一个 `saveSettings()`，`JSON.stringify(this.settings)` 整体 POST。分区按钮的措辞暗示了分区保存，实际不存在。建议只保留顶部那个「保存全部设置」，或改为吸底单按钮。

另外同一个 tab 被拆成两个平级容器：`connections` 在 `:37` 和 `:111`，`maintenance` 在 `:595` 和 `:622`，每个 tab 的可见性判断被求值两次。

---

## 二、有实现但没有入口的功能

这一类比死代码更值得关注：后端完整可用，只是用户碰不到。

### 2.1 媒体库刷新（Emby / Jellyfin / Plex）—— 零 UI、零文档

`src/services/media_library.rs` 是一个 103 行的完整实现，按 provider 分派认证头（Emby/Jellyfin 用 `X-Emby-Token`，Plex 用查询参数，其余用 Bearer），已接入 `src/services/post_transfer.rs:83` 的转存后钩子，4 个设置项在 `api/settings.rs` 有完整的读写与密钥脱敏。

但 `grep media_library static/` **零命中**，`docs/` 和 `README.md` 也**零命中**。用户只能直接 POST `/api/settings` 才能启用。

这是本次评估影响最大的一条：功能是好的，代价只是没人知道它存在。

### 2.2 手动排期（manual_schedule）—— 零 UI

后端 `src/services/media_calendar.rs:185-198` 用它驱动更新日历的排期推算，有独立的 `validate_manual_schedule`。前端 `stores/subscriptions.js` 有 7 个对应字段、`manualSchedulePayload()` 和 `shouldSendManualSchedule()`，创建和编辑订阅时都会提交。

但生成的 `static/index.html` 里 `manual_schedule` **0 处命中**——没有任何表单能设置它。星期选择器的辅助函数 `toggleManualScheduleWeekday` 和配套的 `.schedule-weekdays` 样式都还在，但绑定它们的 HTML 已被移除（本次已作为死代码清理）。

现状不会丢数据（已有值会正常回填并回传），但用户无法新建或修改排期。

### 2.3 两个不起作用的开关

`settings.probe_quark_files`（默认嗅探文件列表）和 `settings.filter_bad_links`（默认过滤失效链接）在 `page-settings.html:611-612` 有 checkbox，能保存、能持久化，但 **Rust 侧除了 setter 之外没有任何读取点**。勾选它们不改变任何行为。

要么接上，要么连同 UI 一起移除——现状是最坏的：用户以为自己配置了什么。

### 2.4 `trust_proxy_headers` 无 UI（可接受）

影响限流的 IP 归属判断（`src/api/mod.rs:174`），无界面入口，但 README 和升级文档有记载，属于部署级设置。可以维持现状，若要收拢建议放进设置页的安全区块。

---

## 三、API 层面的冗余

完整的 105 个 operation 逐条核对结果：

### 3.1 幽灵路由（已修复）

`openapi.json` 曾对外宣称 `GET /strm/quark/{fid}/{file_name}`，但**服务器从不提供这条路由**：`src/api/strm.rs` 从未被 `mod` 声明，整个文件不参与编译。`scripts/check-openapi.py` 用文本扫描 `src/api/**`，把这个死文件里的 `.route(...)` 字面量当成了真实路由写进规范。

本次删除 `api/strm.rs` 后契约检查立刻报错，已同步从规范中移除该 operation（92 路径 / 104 operation）。

### 3.2 12 个孤儿端点（约 351 行，未处理）

零消费方——前端、测试、smoke 脚本、CI、文档全都不引用：

| 端点 | 位置 | 可省行数 |
|---|---|---|
| `GET /api/push/status` | `api/push.rs:457` | 121 |
| `POST /api/push/webhook/rotate-secret` | `api/push.rs:448` | 41 |
| `POST /api/subscriptions/{id}/source-candidates/probe` | `api/subscription_source.rs:337` | 38 |
| `GET /api/push/diagnostics` | `api/push.rs:445` | 32 |
| `POST /api/push/template/preview` | `api/push.rs:446` | 30 |
| `GET /api/automation/events` | `api/automation.rs:347` | 29 |
| `GET /api/jobs/archive` | `api/jobs.rs:123` | 19 |
| `POST /api/storage/compact` | `api/storage.rs:218` | 12 |
| `GET /api/jobs/{id}/pipeline` | `api/automation.rs:354` | 11 |
| `GET /api/jobs/{id}` | `api/jobs.rs:125` | 10 |
| `GET /api/metrics` | `api/metrics.rs:60` | 4 |
| `GET /api/automation-token/scopes` | `api/automation_token.rs:46` | 4 |

其中几组是功能重叠而非单纯闲置：

- `POST /api/storage/compact` 与 `/api/storage/cleanup` **调用同一个 `execute_cleanup`**，只有确认短语不同（`"COMPACT JSON"` vs `"CLEANUP DATA"`），`docs/api-contract.md:176` 自承是兼容入口。
- `GET /api/metrics` 的载荷已被 `GET /api/diagnostics` 内嵌（`api/diagnostics.rs:35`），且与 Prometheus 端点 `/metrics` 同源不同序列化。三向重叠，只有 `/metrics` 有真实消费方。
- `.../source-candidates/probe` 是 `.../preview` 的前半段，handler 注释自承"为兼容旧调用"。
- `GET /api/push/status` 那 99 行手写 if 链重复实现了 `services/push.rs:493` 的 `enabled_channels()`。

### 3.3 一条前次评审的更正

`code-review-2026-07-26.md` 提到「`/api/drive/aria2/*` 共 12 个 operation，任何测试都未引用」。测试无覆盖属实，但**这 11 个 operation 全部有前端调用点**（`stores/downloads.js:150/172/199/221`、`stores/drive.js:381`、`stores/subscriptions.js:1620`、`features/settings.js:410`）。删除会直接砸掉下载管理页。真正的问题是测试缺口，不是死代码。

---

## 四、前端架构

### 4.1 `stores/subscriptions.js` 是 2700 行 / 210 方法的 god object

占全部前端 JS 的 34%，至少混装 6 个关注点：订阅向导、命名规则中心、自定义分类、元数据搜索、季集解析、状态格式化。

更值得注意的是**反向依赖**：设置页从这个订阅 store 里取用约 10 个方法——`sanitizeCheckInterval`、`sanitizeQuarkSigninHour`、`sanitizeSourceSwitchPolicy`、`normalizeCustomCategories`、`addCustomCategory`、`removeCustomCategory`、`previewRuleCenter`、`saveRuleCenterPreset`、`deleteRulePreset`、`useRuleCenterAsDefaultTemplate`。规则中心的 UI 在设置页，实现却整个住在订阅 store 里。

拆分建议：`stores/rule-center.js`（规则预设与预览）、`stores/subscription-form.js`（向导与表单），主 store 只留列表与详情。

### 4.2 单文档 260KB，全部页面常驻 DOM

`index.html` 是 2875 行的单文档，含 276 个 `x-show`、74 个 `x-for`、123 个 `x-model`，全部挂在同一个 Alpine 组件（738 键的摊平 store）上。所有 tab 的 DOM 始终存在，仅靠 `x-show` 切换可见性。

设置页一个就是 737 行 / 66KB（占全部页面内容的 33%），停在工作台时它依然完整存在于 DOM 中并被 Alpine 遍历。对一个以 390px 移动端为目标的 PWA，把大页面改成 `x-if` 惰性挂载会明显降低首屏解析与水合成本。设置项状态存在 store 而非 DOM，所以 `x-if` 的重建是安全的，但需要实测。

### 4.3 任务数据同时走 SSE 和轮询

`stores/jobs.js:285` 开着 `/api/jobs/events` 的 EventSource（`snapshot` 事件直接整体覆盖 `this.jobs`，`job` 事件增量更新），同时 `core/notifications.js:180` 每 30 秒轮询 `loadActivity()` → `loadJobs()` 重新拉取整个任务列表。

SSE 连接正常时轮询是纯冗余。合理的做法是把轮询降级为 SSE 断线后的兜底（监听 `onerror` 再启动）。

---

## 五、本次已执行的清理

全部改动均通过：488 个后端测试、85 个前端测试、clippy `-D warnings`、eslint、OpenAPI 契约校验、真实浏览器 E2E。

### 后端（约 570 行）

- **4 个完全死的文件**：`src/api/strm.rs`（136 行，从未 `mod` 声明，不参与编译）、`src/models/transfer.rs`（106 行，活的版本在 `services/transfer_rule.rs`）、`src/store/session.rs`（115 行，`telegram_bot` 用的是同名但不同的私有 struct）、`src/models/search.rs`（135 行，唯一消费者就是 `store/session.rs`）。
- **3 个零使用依赖**：`anyhow`、`thiserror`（`error.rs` 手写 `Display`，全仓无 `#[derive(Error)]`）、`tokio-test`；`tower` 从 `[dependencies]` 移到 `[dev-dependencies]`（唯一使用点是 `tests/api_integration.rs:11`）。`Cargo.lock` 少 14 行。
- **7 处说谎的 `#[allow(dead_code)]`**：标注的代码其实都在用（`quark_save.rs` 的 `delete_items`、`subscription_scheduler.rs` 的 `reload`、`models/mod.rs` 的 calendar 模块），以及 4 处模块级 `#![allow(dead_code)]` 毯子。

摘掉毯子后编译器立刻抖出一个真问题：`src/services/episode.rs:740` 的 `test_matches_subscription_season_uses_parent_path_context` **缺 `#[test]` 标注，从未运行过**。这个测试覆盖的是「第六季」父目录上下文的季号匹配，属于易错逻辑。补上标注后测试通过——生产代码本来是对的，只是这份断言一直没生效。后端测试数 487 → 488。

### 前端（约 480 行）

- **2 个孤儿 partial**：`page-jobs.html`（137 行）、`page-notifications.html`（75 行）。v2.2.12 活动中心上线后就不在 `index.tmpl.html` 的 include 列表里，物理上已进不了 `index.html`。它们用到的 handler 全部在活跃文件中仍有引用，删除无级联。
- **214 行零引用 store 成员**（30 个方法/getter），最大的几个是 `transferToQuark`(20)、`selectQuickDirForSub`(18)、`filteredBackgroundJobs`(18)、`backgroundJobStatuses`(11)。
- **11 个退化为只写的状态字段**及其 localStorage 持久化条目。
- **3 处不可达的 `transferHistory` 分支**：`currentTab` 的三个赋值点都经过 `normalizeTab` 归一化，永远不会取到这个值。`router.js:20` 的别名映射保留——它是老书签 `?tab=transferHistory` 仍然可用的原因。
- **死 CSS**：`.schedule-weekdays` 整块、`.inbox-item`、`.badge-info`、以及 6 处分组选择器里的死 `.inbox-*` token。`.inbox-level` 保留——它由 `page-dashboard.html:141` 的 `'inbox-level ' + item.level` 动态拼接。
- `static/icons/app-icon.svg`、`tests/fixtures/mock_quark_share.json`：零引用。

> `static/styles.css` 未重新编译（本机无 Tailwind standalone CLI）。已删规则仍留在编译产物中，属无害冗余，下次 `scripts/build-css.sh` 会自然消失。

---

## 六、未处理，需要决定

### 6.1 STRM 死链（约 530 行）

`src/services/strm.rs`（435 行）加 `subscription_transfer.rs` 的三个方法。`services/mod.rs:38` 的 `STRM_MODULE_ENABLED = false` 让私有的 `generate_strm_files` 常量折叠后恒返回 `None`，两个 pub 入口零调用。

**没有动它的原因**：`scripts/check-openapi.py:24` 的注释写明「STRM is intentionally retired in v2.2.0 and will return as an independent module」。这 435 行是能工作的实现，删不删取决于它是否真的会回来。若确定不回来，还可连带清理 `models/settings.rs:200-219` 的 5 个 STRM 设置项和 `subscription_status.rs` 的流水线阶段（合计约 200 行），但需要考虑历史 `settings.json` 的反序列化兼容。

### 6.2 12 个孤儿端点（约 351 行）

见 3.2。其中 `storage/compact`、`api/metrics`、`source-candidates/probe` 三条是明确的重复入口，删除风险最低。其余 9 条建议先确认没有外部脚本在调。

### 6.3 `api/drive/automation.rs:120-253`

134 行 `#[cfg(test)]` 测试辅助函数，逐字节复制自 `download_monitor.rs` 的私有函数（`format_bytes` 两份完全相同）。副本存在的唯一理由是让 `api/drive/mod.rs:348-547` 的测试有东西可测——等于在测副本而不是测真实实现。应改为测 `download_monitor.rs` 本体。

另有 `format_bytes` 共 4 份实现（`api/update.rs:326`、`services/quark_signin.rs:339`、`services/download_monitor.rs:562`、以及上述副本），行为有细微差异，可收敛到 `src/utils/`。

### 6.4 `filterNotificationItems`

`core/notifications.js:21` 的导出纯函数，应用内唯一消费者 `filteredNotifications` 已随本次清理删除，现在只剩 `tests/frontend_p4_stores.test.js:116` 在测它。留着或连测试一起删都可以。
