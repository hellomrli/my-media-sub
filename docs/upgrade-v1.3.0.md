# v1.3.0 升级、验证与回滚指南

> 适用目标：升级到 `my-media-sub v1.3.0`。从 v1.2.0 升级不提升 JSON schema；从 v1.1.x 直接升级时，还必须同时理解 [`upgrade-v1.2.0.md`](upgrade-v1.2.0.md) 中的 schema v1 和统一 API 信封变化。

## 1. 本次升级的兼容性结论

- `settings.json`、`subscriptions.json`、`notifications.json` 和 `jobs.json` 继续使用 `schema_version: 1`。
- 订阅新增可选字段 `manual_schedule`；历史数据缺少该字段时自动按“未设置手动排期”读取。
- 日历是读取订阅、设置、任务和通知后的纯计算结果，不新增独立持久化文件。
- 新增 `GET /api/calendar`，其他 JSON API 继续使用 v1.2.0 起的统一响应信封。
- v1.3.0 二进制必须配套 v1.3.0 `static/`；只替换其中一部分会造成页面、字段或接口不匹配。

## 2. 升级前备份

先确认真实的 `DATA_DIR`，停止会修改数据的实例，并备份程序、静态资源和完整数据目录：

```bash
# 按实际部署路径修改
systemctl stop my-media-sub

cp -a /path/to/my-media-sub /path/to/my-media-sub.pre-v1.3.0
cp -a /path/to/static /path/to/static.pre-v1.3.0

tar -C /实际路径 -czf \
  /安全备份位置/my-media-sub-data-pre-v1.3.0.tar.gz \
  data
sha256sum /安全备份位置/my-media-sub-data-pre-v1.3.0.tar.gz
```

Docker 部署至少确认：

```bash
docker compose config
# 记录当前镜像 ID 与挂载

docker inspect my-media-sub --format '{{.Image}} {{json .Mounts}}'
```

升级前建议额外记录：

- 当前 `/health` 版本；
- 订阅数量和已启用数量；
- `subscriptions.json` 的 SHA256；
- 当前手工指定的镜像 tag 或二进制路径；
- 反向代理和浏览器静态资源缓存策略。

## 3. 数据与 API 变化

### 3.1 `manual_schedule`

订阅可新增以下结构：

```json
{
  "manual_schedule": {
    "start_date": "2026-07-10",
    "weekdays": [5],
    "air_time": "20:00",
    "interval_weeks": 1,
    "first_episode_number": 1,
    "total_episodes": 12
  }
}
```

字段规则：

- `start_date`：必填，格式 `YYYY-MM-DD`；
- `weekdays`：ISO 星期 1–7，可多选；空数组使用 `start_date` 自身星期；
- `air_time`：可空，非空时使用 `HH:MM`；
- `interval_weeks`：1–52；
- `first_episode_number`：正整数；
- `total_episodes`：可空；设置时不得小于首集编号。

更新订阅时必须区分：

```json
{}
```

表示保持当前手动排期不变；

```json
{"manual_schedule": null}
```

表示显式清除手动排期。清除覆盖不会删除 `metadata`。

### 3.2 日历 API

```text
GET /api/calendar
GET /api/calendar?from=2026-07-06&to=2026-07-12
GET /api/calendar?status=aired_undiscovered&media_type=series
GET /api/calendar?subscription=<订阅ID>
```

- `from` / `to` 是闭区间，默认上海时区当前自然周；
- 最大范围为 367 个自然日；
- 时间统一按 `Asia/Shanghai` 返回；
- 成功响应仍为 `{"ok":true,"data":...}`；
- 完整规则见 [`media-calendar.md`](media-calendar.md) 和 [`api-contract.md`](api-contract.md)。

### 3.3 无 schema 迁移

从 v1.2.0 升级时不会创建新的 schema 备份，也不会提高 `schema_version`。只有在用户保存手动排期或其他订阅变更后，`subscriptions.json` 才会自然包含 v1.3.0 新字段。

从 v1.1.x 直接升级时，v1.2.0 引入的 schema v0 → v1 自动迁移仍会执行；迁移与旧 API 客户端注意事项见 [`upgrade-v1.2.0.md`](upgrade-v1.2.0.md)。

## 4. 升级方式

### 4.1 手工二进制升级

