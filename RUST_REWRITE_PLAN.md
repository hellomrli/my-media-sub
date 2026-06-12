# Rust Rewrite Plan - my-media-sub

## 🎯 项目目标

用 Rust 完全重写 my-media-sub，保持所有功能，大幅提升性能。

---

## 📊 当前架构分析

### Python 版本
```
my-media-sub/
├── src/
│   ├── api/              # FastAPI 路由
│   ├── services/         # 业务逻辑
│   ├── clients/          # 外部 API 客户端
│   ├── stores/           # 数据存储（JSON）
│   └── schemas/          # Pydantic 模型
├── static/               # 前端资源
└── data/                 # 运行时数据
```

### 核心功能模块
1. **夸克网盘客户端** - Cookie 认证、文件管理、转存
2. **订阅管理** - 增删改查、自动检查更新
3. **资源搜索** - PanSou 集成（91+ 插件）
4. **推送服务** - 7 种推送渠道
5. **WebUI** - HTML/CSS/JavaScript

---

## 🦀 Rust 技术栈选择

### Web 框架
- **Axum** ⭐⭐⭐⭐⭐
  - 基于 Tokio，性能极佳
  - 类型安全
  - 中间件支持好
  - 活跃开发

备选：Actix-web（更成熟但学习曲线陡）

### HTTP 客户端
- **reqwest** ⭐⭐⭐⭐⭐
  - 异步 HTTP 客户端
  - 支持 Cookie、代理、重试
  - 生态成熟

### 异步运行时
- **Tokio** ⭐⭐⭐⭐⭐
  - Rust 标准异步运行时
  - 高性能
  - 生态完善

### JSON 序列化
- **serde + serde_json** ⭐⭐⭐⭐⭐
  - Rust 标准序列化库
  - 类型安全
  - 性能优秀

### 数据存储
- **sled** ⭐⭐⭐⭐ - 嵌入式 K-V 数据库
- 或 **JSON 文件** ⭐⭐⭐ - 保持兼容性

### 日志
- **tracing + tracing-subscriber** ⭐⭐⭐⭐⭐
  - 结构化日志
  - 性能追踪

### HTML 模板
- **askama** ⭐⭐⭐⭐⭐
  - 编译时模板
  - 类型安全
  - 高性能

---

## 📁 Rust 项目结构

```
my-media-sub-rust/
├── Cargo.toml                  # 项目配置
├── src/
│   ├── main.rs                 # 入口
│   ├── config.rs               # 配置管理
│   ├── error.rs                # 错误处理
│   │
│   ├── api/                    # HTTP API
│   │   ├── mod.rs
│   │   ├── routes.rs           # 路由定义
│   │   ├── handlers/           # 请求处理
│   │   │   ├── mod.rs
│   │   │   ├── subscription.rs
│   │   │   ├── search.rs
│   │   │   ├── push.rs
│   │   │   ├── quark.rs
│   │   │   └── settings.rs
│   │   └── middleware.rs       # 中间件
│   │
│   ├── models/                 # 数据模型
│   │   ├── mod.rs
│   │   ├── subscription.rs
│   │   ├── resource.rs
│   │   ├── settings.rs
│   │   └── push.rs
│   │
│   ├── services/               # 业务逻辑
│   │   ├── mod.rs
│   │   ├── subscription.rs
│   │   ├── search.rs
│   │   ├── push.rs
│   │   ├── quark.rs
│   │   └── scheduler.rs
│   │
│   ├── clients/                # 外部客户端
│   │   ├── mod.rs
│   │   ├── quark.rs            # 夸克 API
│   │   ├── pansou.rs           # PanSou 集成
│   │   └── push/               # 推送客户端
│   │       ├── mod.rs
│   │       ├── telegram.rs
│   │       ├── bark.rs
│   │       ├── wxpusher.rs
│   │       ├── gotify.rs
│   │       ├── pushplus.rs
│   │       ├── serverchan.rs
│   │       └── wecom.rs
│   │
│   ├── store/                  # 数据存储
│   │   ├── mod.rs
│   │   ├── subscription.rs
│   │   ├── resource.rs
│   │   └── settings.rs
│   │
│   └── utils/                  # 工具函数
│       ├── mod.rs
│       ├── time.rs
│       ├── crypto.rs
│       └── validator.rs
│
├── static/                     # 前端资源（复用 Python 版本）
│   ├── index.html
│   ├── app.js
│   └── style.css
│
├── templates/                  # 模板（如果需要）
│
└── tests/                      # 测试
    ├── integration.rs
    └── unit/
```

---

## 🔄 开发阶段划分

### Phase 1: 基础框架（2-3天）
- [x] 创建 Rust 项目
- [ ] 设置 Axum web 服务器
- [ ] 静态文件服务
- [ ] 基础路由
- [ ] 错误处理
- [ ] 日志系统
- [ ] 配置管理

**输出：** 能启动的 Web 服务器，提供静态文件

---

### Phase 2: 数据模型与存储（1-2天）
- [ ] 定义所有数据结构（serde）
- [ ] 实现 JSON 存储层
- [ ] 数据迁移工具（Python → Rust）
- [ ] Store trait 定义

