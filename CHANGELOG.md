# Changelog

My Media Sub 的版本变更记录。新版本写在上方。

升级步骤见对应的 [`docs/upgrade-v*.md`](docs/)；当前版本发布说明摘要也写在 [`README.md`](README.md) 的「版本说明」中。

## 2.2.7

### 修复

- Job Worker：超时标记、成功清理错误字段、附加 `error_class`、自动重试入队等路径在 store 写失败时写入日志并累加 `job_store_update_failures` 指标，不再静默丢弃。
- Job 运行句柄注册表：mutex poison 时恢复内层 map 并告警，避免后续取消/中止路径 panic。
- 延迟唤醒与重试信号在 channel 关闭时记录警告。

### 文档

- `docs/roadmap.md` / `docs/architecture.md` 对齐当前基线；架构图转存流水线标注 STRM 已下线。

### 其他

- Telegram 搜索结果上限 5 → 10。
- PWA 缓存代次与前端资源版本升至 2.2.7。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.6 升级，保留现有 `data/`。

### 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。

## 2.2.6

### 新功能

- 多季订阅：季号支持 `1-4` / `season_spec`；目标目录仅写剧名，转存与 Aria2 按文件季号自动进入 `Season N`。
- 无季号提示的文件在多季订阅中回落到起始季，避免整批被跳过。
- 剧名魔法匹配下沉到 Rust（`title_normalize` + `/api/utils/normalize-title`）；搜索结果返回 `display_title`。
- 重命名预览返回服务端 `groups` / `show_root` / `multi_season`；多季 UI 按 Season 折叠。
- 订阅列表附加 `season_label` / `status_*` / `progress_*` 展示字段。
- Telegram：主菜单、`/search`、`/subscribe`、`/switch`、`/switch_apply`；会话跨重启持久化；内联按钮选序号；创建订阅后提交元数据刮削。
- 换源强制应用：不可越过探测失败与季度不匹配。

### 修复

- 编辑订阅不再误清手动排期；批量删除确认、一键转存密码、网盘 list 错误语义、find-path 只读等功能问题。
- 单季 Aria2 同步下载默认落到 `类型目录/剧名/Season N`，与网盘结构对齐。

### 兼容性

- JSON Store schema 未变化（Telegram 会话字段新增且有默认值）。
- 可直接从 v2.2.5 升级，保留现有 `data/`。

### 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。

## 2.2.5

### 修复

- 编辑保存订阅时不再误清空已有手动排期；仅在明确启用并填写排期时才提交 `manual_schedule`。
- 网盘批量删除补齐后端要求的确认文本，修复“确认后仍删除失败”。
- 搜索结果一键转存会携带分享密码，加密分享可正常转存。
- 网盘列表失败返回真实错误，不再把 Cookie 失效/限流伪装成空目录。
- `find-path` 改为只读解析路径，查找目录时不会在夸克上自动创建缺失路径。
- 设置完成度正确识别自定义分类的 `dir` 字段。
- 搜索进行中会点亮全局 busy 状态（`searching`）。
- 订阅列表加载失败会提示用户，不再静默显示为空。
- 重命名预览在分享探测失败时返回 `probe_warning`，避免误以为已探活。
- 订阅对话框支持浏览网盘选择目标目录；创建订阅会保留所选 `target_fid`。
- 定时调度尊重订阅规则中的 `check_interval_minutes` / `check_weekdays`；手动“检查全部”仍检查全部启用订阅。
- 网盘按 `path` 可列出非根目录；进入/返回目录时强制刷新，减少陈旧缓存。
- 移除 WebUI 中已下线的 STRM 入口提示；OpenAPI 版本与发布版本对齐。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.4 升级，保留现有 `data/`。

### 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。

## 2.2.4

### 修复

- TMDB 海报、搜索结果和日历缩略图改为通过应用同源接口加载，浏览器不再直接请求 `image.tmdb.org`，避免登录或订阅检查后受第三方图片缓存、DNS、隐私拦截和连接复用影响。
- VPS 图片代理使用共享 HTTP 客户端与有界内存缓存，并向浏览器返回长时间 immutable 缓存头。
- 所有关键 JS/CSS 资源增加与应用版本一致的查询参数，确保被旧 Service Worker 控制的客户端也会获取当前前端代码。
- PWA 缓存代次和预缓存资源同步升级到 v2.2.4。

### 安全性

- 图片代理只允许明确列出的 TMDB 图片尺寸、安全文件名和常见栅格图片扩展名。
- 拒绝路径穿越、SVG/非图片响应和超过 8 MiB 的图片。
- 代理接口继续受 Basic Auth 或只读自动化 Token 保护。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.3 升级，保留现有 `data/`。

### 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。

## 2.2.3

### 修复

- 修复批量检查完成并重新加载订阅数据时，Alpine 按相同订阅 ID 复用图片 DOM 节点，使 `remote-image-failed` 继续生效并将实际已存在的海报显示为透明的问题。
- 订阅数据刷新后主动恢复失败图片节点，清理失败、重试计数和旧版本遗留的 `hidden` 状态，然后使用新的 cache-busting URL 重试。
- 新增相同 URL 和相同 DOM 节点被复用时的回归测试，同时确保正常图片不被重复请求。
- 提升 PWA 缓存版本，确保客户端获取本次图片节点恢复逻辑。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.2 升级，保留现有 `data/`。

### 升级

```bash
# Docker
docker compose pull && docker compose up -d

# 二进制：备份 DATA_DIR → 校验新包 → 同时替换二进制和整个 static/ → 保留 data/ → 启动后检查 /health
```

