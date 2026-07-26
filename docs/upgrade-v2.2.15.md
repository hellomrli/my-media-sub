# v2.2.15 升级指南

Docker Compose 部署：

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

如果 Compose 文件固定了镜像版本，请先把镜像标签改为 `ghcr.io/hellomrli/my-media-sub:2.2.15`，再执行上述命令。

v2.2.15 不修改 JSON Store schema，可直接复用 v2.2.13 或 v2.2.14 的 `data/`，无需任何迁移动作。v2.2.14 已提交但未发布 Release，其内容包含在本版本中，因此从 v2.2.13 直接升级即可。

## 建议顺带做的一件事

如果你自建了 `docker-compose.yml`，可以把新增的日志轮转配置一并抄过去——容器日志只写 stdout，json-file 驱动默认没有上限，长期运行会在宿主机上无界增长：

```yaml
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
```

## 本版本修了什么

**浏览器 Push 开关此前完全不可用。** 前端 PWA 模块缺少一处依赖声明，导致启用和关闭两条路径都会抛 `ReferenceError`，而外层的错误处理把它转成了一句通用的「浏览器 Push 操作失败」提示——看起来像 Push 服务出了问题，实际是代码缺陷。升级后该功能恢复正常；如果你之前尝试开启过浏览器 Push 但一直失败，现在可以重试。

**发布链路补了几道门禁。** `:latest` 镜像此前在 `main` 有推送时就会构建，clippy 挂了、测试红了也照样发布；现在必须 CI 全绿才发，且构建的一定是通过了 CI 的那个提交。浏览器端到端测试此前用 `file://` 渲染页面，前端脚本实际从未执行过，测试长期静默通过；现在改为经 HTTP 渲染真实服务器并断言 Alpine 确实完成了水合。

**清理了约 1130 行前后端死代码**，其中包括一处对外可见的问题：`openapi.json` 曾宣称提供 `GET /strm/quark/{fid}/{file_name}`，但该路由的实现文件从未参与编译，服务器根本不响应它。如果你有脚本依赖 OpenAPI 规范自动生成客户端，该端点会从规范中消失——它本来也从未工作过。

## 二进制部署

手动升级时必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。前端资源 URL 带有 `?v=2.2.15`，Service Worker 缓存代次为 `v2.2.15-dead-code-sweep-1`，浏览器会在下次打开时提示「新版本静态资源已就绪」。

普通 Linux 二进制部署可继续使用 WebUI 的在线升级（「系统设置 → 维护 → 在线更新」），升级器会校验 SHA256 并把二进制与完整 `static/` 作为同一个可回滚事务替换。

## 需要注意

本版本移除了三个未使用的 Rust 依赖并把 `tower` 移到 `dev-dependencies`，`Cargo.lock` 因此有变化。从源码构建请重新执行 `cargo build --release --locked`。

如果你 fork 了本仓库并在 CI 上跑自己的流水线，注意新增了一步 `npx --yes eslint@10.8.0 'static/**/*.js'`，需要构建环境能访问 npm registry。
