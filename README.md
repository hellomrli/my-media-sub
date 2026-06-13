# 🎬 My Media Sub

一个优雅的媒体订阅管理工具，自动追踪、转存和管理你的影视资源。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96+-orange.svg)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/axum-0.8-blue.svg)](https://github.com/tokio-rs/axum)
[![Docker](https://img.shields.io/badge/docker-ready-brightgreen.svg)](https://hub.docker.com/)

> 🦀 **v0.6.0 重大更新**: 完整 Rust 重写，性能提升 3-5 倍，内存占用降低 60%！

## 🚀 版本说明

### 🆕 Rust 版本 (v0.6.0+) - 当前主分支
- **语言**: Rust 1.96+
- **性能**: 启动时间 < 100ms，内存占用 ~20MB
- **并发**: 基于 Tokio 异步运行时，高并发处理能力
- **部署**: Docker 镜像仅 144MB
- **状态**: ✅ 生产就绪

### 📦 Python 版本 (v0.5.x) - 备份分支
- **分支**: [main-python-backup](https://github.com/hellomrli/my-media-sub/tree/main-python-backup)
- **状态**: 🔒 维护模式（仅修复关键 bug）

## 📸 功能展示

### 💻 桌面端
- **资源搜索** - 多平台搜索，一键订阅
- **订阅管理** - 批量管理，灵活过滤
- **网盘浏览** - 文件管理，批量操作
- **系统设置** - 自定义分类，通知配置

### 📱 移动端
- **响应式设计** - 完美适配 iPhone、Android
- **触摸优化** - 增大点击区域，流畅操作
- **PWA 支持** - 添加到主屏幕，像原生 App

### 👨‍💻 开发者友好
- **简单部署** - Docker 一键启动
- **RESTful API** - 标准化接口
- **类型安全** - Rust 静态类型系统

## ✨ 核心特性

### 📺 智能订阅
- **自动追踪** - 定时检查订阅更新，发现新集自动通知
- **手动订阅** - 直接输入网盘链接，自动嗅探文件并创建订阅
- **搜索转订阅** - 搜索资源后一键创建订阅
- **灵活过滤** - 支持关键词包含/排除、正则匹配、质量筛选

### 🗂️ 分类管理
- **默认分类** - 电影、连续剧、动画三大类
- **自定义分类** - 可添加综艺、纪录片、演唱会等任意分类
- **自动分类转存** - 根据媒体类型自动保存到对应目录

### 🔍 资源搜索
- **多平台搜索** - 支持夸克、阿里云、百度等主流网盘
- **链接有效性检测** - 自动过滤失效、需要验证码的链接
- **质量识别** - 自动识别 4K/1080p/720p 等分辨率
- **去重优化** - 智能去重，优先保留高质量资源

### 💾 自动转存
- **夸克网盘** - 自动转存新资源到你的夸克网盘
- **分类目录** - 支持基础目录 + 分类子目录组合
- **批量转存** - 多文件自动批量处理
- **进度跟踪** - 记录已转存文件，避免重复

### 📡 多渠道推送 (7种)
- **企业微信** - 企业微信机器人推送
- **WxPusher** - 微信消息推送
- **Telegram** - Telegram Bot 通知
- **Bark** - iOS Bark 推送
- **Gotify** - 自托管推送服务
- **PushPlus** - 微信推送服务
- **Server酱** - 微信推送服务

## 🚀 快速开始

### 方式 1: Docker Compose (推荐)

```bash
# 1. 克隆项目
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub

# 2. 配置环境变量（可选）
cp docker-compose.yml docker-compose.override.yml
# 编辑 docker-compose.override.yml 配置推送、夸克 Cookie 等

# 3. 启动服务
docker-compose up -d

# 4. 查看日志
docker-compose logs -f

# 5. 访问服务
open http://localhost:56001
```

### 方式 2: Docker Run

```bash
docker run -d \
  --name my-media-sub \
  -p 56001:56001 \
  -v $(pwd)/data:/app/data \
  -e SERVER_PASSWORD=your-password \
  -e QUARK_COOKIE="your_cookie_here" \
  my-media-sub:rust-v0.6.0
```

### 方式 3: 本地编译

```bash
# 1. 安装 Rust (https://rustup.rs/)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 克隆并编译
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub
cargo build --release

# 3. 运行
./target/release/my-media-sub

# 4. 访问服务
open http://localhost:56001
```

## 📖 使用指南

### 环境变量配置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `56001` |
| `SERVER_USERNAME` | HTTP Basic Auth 用户名 | `admin` |
| `SERVER_PASSWORD` | HTTP Basic Auth 密码 | `change-me` |
| `DATA_DIR` | 数据目录 | `./data` |
| `QUARK_COOKIE` | 夸克网盘 Cookie | - |
| `WECOM_BOT_URL` | 企业微信机器人 URL | - |
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token | - |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID | - |

完整配置见 [DOCKER.md](DOCKER.md)

### API 端点

- `GET /health` - 健康检查
- `GET /api/subscriptions` - 获取订阅列表
- `POST /api/subscriptions` - 创建订阅
- `PUT /api/subscriptions/:id` - 更新订阅
- `DELETE /api/subscriptions/:id` - 删除订阅
- `GET /api/settings` - 获取设置
- `POST /api/settings` - 更新设置
- `POST /api/search` - 搜索资源
- `GET /api/notifications` - 获取通知历史

### 1️⃣ 初始配置

访问 `http://localhost:56001`，进入 **⚙️ 系统设置**：

1. **基础设置** - 设置用户名密码（默认 admin/change-me）
2. **夸克配置** - 配置夸克 Cookie 和分类目录
3. **推送配置** - 配置你喜欢的推送渠道

### 2️⃣ 创建订阅

#### 方式一：搜索转订阅
1. 进入 **⌕ 资源搜索** 页面
2. 搜索影视资源（如：某某剧 第一季）
3. 点击搜索结果右侧的 **订阅** 按钮
4. 设置订阅规则并保存

#### 方式二：手动订阅
1. 进入 **◌ 订阅清单** 页面
2. 点击右上角 **手动订阅** 按钮
3. 输入网盘分享链接和密码（如有）
4. 点击 **嗅探文件** 查看内容
5. 确认后点击 **创建订阅**

### 3️⃣ 管理订阅

- **刷新所有** - 手动触发全部订阅更新检查
- **编辑订阅** - 点击订阅项可编辑规则
- **启用/禁用** - 切换订阅状态
- **标记完结** - 完结后不再检查更新

## 🔧 性能对比

| 指标 | Python 版本 | Rust 版本 | 提升 |
|------|------------|----------|------|
| 启动时间 | ~3-5s | ~100ms | **30x** |
| 内存占用 | ~50MB | ~20MB | **60%↓** |
| 请求响应 | ~50ms | ~10ms | **5x** |
| 并发处理 | ~100 req/s | ~500 req/s | **5x** |
| Docker 镜像 | ~800MB | ~144MB | **82%↓** |

## 📚 文档

- [Rust 迁移文档](RUST_MIGRATION_V2.md) - 技术细节和架构说明
- [Docker 部署指南](DOCKER.md) - 详细的 Docker 部署文档
- [API 文档](#api-端点) - RESTful API 接口说明

## 🛠️ 开发

### 运行测试

```bash
cargo test
```

### 构建 Docker 镜像

```bash
# 方式1: 使用本地构建 (推荐)
cargo build --release
docker build -f Dockerfile.local -t my-media-sub:latest .

# 方式2: 多阶段构建 (需要 Rust 1.83+ 镜像)
docker build -t my-media-sub:latest .
```

### 技术栈

- **Web 框架**: [Axum](https://github.com/tokio-rs/axum) 0.8
- **异步运行时**: [Tokio](https://tokio.rs/)
- **HTTP 客户端**: [Reqwest](https://github.com/seanmonstar/reqwest)
- **序列化**: [Serde](https://serde.rs/)
- **正则表达式**: [Regex](https://docs.rs/regex/)
- **日志**: [Tracing](https://github.com/tokio-rs/tracing)

## 🤝 贡献

欢迎贡献代码、报告问题或提出建议！

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

本项目基于 MIT 许可证开源 - 详见 [LICENSE](LICENSE) 文件

## 🙏 致谢

- [Axum](https://github.com/tokio-rs/axum) - 出色的 Web 框架
- [Tokio](https://tokio.rs/) - 强大的异步运行时
- 所有贡献者和用户的支持

## 📞 联系方式

- GitHub Issues: [提交问题](https://github.com/hellomrli/my-media-sub/issues)
- GitHub Discussions: [讨论区](https://github.com/hellomrli/my-media-sub/discussions)

---

**⭐ 如果这个项目对你有帮助，请给一个 Star！**