不要只替换二进制而继续使用旧版 `static/`。详细步骤与回滚见对应版本的升级指南。

## 2.2.2

### 修复

- PWA 关键 JS、CSS 和 Worker 改为 network-first；在线时不再先执行旧缓存，离线时仍可回退到当前版本缓存。
- 提升 Service Worker 缓存版本，升级后会激活新 Worker 并清理 v2.1.2 遗留的应用壳层与静态缓存。
- 海报、搜索结果和日历缩略图发生临时网络错误时自动退避重试两次，不再设置会跨后续状态持续生效的原生 `hidden`。
- 发布测试从 `Cargo.toml` 动态校验 PWA 缓存版本，避免后续版本再次漏升缓存键。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.1 升级，保留现有 `data/`。

## 2.2.1

### 修复

- 更新日历不再把缺少播出日期的元数据占位集推断为未来排期。
- 非同步下载订阅在已知集数达到目标集数、但历史转存文件名无法解析时，检查后会正确标记为完结。

### 兼容性

- JSON Store schema 未变化。
- 可直接从 v2.2.0 升级，保留现有 `data/`。

## 2.2.0

### 变更

- 暂时下线 STRM HTTP 代理、生成/审计 API 和 WebUI 配置；保留旧数据字段，后续以独立 Rust 模块重新接入。
- 新增 Rust 原生 `PostTransferModule` 转存后处理模块注册表。
- Telegram Bot 新增 `/subscription <ID>` 与 `/job <ID>` 详情查询。
- 首屏请求并发化，Rust 服务端启用 Brotli/Gzip 压缩和静态资源缓存。

### 兼容性

- JSON Store schema 未变化。
- v2.1.2 的 `strm_*` 字段可以继续读取，但 v2.2.0 不会执行 Strm 生成或提供 Strm HTTP 代理。

## 2.1.2

v2.1.2 是一次针对 Aria2 WebUI 轮询开销的性能修复版本。

### WebUI 性能

- 仅在存在活动或排队中的 Aria2 任务时进行 2 秒高频轮询。
- 进入工作台或下载页、以及提交/暂停/继续/停止任务后仍会立即刷新一次。
- 活动任务归零后自动停止前台轮询，避免空闲页面持续请求远程 Aria2。
- 提升 PWA 静态缓存版本，确保已安装客户端及时获取新的轮询逻辑。

### 兼容性

- 后台下载监控逻辑保持不变，仍负责完成状态、订阅进度和异常恢复。
- 存储 `schema_version` 与 OpenAPI 契约保持不变，可从 v2.1.1 直接升级。

## 2.1.1

v2.1.1 是一次聚焦转存、下载状态和后台任务可靠性的补丁版本。它修复了文件落错目录、通知清理后订阅无法完结、换季继承旧进度、并发导入重复执行以及停机时任务脱离调度器继续运行等问题。

### 转存安全

- 创建或查找非根目标目录失败时，本次转存直接失败，不再回退到网盘根目录。
- 失败路径不会调用云盘转存接口，也不会写入已转存文件状态，后续任务可以正常重试。

### Aria2 下载状态

- 在订阅中持久化 Aria2 GID、文件名、下载目录、目标目录、提交时间和完成时间。
- 下载完成的业务状态先于展示通知写入；即使通知已经存在，仍会补偿此前失败的订阅状态更新。
- 处理失败时释放内存去重键，下一轮监控可以重试，不会因一次瞬时错误永久跳过。
- 订阅详情与下载自动化上下文优先读取持久记录，通知清理或压缩后仍能展示排队和完成状态。
- 保留通知历史解析作为旧数据兼容路径；升级前已提交的任务无需重新创建。

### 订阅更新与导入

- 季数或媒体类型变化时清理旧的当前集数、总集数、已知文件、转存记录、同步下载记录、检查历史和完结状态，再按新季元数据或规则计算总集数。
- URL/密码换源仍保留原有的“继承进度”选项，换季重置与普通换源语义分离。
- 订阅导入在同一个幂等临界区内完成 Key 检查、批量写入和结果保存；相同 `Idempotency-Key` 的并发请求只执行一次。

### 任务关闭可靠性

- 外层调度包装器被取消时，会同步 abort 内层业务任务，避免 Tokio `JoinHandle` 被丢弃后 detach。
- 关闭宽限期超时会主动中止全部已注册任务；等待 Worker 本身超时也会 abort Worker。
- 进度和完成写入只接受 `Running` 状态，已失败、已取消或已成功任务不会被迟到回调重新激活。

### 兼容性

- `schema_version` 仍为 1；新增同步下载记录使用 serde 默认值，旧订阅文件可直接读取，无需人工迁移。
- OpenAPI 路径与操作数量保持 91 / 103，现有 API 调用方式不变。
- 可从 v2.1.0 直接升级。回滚到 v2.1.0 时旧程序会忽略新增字段，但再次写入订阅可能丢弃持久下载记录，因此回滚前仍建议备份 `DATA_DIR`。

## 2.1.0

v2.1.0 是一次以 WebUI 视觉重设计为主的版本。它把界面的设计语言从原来的靛蓝／青绿科技风，改为「Cinema Slate（影院石板）」——中性炭黑底配琥珀金强调（深色），暖纸白配深琥珀（浅色）。本版本不涉及后端逻辑、存储格式或 API 变更，是纯前端外观升级，可从 v2.0.0 无缝升级。

### WebUI 重设计（Cinema Slate）

