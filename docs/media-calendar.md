# 媒体更新日历规则

> 当前媒体日历与逐集状态合同；历史起点为 P1 / v1.3.0。
> 固化日期：2026-07-10

## 1. 时间口径

- 所有日历日期和手动播出时间统一按 `Asia/Shanghai`（UTC+08:00）解释。
- “今日”是上海时区的自然日，不使用浏览器时区或服务器本地时区。
- 自然周固定为周一 00:00 至周日 23:59:59；跨月、跨年时仍按连续的周一至周日计算。
- 元数据只有 `YYYY-MM-DD` 时，只能判断播出日期，不能推断具体时刻；手动填写 `HH:MM` 后才返回可靠的 `scheduled_at`。
- `Settings.check_weekdays` 只控制系统在哪些星期执行订阅检查，**不是媒体播出星期，也不得用于生成日历排期**。

## 2. 状态定义

一个日历项可以同时拥有时间状态和流水线状态，`statuses` 保留全部状态，`primary_status` 按风险和可操作性确定主状态。

| 状态 | 规则 |
|---|---|
| `today` | 排期日期等于上海时区今日，不论该集是否已经发现或处理完成。 |
| `this_week` | 排期日期晚于今日，且不晚于当前自然周周日。 |
| `aired_undiscovered` | 排期日期早于今日，且该集尚未出现在 known/探测结果中。当天尚未结束时不提前判为漏更。 |
| `discovered_pending_transfer` | 已发现，但尚未转存；仅通知模式不进入此状态。 |
| `transferred_pending_download` | 已转存、订阅启用了同步下载，但尚无 Aria2 完成记录。 |
| `completed_missing` | 订阅已标记完结，排期不晚于今日，且该集仍未发现。 |
| `ready` | 已发现，并完成当前订阅启用的转存/下载阶段。 |
| `scheduled` | 有可信排期，但尚未进入以上异常或处理状态。 |
| `unknown_schedule` | 没有可用的手动排期、逐集元数据、下一集日期或发布日期。 |

`primary_status` 优先级：完结缺集 → 已转存待下载 → 已发现待转存 → 已播未发现 → 今日 → 本周 → 已就绪 → 已排期 → 排期未知。

## 3. 排期来源与优先级

按订阅维度使用以下优先级；高优先级存在时不混用低优先级日期：

1. **手动排期 `manual_schedule`**：高可信度。用于覆盖元数据，但不修改或删除原始 `MediaMetadata`。
2. **`MediaMetadata.episodes[].air_date`**：高可信度；按当前订阅季度筛选。
3. **`MediaMetadata.next_episode_to_air.air_date`**：高可信度；与逐集元数据重复时去重。
4. **`MediaMetadata.release_date`**：中可信度；电影表示上映日，剧集在没有逐集排期时视为第 1 集日期。
5. **由至少两个已知逐集日期推断的稳定周期**：低可信度；只向后补齐到总集数，周期必须可整除且在 1–28 天之间。
6. **无排期**：未知可信度，生成单个 `unknown_schedule` 项。

手动排期字段：

- `start_date`：`YYYY-MM-DD`，必填；
- `weekdays`：ISO 星期 1–7（周一至周日），可多选；为空时使用 `start_date` 的星期；
- `air_time`：`HH:MM`，可空；
- `interval_weeks`：1–52，默认 1；
- `first_episode_number`：`start_date` 对应的集号，默认 1；
- `total_episodes`：最后一集编号；为空时回退订阅或元数据总集数。

## 4. 数据合并

- “已发现”来自 `known_episodes`、`known_files` 和最近探测结果；旧数据缺少逐集列表时沿用订阅详情服务的连续进度推断。
- “已转存”来自 `transferred_file_keys`、`transferred_files` 和转存通知。
- STRM 与 Aria2 状态复用订阅详情聚合逻辑，避免日历和详情页产生两套判定。
- 日历计算为纯计算过程，不写回订阅、元数据、任务或通知。

## 5. API 查询

`GET /api/calendar` 支持：

- `from` / `to`：闭区间日期，格式 `YYYY-MM-DD`；默认当前自然周，最长 367 个自然日；
- `status`：按任一 `statuses` 值筛选；
- `media_type`：按媒体类型筛选；
- `subscription`：按订阅 ID 筛选。

返回上海时区今日、自然周边界、状态/媒体类型摘要、排期来源与可信度，以及订阅详情、立即检查和补集操作能力。
