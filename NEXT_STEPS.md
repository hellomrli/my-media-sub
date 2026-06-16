# 📋 my-media-sub 后续开发计划

> 最后更新：2026-06-16 20:10

## 🎯 当前状态

### ✅ 已完成
- **版本**: v0.7.10
- **核心功能**: 订阅管理、自动转存、智能重命名、定时调度 **全部完成**
- **推送功能**: 7 个推送渠道（Telegram、Bark、Server酱、企业微信、WxPusher、Gotify、PushPlus）**已实现**
- **业务事件推送**: 订阅更新、订阅失效、订阅完结、转存完成已接入推送场景开关
- **搜索优化**: 嗅探文件列表、过滤失效链接、搜索历史记录 **已实现**
- **部署**: Docker 镜像已推送到 GHCR
- **安全与配置**: 已补基础 Basic Auth；设置接口已脱敏密钥并支持完整保存
- **运行时配置**: 设置保存后调度器会自动 reload；搜索嗅探/转存会读取最新 Cookie
- **网盘管理**: 目录浏览、创建、删除、重命名接口已对齐前端
- **测试基线**: `cargo check` / `cargo test` / `cargo build` 已通过且无 warning
- **文档**: README、工作进度、构建说明 已更新
- **代码**: 已清理未使用导入/变量；保留型兼容代码已显式标注

### 🚀 快速启动

**本地运行：**
```bash
cd /home/lain/my-media-sub
docker-compose up -d
# 访问: http://localhost:56001
```

**查看日志：**
```bash
docker-compose logs -f
```

**停止服务：**
```bash
docker-compose down
```

## 📝 待实现功能（按优先级）

### 🔔 推送功能
- [x] 实现推送测试 API - `POST /api/push/test`
- [x] 支持 Telegram Bot 推送
- [x] 支持 Bark 推送（iOS）
- [x] 支持 Server酱 推送（微信）
- [x] 支持企业微信机器人
- [x] 支持 WxPusher（微信推送）
- [x] 支持 Gotify
- [x] 支持 PushPlus
- [x] 前端推送配置界面

### 🔍 搜索优化
- [x] 实现"嗅探文件列表"功能
- [x] 实现"过滤失效链接"选项
- [x] 搜索历史记录
- [ ] 多关键词搜索支持（OR/AND 逻辑）

### 📊 功能增强（待实现）
- [x] 自动推送接入订阅更新/失效/转存完成业务事件
- [ ] 转存进度实时显示（WebSocket/SSE）
- [x] 转存历史记录面板
- [ ] 订阅统计和分析图表
- [ ] 批量订阅管理操作
- [ ] 订阅导入/导出功能
- [ ] 对接豆瓣或 TMDB 做订阅内容初步刮削（低优先级）

### 🧾 元数据刮削规划（低优先级，排在核心体验优化之后）
- [ ] 选择数据源策略：优先 TMDB 官方 API，豆瓣作为可选手动补充或兼容字段
- [ ] 为订阅增加 `metadata` 字段：原名、年份、海报、简介、评分、总集数、TMDB/Douban ID
- [ ] 新增手动刮削接口：按订阅标题、年份、媒体类型搜索并绑定元数据
- [ ] 在创建订阅时尝试轻量自动匹配，但不阻塞订阅创建和检查
- [ ] WebUI 增加元数据预览和重新匹配入口
- [ ] 将刮削到的总集数用于 `finish_after_episode` 建议值，不自动覆盖用户规则

### 🎨 UI/UX 改进（待实现）
- [ ] 深色模式支持
- [ ] 移动端优化
- [ ] 键盘快捷键
- [ ] 操作确认对话框

## 🛠️ 开发流程

### 继续开发时：

1. **检查当前版本**
   ```bash
   cd /home/lain/my-media-sub
   git status
   git log --oneline -5
   ```

2. **查看运行状态**
   ```bash
   docker ps | grep my-media-sub
   curl http://localhost:56001/health
   ```

3. **开始新功能开发**
   ```bash
   # 创建新分支（可选）
   git checkout -b feature/push-notifications
   
   # 编辑代码
   # src/api/push.rs - 后端 API
   # static/index.html - 前端界面
   
   # 本地测试
   cargo build --release
   docker build -f Dockerfile.local -t my-media-sub:test .
   docker run -p 56002:56001 my-media-sub:test
   ```

4. **发布新版本**
   ```bash
   # 使用一键脚本
   ./build-and-push.sh v0.8.0
   
   # 或手动步骤（见 README.md）
   ```

## 📚 重要文件位置

- **工作进度**: `.WORK_PROGRESS.md`
- **构建脚本**: `build-and-push.sh`
- **部署配置**: `docker-compose.yml`
- **后端代码**: `src/`
- **前端代码**: `static/index.html`
- **Docker镜像**: `Dockerfile.local` (本地构建)

## 🔗 相关链接

- **GitHub 仓库**: https://github.com/hellomrli/my-media-sub
- **最新 Release**: https://github.com/hellomrli/my-media-sub/releases/tag/v0.7.10
- **Docker 镜像**: ghcr.io/hellomrli/my-media-sub:latest
- **本地访问**: http://localhost:56001

## 💡 开发提示

1. **代码风格**: 项目使用 Rust 1.96+，遵循标准 Rust 规范
2. **前端框架**: Alpine.js + Tailwind CSS（无需 npm）
3. **API 风格**: RESTful，JSON 响应
4. **数据存储**: JSON 文件（在 `data/` 目录）
5. **日志级别**: 通过环境变量 `RUST_LOG=debug` 调整

## 🐛 已知问题

- 部分高级筛选选项未实现（低优先级）

## 📞 需要帮助？

查看以下文档：
- `README.md` - 完整使用说明
- `.WORK_PROGRESS.md` - 详细开发历史
- `DOCKER.md` - Docker 部署指南
- `RUST_MIGRATION_V2.md` - 技术架构文档

---

**祝开发顺利！** 🚀