- 深色主题改为中性炭黑基底 + 琥珀金强调色，浅色主题改为暖纸白 + 深琥珀，整体更贴合「媒体追更」的产品气质。
- 卡片扁平化：移除多层渐变光晕、网格纹理和厚投影，改为发丝线边框 + 克制阴影，信息密度更高、长时间浏览更安静。
- 侧边栏激活项改为琥珀左条 + 淡金底；工作台 hero 精简，主按钮改为实色琥珀配深色文字，对比度更高。
- 圆角统一为两档（卡片 12px、控件 8px），基准字号从 17px 调整为 16px，标题字重提升。
- 深浅双主题 token 完整重做，海报货架继续作为工作台主视觉。

### 兼容性

- 纯 CSS token 与组件样式重写，未改动任何 Alpine 绑定或 DOM 结构，功能与交互保持不变。
- 后端、存储 `schema_version: 1`、OpenAPI 契约（91 条路径、103 个操作）均无变化。
- 可从 v2.0.0 直接升级；PWA 缓存版本已更新，客户端会自动拉取新样式。升级仍需二进制与完整 `static/` 配套替换。

## 2.0.0

v2.0.0 是一次以安全加固、后台可靠性和前端工程化为主线的大版本。它修复了一批经安全复审确认的认证与并发缺陷，重写了任务生命周期与优雅停机路径，并把单文件 WebUI 拆分为可维护的模板与分片。本版本不修改 Store `schema_version: 1`，可从 v1.13.x 直接升级，但因涉及认证与部署默认值变更，升级前请阅读对应升级指南。

### 安全

- 登录不再接受默认密码 `change-me`：未通过 `APP_PASSWORD`/`SERVER_PASSWORD` 或系统设置设置过密码时，Basic 认证直接拒绝并在日志中提示运维设置密码，避免公网暴露实例被默认口令登录。
- 登录限流不再信任客户端可伪造的 `X-Forwarded-For`：默认按连接对端 IP 计数，仅在新增设置 `trust_proxy_headers` 显式开启（部署于可信反向代理之后）时才采用 XFF 首段。
- 失败计数表达到上限时改为淘汰最旧记录，杜绝被伪造 Key 灌满后静默停止记录、绕过锁定的问题；失败的自动化 Token 认证也纳入限流。
- 自动化 Token 的读权限改为显式路径白名单，未列出的 `/api` 路径对 Token 默认拒绝，新增只读端点不再被自动纳入。
- 设置公开视图中的密钥掩码改为固定长度，不再泄漏每个密钥的精确长度。

### 后台任务与可靠性

- 任务存储裁剪只淘汰终态任务（成功/失败/取消），排队中与运行中的任务永不被裁剪，必要时允许暂时超过上限。
- 新增 SIGTERM/Ctrl+C 优雅停机：停止认领新任务，给运行中任务有限宽限期到达持久化点，超时中止后把残留运行任务收敛为可手动重试的中断态并落盘。
- 卡死看门狗改为基于心跳判定：仅在任务在阈值内没有任何进度更新时才判定卡死，缓慢但持续汇报进度的批量转存不再被误杀重试。
- 取消运行中任务现在真正中止其执行并立即释放并发槽，不再出现界面显示“已取消”而任务仍占用互斥槽的情况。

### 订阅检查与转存

- 批量检查回写改为按字段合并进当前记录，不再用内存快照整体覆盖：并发写入的已转存状态与用户在检查期间的编辑不再被回滚，消除重复转存与静默回退。
- 批量回写跳过在检查期间被删除的订阅并继续持久化其余结果，单个删除不再导致整批检查结果丢失、重复推送。
- 转存成功后立即持久化已转存状态，其后的重命名或列目录失败只记录告警并可重试，不再因瞬时错误导致下个周期重复转存。

### 通知与推送

- 摘要推送新增重启恢复：进程重启后扫描待发送的 `digest_pending` 通知并重新排期；同一窗口只保留单个定时器。
- 浏览器推送对单个订阅的错误只记录并跳过，不再中断整批；识别 404/410 失效端点并从存储中清理。
- Telegram Bot 以受限并发方式处理更新，慢命令（如 `/check all`）不再阻塞 `getUpdates` 长轮询。
- 安静时段改用主机时区（可经 Docker `TZ` 设定），不再硬编码 UTC+8；若干长期只增集合加入上限淘汰。

### 部署

- 容器改为非 root 用户运行，并说明绑定挂载的属主处理。
- `docker-compose.yml` 通过 `${SERVER_PASSWORD:?}` 引用 `.env`，不再在文件内内联明文口令。

### WebUI 工程化

- 3027 行的 `static/index.html` 拆分为 `static/partials/` 下的 17 个分片，由零依赖的 `scripts/build-frontend.mjs` 从 `static/index.tmpl.html` 组装，构建产物逐字节稳定；WebUI 行为与外观保持不变。

### 兼容性

- `schema_version` 保持 1，不创建 SQLite，不改变现有 JSON Store 格式。
- OpenAPI 保持 91 条路径、103 个操作。
- 可从 v1.13.x 直接升级；二进制与完整 `static/` 必须配套替换。升级后如为公网直连实例，请确认已设置强密码，并按需开启 `trust_proxy_headers`。

## 1.13.1

v1.13.1 是 v1.13.0 的可靠性补丁版本，修复订阅完结归类、剧集缩略图和 Alpine 动态列表问题，并把工作台与架构文档收敛到当前运行状态。本版本不修改 Store schema、自动化 API 或单实例安全模型。

### 修复

