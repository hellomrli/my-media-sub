# my-media-sub 项目评审与更新建议（2026-09-21）

> **修复状态（2026-09-21 更新）**：v2.7.2 处理了本文件的大部分条目，详见
> [CHANGELOG.md](../CHANGELOG.md) 的 `## 2.7.2` 段与
> [docs/upgrade-v2.7.2.md](upgrade-v2.7.2.md)。逐条状态如下（未处理项已登记为
> [docs/roadmap.md](roadmap.md) 的 `P23-11`…`P23-18`）：
>
> | 条目 | 状态 |
> |---|---|
> | P0-1 调度器 reload 报错 | ✅ 已修复，含「上游 `start()` 不幂等」根因回归测试 |
> | P0-2 CSS 产物过期 + 9 个未定义类 | ✅ 已修复（用 v3.4.17 逐字节复现后重建） |
> | P0-3 CI 缺 CSS 门禁 | ✅ 新增 `scripts/check-css-classes.py` 并接入 CI（已用修复前产物验证会失败） |
> | P0-3 roadmap 指针漂移 | ✅ 已修复，并新增 `scripts/check-doc-drift.py` 门禁 |
> | P1-1 维护模式队列无界增长 | ✅ 已在 `auto_transfer_disabled_reason` 拦入队（force 放行） |
> | P1-2 转存重复 | ✅ 已修复：新增 `pending_transfers` 意图记录（调用云端前落盘），重试时以「有意图」+「目标目录有同名文件」双条件判定，三条回归测试守住两个方向的边界 |
> | P1-3 sync_download 无对账 | ✅ 已修复：新增 `pending_downloads` + 检查入口周期对账重试（存 fid 而非过期直链），三条回归测试 |
> | P1-4 备份阻塞运行时 | ✅ 已抽出 `BackupFsWorker`，整段移入 `spawn_blocking` |
> | P1-5 aria2 目录浏览阻塞 | ✅ 已移入 `spawn_blocking` 并去掉逐条 `canonicalize` |
> | P1-6 限流拦下正确凭据 | ✅ 已修复（限流只惩罚凭据错误的请求，阈值语义不变） |
> | P1-7 scope `read` 通配符 | ✅ 已改为精确匹配（**破坏性**：见升级指南） |
> | P1-8 状态一致性批次 | ✅ 大部分：单实例锁、token 信封、信号丢失兜底 + worker 周期补扫、取消写入 abort 安全（7 个 Store 统一「先内存后落盘 + 失败回滚」）、溢出防护与守卫点已完成；隔离策略（默认中止 + 逃生舱 + 隔离文件名防覆盖）与 schema 版本策略（v2 降级保护）已完成；未知字段透传经评估决定不做（与降级保护方向相反，理由见 roadmap P23-15） |
> | P1-9 bfcache | ✅ 已修复（persisted 时不再 destroy，pageshow 重建 SSE 并重跑取数） |
> | P2-1 Cargo.lock 落后 132 个 crate | ✅ 已 `cargo update`，现为 0 |
> | P2-2 移除 `web-push` | ✅ 已完成：自实现（`ece` + `p256`/`ring`），依赖 370→227，RustSec 例外已消除；6 项 RFC 8291 官方向量测试 |
> | P2-3 security-audit quinn 表述 | ✅ 已重写，明确标注「不在构建图中」；`RUSTSEC-2023-0071` 例外已随 web-push 移除而删除 |
> | P2-4 依赖自动更新 | ✅ 已加 `.github/dependabot.yml` |
> | P3-1 God module 拆分 | ✅ 已完成：内联测试全部外移；`do_check_subscription_with_options` 361→203 行；`telegram_bot.rs` 1914→**1061** 行（拆出 callback_sign/auth/format/dispatch）；`api/update.rs` 1387→**794** 行（拆出 progress/package/github/runtime/signature） |
> | P3-2 前端测试补齐 | ✅ 已完成：新增 DOM 安全/无障碍/剧名语料、SSE 并发交错、drive 破坏性操作确认路径、诊断页备份恢复流程；前端测试 120 → 144 项。`stores/subscriptions.js` 仍有大量未覆盖方法 |
> | P3-3 无障碍 | ✅ 95 个 label 关联 + 7 处按钮组改用 `role="group"` + 13 个控件补 `aria-label` + 10 个对话框聚焦 + 日历状态文字 |
> | P3-4 前端刷新逻辑 | ✅ 订阅列表按 TTL 刷新 + 转存作业触发刷新；下载空闲轮询 15s |
> | P3-5 MD5 → SHA-256 | ✅ 已完成（`src/stable_id.rs`，7 处调用点，`md5` 依赖已移除） |
> | P3-6 overflow-checks + drain | ✅ 已开启 `overflow-checks` 并把 11 处减法改为 `saturating_sub` |
> | P3-7 长驻任务监督 | ✅ 已完成（`spawn_supervised` + 全局 panic hook，覆盖 6 个长驻循环） |
> | 新增（本轮发现） | 单实例锁 `src/instance_lock.rs`：两个进程共用 DATA_DIR 会整份互相覆盖并重复执行作业，此前无任何防护 |
> | P3-8 守卫点改写 | ✅ 已全部改写（7 处） |
> | P3-9 文档漂移 | ✅ 已修复（roadmap / README / api-contract / pwa / 环境变量 / CHANGELOG），并加门禁 |
> | P3-10 工程化 | ✅ 已完成：工具链固定 + MSRV 声明 + Dependabot + release 档加固 + `deny.toml`（cargo-deny 通过）+ 覆盖率基线（lib 62.62%，CI 只报告不设阈值） |
> | S1 自更新签名 | ✅ 机制已完成：主机白名单 + 逐跳重定向校验（无需外部密钥即生效）；minisign/Ed25519 签名校验（含自实现 BLAKE2b-512，RFC 7693 向量验证）与发布端签名步骤。⚠️ 需维护者生成密钥对并配置 Secret 后才实际生效 |
> | S2 push SSRF 过滤 | ✅ 已补 IPv4-mapped / CGNAT / 保留段，含正反单元测试 |
> | S3 aria2 `out` 清洗 | ✅ 已在提交边界强制清洗（`src/filename.rs`） |
> | S4 `no-store` | ✅ `/api/*` 统一 `no-store`（`/api/images/` 除外） |
> | S5 CSRF 失败开放 | 📝 已把「为何刻意不 fail-closed」写进代码；未改行为（fail-closed 会破坏全部文档化的 curl 用法） |
> | S6 webhook 无体积上限 | ✅ 已加 64 KiB 上限 |
> | S7 无 Host 白名单 / 无 HSTS | ✅ 已补 HSTS（不含 `includeSubDomains`）；新增可选 `allowed_hosts` 白名单（默认关闭，配置后拒绝列表外 Host，含三种写法归一化），3 项测试覆盖 |
> | S8 `constant_time_eq` 长度泄露 | 📝 仅信息性，未改（网络环境下不可利用） |
> | S9 容器加固 | ✅ cap_drop + cap_add 最小集 + no-new-privileges + read_only + tmpfs + 端口默认绑本机 |
>
> 图例：✅ 已修复 / ⏳ 未处理 / 📝 已评估但不改（附理由）。
>
> **本文件保留 2026-09-21 评审当时的结论与证据**，作为修复依据与后续待办的来源。

## 一、结论摘要

项目整体健康度**明显高于同类自托管项目**。分层纪律、默认拒绝的鉴权模型、原子落盘与有界 Store、完整的 CI 门禁都不是摆设——我在源码里逐条验证过（第七节）。本次评审也纠正了两条流行但错误的印象：原始 `unwrap` 计数**不代表** panic 风险，`app-input` 这类"未定义类"**也不都是**缺陷。

但发现了 **1 个稳定复现的用户可见缺陷**、**1 组已发布的样式缺陷**、**1 个未认证即可触发的拒绝服务**、**1 条隐性提权**、**3 条状态一致性风险**、**2 个真实运行时阻塞点**，以及一批文档漂移。

| 优先级 | 事项 | 影响 | 证据强度 |
|---|---|---|---|
| **P0-1** | **调度器 `reload()` 在首次启动后必定返回错误**，而设置已经保存成功 | 用户改检查间隔 / 夸克 Cookie / 签到设置时**每次都报错**，但配置其实已生效——误导性失败提示 | **[实测]** 读源码 + 依赖 crate 源码确认 |
| **P0-2** | `static/styles.css` 未随 v2.7.0/v2.7.1 重建，11 个类名在任何样式表中都不存在 | 跳季按钮内边距、徽标透明度、加载动画、面板配色等在线上失效 | **[实测]** |
| **P0-3** | CI 没有 CSS 新鲜度门禁 | P0-2 会**静默复发**（源与产物分离且产物入库） | **[实测]** |
| **P1-1** | 维护模式下**队列无界增长**：`job_maintenance_mode` 只拦 worker，不拦入队 | 开启维护后 `jobs.json` 无限增长，每次入队全量重写 + 2 次 fsync → O(n²) I/O | **[实测]** |
| **P1-2** | 云端转存与本地落盘不原子，崩溃/超时后**重复转存** | 网盘里出现重复文件，且会反复发生 | **[深审]** 逻辑链已复核 |
| **P1-3** | `sync_download` 可能"已标记转存但从未下载"，且无对账 | 该集永久不下到本地，UI 却显示已处理 | **[深审]** |
| **P1-4** | 备份创建/校验把整段写盘 + 隔离恢复 + 全量 SHA-256 跑在 tokio worker 上 | 定时任务每轮触发，大数据集下阻塞运行时数秒到数十秒 | **[深审]** 已复核调用点 |
| **P1-5** | `GET /api/drive/aria2/browse` 在 async handler 里做阻塞 `read_dir` + 逐条 `canonicalize` | 下载目录挂 NFS/SMB 时占满 worker 池 | **[实测]** |
| **P1-6** | **认证限流在凭据校验之前判定**：失败计数饱和后，**正确密码也被 429** | 未认证攻击者以 ~5 次/分钟即可让整个 UI/API 不可用（仅 `/health` 幸存）；默认 `trust_proxy_headers=false` 时反代后所有请求同键，一人打满即全员锁死 | **[实测]** |
| **P1-7** | scope `read` 被当作通配符，可满足任意 `*:read`（含 `diagnostics:read`） | 只授 `read` 的 Token 可读 `/api/telegram/audits`，其中 `target` 是原始命令参数——含分享链接与提取码 | **[实测]** |
| **P2-1** | `Cargo.lock` 落后 **132** 个 crate | 错过上游修复；一次 `cargo update` 可收敛 | **[实测]** |
| **P2-2** | `web-push` 引入第二套 HTTP/TLS 栈与 BoringSSL | 唯一的 RustSec 例外完全来自这条链，去掉即可移除例外 | **[实测]** |
| **P3** | `do_check_subscription_with_options` 单函数 361 行；5 个长驻任务无监督；61/139 个 `<label>` 无关联；文档多处漂移 | 长期维护成本与回归风险 | 混合 |

**安全方面无 Critical / High**：未发现远程未授权访问、注入或不可恢复的数据丢失路径。安全基线（默认拒绝的 scope 表、常量时间比较、日志脱敏、路径校验、密钥 0600 落盘）确实扎实，详见 9.4 的负向清单；需处理的 2 个 Medium 已列为 P1-6（限流判定顺序）与 P1-7（scope 通配符）。持久化实现同样经得起审视（第七节）。

---

