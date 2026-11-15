# My Media Sub 文档入口

[项目首页与 Docker 快速开始](../README.md)

## 安装和第一次使用

| 任务 | 文档 |
|---|---|
| 安装服务、远程连接和排查启动 | [部署指南](deployment.md) |
| 保存第一部电影、订阅第一部剧 | [首次使用](usage.md) |
| 接入夸克、Aria2、TMDB 和通知 | [集成与设置](integrations.md) |
| 选择正片和固定剧集画质 | [电影保存与画质](media-save-quality.md) |
| 匹配标准标题，处理查不到的剧名 | [剧名匹配](title-matching.md) |

## 日常使用和运维

- [媒体日历](media-calendar.md)：排期、时区、推断规则与状态。
- [资源质量与换源](source-quality.md)：候选评分、进度校验及自动换源边界。
- [Telegram Bot](telegram-bot.md)：手机命令、白名单、确认和审计。
- [PWA](pwa.md)：安装、缓存和手机排错。
- [数据维护](data-maintenance.md)：备份、迁移、升级、恢复和损坏隔离。
- [HTTPS 反向代理](https-reverse-proxy.md)：访问入口和安全设置。
- [Docker 在线更新](docker-online-update.md)：运行载荷、镜像优先级、签名和回滚。
- [存储扩展](storage-scaling.md)：JSON 性能基线、容量门槛和迁移决策。

## 自动化和开发

- [自动化 API](automation-api.md)：Token、导出/导入、Job 查询及调用示例。
- [HTTP API 契约](api-contract.md)：响应信封、错误码、SSE 和例外。
- [OpenAPI 机器契约](../static/openapi.json)：接口源数据；运行服务后访问 `/api-docs.html`。
- [自动化事件](automation-events.md)：事件字段、存储和流水线投影。
- [本地开发](development.md)：构建、检查和浏览器回归。
- [架构](architecture.md)：业务、认证、队列与持久化合同。
- [发布流程](release-workflow.md)：版本对齐、CI、标签和实际产物核验。
- [依赖审计](security-audit.md)：RustSec 例外、公告和锁文件策略。
- [路线图](roadmap.md)：唯一持久化计划入口。

## 升级指南

从自己的部署版本选择指南，不要只按最新版本猜测数据兼容性。

| 系列 | 指南 |
|---|---|
| 2.8 | [2.8.2](upgrade-v2.8.2.md) · [2.8.1](upgrade-v2.8.1.md) · [2.8.0](upgrade-v2.8.0.md) |
| 2.7 | [2.7.7](upgrade-v2.7.7.md) · [2.7.6](upgrade-v2.7.6.md) · [2.7.5](upgrade-v2.7.5.md) · [2.7.4](upgrade-v2.7.4.md) · [2.7.2](upgrade-v2.7.2.md) · [2.7.1](upgrade-v2.7.1.md) · [2.7.0](upgrade-v2.7.0.md) |
| 2.6 | [2.6.1](upgrade-v2.6.1.md) · [2.6.0](upgrade-v2.6.0.md) |
| 2.5 / 2.4 | [2.5.1](upgrade-v2.5.1.md) · [2.5.0](upgrade-v2.5.0.md) · [2.4.0](upgrade-v2.4.0.md) |

[CHANGELOG](../CHANGELOG.md) · [历史版本摘要](version-history.md) · [GitHub Releases](https://github.com/hellomrli/my-media-sub/releases)

## 历史评审

以下记录保留评审日期、当时结论与后续修复状态，不能当作当前未修问题列表：

- [代码评审 2026-09-21](code-review-2026-09-21.md)
- [代码评审 2026-07-26](code-review-2026-07-26.md)
- [前端评审 2026-07-26](frontend-design-review-2026-07-26.md)
- [前端评审 2026-11-12](frontend-design-review-2026-11-12.md)
