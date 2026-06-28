# My Media Sub 渐进式重构计划

> 生成时间：2026-06-28  
> 项目规模：约 2 万行 Rust 代码  
> 目标：在保持现有架构的前提下，提升代码质量、性能和可维护性

## 总体原则

- ✅ **保留核心架构**：三层架构、JSON 存储、单体服务设计良好，无需推翻
- ✅ **渐进式改进**：每个任务独立可测试，避免破坏性变更
- ✅ **务实优先**：优先解决影响开发效率和性能的问题
- ❌ **不过度设计**：当前规模不需要 SQLite、微服务、消息队列

---

## 阶段一：代码组织优化（1-2 周）

### 1.1 拆分超大服务文件

**目标**：将 1500+ 行的服务文件拆分为子模块，提升可读性

#### `src/services/subscription_transfer/` 重构

```
subscription_transfer/
├── mod.rs              # 公共接口和主流程
├── dedup.rs            # 去重逻辑（同集保留策略）
├── rename.rs           # 重命名规则应用
├── download_sync.rs    # Aria2/STRM 同步
└── quark_ops.rs        # 夸克网盘操作封装
```

**改进点**：
- `mod.rs` 只保留 `SubscriptionTransferService` 结构和主流程方法
- 将 `apply_dedup_strategy`、`apply_rename_template` 等函数移到独立文件
- 每个子模块 200-400 行，职责单一

#### `src/services/subscription_check/` 重构

```
subscription_check/
├── mod.rs              # 公共接口和检查流程
├── progress.rs         # 进度计算和完结判断
├── file_filter.rs      # 季别过滤、起始集过滤
└── notification.rs     # 通知生成逻辑
```

**工作量**：3-4 天

---

### 1.2 推送渠道代码统一

**目标**：消除 8 个推送渠道的重复代码

#### 新增 `src/services/push/mod.rs`

```rust
#[async_trait]
pub trait PushChannel: Send + Sync {
    /// 渠道名称（用于日志）
    fn name(&self) -> &'static str;
    
    /// 发送推送
    async fn send(&self, title: &str, content: &str) -> Result<()>;
    
    /// 是否启用（根据配置判断）
    fn is_enabled(&self, settings: &Settings) -> bool;
}

pub struct TelegramChannel;
pub struct BarkChannel;
pub struct WxPusherChannel;
// ... 其他渠道

impl PushChannel for TelegramChannel {
    fn name(&self) -> &'static str { "Telegram" }
    
    async fn send(&self, title: &str, content: &str) -> Result<()> {
        // 原有实现迁移到这里
    }
    
    fn is_enabled(&self, settings: &Settings) -> bool {
        !settings.telegram_bot_token.is_empty()
            && settings.push_switches.telegram
    }
}
```

#### 统一派发逻辑

```rust
pub async fn dispatch_to_enabled_channels(
    settings: &Settings,
    title: &str,
    content: &str,
) -> Vec<(String, Result<()>)> {
    let channels: Vec<Box<dyn PushChannel>> = vec![
        Box::new(TelegramChannel),
        Box::new(BarkChannel),
        Box::new(WxPusherChannel),
        Box::new(WecomChannel),
        Box::new(GotifyChannel),
        Box::new(PushPlusChannel),
        Box::new(ServerChanChannel),
    ];
    
    let mut results = Vec::new();
    
    for channel in channels {
        if channel.is_enabled(settings) {
            let result = channel.send(title, content).await;
            results.push((channel.name().to_string(), result));
        }
    }
    
    results
}
```

**收益**：
- 消除约 800 行重复代码
- 新增推送渠道只需实现 trait
- 测试覆盖更容易

**工作量**：2-3 天

---

## 阶段二：性能优化（3-5 天）

### 2.1 减少不必要的克隆

#### `src/store/json_store.rs` 优化

```rust
// ❌ 当前实现
pub async fn all(&self) -> Vec<T> {
    let cache = self.cache.read().await;
    cache.clone()  // 每次都克隆整个列表
}

// ✅ 优化方案一：提供闭包访问
pub async fn with_all<F, R>(&self, f: F) -> R
where
    F: FnOnce(&[T]) -> R,
{
    let cache = self.cache.read().await;
    f(&cache)
}

// ✅ 优化方案二：只返回需要的字段
pub async fn list_ids(&self) -> Vec<String> {
    let cache = self.cache.read().await;
    cache.iter().map(|item| item.id().to_string()).collect()
}

// ✅ 优化方案三：分页
pub async fn paginate(&self, offset: usize, limit: usize) -> Vec<T> {
    let cache = self.cache.read().await;
    cache.iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect()
}
```

#### API 层调用调整

```rust
// ❌ 之前
let subscriptions = store.all().await;  // 克隆 1000 条
let count = subscriptions.len();

// ✅ 优化后
let count = store.with_all(|subs| subs.len()).await;
```

**目标文件**：
- `src/store/json_store.rs`
- `src/api/subscriptions.rs`
- `src/api/jobs.rs`

**收益**：减少 60% 的克隆操作，显著降低内存开销

