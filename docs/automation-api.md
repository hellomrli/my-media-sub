# 自动化 API 示例

所有示例使用单实例自动化 Token。明文 Token 只在轮换响应中显示一次，不要提交到 Git 或写入日志。

## 创建或轮换 Token

管理员 Basic Auth：

```bash
curl -u admin:password -X POST http://localhost:56001/api/automation-token \
  -H 'Content-Type: application/json' \
  -d '{"scopes":["subscriptions:read","subscriptions:write","subscriptions:check","jobs:read"],"expires_days":90}'
```

后续调用：

```bash
export MMS_TOKEN='mms_...'
curl -H "Authorization: Bearer $MMS_TOKEN" http://localhost:56001/api/subscriptions
```

撤销：

```bash
curl -u admin:password -X DELETE http://localhost:56001/api/automation-token
```

## 导出与导入订阅

```bash
curl -H "Authorization: Bearer $MMS_TOKEN" \
  http://localhost:56001/api/subscriptions/export > subscriptions-envelope.json
```

导入先预览，再使用唯一幂等键确认执行：

```bash
jq '{archive:.data,strategy:"skip",confirmation:"IMPORT SUBSCRIPTIONS"}' \
  subscriptions-envelope.json > import-request.json

curl -H "Authorization: Bearer $MMS_TOKEN" -H 'Content-Type: application/json' \
  -X POST --data @import-request.json \
  http://localhost:56001/api/subscriptions/import/preview

curl -H "Authorization: Bearer $MMS_TOKEN" -H 'Content-Type: application/json' \
  -H "Idempotency-Key: import-$(sha256sum import-request.json | cut -d' ' -f1)" \
  -X POST --data @import-request.json \
  http://localhost:56001/api/subscriptions/import
```

策略为 `skip`、`update` 或 `new_id`。相同 Idempotency-Key 与相同请求在 24 小时进程窗口内返回原结果；键相同但请求不同会被拒绝。

## 轮询 Job

```bash
curl -H "Authorization: Bearer $MMS_TOKEN" http://localhost:56001/api/jobs
curl -H "Authorization: Bearer $MMS_TOKEN" http://localhost:56001/api/jobs/JOB_ID/pipeline
```

## Webhook v1.0

Webhook 请求包含 `X-Media-Sub-Webhook-Version: 1.0`，正文结构：

```json
{
  "version": "1.0",
  "event_id": "uuid",
  "event": "notification",
  "occurred_at": 1700000000,
  "correlation_id": "...",
  "subscription_id": "...",
  "job_id": "...",
  "data": {"title": "...", "message": "...", "level": "success"}
}
```

接收端应以 `event_id` 去重，并继续验证 `X-Media-Sub-Signature-256` HMAC-SHA256 签名。

## Python 示例

```python
import json, os, urllib.request
request = urllib.request.Request(
    "http://localhost:56001/api/subscriptions",
    headers={"Authorization": f"Bearer {os.environ['MMS_TOKEN']}"},
)
with urllib.request.urlopen(request, timeout=10) as response:
    print(json.load(response)["data"])
```

## GitHub Actions 示例

```yaml
name: Export subscriptions
on:
  workflow_dispatch:
jobs:
  export:
    runs-on: ubuntu-latest
    steps:
      - run: |
          curl --fail --silent --show-error \
            -H "Authorization: Bearer $MMS_TOKEN" \
            "$MMS_URL/api/subscriptions/export" > subscriptions.json
      - uses: actions/upload-artifact@v4
        with:
          name: subscriptions
          path: subscriptions.json
    env:
      MMS_URL: ${{ secrets.MMS_URL }}
      MMS_TOKEN: ${{ secrets.MMS_TOKEN }}
```