- 后端持久化的 `completed/status` 保持为订阅列表的权威状态，前端不再因 `current_episode_number` 暂时滞后而把已完结订阅显示为追更中。
- 元数据、总集数和规则更新后重新协调完结状态：仅通知订阅使用发现证据，普通订阅使用转存证据，同步下载订阅仍等待 DownloadMonitor。
- 同一 TMDB 条目刷新遇到部分季请求失败时，保留已有媒体海报、季海报、剧集 still 和缺失季/剧集数据；更换条目时不合并旧图片。
- 外部图片加载失败后允许新 URL 或后续重试恢复显示。
- Aria2 跨状态快照按 GID 去重；日历、搜索、下载、订阅、任务、通知和诊断等高频列表使用防碰撞 render key，避免 Alpine DOM 移动锚点丢失。

### WebUI 与文档

- 工作台改为运行概览，直接展示失效订阅、失败任务、未读通知、运行任务和处理入口。
- 删除“你的媒体库，正在自动生长”等广告式文案及无操作价值的装饰进度。
- `architecture.md` 与 Graphviz PNG/SVG 重写为当前 v1.13.1 架构，补齐自动化 Token、Telegram、PWA、事件投影、备份与生命周期边界。
- README、API 契约、日历文档和 PWA cache version 已同步。

### 兼容性

- `schema_version` 保持 1，不创建 SQLite，不改变现有 JSON Store 格式。
- OpenAPI 保持 91 条路径、103 个操作，与 v1.12.0 稳定基线兼容。
- 可从 v1.13.0 直接升级；二进制与完整 `static/` 必须配套替换。

## 1.13.0

v1.13.0 完成 P20–P21，在 v1.12.0 的诊断与数据生命周期基线上增加稳定自动化 API 和 Telegram 主动控制机器人。本版本继续服务单实例、单管理员场景，保持 `schema_version: 1` 和 JSON 单写，不引入多用户、任意远程 Shell 或 SQLite 双写。

### API 与自动化集成

- OpenAPI 与 Axum literal route 自动双向核对，当前登记 91 条路径、103 个操作。
- `docs/openapi-baseline-v1.12.0.json` 继续作为兼容基线，删除稳定路径/方法或修改 Success/Error 信封会阻断 CI 和 Release。
- 单实例自动化 Token 只保存 SHA-256 哈希，支持轮换、撤销、过期、最后使用时间和最小 scope。
- Token scope 覆盖订阅读取/写入/检查、Job 读取/写入、通知读取/写入、诊断读取和 `quark:signin`；设置、备份恢复、清理和升级接口不向 Token 开放。
- 订阅导出使用版本化信封；导入支持冲突预览、skip/new_id 策略、确认短语、原子批量 Store 写入和 Idempotency-Key 重放保护。
- 推送 Webhook 使用版本化事件信封，包含 event/request/correlation/subscription/job 上下文，并保留 HMAC 双签名轮换能力。

### Telegram 安全接入与只读控制

- 支持 `disabled`、long polling 和 webhook 三种模式，单实例同一时间只启用一种接入。
- Long polling 自动删除旧 Webhook，使用最长 25 秒 `getUpdates` 并在失败时 2–60 秒退避。
- Webhook 使用随机 URL 路径和 `X-Telegram-Bot-Api-Secret-Token` 双重常量时间校验；两个 Secret 首次创建设置时自动生成并按密钥脱敏。
- 授权只使用 Telegram 数字 user ID 和 chat ID，不信任 username；默认仅允许 private chat。
- 提供 `/start`、`/help`、`/status`、`/subscriptions`、`/calendar`、`/jobs`、`/notifications`、`/diagnostics`，列表分页且消息按 Unicode 边界限制长度。
- 诊断 API 和 WebUI 展示接入模式、运行状态、最近 Update/成功、脱敏错误、审计、待确认、去重和限流统计。

### 受控写命令与交互确认

- 增加 `/check <订阅ID|all>`、`/retry <Job ID>`、`/cancel <Job ID>`、`/signin`、`/read <通知ID|all>`。
- 每个写动作映射到 P20 最小 scope，并复用 `SubscriptionCheckService`、`JobQueue`、`QuarkSigninService` 和 `NotificationStore`，不复制业务规则。
- 所有写命令使用 120 秒一次性 Inline Keyboard 确认，绑定 user/chat/action/resource/scope；跨用户、跨 chat、过期、重启后和重复确认均拒绝。
- `update_id`、Callback Query ID 和业务 Idempotency-Key 三层持久化去重；结果返回 request/correlation，Job 操作同时返回 Job ID。
- 明确不开放删除订阅、恢复备份、清理 Store、直接 Store 编辑、任意路径读取或任意命令执行。

### 主动通知、审计与限流

- Telegram 通知可附加 HMAC-SHA256 签名的查看详情、标记已读和重新检查按钮；签名绑定动作、资源、有效期、user/chat，Callback 数据不超过 64 字节。
- 追更、失效、完结、转存、下载和队列积压继续经过事件开关、渠道路由、最低级别、上海时区安静时段、错误绕过、重复限频和摘要策略。
- 新增私有权限 `telegram_bot.json`，持久化最近 2,000 条 Update、Callback、业务幂等键和脱敏命令审计，并纳入完整 DATA_DIR 备份/恢复校验。
- `GET /api/telegram/audits` 通过 Basic Auth 或 `diagnostics:read` Token 返回有界审计列表。
- 命令按 user、chat、command 三层限流；连续三次写动作失败后冷却 60 秒，状态不与普通 Telegram 推送共享。
- 错误和审计清理 Bot Token、Webhook Secret、Cookie、Token、Password、Key、Authorization/Bearer，并限制输出长度。

### 测试、文档与运维

