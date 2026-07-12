# v2.0.0 升级、验证与回滚指南

## 适用范围

本指南用于从 v1.13.x 升级到 v2.0.0。该版本不修改 `schema_version: 1`，不需要离线迁移，但涉及认证与部署默认值变更，升级后需要确认密码与代理设置。仍应同时替换二进制和完整 `static/`。

## 重要变更（升级前必读）

1. **默认密码被拒绝**：v2.0.0 起，登录不再接受默认密码 `change-me`。升级前请确认已通过 `APP_PASSWORD`/`SERVER_PASSWORD` 环境变量或系统设置设置过真实密码，否则升级后将无法登录。
2. **限流按对端 IP**：默认按连接对端 IP 做登录限流。如果实例部署在反向代理之后且希望按真实客户端 IP 限流，请在系统设置中开启 `trust_proxy_headers`，并确保代理会覆盖 `X-Forwarded-For`。
3. **容器以非 root 运行**：镜像默认使用 uid/gid 1000 的 `app` 用户。入口脚本会在启动时自动把 `DATA_DIR` 的属主修正为该用户，因此从旧的 root 镜像升级无需手动操作；只读挂载或用 compose `user:` 覆盖身份时会自动跳过。
4. **compose 口令来自 .env**：`docker-compose.yml` 现在通过 `${SERVER_PASSWORD:?}` 读取，请在同目录 `.env` 中设置 `SERVER_PASSWORD`（参考 `.env.example`）。

## 升级前

1. 通过 WebUI 导出完整备份，或停止实例后复制整个 `DATA_DIR`。
2. 保留 v1.13.x 二进制/镜像与对应 `static/`。
3. 确认没有两个实例同时写同一个 `DATA_DIR`。
4. 确认已设置非默认管理员密码。

## Docker

```bash
# 在 docker-compose.yml 同目录准备 .env
printf 'SERVER_PASSWORD=replace-with-a-strong-password\nTZ=Asia/Shanghai\n' >> .env

docker pull ghcr.io/hellomrli/my-media-sub:2.0.0
docker compose pull
docker compose up -d
docker compose logs --tail=150 my-media-sub
```

也可固定 minor 标签 `ghcr.io/hellomrli/my-media-sub:2.0`；升级完成后应通过镜像 digest 确认已拉取新版本。

## Linux 二进制

```bash
sha256sum -c my-media-sub-v2.0.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v2.0.0-linux-x86_64.tar.gz
cd my-media-sub-v2.0.0-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

发布包中的二进制、`static/`、README、CHANGELOG 和 docs 应作为一个整体部署。

## 升级后验证

1. `GET /health` 返回版本 `2.0.0`。
2. 使用真实密码可以登录；用 `change-me` 登录被拒绝并在日志出现设置密码的提示。
3. 系统设置中出现 `trust_proxy_headers` 选项，读取设置时密钥显示为固定长度掩码。
4. 提交一个后台任务并取消，确认任务立即结束且并发槽释放；`docker stop` 时日志出现优雅停机记录且任务状态已落盘。
5. 若配置了浏览器推送/Telegram，确认推送正常、慢命令不再卡住 Bot。

## 回滚

1. `docker compose down`（或停止二进制进程）。
2. 恢复升级前保留的 `DATA_DIR` 备份。
3. 部署 v1.13.x 的二进制与配套 `static/`，或 `docker pull ghcr.io/hellomrli/my-media-sub:1.13.1` 并 `up -d`。
4. 因 `schema_version` 未变，v2.0.0 运行期间产生的数据可被 v1.13.x 读取；回滚无需数据迁移。
