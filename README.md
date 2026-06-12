# 🎬 My Media Sub

一个优雅的媒体订阅管理工具，自动追踪、转存和管理你的影视资源。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Python 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![FastAPI](https://img.shields.io/badge/FastAPI-0.100+-green.svg)](https://fastapi.tiangolo.com/)

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
- **简单部署** - Docker / Python 一键启动
- **API 文档** - FastAPI 自动生成文档
- **代码清晰** - 类型注解，文档完善

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

### 📁 网盘文件管理 ✨ NEW
- **批量操作** - 多选文件进行批量删除、移动、复制
- **文件管理** - 移动、复制、重命名、删除文件和文件夹
- **排序筛选** - 按名称/大小/时间排序，按类型筛选
- **下载功能** - 直接下载到本地或发送到 Aria2
- **文件详情** - 显示文件大小、修改时间、类型图标

### 📱 响应式设计 ✨ NEW
- **移动端优化** - 完美适配 iPhone、Android 手机
- **平板支持** - iPad、Android 平板横竖屏优化
- **触摸友好** - 增大点击区域，优化触摸体验
- **PWA 就绪** - 支持添加到主屏幕
- **可访问性** - 支持减少动画、高对比度模式

### 📡 多种通知
- **企业微信** - 支持企业微信机器人推送
- **WxPusher** - 微信消息推送
- **Telegram** - Telegram Bot 通知（v0.4.0+）

## 🚀 快速开始

### 环境要求
- Python 3.11+
- 2GB+ 内存
- 500MB+ 磁盘空间

### 安装部署

```bash
# 1. 克隆项目
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub

# 2. 创建虚拟环境
python3 -m venv venv
source venv/bin/activate  # Windows: venv\Scripts\activate

# 3. 安装依赖
pip install -r requirements.txt

# 4. 配置环境变量（可选）
cp .env.example .env
# 编辑 .env 文件配置夸克 Cookie、通知方式等

# 5. 启动服务
uvicorn src.app:app --host 0.0.0.0 --port 8787
```

### Docker 部署

```bash
docker run -d \
  --name my-media-sub \
  -p 8787:8787 \
  -v $(pwd)/data:/app/data \
  -e QUARK_COOKIE="your_cookie_here" \
  hellomrli/my-media-sub:latest
```

## 📖 使用指南

### 1️⃣ 初始配置

访问 `http://localhost:8787`，进入 **⚙️ 系统设置**：

1. **基础设置** - 设置用户名密码（默认 admin/change-me）
2. **夸克配置** - 配置夸克 Cookie 和分类目录
3. **自定义分类** - 点击 "+ 添加" 创建你的分类

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

## 🔧 高级功能

### 定时任务
在 **⚙️ 系统设置** 中启用订阅调度器，设置检查间隔（默认 60 分钟）。

### Aria2 下载
配置 Aria2 RPC 地址后，支持将资源发送到 Aria2 下载。

### NAS 同步
配置挂载目录和目标目录，自动同步夸克网盘内容到本地 NAS。

## 📂 目录结构

```
my-media-sub/
├── src/
│   ├── api/              # API 路由
│   ├── clients/          # 第三方客户端（夸克、泛搜等）
│   ├── services/         # 业务逻辑
│   ├── stores/           # 数据存储
│   └── utils/            # 工具函数
├── static/               # 前端静态文件
├── data/                 # 数据目录
│   ├── settings.json     # 配置文件
│   ├── subscriptions.json # 订阅数据
│   └── notifications.json # 通知记录
├── docs/                 # 文档
└── requirements.txt      # Python 依赖
```

## 🔐 安全建议

1. **修改默认密码** - 首次使用务必修改默认密码
2. **保护 Cookie** - 夸克 Cookie 具有完整账号权限，请妥善保管
3. **内网访问** - 建议仅在内网使用，或通过 VPN/反向代理暴露
4. **定期备份** - 定期备份 `data/` 目录

## 🛠️ 开发

### 本地开发

```bash
# 安装开发依赖
pip install -r requirements.txt

# 启动开发服务器（热重载）
uvicorn src.app:app --reload --port 8787

# 代码格式化
ruff check --fix .

# 类型检查
mypy src/
```

### 运行测试

```bash
python test_improvements.py
```

## 📝 更新日志

### v0.5.2 (2026-06-12)
- ✨ **网盘完整文件管理**
  - 批量删除、移动、复制文件
  - 单文件移动、复制、重命名、删除
  - 按名称/大小/时间排序
  - 按类型筛选（全部/文件夹/视频/图片/文档/其他）
  - 直接下载或发送到 Aria2
  - 显示文件大小、修改时间、类型图标
- 📱 **WebUI 响应式设计和移动端适配**
  - 完美适配 iPhone、Android 手机（375px - 640px）
  - iPad、Android 平板优化（横竖屏）
  - 触摸设备优化（增大点击区域、触摸反馈）
  - iOS 刘海屏安全区域适配
  - 6+ 响应式断点覆盖所有设备
  - PWA 就绪（支持添加到主屏幕）
  - 可访问性支持（减少动画、高对比度、大字体）
- 📊 **推送服务优化**
  - 推送异步化，3.5x 性能提升
  - WebUI 推送历史显示（最近 20 条记录）
  - 推送统计（总数/成功/失败/成功率）
- 📝 完整的功能文档和报告

### v0.5.0 (2026-06-12)
- ✨ 新增手动订阅功能
- ✨ 新增自定义分类管理
- 🐛 修复前端 JS 缓存问题
- 📝 优化 README 文档

### v0.4.0 (2026-06-10)
- ✨ 异步客户端优化
- ✨ 任务队列系统
- ✨ SQLite 数据库支持
- ✨ Telegram 通知
- 🚀 性能提升 3-5x

查看完整更新日志：[docs/v0.4.0-release-notes.md](docs/v0.4.0-release-notes.md)

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

MIT License

## 🙏 致谢

- [FastAPI](https://fastapi.tiangolo.com/) - 现代化 Python Web 框架
- [泛搜](https://pansou.fun/) - 网盘资源搜索
- [夸克网盘](https://pan.quark.cn/) - 资源存储

---

**⭐ 如果这个项目对你有帮助，欢迎 Star！**