- 无网络测试覆盖 Webhook 双 Secret、白名单、伪造 username、重复 Update/Callback、重启恢复、过期/跨用户 Callback、并发确认、HMAC 按钮、429/5xx 和核心流水线隔离。
- 增加 `scripts/smoke-telegram.sh`；CI 未配置 Secret 时安全跳过，配置沙箱 Token 后执行真实 `getMe`，发送测试消息仍需显式开启。
- 增加 BotFather、long polling、Webhook 反向代理、命令、Token 吊销和应急停用完整指南。
- 426 个 Rust 测试登记，425 个通过、1 个真实 PanSou 网络测试按设计忽略；14 个前端 Node 测试通过。

### 兼容性与升级

- `schema_version` 保持 1；Settings 新字段均有默认值，旧实例数据可直接加载。
- 首次启动会创建 `telegram_bot.json` 并生成 Webhook Secret；控制模式默认 `disabled`，不会因升级自动接收命令。
- v1.12.0 自动化 API 基线保持兼容，新增路由和 `quark:signin` scope 为向后兼容扩展。
- 必须同时替换二进制和完整 `static/`；不要让不同版本同时写入同一个 DATA_DIR。

## 1.12.0

v1.12.0 完成 P17–P19，重点提升移动端 WebUI、可观测性、故障排查、备份可恢复性和 JSON Store 生命周期治理。本版本继续面向单用户夸克自动化，保持 `schema_version: 1`，不引入第二网盘、多用户或 SQLite 双写。

### WebUI 与移动端

- 移动端订阅详情使用安全区、粘性导航和操作栏、44px 触控目标及单列弹窗。
- 订阅支持当前窗口全选、受限并发检查和短语确认批量删除；列表筛选与视图偏好持久化。
- 订阅、Job、通知和网盘大列表使用分段可见窗口与加载更多，避免一次渲染全部记录。
- 统一 loading、空状态、错误边界、危险确认、键盘焦点与诊断复制体验。
- 自动化详情提供有界时间线和原始诊断 JSON；真实浏览器持续验证 390×844 与 1440×1000。

### 可观测性与故障诊断

- HTTP、订阅检查和后台 Job 串联 `request_id`、`correlation_id`、`subscription_id` 与 `job_id`。
- Job 持久化关联上下文并保持旧数据兼容；自动换源、转存和推送沿用同一 correlation。
- `LOG_FORMAT=json` 输出结构化 JSON；`GET|PUT /api/observability/log-filter` 可热更新 EnvFilter。
- `GET /metrics` 提供受认证的 Prometheus 0.0.4 指标；JSON 指标继续由 `/api/metrics` 提供。
- 所有外部 HTTP 请求记录服务级次数、非 2xx/传输失败、累计和最大延迟；HTTP、订阅、转存及 Job 支持慢操作告警。
- 诊断快照增加 DATA_DIR 容量/权限、上海时区偏移、已配置服务 DNS、五类 Store 一致性和只读处理建议。

### 备份与数据生命周期

- 备份预览返回格式、Schema、安全路径、Base64、大小、SHA-256、业务模型和 settings 完整性清单。
- 每次服务器备份在隔离目录完整恢复并逐文件复核；成功和失败报告持久化，可定期或手动重验。
- `BACKUP_EXTERNAL_DIR` 支持将已验证备份以 0600 权限原子复制到 DATA_DIR 外部，并独立清理历史副本。
- `/api/storage/cleanup` 提供 Store 记录数、独立保留上限、预计处理量、文件大小和增长预警的只读预览。
- 清理要求 `CLEANUP DATA`，在变更前创建并验证 `pre-cleanup` 备份，再原子应用订阅、通知、Job 和自动化事件保留策略。
- SQLite 决策门继续使用 500 订阅、10,000 历史记录、32 MiB Store 或复杂查询需求；达到门槛也只进入决策，不自动建库或长期双写。

### API 与运维

- 新增 `/metrics`、`/api/observability/log-filter`、`/api/backups/verification`、`/api/storage/cleanup` 和 `/api/storage/decision`。
- OpenAPI、README、环境变量示例、架构与 API 合同同步更新。
- Linux x86_64 发布包包含二进制、完整 `static/`、文档、README 和本变更记录。
- GHCR 发布 `1.12.0`、`1.12`，同时维护 `latest` 和 main/sha 开发标签。

### 兼容性

- `schema_version` 仍为 1，无需离线数据迁移。
- Job 新关联字段均有默认值，旧 Job 可直接读取。
- 新环境变量均有安全默认值；未配置外部备份目录时保持原行为。
- 不要让不同版本同时写入同一个 DATA_DIR；升级与回滚必须同时替换二进制和完整 `static/`。

## 1.11.0

v1.11.0 完成 P15–P16，聚焦单用户夸克自动化的任务队列可靠性与通知策略中心。本版本继续保持 `schema_version: 1`，不增加第二网盘或多用户系统。

### 任务队列与调度

- Job 增加 high、normal、low 优先级，使用 3:2:1 加权公平调度并按订阅轮转。
- 增加全局、transfer/metadata/push 类别及同订阅互斥三层并发限制。
- 增加错误分类、最多三次指数退避与抖动、分层熔断及半开恢复探测。
- 增加 30 分钟卡死检测、维护模式、100 条积压告警和诊断状态。
- 旧终态 Job 自动归档到 `jobs.archive.json`，支持分页查询和完整备份校验。

### 通知中心