## 二、质量门实测结果（全部通过）

环境 `rustc 1.97.0` / `node v24.20.0`：

| 检查 | 命令 | 结果 |
|---|---|---|
| 编译 + lint | `cargo clippy --all-targets` | ✅ 0 警告（CI 另加 `-D warnings`） |
| Rust 测试 | `cargo test --all --locked` | ✅ **641 passed / 0 failed / 2 ignored** |
| 前端测试 | `node --test tests/frontend_*.test.js` | ✅ **120 pass / 0 fail**（20 个文件） |
| 前端产物一致性 | `node scripts/build-frontend.mjs --check` | ✅ 同步 |
| OpenAPI 契约 | `python3 scripts/check-openapi.py` | ✅ 94 paths / 106 operations |
| 运行时冒烟 | `scripts/smoke-*.sh`（CI） | ✅ 含真实 Chromium 390px/1440px E2E、10 秒 soak、Docker 卷升级 |

**测试密度与门禁完备度是这个项目最强的资产**，下面的建议都是在此基线之上的增量。

---

## 三、P0：立即修复

### P0-1 调度器 `reload()` 在首次启动后必定失败（误导性报错，稳定复现）**[实测]**

这是本次评审发现的**唯一一个用户必然撞到的功能缺陷**。

调用链：

1. `src/app.rs:191` 启动时调用 `self.scheduler.start().await`（`quark_signin_scheduler` 同理，`:194`）。
2. `SubscriptionScheduler::start()`（`src/services/subscription_scheduler.rs:126`）末尾执行 `self.scheduler.start().await?`。
3. `tokio-cron-scheduler` 0.13.0 的 `JobScheduler::start()`：

```rust
// ~/.cargo/registry/src/index.crates.io-*/tokio-cron-scheduler-0.13.0/src/scheduler.rs:232-238
pub async fn start(&mut self) -> Result<(), JobSchedulerError> {
    let is_ticking = self.ticking.load(Ordering::Relaxed);
    if is_ticking { Err(JobSchedulerError::TickError) } else { self.ticking.swap(true, ...); ... }
}
```

`ticking` 初始为 `false`（`:32`），**只有置 `true` 一处写入（`:237`），全 crate 再无任何地方把它复位**；`shutdown()` 设置的是另一个独立标志。因此**第二次及之后每次 `start()` 都返回 `Err(TickError)`**。

4. `SubscriptionScheduler::stop()`（`:140-153`）只做 `self.scheduler.remove(&uuid)`，**既不调用 `shutdown()` 也不重建 `JobScheduler`**（字段是 `scheduler: JobScheduler`，非 `Option`）。
5. `reload()`（`:156-160`）= `stop()` 然后 `start()` → 必然在 `start()` 的第 3 步失败。
6. `src/api/settings.rs:1195-1208`：**先持久化设置**，之后才 `state.scheduler.reload().await?` / `state.quark_signin_scheduler.reload().await?` —— 错误直接冒泡成 HTTP 失败响应。

**用户可见后果**：只要 `subscription_scheduler_enabled` / `subscription_check_interval_minutes` / `quark_cookie` 有变化，保存设置就报错；`quark_signin_enabled` / `quark_signin_hour` / `quark_signin_cookie` 同样。而设置**其实已经存盘、新 cron 任务也已经 add 成功并会正常运行**。用户会以为失败而反复重试或改回去，这是典型的"错误提示比没提示更糟"。

> 注意：首个 `reload()` 之前若调度器未启用，`start()` 会在 `:52-54` 提前返回、不触碰 `ticking`，所以**禁用状态下不受影响**；一旦启用（默认路径），此后每次保存都失败。

**修复**（任选，推荐第 2 个）：

1. 把 `start()` 的结果按幂等处理：`if let Err(JobSchedulerError::TickError) = self.scheduler.start().await { /* 已在运行，视为成功 */ }`。
2. 让 `reload()` 重建调度器：`stop()` 中调用 `self.scheduler.shutdown().await` 并替换为新的 `JobScheduler::new()`，或在 `reload()` 里构造新实例——语义最干净。
3. 并且在 `src/api/settings.rs` 调整顺序：**先 reload，成功后再持久化**，或 reload 失败时回滚已写入的设置，避免"报错但已生效"。

**强烈建议补一条回归测试**：连续两次 `start()` 不返回错误。当前 641 个测试没有一个覆盖这条路径——这也解释了它为何能存活至今。

### P0-2 编译后的 Tailwind CSS 落后，11 个类名在任何样式表中都不存在 **[实测]**

`static/styles.css` 是**入库的构建产物**，最后一次更新是 `27bc59c`（2026-08-22，v2.6.0）；`static/partials/modals.html` 改于 v2.7.0、`page-diagnostics.html` 改于 v2.7.1，都没有重新编译。

**(a) 标准 Tailwind 工具类缺失**（本应被重新编译生成）：

| 类 | 使用位置 | `static/styles.css` |
|---|---|---|
| `py-1.5` | `static/index.html:2517`、`static/partials/modals.html:325` | ❌ 只有 `.py-1` |
| `opacity-75` | `static/index.html:2520`、`static/partials/modals.html:328` | ❌ 完全没有 |

`git log -S'opacity-75' --oneline -- static/` → `98102ea`（v2.7.0），确认是 v2.7.0 引入、CSS 未重建。影响：跳季订阅季度按钮（`class="px-3 py-1.5 rounded-lg text-sm transition-colors"`）垂直内边距丢失；季度文件数徽标（`class="ml-1 text-xs opacity-75"`）不再弱化。

**(b) 在 partials 中使用、但在 `tailwind/input.css` 与 `static/styles.css` 中都不存在的类**（`grep -c` 均为 0，仅把 `static/index.html` 计入使用次数）：

| 类 | 使用次数 | 判断 |
|---|---|---|
| `app-select` | 5 | 与 `app-input` 同属**无影响**——`tailwind/input.css:123` 的 `input, select, textarea` 元素选择器已统一接管 |
| `app-input` | 11 | 同上，**不是缺陷** |
| `field-help` | 3 | 辅助文本样式缺失，退化为普通文本 |
| `bg-panel` | 2 | **明确失效**：`panel` 不在 `tailwind.config.js` 的 colors 中，无法生成；正确写法是 `bg-surface`/`bg-app`。用于重命名预览的**表头行背景**，现为透明 |
| `text-text-muted` | 1 | **明确失效**：`text-muted` 才是有效 token（`.text-muted` 存在于 styles.css）。用于 Telegram 授权说明文字，现为默认文字色 |
| `page-header` | 1 | 诊断页容器修饰，无影响或轻微 |
| `page-heading` | 1 | 导航标题修饰，同上 |
| `section-kicker` | 1 | 诊断页 "Operations" 小标签样式丢失 |
| `loading-spinner` | 1 | **明确失效**：`<span class="loading-spinner"></span>` 是**空元素**，无 `@keyframes spin` 绑定、无尺寸——日历加载指示器**完全不可见**（styles.css 里存在 `@keyframes spin`，但没有任何类使用它） |
| `dashboard-zone-compact` | 1 | 与 `.dashboard-zone` 搭配的修饰类，紧凑意图静默失效 |
| `automation-event-panel` | 1 | 与 `.app-panel subscription-activity-panel` 搭配，冗余修饰 |

复现：

```bash
for c in py-1.5 opacity-75 app-select bg-panel text-text-muted loading-spinner section-kicker; do
  echo "$c: input.css=$(grep -c -- "\.$c" tailwind/input.css) styles.css=$(grep -c -- "\.$c" static/styles.css)"
done
```

**修复**：在 `tailwind/input.css` 补上 `bg-panel`/`text-text-muted`/`loading-spinner`/`section-kicker` 等的真实定义（或把引用改成有效 token），然后运行 `scripts/build-css.sh` 并提交 `static/styles.css`。

### P0-3 CI 缺少 CSS 新鲜度门禁（缺陷根因）**[实测]**

`scripts/build-css.sh` 依赖外部 `tailwindcss` 二进制（本机与 CI 均未安装）。CI 只检查 `build-frontend.mjs --check`（HTML 拼接）、`node --check`、eslint 与 OpenAPI，**没有任何步骤校验 `static/styles.css`**。这是"源与产物同时入库、产物却无校验"的结构性问题，P0-2 只是它的表现。

值得注意的是，这个问题**项目自己已经记录过**：`docs/code-review-2026-07-26.md:166` 写着"styles.css ↔ tailwind/ 的新鲜度检查未做…留待后续"，`docs/roadmap.md` 的质量门清单里 `Q-01 scripts/build-css.sh` 也是唯一真正无人执行的一项。

**修复（推荐第 1 个）**：

1. **无需 Tailwind CLI 的类覆盖检查**：断言 `static/index.html` 中每个类名 token 都能在 `static/styles.css` 里找到对应选择器（过滤掉 Alpine `:class` 表达式后约 800 个 token）。这同时抓住"产物过期"和"类名拼错"两类问题，成本极低。
2. 把 `tailwindcss` standalone 二进制加入 CI（约 40MB，可缓存），新增 `scripts/build-css.sh && git diff --exit-code -- static/styles.css`。
3. 彻底方案：`static/styles.css` 不再入库，改为构建期生成（Dockerfile 与 release workflow 各加一步）。

---

## 四、P1：正确性与可用性

### P1-1 维护模式下作业队列无界增长 **[实测]**

`job_maintenance_mode` 全库**只有一处**被当作门禁使用：

```bash
grep -rn 'job_maintenance_mode' src/ --include=*.rs | grep -v 'tests\|// '
# src/jobs/worker.rs:163:  if !settings.job_maintenance_mode {   ← 唯一的门禁
# src/api/settings.rs:430,820 · src/api/diagnostics.rs:62,286 · src/models/settings.rs:125,640  ← 仅设置/展示
```

即：维护模式**只拦 worker 执行**，不拦订阅检查继续入队。而 `truncate_jobs`（`src/jobs/store.rs:359-372`）的注释明确写着"**绝不淘汰排队或运行中的任务**。若终态任务不足以降到容量以内，则允许暂时超出容量"，且每次入队都要全量重写 `jobs.json` 并做 2 次 fsync。

**失败场景**：对一部在追的剧开启维护模式 → 每个检查周期都入队新转存任务、无人执行、终态任务为 0 故永不淘汰 → `jobs.json` 无界增长，单次入队 I/O 随规模线性上升（累计 O(n²)），`GET /api/jobs` 与每次 `list()` 克隆同步变大。

**修复**：把维护模式检查下移到自动转存入队点（或在 enqueue 处加存活任务硬上限，超限时拒绝并通知）。这是单点修改。

### P1-2 云端转存与本地记录不原子 → 重复转存 **[深审]**

`src/services/subscription_transfer.rs:307-322` 的顺序是 `provider.transfer(...)` 然后 `mark_files_as_transferred(...)`——`transferred_file_keys` 只在成功后写入。而检查路径是**依据 transferred 键**重算候选（`subscription_check/file_filter_methods.rs:189-223`）并**新建作业**（`subscription_check.rs:500-516`），手动重试也会重放同一 payload。唯一的守卫 `filter_already_transferred_files`（`subscription_transfer.rs:210-211`）读的正是崩溃导致没写上的那批键。

**失败场景**：夸克已接受转存但响应丢失（超时/RST）→ 作业 Failed → 下次检查重新选中同一批文件 → **网盘里出现重复副本，且会反复发生**。`src/jobs/model.rs:114-121` 自己也承认转存非幂等，只靠"不自动重试"兜底。

