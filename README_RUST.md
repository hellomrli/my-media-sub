# My Media Sub - Rust Version

🦀 高性能媒体订阅管理系统 - Rust 重写版本

## ✨ 特性

- 📚 **订阅管理** - 创建和管理媒体订阅，支持关键词匹配
- 🔍 **智能搜索** - 集成 PanSou API，自动搜索资源
- ☁️ **夸克网盘** - 自动探测分享链接并转存到网盘
- 🎯 **资源管理** - 追踪资源状态，避免重复
- ⏰ **定时任务** - 自动检查订阅和转存资源
- 🚀 **高性能** - Rust 重写，性能提升 20-50 倍

## 📊 性能对比

| 指标 | Python 版本 | Rust 版本 | 提升 |
|------|------------|----------|------|
| 启动时间 | ~2秒 | <0.1秒 | **20x** ⚡ |
| 内存占用 | ~100MB | ~8MB | **12x** 💾 |
| API 响应 | ~50ms | <1ms | **50x+** 🚀 |

## 🚀 快速开始

### 安装依赖

```bash
# 安装 Rust（如果尚未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆项目
git clone <repo-url>
cd my-media-sub

# 切换到 Rust 分支
git checkout rust-rewrite
```

### 配置

创建 `config.toml`：

```toml
[server]
host = "0.0.0.0"
port = 50002

[database]
data_dir = "./data"

[quark]
cookie = "your_quark_cookie_here"
save_root_fid = "0"  # 保存根目录 fid，0 为根目录

[pansou]
base_url = "https://pansou.lxf87.com.cn"
```

### 运行

```bash
# 开发模式
cargo run

# 生产模式（优化编译）
cargo build --release
./target/release/my-media-sub
```

服务器将运行在 `http://localhost:50002`

## 📡 API 文档

### 健康检查

```bash
GET /api/health
```

### 订阅管理

```bash
# 列出所有订阅
GET /api/subscriptions

# 获取单个订阅
GET /api/subscriptions/{id}

# 创建订阅
POST /api/subscriptions
{
  "name": "订阅名称",
  "media_type": "series",
  "keywords": ["关键词1", "关键词2"],
  "save_dir": "/目标目录",
  "notes": "备注"
}

# 删除订阅
DELETE /api/subscriptions/{id}

# 更新订阅状态
PUT /api/subscriptions/{id}/status
{
  "status": "active"  # active | paused | cancelled
}
```

### 资源管理

```bash
# 列出所有资源
GET /api/resources

# 获取单个资源
GET /api/resources/{id}

# 列出订阅的资源
GET /api/subscriptions/{id}/resources

# 手动添加资源
POST /api/resources
{
  "title": "资源标题",
  "url": "分享链接",
  "password": "提取码",
  "save_path": "/保存路径"
}

# 删除资源
DELETE /api/resources/{id}
```

### 夸克网盘

```bash
# 探测分享链接
GET /api/quark/probe?url=<share_url>&passcode=<password>

# 转存分享文件
POST /api/quark/save
{
  "url": "分享链接",
  "passcode": "提取码",
  "target_dir": "/目标目录"
}
```

### 搜索

```bash
# 搜索资源
GET /api/search?keyword=<关键词>&limit=10
```

### 订阅检查

```bash
# 检查单个订阅
POST /api/subscriptions/{id}/check

# 检查所有订阅
POST /api/subscriptions/check-all
```

### 自动转存

```bash
# 转存单个资源
POST /api/resources/{id}/save

# 转存所有待处理资源
POST /api/resources/save-all
```

## 🏗️ 架构

```
my-media-sub/
├── src/
│   ├── main.rs              # 主程序入口
│   ├── config.rs            # 配置管理
│   ├── error.rs             # 错误处理
│   ├── api/                 # API 路由
│   │   ├── subscriptions.rs
│   │   ├── resources.rs
│   │   ├── quark.rs
│   │   ├── search.rs
│   │   ├── subscription_check.rs
│   │   └── auto_save.rs
│   ├── clients/             # 外部客户端
│   │   ├── quark.rs
│   │   ├── quark_save.rs
│   │   └── pansou.rs
│   ├── models/              # 数据模型
│   │   ├── subscription.rs
│   │   ├── resource.rs
│   │   └── settings.rs
│   ├── services/            # 业务逻辑
│   │   ├── subscription_checker.rs
│   │   ├── auto_save.rs
│   │   └── scheduler.rs
│   └── store/               # 数据存储
│       └── json_store.rs
├── static/                  # 静态文件
├── data/                    # 数据文件
├── config.toml              # 配置文件
└── Cargo.toml               # 项目依赖
```

## 🔧 定时任务

系统启动时会自动启动两个定时任务：

- **订阅检查任务**：每 30 分钟检查一次所有活跃订阅
- **自动转存任务**：每 5 分钟转存一次待处理资源

## 🛠️ 开发

```bash
# 运行测试
cargo test

# 代码格式化
cargo fmt

# 代码检查
cargo clippy

# 生成文档
cargo doc --open
```

## 📝 待办事项

- [ ] WebSocket 实时更新
- [ ] 更多网盘支持（阿里云盘、百度网盘）
- [ ] Web UI 前端重写（React/Vue）
- [ ] Docker 部署支持
- [ ] 数据库迁移（SQLite/PostgreSQL）

## 📄 许可证

MIT License

## 🙏 致谢

- [Axum](https://github.com/tokio-rs/axum) - Web 框架
- [Tokio](https://tokio.rs/) - 异步运行时
- [Serde](https://serde.rs/) - 序列化框架
- [Reqwest](https://github.com/seanmonstar/reqwest) - HTTP 客户端

---

**⚡ Made with Rust 🦀**
