# v2.1.0 升级、验证与回滚指南

## 适用范围

本指南用于从 v2.0.0 升级到 v2.1.0。该版本仅重设计 WebUI 外观，不修改后端逻辑、`schema_version: 1`、API 契约或部署默认值，无需任何数据迁移或配置调整。

## 升级前

1. 通过 WebUI 导出完整备份，或停止实例后复制整个 `DATA_DIR`（例行谨慎，本版本不改数据）。
2. 保留 v2.0.0 二进制/镜像与对应 `static/`。

## Docker

```bash
docker pull ghcr.io/hellomrli/my-media-sub:2.1.0
docker compose pull
docker compose up -d
docker compose logs --tail=120 my-media-sub
```

也可固定 minor 标签 `ghcr.io/hellomrli/my-media-sub:2.1`；升级完成后应通过镜像 digest 确认已拉取新版本。

## Linux 二进制

```bash
sha256sum -c my-media-sub-v2.1.0-linux-x86_64.tar.gz.sha256
tar -xzf my-media-sub-v2.1.0-linux-x86_64.tar.gz
cd my-media-sub-v2.1.0-linux-x86_64
SERVER_PASSWORD='replace-with-a-strong-password' ./my-media-sub
```

发布包中的二进制、`static/`、README、CHANGELOG 和 docs 应作为一个整体部署。**务必同时替换整个 `static/`**——本版本的视觉变化全部来自新的 `static/styles.css` 与相关资源，只换二进制不换 `static/` 会看到旧界面。

## 升级后验证

1. `GET /health` 返回版本 `2.1.0`。
2. 打开 WebUI：深色主题为炭黑底 + 琥珀金强调，浅色主题为暖纸白 + 深琥珀。
3. 若仍显示旧的蓝色界面，多为 PWA 缓存：强制刷新（Ctrl/Cmd+Shift+R）或在应用内触发 Service Worker 更新即可；缓存版本已随本版本更新。
4. 逐页确认（工作台、日历、搜索、订阅、网盘、下载、任务、通知、设置）布局与交互与升级前一致。

## 回滚

1. `docker compose down`（或停止二进制进程）。
2. 部署 v2.0.0 的二进制与配套 `static/`，或 `docker pull ghcr.io/hellomrli/my-media-sub:2.0.0` 并 `up -d`。
3. 因未改动数据格式，回滚无需任何数据迁移；恢复旧 `static/` 后界面即回到 v2.0.0 外观。