```bash
# 1. 校验发布包
sha256sum -c my-media-sub-v1.3.0-linux-x86_64.tar.gz.sha256

# 2. 解压
mkdir -p /tmp/my-media-sub-v1.3.0
tar -xzf my-media-sub-v1.3.0-linux-x86_64.tar.gz \
  -C /tmp/my-media-sub-v1.3.0

# 3. 已停止服务且完成备份后，同时替换二进制和 static
cp /tmp/my-media-sub-v1.3.0/my-media-sub-v1.3.0-linux-x86_64/my-media-sub \
  /path/to/my-media-sub
rm -rf /path/to/static
cp -a /tmp/my-media-sub-v1.3.0/my-media-sub-v1.3.0-linux-x86_64/static \
  /path/to/static
chmod 0755 /path/to/my-media-sub

# 4. 启动
systemctl start my-media-sub
```

### 4.2 Docker Compose

将服务镜像固定到 `1.3.0`（或发布 tag `v1.3.0`），继续挂载原数据目录：

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 my-media-sub
```

不要在升级时换成新的空数据卷。确认稳定后再决定是否恢复使用 `latest`。

### 4.3 WebUI 在线更新

在线更新器会校验发布包 SHA256，备份旧二进制和 `static/`，然后等待用户确认重启。它不会归档完整 `DATA_DIR`，因此使用在线更新前仍应执行第 2 节的数据备份。

## 5. 升级后验证

### 5.1 服务、版本和基础 API

```bash
curl -fsS http://127.0.0.1:56001/health | jq
curl -fsS -u admin:你的密码 \
  http://127.0.0.1:56001/api/subscriptions | jq '.ok, (.data | length)'
curl -fsS -u admin:你的密码 \
  http://127.0.0.1:56001/api/calendar | jq \
  '.ok, .data.timezone, .data.from, .data.to, .data.summary.total'
```

应确认：

- `/health` 显示 `1.3.0`；
- 订阅数量与升级前一致；
- 日历返回 `timezone: "Asia/Shanghai"`；
- 没有新增 `.corrupt-*` 文件；
- `schema_version` 仍为 1。

### 5.2 WebUI

至少检查：

- 侧边栏可以进入“更新日历”；
- 周、月、列表三种视图均能切换；
- 前后周期、返回今天、状态和媒体类型筛选正常；
- 日历项可以进入订阅详情并触发检查操作；
- 订阅编辑器可保存、重新打开和清除手动排期；
- 清除手动排期后元数据仍存在；
- 1440px 和 390px 宽度下无页面级横向溢出；
- 浏览器控制台无 Alpine 初始化错误、JavaScript 异常和意外失败请求。

如果页面仍是旧版，先清理反向代理缓存并执行浏览器强制刷新。

### 5.3 手动排期最小验证

1. 选择一个测试订阅并记录现有 `metadata`；
2. 设置未来日期、播出星期和时间；
3. 保存后确认日历来源显示为“手动排期”；
4. 关闭手动排期并保存；
5. 确认日历回退到元数据/推断来源，原始 `metadata` 未变化。

## 6. 回滚

### 6.1 回滚到 v1.2.0

v1.2.0 与 v1.3.0 都使用 schema v1，因此旧程序通常可以直接读取当前数据。但 v1.2.0 不认识 `manual_schedule`，在它下一次重写 `subscriptions.json` 时可能丢弃该字段。

需要精确保留升级前状态时，推荐：

```bash
systemctl stop my-media-sub

# 保存失败现场
cp -a /实际路径/data /实际路径/data.failed-v1.3.0

# 恢复升级前数据、二进制和 static
rm -rf /实际路径/data
mkdir -p /实际路径/data
# 按备份实际目录层级解压
tar -xzf /安全备份位置/my-media-sub-data-pre-v1.3.0.tar.gz \
  -C /实际路径
cp -a /path/to/my-media-sub.pre-v1.3.0 /path/to/my-media-sub
rm -rf /path/to/static
cp -a /path/to/static.pre-v1.3.0 /path/to/static
chmod 0755 /path/to/my-media-sub

systemctl start my-media-sub
```

Docker 回滚时固定旧镜像 tag，并保持同一数据卷；若需要保留手动排期，先保存 v1.3.0 数据副本，再恢复升级前数据备份。

### 6.2 回滚到 v1.1.x

v1.1.x 不能直接读取 schema v1 信封。必须同时恢复旧二进制、旧静态资源，以及 v1.1.x 可读的数据备份或 `*.schema-v0.bak`。完整步骤见 [`upgrade-v1.2.0.md`](upgrade-v1.2.0.md) 的回滚章节。

## 7. 已知边界

- 当前 Rust 时间计算使用上海时区固定 UTC+08:00 偏移；上海无夏令时，因此与当前业务口径一致。
- 日历不创建独立事件存储，状态来自当前订阅、任务和通知快照。
- 手动排期只影响日历展示，不会回写或修正 TMDB 元数据。
- `check_weekdays` 不是播出星期；媒体排期必须来自手动排期、元数据或稳定周期推断。
