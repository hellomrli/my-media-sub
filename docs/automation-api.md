# 用 Token 调用自动化 API

[返回文档入口](index.md) · [HTTP API 契约](api-contract.md) · [OpenAPI](../static/openapi.json)

以下命令使用 Bash、curl、jq 与 GNU sha256sum；Python 示例另需 Python 3。先部署并配置夸克 Cookie。远程请求使用自己的 HTTPS 入口。

## 创建或轮换 Token

本实例只保存一个自动化 Token，轮换会立即使旧 Token 失效。明文只在本次响应的 `data.token` 返回，服务端保留哈希与前缀。

下面的作用域用于本页的订阅创建/检查/导入和 Job 查询；只导出订阅时仅保留 `subscriptions:read`。可选作用域通过管理员调用 `GET /api/automation-token/scopes` 获取。

```bash
export MMS_URL=http://127.0.0.1:56001
read -r -p '管理员用户名（默认 admin）：' MMS_USERNAME
MMS_USERNAME=${MMS_USERNAME:-admin}
read -r -s -p '管理员密码：' MMS_PASSWORD
printf '\n'
export MMS_TOKEN="$(curl --fail --silent --show-error \
  -u "$MMS_USERNAME:$MMS_PASSWORD" -X POST "$MMS_URL/api/automation-token" \
  -H 'Content-Type: application/json' \
  -d '{"scopes":["subscriptions:read","subscriptions:write","subscriptions:check","jobs:read"],"expires_days":30}' \
  | jq -er '.data.token')"
unset MMS_PASSWORD

curl --fail -H "Authorization: Bearer $MMS_TOKEN" "$MMS_URL/api/subscriptions"
```

后续代码复用 `MMS_URL` 与 `MMS_TOKEN`。不要记录完整令牌。设置、Token 管理、备份恢复、存储清理和在线更新不对任何自动化 scope 开放。

需要撤销时重新读取管理员密码：

```bash
read -r -s -p '管理员密码：' MMS_PASSWORD
printf '\n'
curl --fail -u "$MMS_USERNAME:$MMS_PASSWORD" -X DELETE "$MMS_URL/api/automation-token"
unset MMS_PASSWORD MMS_TOKEN
```

## 创建并检查订阅

此操作会添加真实订阅，检查后可按订阅规则转存。使用有权访问的分享并确认网盘目录；API 不代替 WebUI 的片源预览。

```bash
read -r -p '剧名：' MMS_TITLE
read -r -p '夸克分享 URL：' MMS_SHARE_URL
read -r -p '提取码（没有则留空）：' MMS_PASSCODE
read -r -p '季度（如 1、1-4 或 1,3）：' MMS_SEASONS
read -r -p '网盘目标目录：' MMS_TARGET_DIR
export MMS_SUBSCRIPTION_ID="$(jq -n \
  --arg title "$MMS_TITLE" --arg url "$MMS_SHARE_URL" \
  --arg password "$MMS_PASSCODE" --arg seasons "$MMS_SEASONS" --arg target "$MMS_TARGET_DIR" \
  '{title:$title,url:$url,password:$password,media_type:"series",season_spec:$seasons,target_dir:$target}' \
  | curl --fail --silent --show-error -X POST "$MMS_URL/api/subscriptions" \
    -H "Authorization: Bearer $MMS_TOKEN" -H 'Content-Type: application/json' --data-binary @- \
  | jq -er '.data.id')"

curl --fail -X POST -H "Authorization: Bearer $MMS_TOKEN" \
  "$MMS_URL/api/subscriptions/$MMS_SUBSCRIPTION_ID/check"
curl --fail -H "Authorization: Bearer $MMS_TOKEN" "$MMS_URL/api/jobs"
```

