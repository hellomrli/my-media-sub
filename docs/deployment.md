# Deployment

## Docker Compose

1. Copy env file:

```bash
cp .env.example .env
```

2. Edit `.env`:

```env
PANSOU_BASE_URL=https://pansou.lxf87.com.cn
OPENLIST_BASE_URL=https://pan.lxf87.com.cn
QUARK_SAVE_ROOT=/pansou
BOT_PORT=8787
```

3. Start:

```bash
docker compose up -d --build
```

4. Open WebUI:

```text
http://127.0.0.1:8787/
```

5. Health check:

```bash
curl http://127.0.0.1:8787/health
```

6. Search test:

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"想看 盗梦空间"}'
```

7. Select test:

```bash
curl -X POST http://127.0.0.1:8787/api/wechat/message \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":"test","text":"选 1"}'
```

## API Endpoints

- `GET /` WebUI
- `GET /health`
- `POST /api/search`
- `POST /api/select`
- `POST /api/wechat/message`

## Current Limitations

当前 Docker 服务已经支持：

- 调 PanSou 搜索夸克资源
- 格式化微信机器人回复文本
- 记住最近一次搜索
- 处理 `选 1` 这类选择消息

尚未完成：

- 夸克分享链接转存
- OpenList 复制/下载到 NAS
- 微信机器人适配器签名/鉴权

## Enable Authentication

Edit `.env`:

```env
APP_USERNAME=admin
APP_PASSWORD=your-strong-password
```

Then restart:

```bash
docker compose up -d --build
```

The WebUI and APIs require HTTP Basic auth. `/health` remains public.

## Link Check and Share File Probe

Enabled by default:

```env
CHECK_LINKS=true
PROBE_QUARK_FILES=true
```

The app will check whether Quark share links are alive and try to list files in the share so TV episode counts can be estimated.

## Filter Dead Links

Enabled by default:

```env
FILTER_BAD_LINKS=true
```

Only links explicitly confirmed as `bad` are removed. `locked`, `unknown`, and `error` results are kept.


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
