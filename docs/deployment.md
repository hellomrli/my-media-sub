# Deployment

## Docker Compose

1. Copy env file:

```bash
cp .env.example .env
```

2. Edit `.env`:

```env
QUARK_SAVE_ROOT=/your/quark/save/root
BOT_PORT=50001
APP_USERNAME=admin
APP_PASSWORD=your-strong-password
```

3. Start:

```bash
docker compose up -d --build
```

4. Open WebUI:

```text
http://127.0.0.1:50001/
```

5. Health check:

```bash
curl http://127.0.0.1:50001/health
```

6. Search test:

```bash
curl -X POST http://127.0.0.1:50001/api/wechat/message   -H 'Content-Type: application/json'   -d '{"chat_id":"test","text":"想看 盗梦空间"}'
```

## API Endpoints

- `GET /` WebUI
- `GET /health`
- `POST /api/search`
- `POST /api/select`
- `POST /api/wechat/message`
- `POST /api/subscriptions/*`
- `POST /api/quark-drive/*`

## Supported Features

- Built-in Quark resource search aggregation; no external PanSou service is required.
- Quark share link checking and file probing.
- Quark share auto-save with target directory and rename rules.
- Quark drive browser and folder operations.
- Aria2 task submission.
- Optional local mount path copy to NAS; no external OpenList API is required.

## Link Check and Share File Probe

```env
CHECK_LINKS=true
PROBE_QUARK_FILES=true
FILTER_BAD_LINKS=true
```

The app will check whether Quark share links are alive and try to list files in the share so TV episode counts can be estimated.

## WebUI Settings

WebUI includes a settings panel for:

- login username/password
- default cloud disk types
- link check / Quark probe / bad-link filtering toggles
- Aria2 RPC URL, secret, and download directory
- Quark Cookie and save root
- subscription scheduler controls
- local mount source path and NAS target path

Settings are persisted to `/data/settings.json` in Docker.

## Aria2

```env
ARIA2_RPC_URL=http://host:6800/jsonrpc
ARIA2_SECRET=
ARIA2_DIR=/downloads
```

After searching, click `Aria2` on a result to send its URL to Aria2. Cloud share URLs are not direct file URLs, so this is mainly useful for direct links, magnets, ed2k, or sources Aria2 can handle.