- 增加事件到渠道路由、最低级别、上海时区安静时段和错误绕过。
- 增加重复限频、延迟摘要及 title/message/event/level 模板预览。
- 增加渠道策略诊断和真实渠道测试接口。
- Webhook 每个目标独立重试，并支持当前/上一密钥双签名重叠轮换。
- 推送策略、入队和发送均脱离核心自动化调用栈，失败不会阻塞订阅、转存、下载监控或签到。

### 兼容性

- `jobs.json` 新字段均有默认值，旧 Job 默认 normal、attempt=1。
- Settings 新字段均有兼容默认值，无需离线迁移。
- 二进制和完整 `static/` 必须一起升级。

## 1.10.0

v1.10.0 聚焦单用户夸克自动化，完成 P11–P14，不增加第二网盘或多用户系统。本版本保持 `schema_version: 1`。

### 稳定性与升级

- Release 二进制、Docker 数据卷、v1.3.0 跨版本升级、备份恢复、PWA 与真实 Chrome 烟雾测试。
- 增加持续健康和诊断请求、备份保留上限及损坏文件检测。

### 追更与日历

- 日历项目增加剧照、季度海报和媒体海报缩略图回退。
- 建立真实文件名语料和可解释集数识别。
- 支持多集范围、中文合集、SP、OVA、OAD 和订阅级 `episode_regex`。
- 转存预览增加完整集数、缺集和重复集诊断。

### 搜索与安全换源

- 增加订阅级搜索词、排除词和分辨率/字幕/编码/发布组偏好。
- 增加稳定去重排序、PanSou 临时错误退避和候选探测缓存。
- 换源预览增加候选集数和当前进度前缺集。
- 保留建议/自动模式、失败审计、冷却、历史阻断和回滚。

### 转存与媒体库

- 增加 skip、overwrite、keep_both 目标冲突策略和跨平台安全文件名。
- 增加 STRM 缺失、孤立、无效内容和重复目标审计。
- Aria2 提交前查重，失败后重新查重并指数退避，降低重复任务风险。
- 转存后支持 Jellyfin、Emby、Plex 和通用 Webhook 自动刷新。

### 升级注意事项

- 二进制和 `static/` 必须一起升级。
- 新增设置字段均有兼容默认值，无需离线迁移。
- 升级前应备份完整 `DATA_DIR`。

## 1.9.0

v1.9.0 汇总并发布 P0–P10 路线图成果，重点补齐云盘 Provider 抽象、备份恢复、诊断与安全、PWA、推送、存储性能治理和 API 文档。本版本保持 `schema_version: 1`。

### 新增

- CloudDriveProvider 能力抽象、Quark 适配器和测试用 Mock Provider。
- 完整数据备份下载、定时备份、保留与容量限制、恢复预览和安全恢复。
- 系统诊断页、脱敏诊断包、请求关联 ID、Store 大小及读写耗时指标。
- 可安装 PWA、离线壳层、安全缓存更新和常用快捷入口。
- 标准 VAPID Browser Push、签名 Webhook 与统一推送报告。
- OpenAPI 3.1 规范及内置 API 文档浏览页。
- JSON 大规模数据性能基线、历史清理和内存索引。

### 改进

- 订阅检查、转存、手动转存和网盘操作经 Provider 边界解耦。
- 登录失败限流、密码风险提示、CSP、安全响应头和全链路敏感信息脱敏。
- 自动化事件、通知、任务和订阅 Store 增加容量治理及指标。
- 完善反向代理、PWA、存储扩展与架构文档。

### 兼容性

- JSON 数据继续使用 `schema_version: 1`，无需离线迁移。
- 新设置字段均有兼容默认值；首次启动后保存配置时会自然写入。
- PWA 和前端接口有同步变更，必须同时升级二进制与整个 `static/` 目录。
- 回滚前应使用内置备份或完整复制 `DATA_DIR`。

### 发布产物

- GitHub Actions 构建 Linux x86_64 二进制归档及 SHA256。
- GHCR 构建并发布 `v1.9.0`、`1.9.0`、`1.9` 和 `latest` Docker 镜像标签。

## 1.3.0

v1.3.0 新增媒体更新日历，将订阅元数据、手动排期、已发现/已转存记录、STRM 与 Aria2 状态聚合为统一的周、月和列表视图。本版本保持 `schema_version: 1`，新增订阅字段均提供兼容默认值。

### 新增

- Media Deck 侧边栏新增“更新日历”页面。
- 提供周视图、月视图和紧凑列表，并支持前后周期导航与返回今天。
- 支持按状态和媒体类型筛选；日历 API 还支持按订阅 ID 和日期范围筛选。
- 日历项可直接进入订阅详情、立即检查，或执行“检查并补集”。
- 订阅编辑器新增手动排期：开播日期、播出星期、上海时间、周期周数、首集编号和总集数。
- 新增 `docs/media-calendar.md`，固化时间口径、状态定义、排期优先级和数据合并规则。

### 后端与 API

- 新增 `src/models/calendar.rs`，定义日历项、摘要、状态、排期来源、可信度和快捷操作。
- 新增纯计算服务 `src/services/media_calendar.rs`，不写回订阅或原始元数据。
- 新增 `GET /api/calendar`，返回标准 `{ok:true,data:...}` 信封。
- `GET /api/calendar` 支持 `from`、`to`、`status`、`media_type` 和 `subscription` 查询参数。
- 默认查询上海时区当前自然周；日期范围为闭区间，最多 367 个自然日。
- 订阅创建和更新接口支持可选 `manual_schedule`；更新时 JSON `null` 表示显式清除，缺少字段表示保持不变。

### 排期与状态规则

