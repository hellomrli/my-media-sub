# 结构化自动化事件与流水线

> 适用阶段：roadmap P3。Job、AutomationEvent、Notification 和 Metrics 各自承担独立职责。

## 1. 职责边界

- **Job**：排队、执行、取消、重试和执行结果；
- **AutomationEvent**：订阅、单集和任务的业务阶段轨迹；
- **Notification**：面向用户的消息与推送结果；
- **Metrics**：聚合计数、耗时和成功率。

订阅状态不再通过通知正文判断 STRM 成败；结构化事件优先，旧通知仅从 metadata 兼容降级。

## 2. 事件模型

每条事件包含：

- `id`、`correlation_id`；
- `subscription_id`、`episode`、`job_id`；
- `stage`、`status`、`attempt`；
- `message`、`error`、`metadata`；
- `created_at`、`updated_at`、`started_at`、`finished_at`。

阶段：

```text
source_check → file_filter → version_select → cloud_transfer
             → rename → strm → aria2 → notification
```

状态：

```text
pending → running → succeeded / skipped / failed / canceled
failed → retrying → pending / running / failed / canceled
```

成功、跳过和取消等终态不能被改回运行态。

## 3. 存储和索引

事件保存到 `DATA_DIR/automation_events.json`，继续使用 `schema_version: 1` 信封、原子写入和 `0600` 权限。

- 普通事件保留 30 天；
- 失败事件保留 90 天；
- 最多保留 5,000 条；
- 按 subscription、correlation 和 job 建立内存索引；
- 同一阶段执行使用稳定事件 ID，并以状态机原位更新生命周期，避免把历史 `running` 误判为当前卡住任务；
- 相同事件 ID 重复写入保持幂等，状态更新保留首次开始时间和可计算耗时；
- 重启加载后重建索引。

## 4. 事件来源

- 订阅检查写入 source_check、file_filter 和 version_select；
- JobStore 状态广播投影 pending/running/succeeded/failed/canceled；
- SubscriptionTransfer 的结构化结果投影 cloud_transfer、rename、strm、aria2 和用户通知结果；
- PushDispatch payload 继承 correlation/subscription 上下文并投影 notification；
- Job payload 中的 `correlation_id` 将检查与后续转存任务串联。

## 5. API

- `GET /api/automation/events`：按 subscription/correlation/job/episode 查询；
- `GET /api/automation/summary`：按 correlation/订阅/集数/阶段折叠为当前状态后，计算成功率、失败、卡住阶段和重试热点；
- `GET /api/subscriptions/{id}/pipeline`：单订阅流水线；增加 `?episode=N` 时返回该集及订阅级公共阶段；
- `GET /api/jobs/{id}/pipeline`：单 Job 流水线；
- `POST /api/automation/events/{id}/retry`：安全重试失败或取消阶段。

独立阶段只有存在 Job handler，或属于来源检查/文件过滤时才允许重试；成功事件不能重试，避免重复副作用。同步检查重试会从 `retrying` 进入 `running`，并最终落到成功或失败；Job 重试由新 Job 的结构化状态继续接管。

## 6. WebUI

- 工作台显示自动化成功率、最近失败和卡住阶段；
- 订阅详情显示结构化事件、阶段、集数、耗时、尝试次数、Job 和错误；
- 失败/取消事件提供安全重试入口；
- 旧七步聚合视图继续保留，结构化事件会覆盖其最新阶段状态。
