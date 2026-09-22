# v2.7.2 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.7.2 **不改变数据结构**，但会把 Store 信封的 `schema_version` 升到 v2（见下文第 3 点，影响回滚）。v1 数据会自动迁移，无需手工操作。

## 需要留意的几点

### 1. 自动化 Token 的 scope 收紧了（可能需要重新轮换）

v2.7.1 及更早版本里，scope `read` 被当作通配符：它可以满足**任意**以 `:read`
结尾的 scope，包括 `jobs:read`、`notifications:read`、`subscriptions:read` 和
`diagnostics:read`。这属于与设计意图不符的隐性提权——`/api/telegram/audits`
的 `target` 字段记录的是 Telegram 命令的原始参数，`/subscribe <分享链接> <提取码>`
会把两者都写进审计日志，而这些日志本来不应对「只读日历」的 Token 开放。

现在 scope 要求精确匹配。如果你的自动化脚本使用了一个只勾选 `read` 的 Token，
并且在调用 `/api/jobs`、`/api/notifications`、`/api/subscriptions` 或
`/api/diagnostics`，升级后这些请求会返回 401。处理方式是轮换出一个带足 scope 的
新 Token（明文只在本次响应中返回一次）：

```bash
curl -u admin:"$SERVER_PASSWORD" -X POST http://127.0.0.1:56001/api/automation-token \
  -H 'Content-Type: application/json' \
  -d '{"scopes": ["read", "subscriptions:read", "jobs:read", "notifications:read"], "expires_days": 30}'
```

可用 scope 以 `GET /api/automation-token/scopes` 为准。

### 2. compose 的端口默认只绑本机

`docker-compose.yml` 现在把发布端口绑到 `127.0.0.1`，因为管理面不应直接暴露到
公网（对外请走反向代理，见 [https-reverse-proxy.md](https-reverse-proxy.md)）。

- 如果你的部署**本来就通过反向代理**访问，无需改动。
- 如果你一直用 `http://<宿主IP>:56001` 直连，升级后会连不上。在 `.env` 里加一行
  `BIND_ADDRESS=0.0.0.0` 即可恢复，或改用反向代理。

### 3. JSON Store schema 版本升到 v2（影响回滚）

v2.7.2 把 Store 信封的 `schema_version` 从 `1` 升到 **`2`**。这**不改变任何数据结构**，
升到 v2 只是为了标记"这份数据含有 v1 无法正确理解的语义字段"。

背景：v2.7.0 引入了跳季订阅（`season_list`，例如只订 S1+S3）。v1 的程序读到这样的
记录时，会把它当成**未知字段**并在下一次写入时抹掉，于是订阅被永久降级成连续区间
（S2 被错误转存，而且再升级回来也恢复不了）。

因此：

- **正常升级**：v2.7.2 会自动把 v1 信封迁移并回写为 v2，数据内容不变，无需任何操作。
- **回滚到 v2.2–v2.7.1**：旧版本读到 v2 信封会**拒绝启动**（而不是静默改写数据）。
  这是有意的保护。若确需回滚，请先恢复升级前的备份，或在**确认订阅里没有使用跳季
  语义**的前提下手工把各 `data/*.json` 里的 `"schema_version": 2` 改回 `1`。
- 迁移前会自动写一份**逐字节原始备份**（`data/<store>.json.schema-v1.bak`），即使迁移
  出问题也能还原；同名备份已存在时不会覆盖。
- **回滚时还要处理 `automation-token.json`**：它在 v2.7.2 首次带上 `schema_version`
  信封，而 v2.7.1 及更早版本的读取器不认识信封，会直接报「解析自动化 Token 失败」并
  拒绝启动。回滚前把该文件恢复成升级前的备份，或删掉它（Token 需要重新生成）。

### 4. 损坏的 Store 默认中止启动