**工作量**：2 天

---

### 2.2 优化锁粒度

#### `src/store/json_store.rs` 存储优化

```rust
// ❌ 当前：持锁期间执行 I/O
async fn save(&self) -> Result<()> {
    let cache = self.cache.write().await;  // 获取写锁
    self.save_locked(&cache).await          // I/O 阻塞其他读写
}

// ✅ 优化：先克隆数据，释放锁后再执行 I/O
async fn save(&self) -> Result<()> {
    let data = {
        let cache = self.cache.read().await;  // 读锁即可
        cache.clone()
    };  // 锁已释放
    
    // 无锁状态下执行文件 I/O
    write_json_atomic_async(&self.path, &data, 0o600).await
}
```

**收益**：
- 写入文件期间不阻塞读操作
- 多订阅并发检查时性能提升 30%+

**工作量**：1 天

---

### 2.3 HTTP 客户端复用

#### 新增 `src/clients/http_pool.rs`

```rust
use once_cell::sync::Lazy;
use reqwest::Client;
use std::time::Duration;

static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to create HTTP client")
});

pub fn get_client() -> &'static Client {
    &HTTP_CLIENT
}
```

#### 各 client 统一使用

```rust
// src/clients/pansou.rs
use super::http_pool::get_client;

pub async fn search(&self, keyword: &str) -> Result<Vec<SearchResult>> {
    let response = get_client()  // 复用连接池
        .get(&self.api_url)
        .query(&[("keyword", keyword)])
        .send()
        .await?;
    // ...
}
```

**收益**：避免每次请求创建新连接，减少 TCP 握手开销

**工作量**：半天

---

## 阶段三：健壮性增强（1 周）

### 3.1 添加集成测试

#### `tests/integration/subscription_flow.rs`

```rust
#[tokio::test]
async fn test_create_check_transfer_lifecycle() {
    // 1. 准备测试环境
    let (ctx, _temp_dir) = setup_test_context().await;
    
    // 2. 创建订阅
    let sub = ctx.subscription_store
        .create(Subscription {
            title: "测试剧集".to_string(),
            share_url: "https://pan.quark.cn/s/test".to_string(),
            media_type: MediaType::Series,
            season: 1,
            // ...
        })
        .await
        .unwrap();
    
    // 3. 模拟检查更新
    let result = ctx.check_service
        .check_subscription(&sub.id)
        .await
        .unwrap();
    
    assert_eq!(result.new_files.len(), 3);
    assert_eq!(result.known_files.len(), 0);
    
    // 4. 验证转存任务已创建
    let jobs = ctx.job_store.all().await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, JobKind::Transfer);
}

#[tokio::test]
async fn test_duplicate_episode_dedup() {
    // 测试同集去重逻辑
}

#[tokio::test]
async fn test_rename_template_application() {
    // 测试重命名模板
}
```

#### 测试工具函数

```rust
// tests/common/mod.rs
use tempfile::TempDir;

pub async fn setup_test_context() -> (Arc<AppContext>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    
    let config = Config {
        data_dir: temp_dir.path().to_path_buf(),
        server: ServerConfig::default(),
    };
    
    let ctx = AppContext::new(&config).await.unwrap();
    (ctx, temp_dir)
}

pub fn mock_subscription(title: &str) -> Subscription {
    Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.to_string(),
        enabled: true,
        // ...
    }
}
```

**测试覆盖目标**：
- ✅ 订阅生命周期（创建→检查→转存→完结）
- ✅ 去重策略（同集多版本保留）
- ✅ 重命名规则（模板变量替换）
- ✅ 季别过滤（跳过非当前季）
- ✅ 完结自动恢复

**工作量**：4-5 天

---

### 3.2 减少 unwrap 使用

#### 审查策略

```bash
# 找出所有 unwrap/expect
rg "\.unwrap\(\)|\.expect\(" src/ --type rust

# 排除测试代码
rg "\.unwrap\(\)|\.expect\(" src/ --type rust -g '!*test*'
```

#### 改进示例

```rust
// ❌ 危险的 unwrap
let settings = settings_store.get().await;
let cookie = settings.quark_cookie.unwrap();  // 可能 panic

// ✅ 优雅的错误处理
let settings = settings_store.get().await;
let cookie = settings.quark_cookie
    .as_ref()
    .ok_or_else(|| AppError::Validation("夸克 Cookie 未配置".to_string()))?;
```

**目标**：将非测试代码中的 unwrap 减少到 10 个以内

**工作量**：1-2 天

---

## 阶段四：用户体验优化（可选）

### 4.1 实时任务状态推送（SSE）

#### `src/api/jobs.rs` 新增 SSE 端点