**修复**：转存前先列目标目录，发现同名文件即视为已完成；或先写"意图记录"再调用云端。

### P1-3 `sync_download` 可能"已转存但从未下载"，且无对账 **[深审]**

`subscription_transfer.rs:320-322` 在云端转存成功后**立即**标记文件为已转存，之后才在 `:391-404` 提交 aria2，`:836-888` 才写 `sync_downloads` 记录；`:816-833` 显示 `add_uri` 失败时**不写任何记录**。由于文件已进 `transferred_file_keys`，后续检查不会再选它（`file_filter_methods.rs:110,157`）。

**失败场景**：转存过程中 aria2 抖动一次 → 该集永久不会下载到本地，UI 却显示已转存/进度已推进，只有作业里的 `aria2_error` 留痕；没有任何机制比对 `transferred_files` 与 `sync_downloads`。另外 `record_sync_downloads` 的保留策略只清理**已完成**记录（`:873-882`），未完成记录会无限累积。

**修复**：持久化"待下载意图"（或把失败落盘），并在启动/定时增加一次对账：已转存但无完成下载记录的文件重新提交。

### P1-4 备份创建/校验在 tokio worker 上同步跑完整套 IO **[深审，已复核]**

`src/services/backup.rs:211-253` 的 `async fn create_stored_backup` 中，**只有 `export_archive` 被 `spawn_blocking` 包住**（`:206`），其余步骤全部同步阻塞在异步运行时上：

| 行 | 调用 | 实际行为 |
|---|---|---|
| `:228` | `backup_storage_size(...)` | → `:896` → `:847` `std::fs::read_dir` + 逐文件 `metadata` |
| `:236` | `write_file_atomic(...)` | 阻塞写入整个归档 + `sync_all` + `rename` + 父目录 fsync |
| `:241` | `verify_path_locked(...)` | 读**整个归档** → 解析 → **把每个文件写回磁盘** → 逐个重读 → 逐文件 SHA-256 → `remove_dir_all` |
| `:232/:245` | `prune_locked(...)` | `list_backups` + `remove_file` |
| `:250-252` | `copy_external_locked` / `prune_external_locked` | 同样阻塞 |

`verify_latest_stored_backup`（`:372-379`）的 `:377` 同样未包装。

**可达路径**：`POST /api/backups`、存储清理端点（以 `pre-cleanup` 创建）、`GET` 校验，以及**两个定时循环**（`backup.rs:176`、`:187`）。

**这不是设计取舍而是遗漏**：同文件里 `export_archive`（`:206`）、`list_stored_backups`（`:254-257`）、`apply_pending_restore`（`:324`）都正确用了 `spawn_blocking`，`src/utils/mod.rs:64` 还专门提供了 `write_json_atomic_async`——**这条路径只是漏了**。

**修复**：把 `verify_path_locked` / `write_verification_report` / `prune_locked` / `prune_external_locked` / `copy_external_locked` / `backup_storage_size` 改为只从 `spawn_blocking` 调用；最干净的做法是让 `create_stored_backup` 在异步侧算好文件名与字节，然后把"写入 → 校验 → 清理 → 外拷"整段交给**一个** `spawn_blocking` 闭包（`operation_lock` 是 tokio Mutex 且跨该段持有，语义不变）。

### P1-5 `GET /api/drive/aria2/browse` 在请求路径上做阻塞目录 IO **[实测]**

`src/api/drive/aria2.rs:324` `browse_aria2_dir`：`:336/:340` 的 `canonical_dir` → `:407` 阻塞 `canonicalize`；`:349` `std::fs::read_dir`；**每个条目**再做 `:352` `entry.file_type()` 与 `:361` `path.canonicalize()`。函数内无 `spawn_blocking`。

**影响**：Aria2 下载目录在自托管场景常挂 NFS/SMB/mergerfs，单次 `canonicalize` 可达数百毫秒，逐条调用使其成为 O(n) 次 syscall。2 万条目目录一次请求即占住 worker 数秒；前端导航时会调该接口，几个并发即可占满 worker 池，连带拖住任务队列、SSE 与下载监控。

**修复**：整段 `read_dir` 移入 `tokio::task::spawn_blocking`，并去掉逐条 `canonicalize`（`root` 已被 `canonical_dir` 解析，`starts_with` 比较即可）。**安全校验那层（`:336-346`）写得对，不要动。**

### P1-6 认证限流在凭据校验之前判定 → 未认证者可锁死整个实例 **[实测]**

`src/api/mod.rs` 中间件的实际顺序是：

```
:178  let settings = state.settings_store.get().await;
:183  let rate_key = auth_rate_key(req.headers(), peer, settings.trust_proxy_headers);
:185  if state.is_blocked(&rate_key, now) { return auth_rate_limited_response(); }   ← 先判封锁
:189  ... Bearer Token 校验 ...
:218  ... Basic Auth 校验 ...
:235  if authorized { state.clear(&rate_key); ... }                                 ← 成功才清除计数
```

`is_blocked`（`:80-90`）在失败次数 ≥5 时返回 `true`；`clear` 只在**成功通过校验之后**才执行（`:235`）。因此一旦失败计数饱和，**携带正确密码/正确 Token 的请求同样在 `:185` 被 429 拦下，永远走不到 `:235` 去清除计数**。限流窗口是 60 秒滑动窗（`prune_auth_failures`，`:118-126`），攻击者只要保持约 5 次/分钟的错误请求就能让窗口持续饱和——**这是无需任何凭据即可维持的拒绝服务**，且 `/health` 之外的所有端点（含 UI 与自动化 API）全部不可用。

放大因素（`auth_rate_key`，`:134-156`）：限流键在 `trust_proxy_headers=false`（默认）时是**对端 IP**。若按官方推荐部署在 nginx 反代之后而该开关保持默认，则所有请求的键都是反代 IP——**一个攻击者打满即等于全体（含管理员）被锁死**。反向的配置同样危险：若开启 `trust_proxy_headers=true` 而 56001 端口仍直接可达，攻击者自行伪造 `X-Forwarded-For` 即可绕过限流无限爆破密码。

**修复**：

1. **把封锁判定移到凭据校验之后**，只对"凭据错误"的请求返回 429；携带正确凭据的请求应当被放行并清除计数。（推荐，语义最正确）
2. 或保留当前位置，但让携带 Basic 凭据的请求先做一次校验，通过则清计数并放行。
3. 部署侧：限流键始终用 socket 对端地址，XFF 仅用于日志；并把 56001 端口绑定到 `127.0.0.1`，只让反代可达（见第七节 F11）。

### P1-7 scope `read` 被当作通配符 → 最小权限失效，泄露分享链接与提取码 **[实测]**

`src/store/automation_token.rs:177-181`：

```rust
fn scope_allows(scopes: &[String], required: &str) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "read" && required.ends_with(":read") || scope == required)
}
```

按 Rust 优先级（`&&` 高于 `||`）等价于 `(scope == "read" && required.ends_with(":read")) || (scope == required)`。于是**只授 `read` 的 Token 可以满足任何以 `:read` 结尾的 scope**。而 `src/api/mod.rs:265-281` 把 `/api/jobs*`、`/api/notifications*`、`/api/diagnostics*`、`/api/telegram/audits`、`/metrics` 分别映射到 `jobs:read` / `notifications:read` / `diagnostics:read`。

关键在于审计日志的内容：`src/services/telegram_bot.rs:370-382` 把 `target` 记为**原始命令参数**，而 `/subscribe <分享链接> <提取码>` 的参数正是分享链接与提取码（`store/telegram_bot.rs:19-35`）。因此一个本意是"只读日历/订阅"的 `read` Token，持有人在泄露后可直接 `GET /api/telegram/audits?limit=500` 拿到用户粘贴过的全部分享链接与提取码。

`TOKEN_SCOPES`（`automation_token.rs:9-20`）把 `read` 与 `diagnostics:read` 并列为独立选项，文档也把 scope 描述为细粒度，因此这是**与设计意图不符的隐性提权**，而非有意的超级 scope。

**修复**：`scope_allows` 改为精确匹配 `scopes.iter().any(|s| s == required)`；若确实想保留 `read` 超级 scope，则必须停止把 `diagnostics:read` 作为独立选项宣传，并明确 `read` 不满足 `diagnostics:read`（即诊断与审计日志保持 Basic Auth 专属）。

### P1-8 其余状态一致性风险（简列，均属 [深审]）

| 问题 | 位置 | 后果 | 修复方向 |
|---|---|---|---|
| 取消 Running 的 MetadataScrape 可能让**磁盘比内存新**：abort 落在 `write_json_atomic_async` 的 `spawn_blocking` 期间（不可取消）→ 文件已写、`replace_memory` 未执行 | `jobs/queue.rs:152-191`、`utils/mod.rs:76`、`store/subscription.rs:110-111` | 下一次基于旧内存的写入会**回滚掉刚抓到的元数据** | 写入前重读合并，或在阻塞任务内完成内存交换 |
| 无**单实例锁**：`src/` 内没有任何 flock/lockfile | 全库 | 两个进程共用 `DATA_DIR` 时，各自整体重写 JSON → 后写者覆盖前者，同一个 Queued 作业可被领取执行两次（重复转存、重复推送） | 启动时对 `DATA_DIR/.lock` 取排他 advisory lock |
| 隔离损坏文件后**以空 Store 继续运行**（settings 例外，会中止启动） | `store/subscription.rs:75-82`、`jobs/store.rs:72-77`、`store/notification.rs:55-59`、`store/automation_event.rs:66-71`、`store/telegram_bot.rs:102-108` | 一次手工误编辑（或上游字段类型变化）就会让**所有订阅从运行中的服务里消失**，首次新写入还会生成全新文件；日志与诊断页都有提示（设计如此），但需要人工介入恢复 | 对"非空业务 Store"改为中止启动或要求显式确认 |
| `try_send` 通道满时把**已持久化的作业标记为 Failed** | `jobs/worker/push_dispatch.rs:141-156` | 一个合法作业被永久判死，而 reconcile 扫描本可以救回；且空闲 worker 阻塞在 `recv()` 上没有定时器 | 通道满不应失败已落盘的作业；增加周期性 due-job 扫描替代进程内定时器 |
| 语义变更未 bump `schema_version`，未知字段在下次写入时被丢弃 | `store/schema.rs:10`、`models/subscription.rs:214-219` | v2.7.1 的跳季订阅（S1+S3）回滚到 v2.6.1 → 中间季被自动转存，且 `season_list` 被首次写入抹掉，**再升级也回不来**（`docs/upgrade-v2.7.0.md` 自己警告过） | 语义变更 bump 版本；模型加 `#[serde(flatten)] extra` 透传未知字段；信封里记录写入方 app 版本 |
| `automation-token.json` **没有** schema 信封与版本门禁（`StoreKind` 只有 6 个变体，不含它） | `store/schema.rs:13-20`、`store/automation_token.rs:59-71`、`:166` | README 的"每个 Store 都用 schema_version 信封"实为 7 之 6 | 补上信封，或修正 README 表述 |

### P1-9 前端：`pagehide` 拆除后没有 `pageshow` 恢复 **[深审，已复核]**

```bash
grep -rn 'pagehide' static/js        # static/js/core/polling.js:110 → this.destroy()
grep -rn 'pageshow|bfcache|persisted' static/js static/partials | wc -l   # 1（且不相关）
```