**输出：** 数据持久化能力

---

### Phase 3: 夸克网盘客户端（3-4天）
- [ ] Cookie 管理
- [ ] 用户信息获取
- [ ] 文件列表
- [ ] 文件搜索
- [ ] 文件保存（转存）
- [ ] 文件管理（删除、重命名、移动、复制）
- [ ] 批量操作
- [ ] 错误重试

**输出：** 完整的夸克网盘操作能力

---

### Phase 4: 订阅管理（2-3天）
- [ ] 订阅 CRUD API
- [ ] 订阅检查逻辑
- [ ] 资源匹配算法
- [ ] 自动转存
- [ ] 订阅状态管理

**输出：** 订阅功能

---

### Phase 5: 搜索功能（1-2天）
- [ ] PanSou Go 二进制集成
- [ ] 子进程调用
- [ ] 结果解析
- [ ] 搜索 API
- [ ] 缓存（可选）

**输出：** 资源搜索功能

---

### Phase 6: 推送服务（2-3天）
- [ ] 推送服务框架
- [ ] Telegram Bot 推送
- [ ] Bark 推送
- [ ] WxPusher 推送
- [ ] Gotify 推送
- [ ] PushPlus 推送
- [ ] Server酱 推送
- [ ] 企业微信推送
- [ ] 场景模板
- [ ] 批量推送
- [ ] 重试机制

**输出：** 7 种推送渠道

---

### Phase 7: 定时任务（1天）
- [ ] 定时器框架
- [ ] 订阅检查调度
- [ ] 每日摘要

**输出：** 自动化任务

---

### Phase 8: API 路由完善（1-2天）
- [ ] 设置 API
- [ ] 订阅 API
- [ ] 搜索 API
- [ ] 推送 API
- [ ] 夸克网盘 API
- [ ] 统计 API

**输出：** 完整的 RESTful API

---

### Phase 9: 测试与优化（2-3天）
- [ ] 单元测试
- [ ] 集成测试
- [ ] 性能测试
- [ ] 错误处理完善
- [ ] 日志优化
- [ ] 文档

**输出：** 稳定可靠的系统

---

### Phase 10: 部署与迁移（1天）
- [ ] 编译优化
- [ ] 二进制打包
- [ ] 数据迁移脚本
- [ ] 部署文档
- [ ] 回滚方案

**输出：** 可部署的 Rust 版本

---

## ⏱️ 总体时间估算

| 阶段 | 工作量 | 备注 |
|------|--------|------|
| Phase 1: 基础框架 | 2-3天 | 搭建基础 |
| Phase 2: 数据模型 | 1-2天 | 数据层 |
| Phase 3: 夸克客户端 | 3-4天 | 最复杂模块 |
| Phase 4: 订阅管理 | 2-3天 | 核心功能 |
| Phase 5: 搜索功能 | 1-2天 | Go 集成 |
| Phase 6: 推送服务 | 2-3天 | 7 种渠道 |
| Phase 7: 定时任务 | 1天 | 调度器 |
| Phase 8: API 完善 | 1-2天 | 路由 |
| Phase 9: 测试优化 | 2-3天 | 质量保证 |
| Phase 10: 部署 | 1天 | 上线 |

**总计：16-24 天**（取决于开发强度）

---

## 📊 性能目标

| 指标 | Python 版本 | Rust 目标 | 提升 |
|------|-------------|-----------|------|
| 内存占用 | ~100MB | <20MB | 5x+ |
| 启动时间 | ~2秒 | <0.2秒 | 10x+ |
| API 响应 | ~50ms | <5ms | 10x+ |
| 并发处理 | ~100 req/s | >5000 req/s | 50x+ |
| CPU 占用 | ~10% | <2% | 5x+ |

---

## 🎯 里程碑

### Milestone 1: Hello Rust (1周)
- [x] 分支创建
- [ ] 基础框架完成
- [ ] 数据模型定义
- [ ] 静态文件服务

### Milestone 2: Core Features (1周)
- [ ] 夸克客户端完成
- [ ] 订阅管理完成
- [ ] 搜索功能完成

### Milestone 3: Full Features (1周)
- [ ] 推送服务完成
- [ ] 定时任务完成
- [ ] API 完善

### Milestone 4: Production Ready (3-5天)
- [ ] 测试完成
- [ ] 文档完成
- [ ] 部署完成

---

## 🚀 立即开始

### 1. 创建 Cargo 项目
```bash
cd ~/my-media-sub
cargo init --name my-media-sub
```

### 2. 添加依赖
编辑 `Cargo.toml`

### 3. 开发第一个 API
实现 `/api/health` 健康检查

---

## 📝 注意事项

1. **保持 Python 版本运行** - Rust 版本开发期间不影响现有服务
2. **渐进式迁移** - 可以先迁移部分功能测试
3. **数据兼容** - 保持 JSON 数据格式兼容
4. **API 兼容** - 前端无需修改
5. **测试充分** - 每个模块都要测试

---

## 🎓 Rust 学习资源

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Axum 文档](https://docs.rs/axum/)
- [Tokio 教程](https://tokio.rs/tokio/tutorial)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)

---

**Let's build it! 🦀**
