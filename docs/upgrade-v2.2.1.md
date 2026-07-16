# v2.2.1 升级指南

```bash
docker compose pull
docker compose up -d
docker compose logs --tail=100 -f
```

v2.2.1 不修改 JSON Store schema。升级后下一次订阅检查会重新收敛已达到目标集数的订阅状态；不会直接修改已有数据。

更新日历会忽略没有播出日期的元数据占位集，避免把尚未确认排期的剧集显示为持续更新。