- 日历业务时间统一按 `Asia/Shanghai` 解释，自然周固定为周一至周日。
- `check_weekdays` 继续只控制订阅检查任务，不参与媒体播出日推导。
- 排期优先级为：手动排期、逐集元数据、下一集元数据、发布日期、稳定周期推断、排期未知。
- 手动排期覆盖日历展示结果，但不修改或删除原始 `MediaMetadata`。
- 状态聚合覆盖今日、本周、已播未发现、已发现待转存、已转存待下载、完结缺集、已就绪、已排期和排期未知。
- known/transferred/STRM/Aria2 判定复用订阅详情聚合结果，避免详情页与日历产生两套状态口径。

### 兼容与迁移

- 持久化格式仍为 `schema_version: 1`，从 v1.2.0 升级不需要额外数据迁移。
- 历史订阅没有 `manual_schedule` 时按 `None` 读取，继续从元数据或稳定周期推导排期。
- 关闭手动排期会向更新接口发送 `manual_schedule: null`，只清除覆盖，不影响媒体元数据。
- 回滚到 v1.2.0 时旧程序可以读取 schema v1 数据，但在后续重写订阅文件时可能丢弃 v1.3.0 新增的手动排期字段；需要完整保留时应恢复升级前备份。

### 测试和发布

- 24 个前端单元测试，覆盖日历日期范围、月/周布局、分组、筛选标签和导航辅助逻辑。
- 295 个 Rust 测试通过，1 个真实网络测试按设计忽略。
- 日历服务测试覆盖跨时区、跨周、同日多集、未来集数、元数据缺失、手动覆盖和原始元数据不变。
- HTTP 集成测试覆盖日历默认查询、筛选、错误日期和超范围参数。
- Release 工作流校验 tag/Cargo 版本、README 版本正文、CHANGELOG 和升级文档，并归档完整发布文档。

### 升级注意事项

- 二进制与 `static/` 必须同时升级，旧静态资源不包含日历页面和手动排期编辑器。
- 使用反向代理缓存静态资源时，升级后应清理缓存并强制刷新浏览器。
- 外部客户端应把 `manual_schedule` 缺失与 `null` 区分：缺失表示不修改，`null` 表示清除。
- 完整升级、验证和回滚步骤见 `docs/upgrade-v1.3.0.md`。

## 1.2.0

v1.2.0 将 WebUI 升级为 Media Deck，并完成 API、数据存储和订阅状态体系的第一轮结构化收口。由于 JSON API 响应和持久化文件格式发生变化，本版本按次版本发布。

### 新增

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

### API 变化

- JSON 成功响应统一为 `{"ok":true,"data":...}`。
- 应用错误统一为 `{"ok":false,"error":"...","message":"..."}`。
- Basic Auth、CSRF、已知/未知 404、405 和请求解析错误统一返回 JSON。
- 401 保留 `WWW-Authenticate`，405 保留 `Allow` 等有效响应头。
- WebUI `apiData()` 兼容当前信封、旧 `{data:...}` 信封和历史裸响应。
- `/health`、STRM、Job SSE 和成功的 204 操作为登记例外。

### 数据安全与兼容

- `settings.json`、`subscriptions.json`、`notifications.json` 和 `jobs.json` 使用 `schema_version: 1` 信封。
- 旧裸 JSON 首次加载时自动迁移。
- 迁移前创建一次性 `*.schema-v0.bak` 原始备份，不覆盖已有备份。
- 业务数据和迁移备份在 Unix 上自动修复为 `0600`。
- 未来 schema 会拒绝读取，但不会被当作损坏文件隔离或覆盖。
- 真正损坏的 JSON 继续隔离为 `.corrupt-<timestamp>`。
- Store 写盘成功后才更新内存，失败不会产生运行态和磁盘状态分叉。
- 删除从未启用、也从未公开的 `nas_sync_*` 占位字段；旧 JSON 字段可安全忽略。

### 行为优化

- 未配置 Aria2 时停止下载任务轮询，避免全新安装持续产生 400 请求。
- 下载、签到、换源、转存和推送页面适配统一 API 响应。
- 搜索结果和网盘时间统一使用共享格式化工具。
- API 未知路由不再落入静态资源 404。

### 测试和发布

- 19 个前端单元测试。
- 284 个 Rust 测试通过，1 个真实网络测试按设计忽略。
- HTTP 集成测试覆盖成功信封、验证错误、401、403、404、405、malformed JSON、204、SSE、STRM 和静态资源。
- Release 工作流执行前端检查、rustfmt、clippy、完整测试、tag/version 校验和 Release 构建。
- Release 归档包含二进制、静态资源、README、CHANGELOG 和 docs。

### 升级注意事项

- 外部 API 脚本需要从 `.data` 读取业务对象。
- v1.1.x 二进制不能直接读取 schema v1 数据文件。
- 回滚时必须同时恢复旧二进制、旧静态资源和 `.schema-v0.bak` 或完整 DATA_DIR 备份。
- 完整步骤见 `docs/upgrade-v1.2.0.md`。

## 1.1.0

发布日期：2026-07-04

### 🎉 新功能

#### 订阅失效自动换源
当订阅的夸克分享链接失效时，系统会自动通过 PanSou 搜索相同资源的替代链接，并通知用户选择换源。

**功能特性**：
- ✅ 自动检测链接失效
- ✅ 智能搜索替代源（使用原标题 + 季度信息）
- ✅ 24小时内不重复搜索（避免资源浪费）
- ✅ 多渠道通知（企业微信/Telegram/Bark等）
- ✅ WebUI 换源界面
  - 查看候选列表
  - 探测链接详情
  - 一键应用换源
