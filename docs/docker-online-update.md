# Docker 在线更新

新版 Docker 镜像把“不可变镜像内容”和“可在线更新的应用载荷”分开：

```text
镜像只读种子 /opt/my-media-sub
  ├── my-media-sub
  ├── static/
  ├── VERSION
  └── PAYLOAD_ID
          │ 首次启动 / 显式切换镜像应用载荷
          ▼
持久化卷 /app/runtime
  ├── my-media-sub
  ├── static/
  ├── .image-version
  ├── .installed-version
  └── *.bak-* / static.bak-*
```

应用实际以 uid/gid `1000` 从 `/app/runtime/my-media-sub` 运行，静态文件服务读取 `/app/runtime/static`。因此在线更新不修改 Docker 镜像层，而是更新独立持久化卷。

## 启用方式

推荐使用仓库当前的 `docker-compose.yml`：

```yaml
volumes:
  - ./data:/app/data
  - ./runtime:/app/runtime
environment:
  - SELF_UPDATE_ENABLED=true
```

旧容器本身无法把只读镜像布局改造成新布局，因此首次迁移仍需在宿主机执行一次：

```bash
mkdir -p runtime
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

启动后进入「系统设置 → 维护」。运行方式显示“Docker（可在线更新）”即表示以下条件同时成立：

- `SELF_UPDATE_ENABLED=true`；
- 已设置 `APP_RUNTIME_DIR`；
- 当前进程确实从该运行目录启动，而不是直接执行镜像内 `/usr/local/bin`；
- `runtime/` 与 `STATIC_DIR` 对 uid/gid `1000` 可写。

使用 `docker run` 时也应显式挂载运行目录：

```bash
docker run -d \
  --name my-media-sub \
  --restart unless-stopped \
  -p 56001:56001 \
  -v "$(pwd)/data:/app/data" \
  -v "$(pwd)/runtime:/app/runtime" \
  -e SERVER_PASSWORD='replace-with-a-strong-password' \
  ghcr.io/hellomrli/my-media-sub:latest
```

镜像声明了 `/app/runtime` 为 volume；未显式挂载时 Docker 会创建匿名卷，但显式绑定目录更便于检查、备份和迁移。

## 在线更新流程

1. 从 GitHub Release 下载 Linux x86_64 归档和对应 `.sha256`（可选还有 `.minisig`）。
2. 与下载地址相关的两道防线：**只接受 GitHub 资产主机**（`github.com`、
   `objects.githubusercontent.com`、`release-assets.githubusercontent.com`、
   `api.github.com`），并且**逐跳校验重定向目标**。`browser_download_url` 直接来自
   GitHub API 的 JSON，属于外部输入；不校验的话，API 响应被篡改就能把下载引到任意
   地址，而下载内容会被解包并覆盖运行中的二进制。
3. 校验 SHA256，拒绝缺少二进制或核心 WebUI 文件（`index.html`、Manifest、Service Worker、OpenAPI）的发布包。
4. **可选但强烈建议**：校验 minisign 分离签名（见下节）。

## 升级包的真实性校验（minisign）

SHA-256 只能证明「下载没坏」，**不能证明「出自发布方」**——校验和与载荷来自同一个
Release，因此能改 Release 的攻击者可以同时替换两者。真实性只能来自一个不走同一下载
渠道的信任根：把公钥编译进二进制，用它校验发布方用私钥签出的签名。

### 启用

```bash
# 一次性：生成密钥对（CI 用无密码密钥）
minisign -G -p minisign.pub -s minisign.key

# 发布端：把私钥存进仓库 Secret
gh secret set MINISIGN_SECRET_KEY < minisign.key
# 客户端：把公钥编译进二进制（推荐）或设为运行时环境变量
export SELF_UPDATE_PUBLIC_KEY="$(tail -n1 minisign.pub)"
cargo build --release
```

`SELF_UPDATE_PUBLIC_KEY` 接受两种形式：

- minisign `.pub` 文件的**整份内容**或其中那行 base64（42 字节：算法 + key id + 公钥）；
- 直接的 32 字节 base64 裸公钥。

### 行为

| 是否配置公钥 | 是否要求签名 | 说明 |
|---|---|---|
| 未配置 | 不要求 | 与旧版本一致，但每次更新会在日志里打一条 WARN 说明只有完整性校验 |
| 已配置 | **强制** | 缺少 `.minisig`、签名格式错误或验签失败都会中止更新 |

一旦配置了公钥，签名就是**失败关闭**的：发布方忘记签名会让升级被拒绝，而不是静默降级
为只校验 SHA-256。

### 推荐做法

用**编译期**固化公钥（`SELF_UPDATE_PUBLIC_KEY` 参与构建），而不是运行时环境变量：
运行时变量可以被能修改服务配置的攻击者一并改掉，编译进二进制的公钥不能。

> 若使用官方发布的镜像/二进制，发布方是否已签名以 Release 是否附有 `.minisig` 为准；
> 没有该文件就说明当前版本只有完整性校验。
3. 在目标目录旁暂存新二进制和完整静态目录，不直接覆盖正在使用的文件。
4. 备份旧二进制和旧 `static/`，再分别通过同目录 rename 原子切换；任一步失败会恢复旧静态资源。
5. 用户确认重启后，主进程停止接收新请求，等待 JobQueue 完成有界优雅关闭，再用新二进制替换当前进程。
6. 容器本身不退出，端口、挂载和 Docker restart policy 保持不变。

默认各保留最近 3 份二进制和 `static/` 回滚副本，可通过 `SELF_UPDATE_BACKUP_RETENTION` 调整为 1–20。

发布包的校验和与归档来自同一个 GitHub Release。它能发现传输损坏或不匹配文件，但不能替代对 GitHub 仓库、发布权限和管理员账号的保护。

## 与镜像升级的优先级

- 相同 Docker 镜像重启或重建：保留 `runtime/` 中的在线更新版本。
- 显式切换到应用载荷不同的 Docker 镜像：入口脚本根据二进制与完整 `static/` 的内容指纹刷新 `runtime/`。即使 `latest/main` 的 Cargo 版本号未变化，新提交也不会被旧 runtime 遮蔽；固定 tag 的升级和回滚仍然优先。
- `APP_RUNTIME_RESET=true`：强制用当前镜像重新初始化运行载荷。该变量持续存在时每次启动都会重置，不应长期配置。

应用在线更新只替换 my-media-sub 和 WebUI，不会更新 Debian、CA 证书、OpenSSL 等镜像层组件。仍建议定期执行：

```bash
docker compose pull
docker compose up -d
```

## 禁用与恢复

如不希望应用进程拥有持久化可执行文件写权限，可设置：

```dotenv
SELF_UPDATE_ENABLED=false
```

设置页会恢复为宿主机镜像升级提示。完整只读部署还应在初始化后按自己的编排策略把运行载荷只读挂载；这会同时禁止在线更新。

若运行载荷损坏，可保留现场后从当前镜像重建：

```bash
docker compose down
mv runtime "runtime.failed-$(date +%Y%m%d%H%M%S)"
mkdir runtime
docker compose up -d
```

确认 `/health`、订阅数据和设置正常后再决定是否删除旧目录。`data/` 与 `runtime/` 相互独立，重建运行载荷不应删除业务数据；涉及 Store schema 回滚时仍应按对应版本升级指南恢复 `data/` 备份。
