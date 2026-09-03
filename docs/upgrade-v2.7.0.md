# v2.7.0 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.7.0 可直接复用 v2.6.x 的 `data/`，无需迁移步骤。

**JSON Store schema**：订阅新增可选字段 `season_list`（跳季订阅的季度集合），只在实际使用跳季订阅时写入；连续季度集合会折叠为区间语义而不写该字段。`season`/`season_end` 仍冗余存储为集合的最小/最大值，因此回滚到旧版本时旧程序会把跳季订阅当作 `season..season_end` 连续区间处理（可能多转中间季），重新升级到 v2.7.0 后自动恢复精确的集合语义。其余历史字段不变。

**API**：新增 `POST /api/subscriptions/seasons`（探测分享链接的季度分布，需已配置夸克 Cookie）；`POST /api/utils/parse-season` 响应新增 `season_list`/`seasons` 字段；订阅创建/更新请求新增可选 `season_list`，更新时传空数组表示清除集合、回到区间语义。既有端点与字段保持兼容。

**前端**：资源 URL 带有 `?v=2.7.0`。如果使用二进制部署，必须同时替换归档中的整个 `static/` 目录；只替换二进制会继续运行旧前端代码。

升级后既有订阅行为不变。若想改为跳季订阅，编辑订阅点「探测该分享的季度」后勾选需要的季即可；也可以直接在季号里填 `1,3`。