- ✅ 保留历史链接（可回滚）
- ✅ 换源后自动重置状态

**API 端点**：
- `GET /api/subscriptions/:id/source-candidates` - 获取换源候选列表
- `POST /api/subscriptions/:id/source-candidates/probe` - 探测候选详情
- `POST /api/subscriptions/:id/source-candidates/apply` - 应用换源
- `POST /api/subscriptions/:id/source-candidates/search` - 手动触发搜索

**数据模型变更**：
- 订阅新增 `source_candidates` 字段 - 换源候选列表
- 订阅新增 `last_source_search_time` 字段 - 上次搜索时间
- 订阅新增 `previous_share_links` 字段 - 历史链接

---

### 🐛 Bug 修复

#### 修复同集重复下载问题 ⭐ 重要
**问题描述**：同一个集数会下载多个不同文件（例如：181.mp4 和 181 4K.mp4）

**根本原因**：
- 去重逻辑只在本次发现的新文件之间比较
- 不会和已转存的文件（`transferred_files`）进行比较
- 导致后续发现的更高质量版本会被重复转存

**修复方案**：
在 `find_new_files` 函数中：
1. 收集所有已知集数（`known_episodes` + 从 `transferred_files` 提取的集数）
2. 在过滤新文件时，直接跳过已知集数
3. 确保同一集数只会转存一次

**影响**：
- ✅ 彻底防止同集重复下载
- ✅ 节省网盘空间和下载带宽
- ✅ 避免重复通知

**代码位置**：
- `src/services/subscription_check/file_filter_methods.rs`

---

### 📝 详细变更

#### 新增文件
1. `src/services/subscription_source_switch.rs` - 换源服务
2. `src/api/subscription_source.rs` - 换源 API

#### 修改文件
1. `src/models/subscription.rs` - 新增换源相关字段
2. `src/services/subscription_check.rs` - 集成自动搜索换源
3. `src/services/subscription_check/file_filter_methods.rs` - 修复去重逻辑
4. `src/services/mod.rs` - 导出新服务
5. `src/api/mod.rs` - 注册新路由
6. `src/api/subscriptions.rs` - 修复结构初始化
7. `Cargo.toml` - 版本号 1.0.5 → 1.1.0

#### 代码统计
- **新增代码**：约 500 行
- **修改代码**：约 150 行
- **总计**：约 650 行改动

---

### 🔄 升级指南

#### Docker 用户

```bash
# 停止旧容器
docker stop my-media-sub

# 拉取新版本
docker pull ghcr.io/hellomrli/my-media-sub:v1.1.0

# 启动新容器
docker start my-media-sub

# 或使用 docker-compose
docker-compose pull
docker-compose up -d
```

#### 二进制用户

```bash
# 下载新版本
VERSION=v1.1.0
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"

# 解压并替换
tar -xzf my-media-sub-${VERSION}-linux-x86_64.tar.gz
# 替换旧的二进制文件

# 重启服务
systemctl restart my-media-sub
```

#### 数据兼容性

✅ **完全向后兼容** - 无需迁移数据，新版本会自动添加新字段

现有订阅数据会自动补充以下字段：
- `source_candidates: []`
- `last_source_search_time: null`
- `previous_share_links: []`

---

### 🧪 测试建议

升级后建议测试以下功能：

#### 1. 测试去重修复
- 创建订阅并检查一次
- 等待分享者上传同集数的不同版本
- 再次检查，验证不会重复转存

#### 2. 测试换源功能
- 创建一个失效链接的订阅
- 手动检查，触发自动搜索
- 在 WebUI 中查看候选列表
- 探测并应用换源

#### 3. 回归测试
- 正常订阅检查和转存
- TMDB 元数据刮削
- Aria2 下载
- 通知推送

---

### ⚠️ 已知限制

#### 换源功能
- 目前只支持 PanSou 搜索
- 限制返回 10 个候选
- 24小时内不重复搜索

#### 去重修复
- 只防止**新的**重复下载
- **已经重复下载的文件不会自动清理**
- 需要手动删除历史重复文件

---

### 🔮 下一步计划

- [ ] WebUI 换源界面实现
- [ ] Telegram Bot 换源交互
- [ ] 多源聚合搜索
- [ ] 换源历史记录
- [ ] 自动清理重复文件

---

### 💡 使用提示

#### 如何手动触发换源搜索

如果某个订阅已经失效，但还没有自动搜索候选：

```bash
curl -X POST http://localhost:56001/api/subscriptions/{订阅ID}/source-candidates/search \
  -u admin:your-password
```

#### 如何查看换源候选

访问订阅详情页，如果有候选会显示在页面上（需要实现前端界面）。

或通过 API：
```bash
curl http://localhost:56001/api/subscriptions/{订阅ID}/source-candidates \
  -u admin:your-password
```

#### 如何清理重复文件

对于已经重复下载的文件，可以通过以下方式清理：

1. 在夸克网盘中手动删除重复文件
2. 或通过 WebUI 的网盘管理界面删除

---

### 📊 性能影响

- **内存**：基本无影响（新增字段很小）
- **CPU**：去重逻辑略微增加，但可忽略
- **网络**：失效时会调用 PanSou API（一天最多一次）
- **存储**：每个订阅增加约 1-2KB（候选列表）

---

### 🙏 致谢

感谢所有反馈问题和建议的用户！

---

**完整更新内容请查看**：
- GitHub Release: https://github.com/hellomrli/my-media-sub/releases/tag/v1.1.0
- 提交历史: https://github.com/hellomrli/my-media-sub/compare/v1.0.5...v1.1.0

