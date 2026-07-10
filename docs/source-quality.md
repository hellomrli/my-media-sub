# 资源质量评分与安全自动换源

> 适用阶段：roadmap P2。后端评分是权威结果，前端历史分析只用于兼容旧响应。

## 1. 评分输出

`SourceQuality` 包含：

- `score`：0–100；
- `grade` / `tone`：旗舰、优质、清晰、普通、谨慎及对应展示色；
- `tags`：清晰度、HDR、编码、音轨、WEB/蓝光和剧集数；
- `risks`：失效链接、广告内容、跨季合集、无视频文件；
- `file_count` / `video_count` / `episode_count`；
- `episode_start` / `episode_end`；
- `total_size` / `updated_at`；
- `recommendation_reasons`。

评分由 `src/services/source_quality.rs` 计算。`tests/fixtures/source_quality.json` 同时被 Rust 和 Node 测试读取，防止迁移后与历史前端算法漂移。

## 2. 评分规则

基础分为 24，主要加减项：

- 清晰度：8K 32、4K 28、2K 23、1080P 19、720P 11、SD 4；
- 链接有效 +18，未知 +5，失效 -32 且最终不超过 24；
- HDR/杜比视界、H.265/AV1、高规格音轨、蓝光/WEB 分别加分；
- 有视频、剧集覆盖、文件数量和最近更新时间加分；
- 广告、跨季合集和无视频文件按风险扣分。

分级：85 以上旗舰、70 以上优质、55 以上清晰、35 以上普通，其余谨慎。

## 3. 候选安全预览

`POST /api/subscriptions/{id}/source-candidates/preview` 会重新探测候选（五分钟内的已持久化探测结果可复用），并返回：

- 当前来源与候选质量分、分差；
- 链接探测是否成功；
- 季度是否匹配；
- 候选是否覆盖当前追更进度；
- 是否属于当前/历史链接；
- 是否为近期失败候选；
- 订阅和候选是否仍在冷却期；
- `can_apply` 和更严格的 `auto_eligible`；
- 推荐理由和阻止应用的警告。

手动应用也必须满足探测成功、季度匹配、进度覆盖、非历史链接和非近期失败候选。自动应用还必须满足最低分、最低分差、连续失效阈值和冷却时间。

## 4. 自动换源设置

- `auto_source_switch_enabled`：总开关，默认 `false`；
- `auto_source_switch_mode`：`search_only` 或 `apply`，默认仅搜索；
- `source_switch_min_score`：默认 70；
- `source_switch_min_score_delta`：默认 10；
- `source_switch_failure_threshold`：默认连续失效 2 次；
- `source_switch_cooldown_hours`：默认 24 小时。

只有总开关开启且模式为 `apply` 时才可能自动应用。第一次失效可以先搜索并保存候选，达到失效阈值后可复用冷却期内的已保存候选，不会因为禁止重复搜索而错过自动应用。

## 5. 状态保护、审计与回滚

换源时保留：

- `known_files` / `known_file_keys` / `known_episodes`；
- `transferred_files` / `transferred_file_keys`；
- 当前集数和总集数；
- 转存、STRM 和 Aria2 关联所依赖的历史数据。

`source_switch_history` 最多保留 50 条成功、失败和回滚记录，包含新旧 URL、候选评分、原因、自动/手动标记、错误和时间。WebUI 支持查看历史并一键回滚上一来源；回滚同样保留追更与转存进度，并触发立即检查。

## 6. API

- `GET /api/subscriptions/{id}/source-candidates`
- `POST /api/subscriptions/{id}/source-candidates/search`
- `POST /api/subscriptions/{id}/source-candidates/probe`
- `POST /api/subscriptions/{id}/source-candidates/preview`
- `POST /api/subscriptions/{id}/source-candidates/apply`
- `GET /api/subscriptions/{id}/source-history`
- `POST /api/subscriptions/{id}/source-history/rollback`

所有 JSON 接口使用统一响应信封。
