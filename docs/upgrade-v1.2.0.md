# v1.2.0 升级、验证与回滚指南

> 适用于从 v1.1.x 升级到 v1.2.0，也用于发布前升级与回滚演练。

## 1. 重要变化

v1.2.0 不只是界面更新，还包含两项兼容性变化：

1. JSON API 使用统一响应信封；
2. 四个业务数据文件使用 `schema_version: 1` 信封。

此外：

- WebUI 重构为 Media Deck；
- 新增统一请求层、搜索质量分析和订阅详情聚合；
- 网盘和 Aria2 任务增加订阅自动化关联；
- 设置中心重组；
- Basic Auth、CSRF、404、405 和请求解析错误改为 JSON；
- 删除从未启用、也从未通过 API 暴露的 `nas_sync_*` 占位配置。旧 JSON 中这些字段会被安全忽略，并在后续保存时移除。

## 2. 升级前检查

### 2.1 确认当前版本和部署方式

```bash
./my-media-sub --version 2>/dev/null || true
docker compose ps
```

记录当前：

- 二进制或镜像版本；
- `DATA_DIR` 实际路径；
- 二进制路径；
- `static/` 路径；
- 服务启动命令、systemd unit 或 compose 文件。

### 2.2 停止写操作

升级前避免同时执行：

- 批量订阅检查；
- 转存和重命名；
- 设置保存；
- 通知清理；
- 在线更新之外的文件替换。

推荐先停止服务后做完整数据备份；使用内置在线更新时，至少先手工备份 `DATA_DIR`。

### 2.3 备份数据

```bash
BACKUP="my-media-sub-data-$(date +%Y%m%d-%H%M%S).tar.gz"
tar -czf "$BACKUP" /实际路径/data
sha256sum "$BACKUP" > "$BACKUP.sha256"
```

至少确认归档包含：

```text
settings.json
subscriptions.json
notifications.json
jobs.json
```

建议同时备份：

```bash
cp -a /path/to/my-media-sub /path/to/my-media-sub.pre-v1.2.0
cp -a /path/to/static /path/to/static.pre-v1.2.0
```

## 3. 数据迁移行为

首次由 v1.2.0 加载旧裸 JSON 时，每个 Store 会：

1. 读取并验证旧数据；
2. 创建一次性原始备份；
3. 写入 schema v1 信封；
4. 将业务文件和备份权限修复为 `0600`。

示例：

```text
settings.json
settings.json.schema-v0.bak
subscriptions.json
subscriptions.json.schema-v0.bak
notifications.json
notifications.json.schema-v0.bak
jobs.json
jobs.json.schema-v0.bak
```

迁移后的文件：

```json
{
  "schema_version": 1,
  "data": {}
}
```

安全规则：

- 已存在的 `.schema-v0.bak` 不会被覆盖；
- 未来 schema 版本会阻止启动，但原文件不会被隔离或改写；
- 真正损坏的 JSON 会移动为 `.corrupt-<timestamp>`；
- 写盘失败时内存状态不会提前改变；
- 旧版 v1.1.x 二进制不应直接读取 schema v1 文件。

## 4. API 迁移

### 4.1 成功响应

旧脚本可能读取：

```json
{
  "list": []
}
```

v1.2.0 JSON API 读取：

```json
{
  "ok": true,
  "data": {
    "list": []
  }
}
```

外部脚本需要从 `.data` 读取业务对象。

### 4.2 错误响应

```json
{
  "ok": false,
  "error": "validation_error",
  "message": "参数错误"
}
```

Basic Auth 401 仍携带：

```text
WWW-Authenticate: Basic realm="my-media-sub"
```

### 4.3 例外

不使用普通信封：

- `/health`；
- `/strm/*`；
- `/api/jobs/events` SSE；
- 成功的 204 操作。

完整契约见 [`api-contract.md`](api-contract.md)。

## 5. 升级方式

### 5.1 手工二进制升级

