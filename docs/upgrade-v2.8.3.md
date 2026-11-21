# v2.8.3 升级指南

[返回文档入口](index.md) · [完整变更](../CHANGELOG.md)

> v2.8.3 仅作为仓库版本记录，未单独发布 tag、Release 或版本镜像。其日历修复已纳入 v2.8.4；实际升级请使用 [v2.8.4 升级指南](upgrade-v2.8.4.md)。下文保留原版本准备记录。

v2.8.3 修复手机上更新日历周视图的显示并整理日历样式，不改变 JSON Store schema 或既有 API 契约，可直接复用 v2.8.2 / v2.8.1 的数据与配置。升级前核对备份，保留 `data/`、`runtime/` 和原 `.env`。

## Docker Compose

在原部署目录执行：

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100
curl --fail http://127.0.0.1:56001/health
```

确认健康检查返回 `"version":"2.8.3"`。固定镜像版本的用户改为 `ghcr.io/hellomrli/my-media-sub:2.8.3`。

拉取镜像并重建容器可同步获取运行层系统补丁；仅执行 WebUI 在线更新会替换程序和静态资源，不会更新镜像基础层。

## 独立二进制与在线更新

从 [v2.8.3 Release](https://github.com/hellomrli/my-media-sub/releases/tag/v2.8.3) 下载 Linux x86_64 归档及同名 SHA-256 文件，校验后停服，替换程序和**完整 `static/` 目录**，保留数据、配置与备份。操作参考 [发布包部署](deployment.md#使用-linux-x86_64-发布包)，在线更新信任配置见 [Docker 在线更新](docker-online-update.md)。

本版页面资源使用 `?v=2.8.3`，PWA 缓存代次为 `v2.8.3-calendar-styles-1`。重新加载旧页面；已安装的 PWA 收到更新横幅后点击“立即更新”。原有主题、订阅视图和工作台布局偏好继续保留。

## 行为变化与核验

1. 确认 `/health` 返回 `2.8.3`，原订阅、任务和设置仍在。
2. 在手机上打开工作台的更新日历周视图：每条排期占满一行，过长的剧名以省略号结尾，日历不超出卡片右缘。
3. 平板和桌面宽度下，日历的周、月、列表视图与 v2.8.2 显示一致。

自动浏览器回归使用本地测试数据，不代替真实夸克保存、Aria2 下载、TMDB 查询或 Web Push 送达验证。

本次发布提供 SHA-256 校验文件；未配置 minisign 签名时没有 `.minisig`，已设置 `SELF_UPDATE_PUBLIC_KEY` 的在线更新客户端会拒绝无签名归档，这些部署可按上面的 Docker Compose 或独立二进制步骤升级。签名与信任配置详见 [Docker 在线更新](docker-online-update.md)。

## 回滚

停服后换回 v2.8.2 镜像或旧二进制与配套 `static/` 即可。不需要数据迁移；同一个数据目录只运行一个实例。