检查响应包含 `new_files`、`new_episodes` 和 `details`。转存作为 Job 异步执行；查询 Jobs 或订阅详情确认最终结果。画质设置见 [电影保存与画质](media-save-quality.md#api)。

## 导出和导入订阅

导出只需 `subscriptions:read`。归档包含分享链接等数据，使用受保护的工作目录：

```bash
umask 077
curl --fail --silent --show-error -H "Authorization: Bearer $MMS_TOKEN" \
  "$MMS_URL/api/subscriptions/export" > subscriptions-envelope.json
jq -e '.ok == true and .data.format == "my-media-sub-subscriptions"' subscriptions-envelope.json
```

导入需要 `subscriptions:write`。先生成请求并预览，确认结果后再执行下一个代码块：

```bash
jq '{archive:.data,strategy:"skip",confirmation:"IMPORT SUBSCRIPTIONS"}' \
  subscriptions-envelope.json > import-request.json
curl --fail -H "Authorization: Bearer $MMS_TOKEN" -H 'Content-Type: application/json' \
  -X POST --data-binary @import-request.json "$MMS_URL/api/subscriptions/import/preview"
```

预览不写入订阅。确认导入数量、冲突与策略后执行：

```bash
MMS_IMPORT_KEY="import-$(sha256sum import-request.json | cut -d' ' -f1)"
curl --fail -H "Authorization: Bearer $MMS_TOKEN" -H 'Content-Type: application/json' \
  -H "Idempotency-Key: $MMS_IMPORT_KEY" -X POST --data-binary @import-request.json \
  "$MMS_URL/api/subscriptions/import"
```

策略取 `skip`、`update` 或 `new_id`。同一个 Key 和相同归档/策略在 24 小时进程内缓存窗口返回原结果；同 Key 不同请求会拒绝。重启会清空幂等缓存，不应依赖它进行跨重启的去重。

## 查询 Job 流水线

使用 `jobs:read`，从 Job 列表或创建任务的响应选择实际 ID：

```bash
curl --fail -H "Authorization: Bearer $MMS_TOKEN" "$MMS_URL/api/jobs"
read -r -p '要查看的 Job ID：' MMS_JOB_ID
curl --fail -H "Authorization: Bearer $MMS_TOKEN" "$MMS_URL/api/jobs/$MMS_JOB_ID/pipeline"
```

实时事件与流水线投影见 [自动化事件](automation-events.md) 和 [API 契约](api-contract.md)。

## 接收 Webhook v1.0

请求带有 `X-Media-Sub-Webhook-Version: 1.0`。以下是字段示意，ID、时间和消息由服务生成：

```json
{
  "version": "1.0",
  "event_id": "事件 UUID",
  "event": "notification",
  "occurred_at": 1700000000,
  "correlation_id": "关联 ID",
  "subscription_id": "订阅 ID",
  "job_id": "任务 ID",
  "data": {"title": "通知标题", "message": "通知正文", "level": "success"}
}
```

接收端验证 `X-Media-Sub-Signature-256` 的 HMAC-SHA256 签名，再以 `event_id` 去重。不要只核对版本头就接受请求。

## 使用 Python 读取订阅

先按前面的 Token 步骤设置两个环境变量，保存并运行下面的完整脚本：

```python
import json
import os
import urllib.request

request = urllib.request.Request(
    os.environ["MMS_URL"].rstrip("/") + "/api/subscriptions",
    headers={"Authorization": "Bearer " + os.environ["MMS_TOKEN"]},
)
with urllib.request.urlopen(request, timeout=10) as response:
    envelope = json.load(response)
    if not envelope.get("ok"):
        raise RuntimeError(envelope.get("message", "API request failed"))
    print(envelope["data"])
```

## 在 GitHub Actions 中导出

在自己的仓库配置 `MMS_URL` 与只含 `subscriptions:read` 的 `MMS_TOKEN` Secrets。runner 必须能访问部署入口；GitHub 托管 runner 无法访问你的本机 `127.0.0.1`。导出内容包含分享信息，核对仓库和 artifact 访问权限。

```yaml
name: Export subscriptions
on:
  workflow_dispatch:
jobs:
  export:
    runs-on: ubuntu-latest
    env:
      MMS_URL: ${{ secrets.MMS_URL }}
      MMS_TOKEN: ${{ secrets.MMS_TOKEN }}
    steps:
      - run: |
          curl --fail --silent --show-error \
            -H "Authorization: Bearer $MMS_TOKEN" \
            "$MMS_URL/api/subscriptions/export" > subscriptions.json
      - uses: actions/upload-artifact@v4
        with:
          name: subscriptions
          path: subscriptions.json
          retention-days: 1
```

## 使用最小签到作用域

只需要夸克签到时，创建仅含 `quark:signin` 的 Token，调用 `POST /api/quark/signin`。它不授予订阅读写、Job 控制或设置访问。轮换这个 Token 会使之前的订阅 Token 失效；Telegram 的受控 `/signin` 使用相同 scope 映射和签到服务。