```bash
# 1. 停止服务
systemctl stop my-media-sub  # 按实际部署修改

# 2. 解压 Release
mkdir -p /tmp/my-media-sub-v1.2.0
tar -xzf my-media-sub-v1.2.0-linux-x86_64.tar.gz -C /tmp/my-media-sub-v1.2.0

# 3. 校验包
sha256sum -c my-media-sub-v1.2.0-linux-x86_64.tar.gz.sha256

# 4. 替换二进制和 static（先自行备份旧文件）
cp /tmp/my-media-sub-v1.2.0/my-media-sub-v1.2.0-linux-x86_64/my-media-sub /path/to/my-media-sub
rm -rf /path/to/static
cp -a /tmp/my-media-sub-v1.2.0/my-media-sub-v1.2.0-linux-x86_64/static /path/to/static
chmod 0755 /path/to/my-media-sub

# 5. 启动
systemctl start my-media-sub
```

实际解压目录以 Release 包为准。

### 5.2 Docker Compose

正式发布后将镜像标签改为 v1.2.0 对应标签：

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 my-media-sub
```

必须继续挂载原 `DATA_DIR`，不要创建新的空数据卷。

### 5.3 WebUI 在线更新

在线更新器会：

- 校验 `.sha256`；
- 将 `static/` 备份为 `static.bak-<timestamp>`；
- 将二进制备份为 `my-media-sub.bak-<timestamp>`；
- 替换文件后等待用户确认重启。

在线更新器不会替你做完整 `DATA_DIR` 归档，因此更新前仍建议手工备份数据。

## 6. 升级后验证

### 6.1 服务与 API

```bash
curl -fsS http://127.0.0.1:56001/health | jq
curl -fsS -u admin:你的密码 http://127.0.0.1:56001/api/settings | jq '.ok, .data.app_username'
curl -fsS -u admin:你的密码 http://127.0.0.1:56001/api/subscriptions | jq '.ok, (.data | length)'
```

应确认：

- `/health` 返回 `status: ok`；
- API 返回 `ok: true`；
- 订阅数量与升级前一致；
- 设置中的 Cookie/Token 显示为脱敏值；
- 不出现 `.corrupt-*` 文件。

### 6.2 数据文件

```bash
for file in settings subscriptions notifications jobs; do
  jq '.schema_version, (.data | type)' "/实际路径/data/${file}.json"
done

find /实际路径/data -maxdepth 1 -name '*.schema-v0.bak' -ls
```

### 6.3 WebUI

检查：

- 首页可以加载，浏览器控制台无异常；
- 深色和浅色主题；
- 搜索、订阅详情、网盘、下载和设置页；
- 浏览器前进/后退；
- 未配置 Aria2 时不重复请求失败；
- Job SSE 能实时更新；
- Basic Auth 错误仍能触发浏览器认证流程。

## 7. 回滚到 v1.1.x

> 关键点：旧二进制不能直接读取 schema v1 信封。必须同时回滚程序文件和数据文件。

### 7.1 停止服务

```bash
systemctl stop my-media-sub
# 或 docker compose down
```

### 7.2 恢复旧数据格式

如果升级前文件来自 schema v0，可恢复自动生成的原始备份：

```bash
cd /实际路径/data
for file in settings subscriptions notifications jobs; do
  if [ -f "${file}.json.schema-v0.bak" ]; then
    cp -a "${file}.json.schema-v0.bak" "${file}.json"
  fi
done
chmod 0600 settings.json subscriptions.json notifications.json jobs.json
```

更稳妥的方式是恢复升级前完整 DATA_DIR 归档。

不要删除 v1.2.0 数据，建议先另行保存：

```bash
cp -a /实际路径/data /实际路径/data.failed-v1.2.0
```

### 7.3 恢复二进制和静态资源

如果通过在线更新：

```bash
cp -a /path/to/my-media-sub.bak-时间戳 /path/to/my-media-sub
rm -rf /path/to/static
mv /path/to/static.bak-时间戳 /path/to/static
chmod 0755 /path/to/my-media-sub
```

如果通过 Docker：

- 将镜像标签改回原 v1.1.x；
- 恢复旧 DATA_DIR；
- 重新 `docker compose up -d`。

### 7.4 启动和验证

```bash
systemctl start my-media-sub
curl -fsS http://127.0.0.1:56001/health
```

验证订阅数量、设置、通知和任务历史与升级前一致。

## 8. 发布前维护清单

正式发布 v1.2.0 前必须更新：

- `Cargo.toml` 和 `Cargo.lock`；
- README 当前版本、镜像标签和版本更新；
- Release workflow 提取的版本正文；
- Docker 标签说明；
- `docs/roadmap.md`；
- 本文的“开发中”状态和示例资产名；
- 完整质量门、二进制升级、Docker 升级和回滚实测结果。
