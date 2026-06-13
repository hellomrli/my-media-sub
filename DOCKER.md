# Docker 部署指南

## 📦 Docker 镜像构建

### 方法 1: 使用构建脚本

```bash
./build-docker.sh
```

### 方法 2: 手动构建

```bash
docker build -t my-media-sub:rust-v0.6.0 .
```

## 🚀 运行方式

### 方法 1: 使用 docker run

```bash
docker run -d \
  --name my-media-sub \
  -p 56001:56001 \
  -v $(pwd)/data:/app/data \
  -e SERVER_USERNAME=admin \
  -e SERVER_PASSWORD=your-password \
  my-media-sub:rust-v0.6.0
```

### 方法 2: 使用 docker-compose（推荐）

1. 编辑 `docker-compose.yml`，配置环境变量
2. 启动服务：

```bash
docker-compose up -d
```

### 方法 3: 使用预构建镜像（如果已推送到仓库）

```bash
docker pull your-registry/my-media-sub:rust-v0.6.0
docker run -d -p 56001:56001 -v $(pwd)/data:/app/data your-registry/my-media-sub:rust-v0.6.0
```

## 🔧 环境变量配置

### 服务器配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `56001` | 监听端口 |
| `SERVER_USERNAME` | `admin` | HTTP Basic Auth 用户名 |
| `SERVER_PASSWORD` | `change-me` | HTTP Basic Auth 密码 |
| `DATA_DIR` | `/app/data` | 数据目录 |

### 夸克配置

| 变量 | 说明 |
|------|------|
| `QUARK_COOKIE` | 夸克网盘 Cookie |

### 推送配置

| 变量 | 说明 |
|------|------|
| `WECOM_BOT_URL` | 企业微信机器人 Webhook URL |
| `WXPUSHER_APP_TOKEN` | WxPusher Token |
| `WXPUSHER_UIDS` | WxPusher UIDs（逗号分隔）|
| `TELEGRAM_BOT_TOKEN` | Telegram Bot Token |
| `TELEGRAM_CHAT_ID` | Telegram Chat ID |
| `BARK_URL` | Bark 推送 URL |
| `GOTIFY_URL` | Gotify 服务器 URL |
| `GOTIFY_TOKEN` | Gotify Token |
| `PUSHPLUS_TOKEN` | PushPlus Token |
| `SERVERCHAN_KEY` | Server酱 Key |

## 📊 管理命令

### 查看日志

```bash
# docker run 方式
docker logs -f my-media-sub

# docker-compose 方式
docker-compose logs -f
```

### 重启服务

```bash
# docker run 方式
docker restart my-media-sub

# docker-compose 方式
docker-compose restart
```

### 停止服务

```bash
# docker run 方式
docker stop my-media-sub

# docker-compose 方式
docker-compose down
```

### 进入容器

```bash
docker exec -it my-media-sub bash
```

## 🏥 健康检查

容器内置健康检查，每 30 秒检查一次：

```bash
# 手动检查
curl http://localhost:56001/health
```

查看健康状态：

```bash
docker inspect --format='{{.State.Health.Status}}' my-media-sub
```

## 🔄 数据持久化

数据目录挂载到 `./data`：

```
./data/
├── subscriptions.json    # 订阅数据
├── settings.json        # 设置
└── notifications.json   # 通知历史
```

## 🛠️ 故障排查

### 1. 容器无法启动

```bash
# 查看日志
docker logs my-media-sub

# 检查容器状态
docker ps -a | grep my-media-sub
```

### 2. 端口冲突

修改 docker-compose.yml 或 docker run 命令中的端口映射：

```yaml
ports:
  - "56002:56001"  # 修改主机端口
```

### 3. 数据丢失

确保正确挂载了数据卷：

```bash
docker inspect my-media-sub | grep Mounts -A 10
```

## 📦 镜像信息

- **基础镜像**: rust:1.80-slim (构建) + debian:bookworm-slim (运行)
- **镜像大小**: ~150MB (多阶段构建优化)
- **架构**: linux/amd64

## 🌐 网络配置

如需与其他容器通信，可以使用自定义网络：

```bash
docker network create my-media-network
docker run -d --network my-media-network --name my-media-sub ...
```

## 🔐 安全建议

1. **修改默认密码**：务必修改 `SERVER_PASSWORD`
2. **使用 HTTPS**：在生产环境使用反向代理（Nginx/Traefik）
3. **限制网络访问**：使用防火墙规则限制访问
4. **定期更新**：及时更新镜像版本

## 📚 更多信息

- API 文档：访问 `http://localhost:56001/health`
- 项目仓库：https://github.com/hellomrli/my-media-sub
