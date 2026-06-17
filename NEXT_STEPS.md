# my-media-sub 后续开发计划

> 最后更新：2026-06-17

## 当前状态

### 已完成

- **版本**: v0.7.10
- **核心功能**: 订阅管理、自动转存、智能重命名、定时调度已完成
- **推送功能**: 7 个推送渠道（Telegram、Bark、Server酱、企业微信、WxPusher、Gotify、PushPlus）已实现
- **业务事件推送**: 订阅更新、订阅失效、订阅完结、转存完成已接入推送场景开关
- **后台推送派发**: 业务推送已改为后台派发，避免阻塞订阅检查和转存流程
- **通知中心**: 系统通知、转存记录和推送记录已保存到通知中心
- **架构整理**: 已引入 `AppContext` 作为组合根，API、调度器和服务复用同一套依赖实例
- **异步任务基础**: 已新增 `jobs/` 模块、任务状态存储、后台 worker，手动转存和订阅自动转存已迁移为后台任务
- **实时进度**: 已提供 `/api/jobs/events` SSE 事件流，WebUI 转存历史页会自动更新任务进度
- **元数据基础**: 已接入 TMDB 搜索、订阅创建时的元数据绑定，以及已有订阅的后台元数据刮削任务
- **搜索优化**: 嗅探文件列表、过滤失效链接、搜索历史记录已实现
- **部署**: Docker 镜像已推送到 GHCR
- **安全与配置**: 已补基础 Basic Auth；设置接口已脱敏密钥并支持完整保存
- **运行时配置**: 设置保存后调度器会自动 reload；搜索嗅探/转存会读取最新 Cookie
- **网盘管理**: 目录浏览、创建、删除、重命名接口已对齐前端
- **测试基线**: `cargo fmt` / `cargo check` / `cargo test` / `cargo clippy` 已纳入常规验证
- **文档**: README、架构文档、后续计划已更新

## 快速启动

本地运行：

```bash
cd /home/lain/my-media-sub
docker compose up -d
```

访问：

```text
http://localhost:56001
```

查看日志：

```bash
docker compose logs -f
```

停止服务：

```bash
docker compose down
```

## 待实现功能（按优先级）

### 核心稳定性

- [x] 自动转存后等待夸克文件实际可见再执行重命名
- [ ] 为夸克保存/列目录引入可 mock 客户端，补齐自动重命名时序回归测试
- [x] 改进长任务取消语义，支持元数据刮削和转存任务在循环阶段感知取消
- [ ] 增加真实数据兼容测试覆盖 `jobs.json` 中新增任务类型

### 推送功能

- [x] 实现推送测试 API - `POST /api/push/test`
- [x] 支持 Telegram Bot 推送
- [x] 支持 Bark 推送（iOS）
- [x] 支持 Server酱 推送（微信）
- [x] 支持企业微信机器人
- [x] 支持 WxPusher（微信推送）
- [x] 支持 Gotify
- [x] 支持 PushPlus
- [x] 前端推送配置界面
- [x] 推送结果保存到通知中心
- [x] 业务推送后台派发，避免阻塞订阅检查和转存
- [ ] 每个渠道独立测试按钮
- [ ] 推送失败原因脱敏记录
- [ ] 推送失败重试和退避策略
- [ ] 可选升级为持久化 `push.dispatch` job

### 搜索优化

- [x] 实现“嗅探文件列表”功能
- [x] 实现“过滤失效链接”选项
- [x] 搜索历史记录
- [ ] 多关键词搜索支持（OR/AND 逻辑）
- [ ] 搜索结果排序策略可配置

### 功能增强

- [x] 自动推送接入订阅更新/失效/转存完成业务事件
- [x] 转存历史记录面板
- [x] 异步任务队列和任务状态存储
- [x] 转存进度实时显示（SSE）
- [x] 订阅元数据后台刮削
- [ ] 订阅统计和分析图表
- [ ] 批量订阅管理操作
- [ ] 订阅导入/导出功能