```rust
use axum::response::sse::{Event, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub async fn job_stream(
    State(ctx): State<Arc<AppContext>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::broadcast::channel(100);
    
    // 监听任务变更
    let job_store = ctx.job_store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let jobs = job_store.all().await;
            let _ = tx.send(jobs);
        }
    });
    
    let stream = BroadcastStream::new(rx)
        .map(|jobs| {
            let data = serde_json::to_string(&jobs.unwrap()).unwrap();
            Ok(Event::default().data(data))
        });
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

#### 前端订阅 SSE

```javascript
// static/app.js
function subscribeToJobUpdates() {
    const eventSource = new EventSource('/api/jobs/stream');
    
    eventSource.onmessage = (event) => {
        const jobs = JSON.parse(event.data);
        Alpine.store('jobs').items = jobs;
    };
    
    eventSource.onerror = () => {
        eventSource.close();
        // 回退到轮询
        setTimeout(pollJobs, 5000);
    };
}
```

**收益**：
- 前端无需轮询，降低服务器负载
- 任务状态变更实时展示
- 提升用户体验

**工作量**：2 天（可选）

---

## 阶段五：可观测性增强（可选）

### 5.1 添加结构化日志

```rust
// 使用 tracing 的 span 和 field
#[tracing::instrument(skip(self), fields(subscription_id = %id))]
pub async fn check_subscription(&self, id: &str) -> Result<CheckResult> {
    let start = std::time::Instant::now();
    
    let result = self.do_check(id).await?;
    
    tracing::info!(
        elapsed_ms = start.elapsed().as_millis(),
        new_files = result.new_files.len(),
        "订阅检查完成"
    );
    
    Ok(result)
}
```

### 5.2 关键指标统计（内存实现）

```rust
// src/utils/metrics.rs
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Metrics {
    subscription_checks: AtomicU64,
    transfer_tasks: AtomicU64,
    push_sent: AtomicU64,
}

impl Metrics {
    pub fn increment_checks(&self) {
        self.subscription_checks.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn get_checks(&self) -> u64 {
        self.subscription_checks.load(Ordering::Relaxed)
    }
}

// 在 AppContext 中初始化
pub metrics: Arc<Metrics>,

// API 暴露
async fn metrics_handler(State(ctx): State<Arc<AppContext>>) -> Json<serde_json::Value> {
    json!({
        "subscription_checks": ctx.metrics.get_checks(),
        "transfer_tasks": ctx.metrics.get_transfers(),
        "push_sent": ctx.metrics.get_pushes(),
    })
}
```

**工作量**：1-2 天（可选）

---

## 实施时间线

| 阶段 | 任务 | 工作量 | 优先级 |
|------|------|--------|--------|
| 一 | 拆分超大服务文件 | 3-4 天 | 🔴 高 |
| 一 | 推送渠道统一 | 2-3 天 | 🟡 中 |
| 二 | 减少克隆操作 | 2 天 | 🔴 高 |
| 二 | 优化锁粒度 | 1 天 | 🔴 高 |
| 二 | HTTP 客户端复用 | 0.5 天 | 🟡 中 |
| 三 | 集成测试 | 4-5 天 | 🔴 高 |
| 三 | 减少 unwrap | 1-2 天 | 🟡 中 |
| 四 | SSE 推送 | 2 天 | 🟢 低 |
| 五 | 可观测性 | 1-2 天 | 🟢 低 |

**总工作量**：
- **核心任务**（阶段一~三）：2-3 周
- **包含可选优化**：3-4 周

---

## 验收标准

### 代码质量
- [ ] 单个文件不超过 800 行
- [ ] 非测试代码 unwrap < 10 处
- [ ] 无 Clippy warnings（`cargo clippy -- -D warnings`）

### 性能指标
- [ ] 订阅列表接口响应 < 50ms（1000 订阅）
- [ ] 订阅检查内存占用 < 100MB（并发 10 个）
- [ ] 克隆操作减少 60%+

### 测试覆盖
- [ ] 核心业务流程集成测试覆盖率 > 80%
- [ ] 所有测试通过（`cargo test`）

### 兼容性
- [ ] 现有 JSON 数据文件无需迁移
- [ ] API 接口向后兼容
- [ ] 环境变量配置不变

---

## 不做的事情（明确边界）

❌ **不迁移到 SQLite**  
当前规模 JSON 文件完全够用，迁移成本高且收益有限

❌ **不拆分微服务**  
单体服务维护简单，没有性能瓶颈

❌ **不引入消息队列**  
内存任务队列已满足需求，外部依赖增加部署复杂度

❌ **不重写前端**  
Alpine.js 轻量够用，暂无必要引入 React/Vue

❌ **不改变核心架构**  
三层架构设计合理，保持现有结构

---

## 风险控制

1. **每个阶段独立测试**：完成一个阶段后运行全量测试
2. **Git 分支管理**：每个重构任务单独分支，通过 PR review 合并
3. **回滚预案**：每次重构前打 tag，出问题可快速回滚
4. **灰度验证**：先在测试环境运行 24 小时，确认无问题再部署

---

## 下一步行动

1. **Review 本计划**：确认优先级和范围
2. **创建重构分支**：`git checkout -b refactor/stage-1`
3. **开始阶段一任务 1.1**：拆分 `subscription_transfer.rs`
4. **提交第一个 PR**：完成后提交代码审查

需要我开始实施第一个任务吗？