`destroy()` 会停掉注册表里的全部资源：轮询定时器、`visibilitychange`、`popstate`、全局 keydown、error/unhandledrejection 处理器以及 jobs 的 EventSource，且 `lifecycleDestroyed` 不会复位。

**影响**：移动端 Safari/Chrome 前进后退触发 bfcache 恢复后，页面**看似正常但完全停止更新**——数据陈旧、快捷键与路由失效、错误边界也没了，且没有任何用户可见提示。

**修复**：`pagehide` 处理器里判断 `event.persisted === true` 时跳过 destroy；或增加 `pageshow` 监听，复位 `lifecycleDestroyed` 并重跑初始化。

---

## 五、P2：依赖更新

### P2-1 `Cargo.lock` 落后 132 个 crate **[实测]**

```bash
cargo update --dry-run
# Locking 132 packages to latest compatible versions
```

**全部在当前 `Cargo.toml` 约束内**，零 API 风险，建议单独提一个 PR。值得注意的：

| crate | 当前 | 可到 | 说明 |
|---|---|---|---|
| `tokio` | 1.52.3 | 1.53.1 | 运行时 |
| `hyper` | 1.10.1 | 1.11.1 | HTTP 栈 |
| `rustls-pki-types` | 1.14.1 | 1.15.1 | TLS |
| `quinn-proto` | 0.11.15 | 0.11.18 | 见 P2-3 |
| `webpki-roots` | 1.0.7 | 1.0.9 | 根证书 |
| `uuid` | 1.23.3 | 1.26.1 | |
| `regex` | 1.12.4 | 1.13.1 | 名字解析热路径 |
| `md5` | 0.8.0 | 0.8.1 | 见 P3-5 |
| `aes-gcm` / `aead` / `ctr` / `ece` / `ed25519-compact` | — | — | Web Push 加密链 |

⚠️ 输出中含一条 **downgrade**：`crypto-common 0.1.7 -> 0.1.6`。执行后请跑 `cargo tree -d` 确认收敛，并按第二节的命令复跑全部门禁。

跨大版本升级空间（属独立议题，勿混合）：`reqwest` 0.12.28→0.13.5、`tower-http` 0.6.11→0.7.1、`tokio-cron-scheduler` 0.13.0→**0.15.1**（注意：升级它可能顺带修掉 P0-1 的 `TickError` 语义，值得先查 0.14/0.15 的 `start()` 实现）、`p256` 0.13.2→0.14.0、`base64` 0.22.1→0.23.1。`axum` 0.8.9 已是最新 ✅。

### P2-2 去掉 `web-push`，可同时消除 RustSec 例外并砍掉一整条 C/C++ 依赖链 **[实测]**

`docs/security-audit.md` 长期忽略 `RUSTSEC-2023-0071`（`rsa` 0.9 无修复版本）。实测这条链**完全由 `web-push` 单点引入**：

```
rsa 0.9.10   ← superboring 0.1.12 ← jwt-simple 0.12.17 ← web-push 0.11.0
isahc 1.8.3  ← web-push            （curl-sys / libnghttp2-sys / libz-sys）
```

即：`web-push` 让这个以 rustls 为准的项目**同时编进第二套 TLS/HTTP 栈**（isahc → libcurl → nghttp2，C 代码）与 **BoringSSL**（superboring，C++ 代码）。

`src/services/push.rs:16-18` 实际只用到 `VapidSignatureBuilder`、`WebPushMessageBuilder`、`IsahcWebPushClient`、`ContentEncoding`、`WebPushError`，而替代件**项目已经具备**：

- VAPID 的 ES256 签名 → `p256` 已是直接依赖（`src/store/settings.rs:69` 已在用它生成密钥）；
- HMAC / 摘要 → `ring` 已在用（`src/services/push.rs:823`）；
- 载荷加密（aes128gcm）→ 直接用 `ece`（目前也经 `web-push` 间接引入）；
- HTTP 发送 → 复用 `reqwest`（可一并获得 `src/clients/http_pool.rs` 的连接池、超时与重试）。

**收益**：移除 `rsa`、`jwt-simple`、`superboring`、`isahc`、`curl-sys`、`libnghttp2-sys`、`libz-sys`；`security-audit.md` 的唯一例外条目可删除；构建时间与二进制体积下降；回到单一 TLS 栈。**成本**：自行实现 VAPID JWT 组装与 `ece` 调用并补测试。中等工作量、高确定性。

### P2-3 `security-audit.md` 中 `quinn-proto` 一条的表述可再精确 **[实测]**

```bash
cargo tree -i quinn-proto --locked --target all
# warning: nothing to print.
```

`quinn`/`quinn-proto` 仅作为 `reqwest` 的**可选特性**（HTTP/3）出现在 `Cargo.lock`，本项目启用的是 `json, rustls-tls, stream`，**这些 crate 不进入编译产物**。锁文件层面的收敛仍有价值（将来开启 http3 会受益），但宜改为"锁文件层面的预防性收敛，不在当前构建图中"，避免读者误以为它修复了随二进制发布的漏洞。

### P2-4 缺少依赖自动更新 **[实测]**

`.github/` 下没有 `dependabot.yml` 或 `renovate.json`。项目已有完整 CI 与 `scripts/bump-version.sh`，加一个 Dependabot 配置（cargo + github-actions + docker）成本极低，能把"一次落后 132 个 crate"变成每周的小 PR。

---

## 六、P3：可维护性、前端与文档

### P3-1 大文件：真问题不是总行数，而是**内联测试**与**超长函数**

按"总行数 / 生产代码行数"重新统计：

| 总行数 | 生产行数 | 文件 |
|---|---|---|
| 2,278 | 1,071 | `src/services/download_monitor.rs`（**1,207 行是内联测试**） |
| 1,909 | 1,909 | `src/services/telegram_bot.rs`（测试在 `telegram_bot/tests.rs`） |
| 1,645 | 1,328 | `src/api/update.rs`（317 行内联测试） |
| 1,562 | 1,315 | `src/services/subscription_check.rs`（测试在 1,548 行的 `tests.rs`） |
| 1,322 | 929 | `src/services/episode.rs`（393 行内联测试） |
| 1,304 | 1,232 | `src/api/settings.rs` |
| 1,165 | 846 | `src/jobs/worker.rs` |
| 1,153 | 1,153 | `src/services/push.rs` |
| 1,143 | 747 | `src/services/subscription_source_switch.rs` |
| 1,137 | 610 | `src/services/media_calendar.rs` |
| 1,125 | 925 | `src/services/backup.rs` |
| 1,028 | 1,028 | `src/services/subscription_transfer.rs` |
| 1,024 | 1,024 | `src/services/telegram_bot/menus.rs` |

**结论一（最省力）**：`subscription_check` 与 `telegram_bot` 已把测试拆到兄弟文件；`download_monitor.rs`（1,207 行）、`episode.rs`（393 行）、`api/update.rs`（317 行）还没做。**仅把内联测试机械外移**就能让 `download_monitor.rs` 从 2,278 降到 1,071 行，零逻辑风险。

**结论二（真正的坏味道）**：`src/services/subscription_check.rs` 的 `do_check_subscription_with_options`（`:170-530`）是**单函数 361 行**，内含至少 5 个提前返回分支——全仓库最难审阅、最易藏 bug 的一段。建议**优先于任何文件拆分**处理：按 `probe_stage()` → `filter_new_files_stage()` → `transfer_or_download_stage()` → `persist_stage()` 分解。同理 `telegram_bot.rs` 的 `handle_message`（249 行）与 `handle_callback`（243 行）。

**结论三（拆分边界）**：五个文件的逐 `fn`/`struct` 归属与新模块接口见 9.3。

### P3-2 零单元测试的大模块

| 行数 | 文件 | 风险点 |
|---|---|---|
| 1,024 | `src/services/telegram_bot/menus.rs` | 回调 ID 生成与会话状态机 |
| 591 | `src/services/subscription_transfer/helpers.rs` | 转存计划辅助逻辑 |
| 507 | `src/services/subscription_check/file_filter_methods.rs` | 文件筛选核心规则 |
| 501 | `src/api/push.rs` | 推送订阅注册/校验 |
| 480 | `src/services/telegram_bot/commands.rs` | 写命令解析 |
| 478 | `src/api/subscription_source.rs` | 换源 API |
| 449 | `src/api/drive/aria2.rs` | 下载器操作 |
| 373 | `src/api/subscriptions/crud.rs` | 订阅增删改（含 ID 生成） |

前端同样有明确空白（120 个测试覆盖不均）：`stores/jobs.js:227-297` 的 `setupJobEvents`（SSE 快照/事件交错竞态，**前端并发最敏感的一段**）零测试；`features/diagnostics.js` 13 个方法（含 `restoreBackup` / `exportBackup` / `compactStorage`）零测试；`stores/drive.js` 36 个方法中 33 个未被引用，含 `deleteDriveItem` / `renameDriveItem` / `batchDeleteDrive` 等破坏性操作。建议复用 `tests/frontend_downloads.test.js:165-191` 已有的确认短语测试范式。

另：`tests/frontend_dom_safety.test.js`（43 行）**名不副实**——它只断言图片 error hack、资源 `?v=` 后缀与 `x-for` key 前缀，**从不检查 `innerHTML`/`x-html`**。

> 好消息（已独立核实）：**当前不存在 XSS sink**。`grep -rn -E 'innerHTML|outerHTML|insertAdjacentHTML|document\.write|eval\(|new Function' static/`（排除 vendor）只命中 `static/js/core/ux.js:47` 的一条注释；仅有的 5 处 `x-html`（`static/partials/nav.html:21,35,49,128,134`）渲染的是 `router.js:81-88` 里硬编码的 SVG；所有服务端字符串走 `x-text`；所有外链经 `safeExternalUrl()`（`ux.js:32-40`，仅 http/https）。**但正因为 CSP 含 `'unsafe-eval'`（Alpine 需要，`src/api/mod.rs:500`），未来的 HTML 注入将等价于 RCE**——所以补一个 sink 白名单断言是有价值的回归护栏。另 `escapeHtml`（`ux.js:41-49`）是**死代码，零调用点**，反而会诱导后人用 `x-html` + 转义。

### P3-3 无障碍：61/139 个 `<label>` 未与控件关联 **[实测，修正深审结论]**

```bash
grep -o '<label' static/partials/*.html | wc -l   # 139
grep -o 'for="'  static/partials/*.html | wc -l   # 78
grep -o 'x-for="' static/partials/*.html | wc -l  # 78（说明上面的 for= 不是 x-for 误报）
```

即 **61 个 `<label>` 没有 `for=`**（约 44%），典型是 `<label class="field-label">RPC URL</label><input ...>` 这类并列写法，输入框也没有 `id`。另有纯 placeholder、完全无标签的输入（`page-search.html:35`、`page-activity.html:78`、`page-drive.html:52`）。影响：读屏软件对这些控件无内容可读、点击标签不聚焦输入框。设置页是用户配置所有集成的必经之路，值得修。可加静态测试强制（与本仓库既有的断言风格一致）。

另两项：**模态框不移动/恢复焦点**（10 个 dialog 都正确带 `role`/`aria-modal`/`trapDialogFocus`，但因焦点从未移入面板，背景 Tab 键根本到不了陷阱处理器，`aria-modal="true"` 名不副实）；**日历周/月视图仅用颜色表达 `primary_status`**（列表视图有 `calendarStatusLabel` 文字，周/月视图没有，构成 WCAG 1.4.1 失败）。两者都是机械修复。