### 架构优化（进行中）

- [x] 引入应用级组合根 `AppContext`，统一初始化 stores、clients、services、scheduler
- [x] 订阅 API 复用全局 `SubscriptionCheckService` / `SubscriptionTransferService`
- [x] 更新架构文档，明确模块边界和异步 worker 演进路线
- [x] 新增 `jobs/` 模块：定义任务模型、任务状态、任务存储和 worker loop
- [x] 将手动转存迁移为后台 job
- [x] 将订阅自动转存迁移为后台 job
- [x] 为任务进度提供 SSE API
- [x] 将元数据刮削接入后台 job
- [ ] 将 `static/index.html` 拆分为更可维护的前端模块（保持无构建或引入轻量构建需再评估）
- [ ] 抽象外部客户端 trait，降低业务服务对 HTTP 实现的直接依赖
- [ ] 统一长任务进度事件和通知事件的记录模型

### 元数据刮削

- [x] 选择数据源策略：优先 TMDB 官方 API，豆瓣作为后续可选
- [x] 为订阅增加 `metadata` 字段：原名、年份、海报、简介、评分、TMDB ID
- [x] 新增手动刮削接口：按订阅标题、媒体类型搜索并绑定元数据
- [x] 在创建订阅时尝试轻量自动匹配，但不阻塞订阅检查
- [x] WebUI 增加元数据预览和重新匹配入口
- [x] 批量刮削已有订阅元数据，并通过后台任务展示进度
- [ ] 增加年份、别名、季度等更精细的匹配评分
- [ ] 对接豆瓣作为可选数据源或手动补充字段
- [ ] 将刮削到的总集数用于 `finish_after_episode` 建议值，不自动覆盖用户规则
- [ ] 为缺失元数据的订阅增加可选定期补刮削

### UI/UX 改进

- [x] 重新设计 WebUI 主要页面布局
- [x] 通知中心页面
- [x] 任务进度面板和 SSE 自动刷新
- [x] 订阅卡片显示元数据概要
- [ ] 更细的元数据刮削任务状态入口
- [ ] 推送状态显示（成功/失败图标）
- [ ] 批量操作确认与反馈优化
- [ ] 移动端细节回归检查
- [ ] 键盘快捷键

### 持久化与迁移

- [x] 保持 JSON 数据兼容测试
- [ ] 为新增任务类型补真实 `jobs.json` 兼容样例
- [ ] 当 JSON 写入竞争明显时，评估 SQLite 或其他嵌入式存储
- [ ] 增加数据文件备份和恢复说明

## 开发流程

继续开发时：

1. 检查当前版本

   ```bash
   cd /home/lain/my-media-sub
   git status
   git log --oneline -5
   ```

2. 查看运行状态

   ```bash
   docker ps | grep my-media-sub
   curl http://localhost:56001/health
   ```

3. 本地验证

   ```bash
   cargo fmt --all
   cargo check --locked
   cargo test --locked
   cargo clippy --locked -- -D warnings
   ```

## 重要文件位置

- **后端代码**: `src/`
- **前端代码**: `static/index.html`
- **架构文档**: `docs/architecture.md`
- **部署配置**: `docker-compose.yml`
- **通知/推送服务**: `src/services/notification.rs`、`src/services/push.rs`
- **后台任务**: `src/jobs/`

## 相关链接

- **GitHub 仓库**: https://github.com/hellomrli/my-media-sub
- **最新 Release**: https://github.com/hellomrli/my-media-sub/releases/tag/v0.7.10
- **Docker 镜像**: `ghcr.io/hellomrli/my-media-sub:latest`

## 已知问题

- 部分高级筛选选项未实现（低优先级）
- 推送后台派发目前是 best-effort，服务退出时不做持久化恢复
- 元数据匹配还只做基础评分，重名或同年多版本内容需要人工确认
