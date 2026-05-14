# my-media-sub

微信机器人 / WebUI + PanSou + 夸克网盘 + OpenList + NAS 的影视资源自动化助手。

目标流程：

```text
微信消息或 WebUI 输入：想看 盗梦空间
  ↓
服务调用 PanSou 搜索夸克资源
  ↓
返回候选结果给用户选择
  ↓
用户回复“选 2”或在 WebUI 点击选择
  ↓
转存夸克分享链接到个人夸克网盘 /pansou
  ↓
OpenList 挂载夸克目录
  ↓
NAS 通过 OpenList 下载/同步到媒体库
```

## 当前已知环境

- PanSou API: `https://pansou.lxf87.com.cn`
- OpenList: `https://pan.lxf87.com.cn/`
- 夸克保存目录：`/pansou`

## 当前能力

- [x] Docker 部署
- [x] HTTP API
- [x] PanSou 夸克资源搜索
- [x] 微信机器人文本接口雏形
- [x] WebUI 搜索与选择
- [x] 会话内保存最近一次搜索结果
- [ ] 夸克分享链接自动转存
- [ ] OpenList 自动复制/下载到 NAS
- [ ] 微信机器人平台专用适配器

## 快速部署

```bash
git clone https://github.com/hellomrli/my-media-sub.git
cd my-media-sub
cp .env.example .env
docker compose up -d --build
```

访问 WebUI：

```text
http://你的服务器:8787/
```

健康检查：

```bash
curl http://127.0.0.1:8787/health
```

## API 示例

### 微信消息入口

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"想看 盗梦空间"}'
```

### 选择结果

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"选 1"}'
```

## 安全原则

- 不把 Cookie、Token、OpenList 密码写进仓库
- 使用 `.env` 或部署环境变量
- PanSou 若公网暴露，建议开启认证或加反代访问控制

## 下一步

1. 接入夸克转存服务
2. 确认 OpenList 中夸克盘挂载路径和 NAS 本地挂载路径
3. 增加 OpenList 复制/同步任务
4. 增加微信机器人具体平台适配器

## 认证

设置环境变量后，WebUI 和业务 API 会启用 HTTP Basic 账号密码认证：

```env
APP_USERNAME=admin
APP_PASSWORD=change-me
```

`/health` 保持公开，方便容器健康检查。

## 链接有效性和文件嗅探

搜索时默认会：

1. 调用 PanSou `/api/check/links` 检测夸克分享链接是否有效。
2. 对可疑似有效的夸克分享链接做 best-effort 嗅探，尝试列出分享内文件和目录，并估算连续剧集数。

可通过环境变量关闭：

```env
CHECK_LINKS=false
PROBE_QUARK_FILES=false
```

注意：夸克公开分享接口可能触发风控、验证码、密码或接口变更；这种情况下结果会显示 `locked`、`http_error` 或 `error`，不会中断搜索。

默认会过滤 PanSou 已确认 `bad` 的失效夸克链接。只过滤明确失效的结果；`locked`、`unknown`、`error` 会保留，方便人工判断或稍后重试。

```env
FILTER_BAD_LINKS=true
```


## WebUI Settings

WebUI now includes a settings panel for:

- login username/password
- PanSou base URL
- OpenList base URL
- default cloud disk types
- link check / Quark probe / bad-link filtering toggles
- Aria2 RPC URL, secret, and download directory

Settings are persisted to `/data/settings.json` in Docker.

## Aria2

Configure Aria2 RPC in the WebUI settings or `.env`:

```env
ARIA2_RPC_URL=http://host:6800/jsonrpc
ARIA2_SECRET=
ARIA2_DIR=/downloads
```

After searching, click `Aria2` on a result to send its URL to Aria2. Note: cloud share URLs may not be direct downloadable file URLs; this is mainly useful for direct links, magnets, ed2k, or sources Aria2 can handle.

## 中文名与订阅

项目中文名：**Lain 的媒体订阅**。

WebUI 已将常见网盘类型本地化展示，例如：夸克网盘、百度网盘、阿里云盘、迅雷网盘、磁力链接等。

订阅 MVP：

- 搜索连续剧后点击结果旁边的“订阅”
- 系统会保存当前夸克分享内已知文件列表
- 在“我的订阅”中点击“检查更新”或“检查全部更新”
- 如果分享目录新增文件，会显示新增文件列表

当前自动下载策略仍是“通知优先”；发现新文件后先提示，后续再接自动转存/Aria2/OpenList 下载策略。

## 参考项目订阅能力记录

参考：

- Cp0204/quark-auto-save：失效分享记录并跳过任务、提取码分享、正则过滤/重命名、任务结束期限、通知推送、媒体库刷新。
- adminpass/aliyundrive-subscribe：订阅检查周期、并发/延迟控制、截止记录 ID、过滤词、保存命名规则、完结状态、Aria2 下载和通知配置。

当前已吸收的能力：

- 订阅状态：`active` / `invalid`
- 检查时识别链接疑似失效
- 失效时写入通知中心
- 发现新增文件时写入通知中心
- WebUI 可查看和标记通知已读

后续计划：定时检查、过滤词、命名规则、完结状态、外部微信/企业微信通知。
