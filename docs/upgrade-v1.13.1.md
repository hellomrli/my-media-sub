# v1.13.1 升级、验证与回滚指南

## 适用范围

本指南用于从 v1.13.0 升级到 v1.13.1。该版本不修改 `schema_version: 1`，不需要离线迁移，但仍应同时替换二进制和完整 `static/`。

## 升级前

1. 通过 WebUI 导出完整备份，或停止实例后复制整个 `DATA_DIR`。
2. 保留 v1.13.0 二进制/镜像与对应 `static/`。
3. 确认没有两个实例同时写同一个 `DATA_DIR`。

## Docker

```bash
docker pull ghcr.io/hellomrli/my-media-sub:1.13.1
docker compose pull
docker compose up -d
docker compose logs --tail=150 my-media-sub
```

也可固定 minor 标签 `ghcr.io/hellomrli/my-media-sub:1.13`；升级完成后应通过镜像 digest 确认已拉取新版本。

## Linux 二进制

```bash
sha256sum -c my-media-sub-v1.13.1-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v1.13.1-linux-x86_64.tar.gz
cd my-media-sub-v1.13.1-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

发布包中的二进制、`static/`、README、CHANGELOG 和 docs 应作为一个整体部署。

## 升级后验证

1. `GET /health` 返回版本 `1.13.1`。
2. 已达到目标集的订阅出现在“已完结”列表；失效订阅仍优先显示为“已失效”。
3. 更新日历中的剧集缩略图可在 TMDB 临时失败后通过刷新恢复。
4. 工作台显示“运行概览”和待处理事项，不再显示旧广告式标题。
5. 浏览器控制台在任务、下载、日历和订阅轮询期间不再出现 `reading 'after'`。
6. PWA 用户看到更新提示后选择“立即更新”，确认缓存版本切换到 v1.13.1。

## 回滚到 v1.13.0

1. 停止 v1.13.1，确保没有后台进程继续写数据。
2. 恢复 v1.13.0 二进制/镜像和完整 v1.13.0 `static/`。
3. 本版本没有 schema 变化，正常情况下可继续使用当前 DATA_DIR；如数据异常则恢复升级前备份。
4. 不要让 v1.13.0 与 v1.13.1 并行写入同一 DATA_DIR。