### P3-4 两个前端逻辑缺陷 **[深审]**

- **订阅列表不会自动刷新**：`stores/jobs.js:274-276` 只在 `metadata_scrape` 成功时刷新；`core/router.js:213-218` 进入订阅页只加载详情、不调 `loadSubscriptions()`（日历则会刷新，`:209-211`）。后果：自动转存完成后，订阅卡片上的集数/进度**长期停留在旧值**，直到手动刷新。
- **下载轮询一旦空闲就不会再启动**：`stores/downloads.js:424-431` 在无可轮询任务时提前返回，`:395-402` 会停止轮询，而唯一调用点是 `:269`（`loadDownloads()` 末尾）。后果：页面开着且当前无任务时，由订阅自动转存或另一设备新建的下载**不会出现**，页面静默退化为快照。

### P3-5 用 MD5 生成标识符与幂等键

`md5` 出现在 6 处业务逻辑：`jobs/model.rs:175`（**Job 幂等键**）、`jobs/reliability.rs:88`（抖动种子）、`api/subscriptions/crud.rs:45`（订阅 ID）、`telegram_bot/menus.rs:140,517`（回调 ID）、`clients/pansou.rs:139`、`api/subscription_exchange.rs:117`（导入幂等键）。

这些都不是抗碰撞场景（输入为内部或管理员数据，不面向远程攻击者），**当前不构成安全问题**；但"用已被攻破的哈希做幂等键"是每次审计都要解释的负担，而 `services/backup.rs:898` 已有 `sha256_hex`。建议统一换为截断 SHA-256；注意 `crud.rs:45` 的订阅 ID 变更需迁移兼容（双读窗口）。

### P3-6 release profile 缺少溢出保护与可诊断性

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
```

1. **`overflow-checks` 未开启**。全库有 **11 处**"先用减法算边界再切片"，例如 `jobs/store.rs:245` `archive.drain(0..archive.len() - archive_retention)`、`store/notification.rs:147`、`:264`、`store/automation_event.rs:327`、`store/subscription.rs:428`、`store/telegram_bot.rs:180,234,241`、`services/subscription_source_switch.rs:427`、`api/push.rs:125`。**每处当前都有紧邻的 `if len > N` 守卫**；但守卫一旦在重构中被移除，debug 下会响亮 panic，release 下则回绕成约 2^64，随后 `drain(0..巨大值)` 仍会 panic 但报的是难以定位的 "range end index out of range"，纯算术回绕则完全静默。注意 `store/notification.rs:264` 的安全性实际由 200 行外的 `configured_notification_retention()`（`:269-275`）的 clamp 保证——"安全在别处"最容易被重构破坏。
   **建议**：加 `overflow-checks = true`（本服务非 CPU 密集，代价很小），并把 `drain(0..len-keep)` 换成**根本不会 panic** 的 `truncate(keep)`。
2. **`strip = true` 让生产 panic backtrace 完全没有符号**。项目自带诊断导出与在线更新，线上崩溃只有地址会极难定位。建议改 `debug = "line-tables-only"`（体积增加很小），或发布流程单独产出 `.debug`。

### P3-7 五个长驻后台任务无监督 **[深审]**

13 处 `tokio::spawn` 丢弃了 `JoinHandle`，且 **`src/` 内没有任何 `panic::set_hook` 或 `catch_unwind`**，`panic = "abort"` 也未设置——panic 会展开、默认 hook 打印到 stderr、**该循环永久消失直到进程重启**。需监督的前五个：

| 位置 | 任务 | 死亡后果 |
|---|---|---|
| `services/download_monitor.rs:122` | 15 秒 Aria2 轮询 | 下载完成通知、`sync_downloads` 完结、订阅自动完结、`.nfo`/海报副作用**全部永久停止** |
| `services/telegram_bot.rs:204` | long-poll 主循环 | **整个 Telegram 控制静默失效** |
| `services/automation_events.rs:15` | 事件投影循环 | `automation_events` 停止记录，日历与 API 变陈旧 |
| `services/backup.rs:171`、`:183` | 定时备份与校验 | **备份停止**——自托管服务最危险的静默失败 |
| `services/notification.rs:348` | 启动补投 digest | 磁盘上已标记 `digest_pending` 的通知永不发出 |

**修复**：前五者保留 `JoinHandle`，由 supervisor `select!` 监听 `JoinError` 并重建；成本最低的 80% 方案是在每个长驻循环体外包 `AssertUnwindSafe(..).catch_unwind()`。**这项工作的重要性高于当前（为零的）可达 panic 数**——正因为现存 panic 都"今天有守卫"，而守卫恰恰是重构最容易移除的东西。

### P3-8 十二处"靠局部推理才安全"的守卫点 **[深审]**

以下 `expect`/索引当前**都不会 panic**，但安全性依赖别处细节。按被重构破坏的难易排序，前三个建议现在就换 `let-else`：

1. `services/subscription_status.rs:92` — `details.next().expect("subscriptions always include a season")`。从 `GET /api/subscriptions/{id}/status` 与 Telegram `/status` 可达。**安全仅因** `Subscription::season_numbers()`（`models/subscription.rs:403-411`）不可能为空——其区间分支以 `.max(self.season_start())`（`:388-392`）收尾；该 `.max()` 一删，订阅详情页即 500。**修复**：`let Some(mut detail) = details.next() else { return empty_detail(subscription) };`
2. `services/subscription_source_switch.rs:234-240` — `is_some()` + `unwrap()`。**修复**：`if let Some(probe) = candidate.probe_info.clone().filter(...)`。
3. `services/telegram_bot.rs:1409` — `settings.telegram_bot_allowed_user_ids[0]`，由 `:1404` 的 `.len() != 1` 守卫。但 `Settings` 经 `PUT /api/settings` 用户可写，若将来放宽为 `> 1`（"允许多管理员"是极自然的改动），每次按钮渲染都会 panic。**修复**：`.first()` + `let-else`。
4. `telegram_bot/commands.rs:84`、5. `subscription_check.rs:631`（后者 panic 会被 `join_next()` 兜住并**静默丢弃该订阅的批量结果**）。

其余 7 处（`reliability.rs:89`、`title_normalize.rs:254/258`、`models/subscription.rs:526-528`、`api/drive/actions.rs:168`、`api/update.rs:743`、`clients/douban.rs:125`）逐条确认有紧邻守卫；`episode.rs:211/221`、`title_normalize.rs:222/226`、`api/update.rs:868` 的中文多字节切片也全部来自 `char_indices()`/`str::find(char)`，**均在字符边界上，写得正确**（这是中文应用最易出错的一类）。

### P3-9 文档漂移（CI 覆盖不到的部分）

CI 的版本门禁只校验 README `### <version>`、CHANGELOG `## <version>`、`docs/upgrade-v<version>.md` 存在、README 索引链接与 `static/openapi.json` 的 `info.version`。以下**全部逃过门禁**：

| 漂移 | 位置 | 实际 |
|---|---|---|
| **roadmap 执行指针落后** | `docs/roadmap.md:5`（最近更新 2026-07-23）、`:16/:17`（当前阶段/任务 v2.5.1）、`:20`（发布基线 v2.5.1） | v2.7.1；且该文件自称"唯一持久化计划入口"，却**不在 README 文档索引里**。P0–P9/P11–P20 全部 `[x]`，已是完成台账而非计划；P10 的 4 项 `[!]` 与 `Q` 质量门 12 项未勾选（其中只剩 Q-01 CSS 门禁确实无人执行） |
| **在线更新"同一事务"表述过强** | `README.md:34`、`:192` 称"把二进制与整个 WebUI 作为同一事务切换" | 与本仓库自己的 `docs/docker-online-update.md:71`（"**分别**通过同目录 rename 原子切换"）矛盾；代码 `src/api/update.rs:1046-1051`（static）与 `:1065`（binary）确实是**两次** rename，存在"旧二进制 + 新 WebUI"窗口。README 应改正，并说明该窗口的兼容性要求 |
| **OpenAPI 数量散文四处不一致** | `README.md:216` 94/106 ✅、`docs/api-contract.md:197` 92/104 ❌、`docs/api-contract.md:182` 基线"84/94"（实际 `docs/openapi-baseline-v1.12.0.json` 为 81/91）、`docs/roadmap.md:41` 91/103 | 建议散文改为指向 `scripts/check-openapi.py` 输出，并加一步 CI 比对 |
| **未文档化的环境变量** | `README.md:160` 称完整清单在 `.env.example` | `STATIC_DIR`（`utils/mod.rs:21`）、`APP_RUNTIME_DIR`（`api/update.rs:586`）、`APP_USERNAME`/`APP_PASSWORD`（`app.rs:212-215`）均未列入；其中前两个**决定在线更新能否工作** |
| **登录限流错误码写错** | `docs/api-contract.md:51` 写 `rate_limited` | 实际返回 `auth_rate_limited`（`src/api/mod.rs:304-309`）。其余描述正确（5 次/60 秒、`Retry-After: 60`） |
| **PWA 移动端描述不存在** | `docs/pwa.md:34`"将六个快捷入口排为三列" | `partials/nav.html:125` 是 `flex gap-2 overflow-x-auto` 的横向滚动 8 标签；六个快捷入口只存在于 OS 级 manifest。另 `tests/frontend_pwa.test.js:172-181` 断言的是 **`tailwind/input.css`（源）**而非 `static/styles.css`（产物），与 P0-2 同一盲区 |
| **两个 2026-07-26 评审已过期且无入口** | `docs/code-review-2026-07-26.md`、`docs/frontend-design-review-2026-07-26.md` | 部分结论已失效（如"7 个保存按钮"现在只有 1 个；"CI 缺 build-frontend --check"现已加上）。建议合并为 `docs/reviews/` 下带"已修/未修"列的单一文件，或把仍开放项提取进 roadmap |
| **`docs/upgrade-v1.13.1.md` 不存在但被引用** | `docs/roadmap.md:188` | `docs/upgrade-v*.md` 实际只有 2.4.0–2.7.1 |
| **CHANGELOG 缺 `## Unreleased`** | `CHANGELOG.md` 从前言直跳 `## 2.7.1` | main 上已有 4 个后续提交，其中 `896e5f0` 含用户可见行为变更 |

**文档瘦身建议**：27 个文件可收敛到约 12 个——把 `docs/upgrade-v2.4.0…2.7.1` 七个近重复小文件合并成单个 `docs/upgrades.md`；把 roadmap 拆成"前瞻计划"与 `docs/history/` 归档；`docs/api-contract.md` 的分阶段手写路由表（已漂移，且反向漏掉 34/94 条 spec 路径）改为由 `check-openapi.py --markdown` 生成。

### P3-10 工程化与流程缺口

