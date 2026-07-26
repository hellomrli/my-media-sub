# v2.2.14 升级指南

v2.2.14 新增 Docker 持久化运行载荷和 WebUI 在线更新。JSON Store schema 未变化，可直接复用 v2.2.13 的 `data/`。

## Docker Compose

首次从 v2.2.13 或更早版本迁移时，需要把当前 Compose 配置中的业务数据卷与应用运行卷分开：

```yaml
volumes:
  - ./data:/app/data
  - ./runtime:/app/runtime
environment:
  - SELF_UPDATE_ENABLED=true
```

确认配置后执行：

```bash
mkdir -p runtime
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

如果 Compose 文件固定了镜像版本，请先改为 `ghcr.io/hellomrli/my-media-sub:2.2.14`。启动后进入「系统设置 → 维护」，运行方式显示“Docker（可在线更新）”即表示迁移成功。

此后 WebUI 在线更新会校验 Release SHA256，并把二进制与完整 `static/` 作为同一可回滚事务写入 `runtime/`；容器重启或应用载荷相同的镜像重建会保留更新结果。显式拉取到二进制或 WebUI 内容不同的镜像时，镜像载荷仍优先刷新运行目录。

应用在线更新不会更新 Debian、OpenSSL、CA 证书等镜像层组件，仍建议定期执行 `docker compose pull && docker compose up -d`。

## Linux 二进制

可继续在 WebUI 中在线升级。手工升级时必须同时替换发布归档中的 `my-media-sub` 和整个 `static/` 目录；不要只替换二进制。

```bash
VERSION=v2.2.14
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz"
curl -LO "https://github.com/hellomrli/my-media-sub/releases/download/${VERSION}/my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-${VERSION}-linux-x86_64.tar.gz.sha256"
```

## 恢复

Docker 运行载荷损坏时，可以临时设置 `APP_RUNTIME_RESET=true` 并重建容器，让入口脚本从当前镜像恢复二进制和 WebUI；恢复后应移除该变量。`data/` 与 `runtime/` 相互独立，不要因为重建运行载荷而删除业务数据。
