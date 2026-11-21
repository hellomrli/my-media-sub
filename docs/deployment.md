# 安装、连接和检查服务

[返回文档入口](index.md) · [首次使用](usage.md)

## 使用 Docker Compose

准备 Docker、Compose v2、curl 和 OpenSSL。首次安装命令见 [README 快速开始](../README.md#快速开始)，不要在已有部署目录重新生成或覆盖 `.env`。

仓库的 Compose 使用 `ghcr.io/hellomrli/my-media-sub:latest`，只向宿主机的 `127.0.0.1:56001` 发布端口。容器入口先修正挂载属主，再降权到 uid/gid `1000`；实际运行需要两个持久化目录：

| 宿主目录 | 容器路径 | 保存内容 |
|---|---|---|
| `./data` | `/app/data` | 设置、订阅、任务、通知、Token 和备份 |
| `./runtime` | `/app/runtime` | 实际运行的二进制、完整 WebUI 和在线更新副本 |

`SERVER_PASSWORD` 是 Compose 必填项。服务端要求至少 12 位且非默认密码。查看本机生成的 `.env` 后使用 `admin` 登录；该文件包含凭据，保持私有。

在安装目录检查状态：

```bash
docker compose ps
docker compose logs --tail=50
curl --fail http://127.0.0.1:56001/health
```

启动完成后，健康检查返回含 `"status":"ok"` 和当前版本的 JSON。更新和备份步骤见 [数据维护](data-maintenance.md) 与 [Docker 在线更新](docker-online-update.md)。

## 连接远程 NAS 或服务器

使用 [HTTPS 反向代理](https-reverse-proxy.md) 提供长期访问入口。临时连接可在自己的电脑运行以下 Bash 命令：

```bash
read -r -p 'SSH 目标（用户名@服务器地址）：' MMS_SSH_TARGET
ssh -N -L 56001:127.0.0.1:56001 "$MMS_SSH_TARGET"
```

保持 SSH 运行，在这台电脑打开 `http://127.0.0.1:56001`。本机 56001 已被占用时，选择另一个本地端口并同步修改浏览器地址。

需要局域网直连时，Compose 可通过 `.env` 的 `BIND_ADDRESS` 改变绑定地址；应按实际网段限制访问。不要直接把管理端口发布到公网。反代后的 `trust_proxy_headers`、Host 白名单及 HSTS 配置见 [集成与设置](integrations.md#配置访问入口)。

## 使用 Linux x86_64 发布包

当前 Release 提供 Linux x86_64 归档及同名 SHA-256 文件，其他平台需要 [从源码构建](development.md)。以下命令用于全新目录，需要 curl、tar、sha256sum 和 OpenSSL：

```bash
MMS_VERSION=2.8.4
curl -fLO "https://github.com/hellomrli/my-media-sub/releases/download/v$MMS_VERSION/my-media-sub-v$MMS_VERSION-linux-x86_64.tar.gz"
curl -fLO "https://github.com/hellomrli/my-media-sub/releases/download/v$MMS_VERSION/my-media-sub-v$MMS_VERSION-linux-x86_64.tar.gz.sha256"
sha256sum -c "my-media-sub-v$MMS_VERSION-linux-x86_64.tar.gz.sha256"
tar -xzf "my-media-sub-v$MMS_VERSION-linux-x86_64.tar.gz"
cd "my-media-sub-v$MMS_VERSION-linux-x86_64"

export SERVER_PASSWORD="$(openssl rand -hex 24)"
printf '管理员密码：%s\n' "$SERVER_PASSWORD"
SERVER_HOST=127.0.0.1 DATA_DIR=./data SERVER_PORT=56001 ./my-media-sub
```

记录刚生成的密码，并为后续启动配置受保护的环境文件或服务管理器配置。独立二进制不会自动读取 `.env`。在另一个终端执行健康检查，再登录设置页。

手工升级时，停服后同时替换程序和**整个 `static/` 目录**，保留原有数据和配置。按 [对应版本升级指南](index.md#升级指南) 核对兼容与回滚条件。

## 排查启动和连接

| 现象 | 检查与处理 |
|---|---|
| Compose 提示缺少密码 | 在安装目录的 `.env` 设置 `SERVER_PASSWORD`；不要保留示例默认值 |
| NAS IP 无法访问 | 默认端口只绑定本机；配置 HTTPS 反代或使用 SSH 转发 |
| 认证失败 | 核对用户名、密码和长度；反代必须传递 Authorization 请求头 |
| 写入或初始化 runtime 失败 | 检查两个卷的路径、权限和空间；运行用户为 uid/gid 1000 |
| Store 损坏导致拒绝启动 | 保留原文件和隔离副本，按 [损坏恢复步骤](data-maintenance.md#恢复损坏的-store) 处理 |
| 更新后仍是旧界面 | 手工部署核对完整静态资源；容器部署核对 runtime 载荷，再按 [PWA 排错](pwa.md#故障排查) 检查缓存 |

健康检查免认证。指标、诊断导出和 API 文档需要相应认证，接口说明见 [API 契约](api-contract.md)。