| 缺口 | 现状 | 建议 |
|---|---|---|
| **工具链未固定** | 无 `rust-toolchain.toml`，CI 用 `dtolnay/rust-toolchain@stable`，`Cargo.toml` 无 `rust-version` | 加 `rust-toolchain.toml`（`channel = "1.97.0"`）+ `rust-version`；`Dockerfile` 的 `rust:1-bookworm` 同样随上游浮动 |
| **无覆盖率度量** | 641 + 120 个测试，但不知覆盖率与趋势 | 引入 `cargo-llvm-cov` 先产出报告（不设硬阈值），再对 P3-2 的模块设增量阈值 |
| **无 `cargo-deny`** | 仅 RustSec advisory 检查 | 补齐**许可证合规**、重复依赖、来源白名单；当前 370 个 crate 无许可证审计 |
| **无 `SECURITY.md` / `CONTRIBUTING.md`** | 均缺失 | 已有完善的 `docs/security-audit.md` 与 `docs/release-workflow.md`，各补一页即可 |
| **无 `deny.toml` / `clippy.toml` / `rustfmt.toml` / `.editorconfig`** | 均缺失 | 低优先；`rustfmt.toml` 可固化当前风格 |
| **edition 仍为 2021** | `Cargo.toml:4` | rustc 1.97 已支持 edition 2024；建议与 P2-1 分开做 |
| **本地磁盘 95% 满** | `/` 用 353G / 剩 22G，其中 `target/debug` = **30G** | 非代码问题但会阻塞构建：`cargo clean --profile dev` 并考虑 `sccache` |
| **`dist/` 残留旧发布物** | 里面是 v2.5.1 的 tar.gz（8MB） | 已 gitignore，清理即可 |

---

## 七、已核实为"设计良好"的部分

以下机制**确实按文档所述工作**，不是纸面承诺。列出来是为了明确基线，避免后续重构误伤。

### 7.1 认证与授权（[实测]）

- 鉴权是**全局中间件**（`src/api/mod.rs:621`），不存在"新加路由忘记加鉴权"的类别性风险；`/health`（`:174`）与 Telegram webhook（`:161`）是仅有的两处显式放行。
- Basic Auth 用常量时间比较（`src/utils/mod.rs:170`，正确处理长度不等），用户名与密码都比对；空密码与默认密码 `change-me` 在 `:207-216` 被显式拒绝。
- 登录失败限流带 `Retry-After: 60`（`:304-314`）；`trust_proxy_headers` 默认关闭且有专门测试。
- 自动化 Token 的 scope 映射是**默认拒绝**结构（`:247-292`）；`/api/settings`、`/api/backups`、`/api/storage`、`/api/update`、`/api/automation-token` 对任何 Token 一律不开放。
- 跨站状态修改被拦截并记脱敏告警（`:164-172`）。
- Telegram webhook 路径密钥 + Header secret **双重**校验、均常量时间比较、且要求 `telegram_bot_mode == "webhook"`（`telegram_bot.rs:228-243`）；路径密钥在日志中统一脱敏（`utils/mod.rs:161-168`）。

### 7.2 持久化（[实测]，逐条对账 README）

| README 承诺 | 结论 |
|---|---|
| temp + fsync + 原子 rename | ✅ `write_file_atomic`（`utils/mod.rs:32-58`）：`create_new`+显式 `mode`(0600) → `write_all` → **`sync_all`** → `rename` → **`sync_parent_dir`（父目录 fsync，`:240-250`）**。**父目录 fsync 是最易漏的一步，这里做了**。异步版 `write_json_atomic_async`（`:64`）把写盘放进 `spawn_blocking` |
| 写入串行、无丢失更新 | ✅ 每个 Store 都有 per-file `save_lock` 覆盖"读快照→保存→replace_memory"，未发现任何未加锁的 read-modify-write。**但无跨进程锁**，见 P1-6 |
| 0600 创建**与**重写 | ✅ `create_new(true).mode(mode)`；重写走新 inode rename 故权限重新施加；加载时 `set_file_mode` 修复历史文件；备份副本同样 0600。仅目录是 0755 |
| 损坏自动隔离 + 诊断持续告警 | ✅ 隔离 + `quarantined_backup_present` 诊断项（`api/diagnostics.rs:467-486`）；但隔离后以空 Store 继续运行，见 P1-6 |
| 每个 Store 都有 `schema_version` 信封 | ⚠️ 6/7：`automation-token.json` 无信封（`StoreKind` 只 6 个变体），见 P1-6 |
| Store 有界、不无界增长 | ✅ `MAX_JOBS=500`、`MAX_ARCHIVED_JOBS=5000`、`MAX_NOTIFICATIONS=300`、`MAX_EVENTS=5000`，加载时也截断。长驻内存结构同样有界：`DedupeKeyCache` 2,000、图片代理双限、Telegram 会话 500、确认 nonce 1,000、`named_lock` 每次获取清理死 `Weak` |

其它：崩溃残留 `.tmp`（含敏感数据）启动时统一清理并告警 ✅；`data/` 已 gitignore，实测 `git ls-files data/` 为空、文件 0600 ✅；`.env.example` 全占位符无真实凭据 ✅。

### 7.3 备份与路径安全（[实测]）

- `safe_relative_path`（`services/backup.rs:813-841`）逐组件校验：拒绝绝对路径、拒绝一切非 `Normal` 组件（覆盖 `..`、根、盘符）、拒绝内嵌分隔符、保留 `backups/` 与 `restart-required.json`；另有重复路径拒绝、符号链接祖先拒绝与 `starts_with` 复检。三个解包点（`:571`、`:644`、`:772`）全部走该函数。**zip-slip 不适用**（不是 zip 解包器，是自定义 JSON 归档 + 逐文件 SHA-256）。
- 恢复不是"直接覆盖运行中的 Store"，而是暂存 + 校验 + **重启时在任何 Store/worker 构造之前应用** + 失败回滚（`:263-316`、`:320-370`、`app.rs:47-48`），并要求精确输入 `RESTORE DATA`。
- 创建成功前会做一次**隔离恢复 + 重新哈希**验证（`:241-244`、`:422-489`）——设计相当扎实。
- 已知不足见 P1-6（明文凭据、成功恢复不清理多余文件、失败回滚掩盖原始错误）。

### 7.4 并发与锁（[实测]，逐项排除）

- **`std::sync` 守卫跨 `.await`：0 处**。`std::sync` 仅用于无 await 的短临界区（TMDB 缓存、鉴权失败计数、正则缓存、指标、日志过滤器、重启状态），其余长驻状态一律 `tokio::sync`。
- **锁中毒 panic：生产代码 0 处**（多处用 `unwrap_or_else(|p| p.into_inner())`）。
- **无无界 channel**（`mpsc::channel(512/64/…)`、`broadcast` 均有界）。
- **锁顺序一致，无死锁**：单订阅检查先取订阅锁再取 url 分享锁，批量路径用非阻塞 `try_lock()`（不参与环路）；分享锁虽跨网络 IO，但所有 HTTP 客户端都有硬超时（`clients/http_pool.rs:6-13`，10/20/30/300 秒）。
- **重叠运行已被阻止**：有效全局守卫是共享的 `batch_check_lock.try_lock()`（`subscription_check.rs:583-588`），第二次批量返回"已在进行中"。无 DST 风险（固定时长 + `chrono::Local` 仅取星期；签到用固定 +08:00）。

### 7.5 Panic 安全：原始计数具有误导性，可达 panic 为 0（[实测] + [深审]）

全库扫描 `.unwrap()` / `.expect(` / `panic!(` / `unreachable!(` / `[0]` / `as <int>`：原始命中约 **968** 处（数字随扫描模式浮动）。剔除测试区域后为 110 生产行，分类为：76 处 `as <int>`（`as` 不 panic）、18 处 `LazyLock` 内硬编码正则 `expect`、7 处有紧邻守卫的索引、3 处不可达的正则编译 `panic!`、3 处常量转换、2 处 tokio 锁、2 处有守卫的 `unwrap`、1 处可证死分支的 `unreachable!()`。

**结论：不存在可被请求或数据触发的生产 panic 路径。** 按原始命中数排名最高的 `download_monitor.rs`（89）、`store/subscription.rs`（65）、`store/notification.rs`（50）**生产命中为 0**——全在各自的 `mod tests` 内。真正需要关注的是 P3-8 那类"靠局部推理才安全"的守卫点。

### 7.6 外呼与 SSRF（[实测]）

图片代理 `api/image_proxy.rs` **天然不构成 SSRF**：URL 由硬编码 `https://image.tmdb.org/t/p/{size}/{file_name}` 拼成（`:89`），host 不可控；`size` 走白名单 `SIZES`（`:40`）；`file_name` 经 `validate_tmdb_image_path`（`:83`）；另有 8MB 上游上限（`:17`、`:101-104`）与 256 条/64MB 有界缓存。

### 7.7 前端（[实测] + [深审]）

- PWA 缓存代次管理正确：`CACHE_VERSION` 与静态资源 `?v=` 均由版本驱动（`service-worker.js:4-6`），有 `SKIP_WAITING`/`clients.claim()` 与多代缓存清理（`:129-134`）；缓存策略为 JS/CSS/HTML network-first 且 **401/403 永不缓存**，与文档一致。
- 静态资源缓存分层合理（`api/mod.rs:520-534`），CSP 含 `frame-ancestors 'none'`、`object-src 'none'`、`base-uri 'self'`（`:500`）。
- **当前无 XSS sink**（见 9.4 负向清单第 11 条）；所有外链经 `safeExternalUrl()`。
- 但存在 P1-9（bfcache）与 P3-4（两处刷新逻辑）等状态同步缺陷。

### 7.8 作业队列本身（[实测] + [深审]）

作业在**执行前**持久化（`add_idempotent` → save → 唤醒信号 → `claim_job` 落 Running 再 spawn）；重试为指数 5s→300s、确定性 ±20% 抖动、最多 3 次、仅限非转存类；毒 payload 干净失败；取消先持久化再 abort；启动恢复在 worker/HTTP 之前同步完成（Queued 重新投递、Running→Failed、push→Canceled、按幂等键去重）；优先级为 3:2:1 公平调度 + 按订阅轮转。批量检查用内存快照 + 三路字段合并（`subscription_check.rs:609-672`、`:1400-1479`）避免逐条写盘，这是真正的亮点。问题集中在 P1-1/P1-2/P1-3 与 P1-8。

---

## 八、建议的更新顺序

**v2.7.2（补丁版，建议立即）**

1. **修 P0-1 调度器 reload**，并补"连续两次 start 不报错"的回归测试——唯一一个用户必然撞到的功能缺陷。
2. **修 P1-6 认证限流顺序**（把封锁判定移到凭据校验之后）——**唯一一个未认证即可触发的拒绝服务**，改动约 5 行，优先级与 P0-1 相当。
3. **重建 CSS + 补 `bg-panel`/`text-text-muted`/`loading-spinner` 等定义**（P0-2）。
4. **加 CSS 类覆盖门禁**（P0-3）——不加则第 3 步会复发。
5. **修 P1-7 scope 通配符**（改为精确匹配）——一行改动，关掉一条隐性提权与审计日志泄露。
6. **刷新 roadmap 指针 / 修正 README 的"同一事务"表述 / 对齐 OpenAPI 散文数量**（P3-9）。

**v2.7.3 或 v2.8.0**

7. **维护模式拦入队**（P1-1，单点修改）+ 备份路径改 `spawn_blocking`（P1-4）+ aria2 目录浏览改 `spawn_blocking`（P1-5）+ aria2 `out` 文件名过 `portable_filename`（S3）+ 密钥响应加 `no-store`（S4）——五项都是小改动、确定性高。
8. `cargo update` 收敛 132 个 crate（P2-1）+ 修 `security-audit.md` 的 quinn-proto 表述（P2-3）+ 加 `rust-toolchain.toml`/`rust-version`/Dependabot（P2-4、P3-10）+ compose 加固（S9）。
9. `overflow-checks = true` + `truncate` 替换 `drain`（P3-6）。
10. 前端三处状态同步修复：bfcache（P1-9）、订阅列表刷新、下载轮询（P3-4）。

