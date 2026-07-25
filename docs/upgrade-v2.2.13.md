# v2.2.13 升级指南

Docker Compose 部署：

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

如果 Compose 文件固定了镜像版本，请先把镜像标签改为 `ghcr.io/hellomrli/my-media-sub:2.2.13`，再执行上述命令。

v2.2.13 不修改 JSON Store schema，可直接复用 v2.2.12 的 `data/`。

本版本修复 Docker 部署点击在线升级后返回「服务内部错误」的问题。Docker 容器中的程序文件属于镜像内容，应用不再尝试在容器内替换自身；设置页会禁用在线替换按钮，并提示在宿主机执行 `docker compose pull && docker compose up -d`。

普通 Linux 二进制部署仍可使用在线升级。手动升级二进制时，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。前端资源 URL 带有 `?v=2.2.13`。