此前除设置外的 Store 解析失败时会隔离原文件并**以空数据继续运行**，服务看起来正常但
订阅、任务、通知全部消失。现在默认**中止启动**，日志会给出隔离文件路径与恢复步骤。
确需先把服务拉起来再手工修复时，设置 `ALLOW_QUARANTINE_STARTUP=true`（会以警告级别
提示当前处于降级状态）。

### 5. 数据目录加了单实例锁

启动时对 `DATA_DIR/.lock` 取排他 `flock`，第二个共用同一数据目录的进程会拒绝启动。
锁随进程退出自动释放，崩溃不会留下死锁；若日志提示「已被另一个实例占用」而你确认
没有其它实例（例如 `.lock` 由 NFS 上的旧挂载残留），删掉该文件后重试即可。
`DATA_DIR` 建议放在本地卷或块存储上，`flock` 在 NFS 上的语义取决于服务端实现。

### 6. compose 的根文件系统只读、capability 收紧

`docker-compose.yml` 现在 `read_only: true`（可写的只有 `data`、`runtime` 两个卷与
`/tmp` tmpfs）、`cap_drop: ALL` 并只保留入口脚本需要的 4 个 capability。若你在 compose
里额外挂了需要写入的路径（例如 aria2 的下载目录由本容器写入），需要显式加到 `volumes`。

### 7. HSTS 与 Host 白名单都是显式开关（默认关闭）

- **HSTS**：设置页「高级选项」新增「发送 HSTS 头」，默认关闭。HSTS 按主机名生效、
  不区分端口，同一主机名下若还有其它走纯 HTTP 的端口，开启后浏览器会把它们一并强制
  升级到 https 而打不开。只在该主机名下所有端口都走 HTTPS 时开启。
- **Host 白名单**（`allowed_hosts`）：用于防御 DNS rebinding，默认为空即不校验。保存时
  要求列表必须包含你**当前访问用的 Host**，否则拒绝保存——避免一次拼写错误把自己锁在
  外面。万一仍被锁住（例如换了访问域名），停服后编辑 `data/settings.json` 把
  `"allowed_hosts"` 改回 `[]` 再启动。

### 8. 自更新签名（维护者）

Release 现在可附带 minisign 签名（`.minisig`）。客户端设置 `SELF_UPDATE_PUBLIC_KEY`
后**强制**验签；未设置时行为与旧版本一致（只校验 SHA-256），更新时会打一条 WARN。
维护者启用方式见 [docker-online-update.md](docker-online-update.md)。

## 本版本的其它变更

- 修复设置保存必然报错（调度器 reload 不幂等）、未认证即可触发的认证限流拒绝
  服务、Token scope 隐性提权。
- 修复已发布的样式缺陷：重新编译 `static/styles.css` 并补齐 9 个被引用但从未定义
  的类（含完全不可见的加载动画），同时新增 CI 门禁防止再次漂移。
- 修复维护模式下作业队列无界增长、备份与目录浏览阻塞运行时、前端 bfcache /
  订阅列表 / 下载轮询的状态失同步。
- 完善剧名匹配：日文标题内部的假名不再被当作分隔符截断；中文逗号 / 顿号之后的
  演员与描述被丢弃（`交锋 4K 完结，王凯` → `交锋`）；`DDP2 0`、`H.264`、`AAC2.0`
  这类被点号或空格拆开的音视频标记先粘合再清洗；书名号 `《》「」` 内的内容视为标题
  本身；噪声词表大幅扩充（平台名、发布形容词、集数区间等）。清洗结果现在还会带出
  年份与季号提示：新建订阅时季号自动回填，元数据搜索用年份区分同名翻拍。
- 容器加固（去掉多余 capability、禁止提权、根文件系统只读），新增
  `scripts/check-css-classes.py` 与无障碍回归测试。

完整清单见 [CHANGELOG.md](../CHANGELOG.md) 的 `## 2.7.2` 段。

前端资源 URL 带有 `?v=2.7.2`。

如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。