**结构性与需要设计的部分**

11. **转存幂等与下载对账**（P1-2、P1-3）——需要产品决策（目标目录预检 vs 意图记录），收益最大。
12. **单实例锁 + 隔离后不静默空跑 + 取消安全性 + schema 版本策略**（P1-8）——持久化健壮性的一组。
13. **去 `web-push`**（P2-2）——独立 PR，可永久移除 RustSec 例外。
14. **给 5 个长驻任务加监督**（P3-7）——先定策略再加新循环。
15. **自更新加签名校验**（S1）——供应链真实性，需要决定签名方案与密钥分发。
16. **拆 `do_check_subscription_with_options`（361 行）**，然后内联测试外移 + God module 拆分（P3-1、9.3）。
17. 补 `file_filter_methods.rs` / `episode.rs` 规则层测试与语料，以及前端 SSE/诊断/drive 破坏性操作测试（P3-2）。
18. MD5 → SHA-256、P3-8 的 `let-else` 改写、无障碍修复、S2/S5/S6/S7 加固、edition 2024 迁移（P3-3、P3-5、P3-8、P3-10、9.4）。

---

## 九、深度审查明细

### 9.1 异步正确性：3 条阻塞调用链，2 条在热路径（[实测]）

对全部 `async fn` 做括号匹配扫描，检查未使用 `spawn_blocking` 的阻塞 API 调用。结果 13 处调用、归为 3 条链：

| 链 | 位置 | 可达性 | 详见 |
|---|---|---|---|
| 备份写入 + 隔离恢复 + 全量 SHA-256 | `services/backup.rs:211-253`、`:372-379` | **HTTP + 两个定时循环** | P1-4 |
| aria2 目录浏览 | `api/drive/aria2.rs:349-368` | **HTTP 请求路径** | P1-5 |
| Store `load()` × 8 | `jobs/store.rs:52`、`store/{subscription:53,settings:94,notification:35,automation_event:45,automation_token:63,telegram_bot:83}`、`app.rs:395` | **仅启动期** | 经核对所有 `.load()` 调用点都在 `AppContext::new` 内（`app.rs:54,61,69,76,82,87,90`），`src/api/` 与 `src/jobs/` 无调用。属冷启动一次性开销，**不必改动**；若要统一规则，`jobs/store.rs:307` 已有正确范例 |

### 9.2 可维护性：`episode.rs` 语料是投入产出比最高的测试改进点（[实测]）

`episode_corpus_tests`（`episode.rs:1178`）通过 `include_str!` 加载 `tests/fixtures/episode_names.json`，方向正确，但**该语料目前只有 12 条**——对全项目误判率最高的模块（v2.7.0、v2.7.1 连续两版修特典年份误判）来说太薄。建议扩成"每条规则至少一个正例 + 一个反例"，使每次误判修复都留下永久护栏。

### 9.3 模块拆分边界（逐个核对 `fn`/`struct` 后给出）

**① `src/services/download_monitor.rs`** — `impl DownloadMonitorService`（`:97-614`，约 520 行）+ 20 个自由函数，含 8 类职责：

| 新模块 | 移入内容 | 约 |
|---|---|---|
| `download_monitor/mod.rs` | 结构体与字段、`new`、`start`、`poll_once`、`aria2_client`、`DownloadBatch` | 170 行 |
| `download_monitor/dedupe.rs` | `DedupeKeyCache` 及 `impl`（`:42-76`）、`download_dedupe_keys`（`:659`）、`MAX_TRACKED_DEDUPE_KEYS`、`failed_download_gids` | 90 行 |
| `download_monitor/messages.rs` | `merged_download_message`（`:703`，70 行）、`batch_records_settled`、`download_completed_title_message`、`download_completed_meta`、`download_failed_title_message`、`download_failed_meta` | 250 行 |
| `download_monitor/history.rs` | `completed_download_already_recorded`、`notification_matches_completed_download`、`subscription_id_for_download_gid`、`download_completed_gids`、`completed_subscription_download_files`、`sync_download_matches_by_file`、`merged_download_already_recorded`、`batch_records_individually_notified` | 220 行 |
| `download_monitor/completion.rs` | `notify_completed_downloads`、`notify_completed_download`、`notify_batch_completed`、`notify_failed_download`、`download_batch_for_task`、`complete_subscription_for_download`（110 行）、`mark_subscription_completed_after_download` | 340 行 |
| `download_monitor/tests.rs` | 把 `:1072-2278`（**1,207 行**）原样外移 | — |

`messages.rs` 与 `history.rs` 都是纯函数（无 `&self`、无 IO），移出后可**脱离 Store 单元测试**，这是当前最缺的一层。

**② `src/services/telegram_bot.rs`** — 单个 `impl` 独占 `:169-1365`（**1,197 行 / 39 个方法**）+ 30 个自由函数：

| 新模块 | 移入内容 | 约 |
|---|---|---|
| `telegram_bot/mod.rs` | 服务结构体、`TelegramBotDependencies`、`new`、DTO 再导出 | 140 行 |
| `telegram_bot/api.rs` | `telegram_request`、`get_updates`、`send_message*`、`edit_message_with_markup`、`answer_callback`、`set_webhook`/`delete_webhook`、`telegram_response_result`、`telegram_parse_error`、`sleep_after_failure` | 230 行 |
| `telegram_bot/dispatch.rs` | `start`、`handle_update*`、`handle_message`（**249 行**）、`handle_callback`（**243 行**）、`run`、`webhook_matches`、`is_authorized`、`valid_*`、`CommandRateState` —— 建议再拆 `dispatch/message.rs` 与 `dispatch/callback.rs` | 600 行 |
| `telegram_bot/commands.rs`（扩充现有） | `command_response`、`parse_command`、`is_write_command`、`bot_action_scope`、`confirmation_prompt`、`confirmation_markup` | 400 行 |
| `telegram_bot/views.rs` | `list_page`、`subscriptions_page`、`jobs_page`、`notifications_page`、`calendar_page`、`page_bounds`、`list_page_markup`、`ListPage` | 260 行 |
| `telegram_bot/format.rs` | `tg_escape`、`one_line`、`short_id`、`timestamp_text`、各 `*_label`、`help_text`、`resolve_unique_id`、`split_message`、`chunk_chars` | 180 行 |
| `telegram_bot/callback_sign.rs` | `telegram_prompt_callback_data`、`verify_prompt_callback_data`、`telegram_callback_signature`、`to_base36`/`from_base36` —— 含 HMAC 与 nonce 绑定，**安全关键，独立成模块便于审计** | 110 行 |
| `telegram_bot/diagnostics.rs` | `TelegramBotDiagnostics`、`diagnostics`、`audits`、`note_*`、`set_*` | 90 行 |

`telegram_bot/menus.rs`（1,024 行，全生产）应再拆为 `menus/mod.rs`（会话存储）、`menus/search.rs`、`menus/source.rs`、`menus/subscription.rs`。

**③ `src/api/update.rs`** — 边界本已清晰，按自然分段切目录即可：

| 新模块 | 移入内容 | 约 |
|---|---|---|
| `update/mod.rs` | 5 个 handler、`routes`、DTO，以及唯一负责组合四者的 `apply_update_inner`（`:346-471`） | 230 行 |
| `update/progress.rs` | `UPDATE_PROGRESS`、`PENDING_RESTART`、`UpdateProgressResetGuard`、`current_update_progress`、`try_begin`、`set_*`、`finish`、`fail`、`store_pending_restart`、`ensure_no_pending_restart` | 200 行 |
| `update/github.rs` | `GithubRelease`/`Asset`、`fetch_latest_release`、`fetch_release_by_tag`、`fetch_releases`、`release_to_response`、`find_asset`，以及版本比较 `normalize_version`/`is_newer_version`/`compare_versions`/`version_parts` | 230 行 |
| `update/runtime.rs` | `detect_runtime`、`online_update_supported*`、`managed_runtime_dir`、`managed_docker_runtime_layout`、`path_is_within`、`directory_is_writable` | 140 行 |
| `update/package.rs` | `download_asset*`、`verify_sha256`、`parse_sha256_checksum`、`checksum_matches_asset_line`、`extract_archive`、`verify_archive_members`、`is_safe_member_path`、`ensure_extracted_inside` | 290 行 |
| `update/install.rs` | `backup_path`、`replace_update_payload*`、`sync_directory`、`find_binary`、`find_static_dir`、`static_payload_is_complete`、`copy_dir_all`、`record_runtime_version`、`prune_update_backups`、`prune_sibling_backups` | 300 行 |

**④ `src/services/subscription_check.rs`** — 先做函数分解，再按职责分目录：

| 新模块 | 移入内容 | 约 |
|---|---|---|
| `subscription_check/mod.rs` | 结构体（含两个 `Weak` 注册表与批量缓存）、`new`、`with_*`、`named_lock`、公开入口、DTO、再导出 | 200 行 |
| `subscription_check/pipeline.rs` | `do_check_subscription_with_options`（`:170-530`）**分解为 4 个阶段函数** + `auto_transfer_disabled_reason`、`reopen_completed_subscription` | — |
| `subscription_check/batch.rs` | `check_all_subscriptions`、`check_due_subscriptions`、`check_subscriptions_internal`、`merge_check_results` | 220 行 |
| `subscription_check/probe.rs` | `probe_share`、`probe_share_uncached`、`detect_share_seasons`、`probe_failure_is_transient`、`mock_probe_result` | 200 行 |
| `subscription_check/persist.rs` | `update_subscription_after_check`（113 行）、`reconcile_completion`、`record_transient_check_failure`、`mark_subscription_invalid` | 250 行 |
| `subscription_check/notify.rs` | `send_update_notification`、`send_completed_notification` | 100 行 |
| `subscription_check/source_switch.rs` | `should_search_source_candidates`、`search_and_save_candidates`、`try_auto_apply_source_candidate`（120 行）、`notify_source_candidates_found` | 200 行 |
| `subscription_check/due.rs` | `subscription_due_for_check` + `due_check_tests` | 110 行 |

**⑤ `src/services/episode.rs`** — 保持 `episode/mod.rs` 作为再导出 facade，使现有约 15 个引用模块**零改动**：

| 新模块 | 移入内容 | 约 |
|---|---|---|
| `episode/mod.rs` | 公开名字的 `pub use` facade | 90 行 |
| `episode/patterns.rs` | 6 张 `LazyLock` 正则表、`EpisodePattern`、`hardcoded_regex`、`cached_regex` | 180 行 |
| `episode/numeric.rs` | `is_likely_explicit_episode_number`、`is_plausible_year`、`special_kind_for`、`is_likely_numeric_fallback_episode`、`numeric_fallback_episode`、`leading_numeric_episode`、`chinese_digit_value`、`parse_chinese_number` | 120 行 |
| `episode/detect.rs` | `detect_episode_explained`（122 行）、`detect_episode`、`detect_episode_with_override`、`is_special_episode_name` | 180 行 |
| `episode/season.rs` | `season_hint_*`、`has_non_current_collection_hint`、`matches_subscription_season*`、`subscription_file_matches_season`、`resolve_file_season` | 110 行 |
| `episode/keys.rs` | `episode_video_key`、`episode_state_key*`、`file_reference`、`progress_file_reference`、`scoped_episode_key`、`parse_episode_key`、`transferred_episode_keys`、`known_episode_keys`、`historical_episode_key`、`rebase_season_progress` | 200 行 |
| `episode/duplicates.rs` | `EpisodeDuplicateCandidate`、`normalize_duplicate_episode_strategy`、`episode_quality_score`、`parse_file_time_score`、`duplicate_candidate_scores`、`is_better_episode_duplicate_candidate`、`split_words`、`match_file` | 110 行 |
| `episode/tests.rs` | 4 个测试模块（含 `episode_corpus_tests`） | — |

### 9.4 安全审查明细 **[深审，关键项已复核]**

**结论：无 Critical / High。** 鉴权、CSRF、密钥处理、路径安全的基线确实扎实（负向清单见本节末）。需要处理的是 2 个 Medium（已升格为 P1-6、P1-7 单独展开）与以下加固项。

#### Medium

**S1 自更新没有真实性校验，只有完整性校验**（`src/api/update.rs`）
资产与 `.sha256` 来自**同一个 GitHub Release**（`:364-367`、`:424-427`），`verify_sha256`（`:704-721`）只能证明"下载未损坏"，不能证明"来自项目维护者"；`asset.browser_download_url` 直接取自 API JSON 且**无 host 白名单**（`:640-702`），reqwest 默认跟随最多 10 次重定向（`clients/http_pool.rs:14-27` 未设 redirect policy）。**攻击路径**：GitHub 账号/仓库/发布流程被攻破，或 TLS 信任被打破 → 攻击者同时提供 payload 与匹配的校验和 → `tar -xzf` 后覆盖运行中的二进制 → 以服务身份执行任意代码。
**修复**：校验**分离签名**（minisign/GPG/sigstore），公钥编译进二进制；并把资产下载 host 固定为 `objects.githubusercontent.com`/`github.com` 且禁用重定向。
*已做对的部分*：成员路径预校验（`:805-843`）、解包后二次 canonicalize（`:888-929`）、`tar` 不走 shell、`--no-same-owner/--no-same-permissions`、暂存式安装 + 回滚（`:1023-1096`）、Docker 下默认关闭（`:555-573`）。

#### Low（加固建议）

| # | 问题 | 证据 | 修复 |
|---|---|---|---|
| S2 | **浏览器推送的 SSRF 过滤可绕过**：`is_private_ip` 未做 `to_ipv4_mapped()`，漏掉 CGNAT 100.64/10；DNS 只在订阅时解析一次，发送时不复检 | `src/api/push.rs:51-84`、`:90-105`；`src/services/push.rs:747-796`（发送在 `:789`） | 用 `to_ipv4_mapped()` 归类 + 补 CGNAT/`0.0.0.0/8`/组播；发送时重新解析并按固定 IP 连接；或直接白名单推送厂商域名。**当前需已认证会话（POST /api/push/browser 不对 Token 开放），故为纵深防御** |
| S3 | **云端文件名未清洗就作为 aria2 `out`**：`info.file_name` 直接传入 `add_uri` → `clients/aria2.rs:405` 原样写进 RPC | `src/clients/quark_save.rs:624-646` → `src/services/subscription_transfer.rs:777`、`src/api/drive/aria2.rs:35`。`portable_filename()` 已存在（`transfer_rule.rs:211-231`）且已用于重命名路径（`subscription_transfer.rs:591`），但**未用于提交路径** | 提交前过一遍 `portable_filename()`，并拒绝空/`.`/`..`。最终安全性目前依赖 aria2 自身的 `out` 清洗 |
| S4 | **明文密钥的响应缺 `Cache-Control: no-store`**：`GET /api/settings/secret/{key}` 返回 `app_password`/`quark_cookie`/`telegram_bot_token`/VAPID 私钥原文；Token 轮换响应返回新 Token | `src/api/settings.rs:1229`、`:676-689`；`src/api/automation_token.rs:28-34`；而 `src/api/mod.rs:519-534` 只为 `/js`、`/icons`、`.css`、`.html`、`/service-worker.js`、`/` 设置缓存头，`/api/*` 全部没有。对照正确范例：`api/backup.rs:38`、`api/diagnostics.rs:177` 都设了 `no-store` | 给 `/api/*`（至少密钥与 Token 端点）加 `Cache-Control: no-store`；更好的做法是把密钥读取改为 POST |
| S5 | **CSRF 在缺失 Origin 与 Sec-Fetch-Site 时失败开放** | `src/api/mod.rs:341-373`（无 Origin 则返回 `false`＝放行）；`:375-380` 覆盖了全部状态变更方法 | 改为失败关闭；或要求变更类请求带自定义头/双提交 Token。**当前实际被挡住**：所有变更 handler 都消费 `Json<T>`，仓库内不存在 form/raw-body 提取器，HTML 表单 POST 会以 415 被拒；且已复核全部 GET 无副作用 |
| S6 | **Telegram webhook 无速率限制，且在验密之前解析 JSON 体** | `src/api/mod.rs:161-163`（早于 CSRF 与限流器 return）；`src/api/telegram.rs:26-43` 先 `Json<TelegramUpdate>`（默认 2MiB 体限制）再 `webhook_matches` | 给该前缀加小体积上限与粗粒度按 IP 限流，置于反序列化之前。**密钥猜测不可行**（128 位路径密钥、常量时间比较、失败一律返回同样的 404），故仅是可被放大的 CPU/带宽消耗 |
| S7 | **无 Host 白名单、无 HSTS** | `src/api/mod.rs:432-437`（`request_host` 信任 Host 头）且 `:356-373` 用它做 CSRF 比较；`:496-509` 设置了 CSP/XFO/nosniff/referrer/permissions 但**没有 `Strict-Transport-Security`** | 校验 Host 白名单、在 TLS 后输出 HSTS、拒绝含外部 authority 的 absolute-form URI。Basic Auth 仍然拦得住，故为加固项 |
| S8 | **`constant_time_eq` 泄露长度**：循环跑 `max(len)` 次并把长度异或并入结果 | `src/utils/mod.rs:170-183`（我已复核实现，等长时内容比较是常量时间） | 先哈希再比较定长摘要，或改用 `subtle::ConstantTimeEq`。**信息性**：网络环境下按长度做时序恢复不现实 |
| S9 | **容器加固缺口**：无 `cap_drop`、`no-new-privileges`、`read_only`、内存上限，且端口发布到所有网卡 | `docker-compose.yml:18-19`；`Dockerfile:65-71` 与 `scripts/docker-entrypoint.sh` 已正确用 gosu 降权到 UID 1000 | 加 `cap_drop: [ALL]`、`security_opt: [no-new-privileges:true]`、`read_only: true` + tmpfs；使用反代时把端口绑到 `127.0.0.1` |

#### 已验证安全（负向清单，避免后续重复审计）

1. **全部路由（含静态文件）都在鉴权之后**：`.fallback_service(serve_static)` 先注册、`.layer(...)` 后应用，而 axum 0.8.9 的 `Router::layer` 明确会包裹 `fallback_router`（crate 源码 `axum-0.8.9/src/routing/mod.rs:303-315`）；已枚举全部 `routes()`，仅 `/health` 与 Telegram webhook 两处前置放行。
2. **webhook 前缀的放行有界**：多余路径段会落到 ServeDir，后者在百分号解码后拒绝 `..`/绝对/前缀组件（tower-http 0.6.11），且 `static/` 下没有 `api/` 子树可泄露。
3. **Basic Auth**：用户名与密码均常量时间比较；空密码与 `change-me` 在比较前即拒绝；401 带 `WWW-Authenticate`；用户名无法被 API 清空。
4. **Bearer Token**：256 位随机、磁盘只存 SHA-256、0600、明文从不持久化（有测试断言磁盘无明文）、撤销与过期即时生效、未登记路径一律默认拒绝、管理类端点显式禁止 Token、Token 从不进日志。
5. **CSRF**：覆盖全部状态变更方法，先判 `Sec-Fetch-Site`，再比较规范化后的 Origin↔Host（拒绝含 userinfo 的 `@` 与 `null` origin）。
6. **日志脱敏**：`redact_sensitive` 应用于每个 `AppError` 的 `Display` 与 `From<reqwest::Error>`，覆盖 query 中的 api_key/token/secret/password/cookie/authorization、敏感头、夸克分享链接；候选探测 URL 经 `redact_url`；webhook 路径密钥在请求日志与 span 中脱敏。**没有任何日志打印设置、请求体、Cookie 或 Authorization 头**。
7. **文件权限与原子性**：所有 Store 0o600（实测 `data/settings.json` 为 `-rw-------`），备份含外拷副本 0o600，临时文件 `create_new(true)+mode()` 后 fsync+rename；仅媒体 NFO/海报为 0o644（有意为之）。
8. **备份/恢复**：非 zip 归档，逐文件 SHA-256+size+base64 校验、schema 校验、256MiB 与文件数上限、重复路径拒绝；`safe_relative_path` 拒绝绝对/`..`/纯 `.`/反斜杠/非 UTF-8/保留位置；恢复时复检 `starts_with(data_dir)` 并拒绝符号链接祖先，写入 0o600；暂存 + 启动前应用 + 预恢复快照 + 回滚。
9. **分享链接不构成 SSRF**：`extract_pwd_id` 只用正则，所有夸克请求指向常量 host；TMDB/豆瓣/图片 host 均为常量；图片代理有 size/文件名/content-type 白名单、8MiB 上限与有界缓存。
10. **无注入**：重命名模板是字面量替换、无 shell/eval，输出经 `portable_filename` 清洗；`regex` 为线性引擎（无 ReDoS）；`Command` 仅用于 `tar`（分离 argv，不走 shell）与 `execve` 重启。
11. **前端无 XSS**：零 `innerHTML`；`x-html` 仅渲染编译期 SVG 图标；用户数据走 `x-text` 或 `:href="safeExternalUrl(...)"`（仅 http/https）；图片经 `remoteImageUrl` 重写到本地代理；CSP 无 `unsafe-inline`。
12. **Telegram Bot**：user+chat 双重白名单 + 仅私聊；按命令的速率限制与冷却；确认绑定 user+chat、一次性、带过期、锁内领取；回调按钮 HMAC 签名并常量时间校验；update/callback 去重；Bot Token 不泄露。
13. **无 CORS 层**；Service Worker 只缓存静态资源，`/api/*` 与 `/health` 为 network-only；全局 `nosniff`、`X-Frame-Options: DENY`、`frame-ancestors 'none'`、`referrer-policy: no-referrer`。
14. **诊断/导出不含密钥**（仅布尔、计数、大小、DNS 状态）且设置了 `no-store`。
15. **依赖**：无 git 源依赖、全部校验和；`rustls 0.23.45`、`quinn-proto 0.11.15`、`event-listener 5.4.2` 均为已修版本；`rsa 0.9.10`（RUSTSEC-2023-0071）经 web-push 引入但 VAPID 走 ES256（`services/push.rs:747-781`），**不存在可达的 RSA 解密路径**，故 `docs/security-audit.md` 的例外理由成立；`ml-dsa 0.1.0-rc.11` 在锁图中但不可达。

### 9.5 对既有评审的一条修正

`docs/code-review-2026-07-26.md:166` 已记录"styles.css ↔ tailwind/ 的新鲜度检查未做"，本次评审确认它**仍未做**，并且已经造成了实际后果（P0-2）。这说明该文档的"留待后续"没有进入 roadmap 的必做项——建议把这类"已知但推迟"的条目统一登记到 roadmap 的 `Q` 质量门并勾选状态，否则它们会沉淀为无声的技术债。
