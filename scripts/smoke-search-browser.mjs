#!/usr/bin/env node
// Exercise real Alpine rendering with deterministic image and search responses.
// Requires Node 22+ and Chrome; no third-party browser driver or live media account.
import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import {spawn} from 'node:child_process';
import {mkdir, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {inflateSync} from 'node:zlib';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = resolve(process.argv[2] || join(root, 'target/release/my-media-sub'));
const temp = await mkdtemp(join(tmpdir(), 'media-sub-search-smoke-'));
const children = [];
const servers = [];
const sockets = new Set();
const imageRequests = [];
const emptyImageRequests = [];
const searches = [];
const metadataSearches = [];
const mediaSaves = [];
const statusFixtureRequests = [];
let statusFixturesEnabled = false;
// Keep data-driven views stable across polling, and exercise their actual API loaders.
const fixtureTime = Math.floor(Date.now() / 1000);
const statusSubscription = {
  id: 'local-status-show', title: '星海旅人（本地状态夹具）', media_type: 'anime',
  url: 'https://pan.quark.cn/s/local-status-show', season: 1, season_end: 3, season_list: [1, 3],
  status: 'active', enabled: true, current_episode_number: 3, total_episode_number: 4,
  last_checked_at: fixtureTime, last_check_summary: '本地演示：第 1 季缺 1 集，第 3 季等待更新',
  rules: {target_dir: '/片库/星海旅人', preferred_resolution: '2160p'},
  metadata: {title: '星海旅人（本地状态夹具）', vote_average: 8.4},
  sync_download_enabled: false,
  check_history: [{time: fixtureTime, state: 'success', summary: '本地检查完成', scanned_count: 6, new_count: 1, transfer_count: 1}]
};
const statusDetail = {
  subscription: statusSubscription,
  summary: {
    range_start: 1, range_end: 4, target_episode: null, expected_count: 6,
    discovered_count: 4, transferred_count: 3, downloaded_count: 2, missing_count: 1,
    pending_transfer_count: 1, pending_download_count: 1, completion_percent: 50,
    data_inferred: false, grid_truncated: false
  },
  episodes: [
    {season: 1, episode: 1, discovered: true, transferred: true, download_status: 'completed'},
    {season: 1, episode: 2, discovered: true, transferred: true, download_status: 'queued'},
    {season: 1, episode: 3, discovered: true, transferred: false, download_status: 'not_started', recent: true},
    {season: 1, episode: 4, missing: true, download_status: 'not_started'},
    {season: 3, episode: 1, discovered: true, transferred: true, download_status: 'completed'},
    {season: 3, episode: 2, discovered: false, transferred: false, download_status: 'not_started'}
  ].map(episode => ({missing: false, recent: false, files: [], ...episode})),
  missing_episodes: [4], pending_transfer_episodes: [3], pending_download_episodes: [2],
  pipeline: [
    {id: 'source_check', label: '来源检查', status: 'success', message: '分享有效', count: 4},
    {id: 'version_select', label: '版本选择', status: 'active', message: '正在选择 4K 版本', count: 1},
    {id: 'cloud_transfer', label: '云盘转存', status: 'warning', message: '还有 1 集待转存', count: 3},
    {id: 'rename', label: '重命名', status: 'error', message: '本地演示失败状态', count: 1},
    {id: 'aria2', label: 'Aria2', status: 'disabled', message: '未启用同步下载', count: 0},
    {id: 'notification', label: '通知', status: 'idle', message: '等待新集通知', count: 0}
  ],
  recent_jobs: [{id: 'local-status-job', title: '本地转存任务', kind: 'subscription_transfer', status: 'failed', error: '本地演示失败提示', updated_at: fixtureTime}],
  recent_notifications: [{id: 'local-status-note', title: '本地新集提醒', message: '发现第 1 季第 3 集', level: 'info', created_at: fixtureTime}],
  recent_events: []
};
const statusPipeline = {
  subscription_id: statusSubscription.id,
  events: ['succeeded', 'running', 'failed'].map((status, index) => ({
    id: 'local-status-event-' + index, stage: ['source_check', 'cloud_transfer', 'rename'][index],
    status, episode: index + 1, attempt: 1, started_at: fixtureTime - 5,
    updated_at: fixtureTime, message: '本地自动化事件演示',
    error: status === 'failed' ? '本地演示重命名失败' : '', job_id: 'local-status-job'
  }))
};
function statusDownload(gid, status, extra = {}) {
  const completed = status === 'complete' ? 2 * 1024 ** 3 : 1024 ** 3;
  return {
    gid, status, file_name: '星海旅人.S01E02.2160p.mkv', dir: '/downloads/星海旅人/Season 1',
    total_length: 2 * 1024 ** 3, completed_length: completed, progress: status === 'complete' ? 100 : 50,
    download_speed: status === 'active' ? 1024 ** 2 : 0, eta_seconds: 1024, connections: 4,
    automation: {
      subscription_id: statusSubscription.id, subscription_title: statusSubscription.title, episode: 2,
      target_dir: '/片库/星海旅人/Season 1', transfer_status: 'completed', rename_status: 'completed'
    },
    files: [{index: 1, file_name: '星海旅人.S01E02.2160p.mkv', path: '/downloads/星海旅人/Season 1/星海旅人.S01E02.2160p.mkv', length: 2 * 1024 ** 3, completed_length: completed}],
    ...extra
  };
}
const statusDownloads = {
  active: [statusDownload('local-active', 'active'), statusDownload('local-paused', 'paused'), statusDownload('local-manual', 'active', {automation: null})],
  waiting: [statusDownload('local-waiting', 'waiting', {automation: {subscription_id: statusSubscription.id, subscription_title: statusSubscription.title, transfer_status: 'pending', rename_status: 'pending'}})],
  stopped: [statusDownload('local-complete', 'complete'), statusDownload('local-failed', 'error', {error_code: '3', error_message: '本地演示：下载链接已过期'})]
};
const mediaFiles = [
  {file_key: 'movie-hd', name: '午后奇遇.mkv', parent_path: '电影 / 1080P', size: 5 * 1024 ** 3, resolution: '1080p'},
  {file_key: 'movie-uhd', name: '午后奇遇.mkv', parent_path: '电影 / 4K', size: 20 * 1024 ** 3, resolution: '2160p'},
  {file_key: 'movie-cut', name: '午后奇遇.导演剪辑版.mkv', parent_path: '电影 / 4K', size: 24 * 1024 ** 3, resolution: '2160p'}
];
const mediaQualities = [
  {resolution: '2160p', label: '4K / 2160P', file_count: 2, total_size: 44 * 1024 ** 3, sample_files: ['4K/午后奇遇.mkv']},
  {resolution: '1080p', label: '1080P', file_count: 1, total_size: 5 * 1024 ** 3, sample_files: ['1080P/午后奇遇.mkv']}
];
const exceptions = [];
let serverLog = '';
let chromeLog = '';
let ws;
let secureImageRequests = 0;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve(server.address().port)));

function luminance(color) {
  const channels = color.match(/[\d.]+/g).slice(0, 3).map(value => {
    const channel = Number(value) / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return channels.reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
}

// Chrome 截图是 8 位 RGB/RGBA 非隔行 PNG；只解这一种，免得为读几个像素引入依赖。
function decodePng(buffer) {
  assert.equal(buffer.subarray(1, 4).toString('latin1'), 'PNG', 'screenshot must be a PNG');
  let width = 0;
  let height = 0;
  let channels = 0;
  const chunks = [];
  for (let offset = 8; offset < buffer.length;) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString('latin1');
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      channels = {2: 3, 6: 4}[data[9]];
      assert.ok(data[8] === 8 && channels && data[12] === 0, 'unsupported screenshot PNG layout');
    } else if (type === 'IDAT') {
      chunks.push(data);
    }
    offset += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(chunks));
  const stride = width * channels;
  const pixels = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const value = raw[y * (stride + 1) + 1 + x];
      const left = x >= channels ? pixels[y * stride + x - channels] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const upLeft = y > 0 && x >= channels ? pixels[(y - 1) * stride + x - channels] : 0;
      let predicted = 0;
      if (filter === 1) predicted = left;
      else if (filter === 2) predicted = up;
      else if (filter === 3) predicted = (left + up) >> 1;
      else if (filter === 4) {
        const estimate = left + up - upLeft;
        const [byLeft, byUp, byUpLeft] = [Math.abs(estimate - left), Math.abs(estimate - up), Math.abs(estimate - upLeft)];
        predicted = byLeft <= byUp && byLeft <= byUpLeft ? left : (byUp <= byUpLeft ? up : upLeft);
      }
      pixels[y * stride + x] = (value + predicted) & 255;
    }
  }
  return {
    width, height,
    pixel(x, y) {
      assert.ok(x >= 0 && y >= 0 && x < width && y < height, `pixel ${x},${y} is outside the ${width}x${height} screenshot`);
      const index = y * stride + x * channels;
      return `rgb(${pixels[index]}, ${pixels[index + 1]}, ${pixels[index + 2]})`;
    }
  };
}

function contrast(foreground, background) {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => a - b);
  return (values[1] + 0.05) / (values[0] + 0.05);
}

async function waitFor(check, label, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`Timed out waiting for ${label}`);
}

try {
  const reservation = net.createServer();
  const appPort = await listen(reservation);
  await new Promise(resolve => reservation.close(resolve));
  const app = spawn(binary, [], {
    cwd: root,
    env: {
      PATH: process.env.PATH, HOME: process.env.HOME,
      SERVER_HOST: '127.0.0.1', SERVER_PORT: String(appPort),
      SERVER_USERNAME: 'smoke-user', SERVER_PASSWORD: 'smoke-temporary-test-password',
      DATA_DIR: join(temp, 'data'), STATIC_DIR: join(root, 'static'),
      BACKUP_INTERVAL_HOURS: '0', RUST_LOG: 'warn'
    }, stdio: ['ignore', 'pipe', 'pipe']
  });
  children.push(app);
  app.on('error', error => { serverLog += error.message; });
  app.stdout.on('data', data => { serverLog = (serverLog + data).slice(-20000); });
  app.stderr.on('data', data => { serverLog = (serverLog + data).slice(-20000); });
  await waitFor(async () => {
    if (app.exitCode !== null) throw new Error(`Server exited: ${serverLog}`);
    try { return (await fetch(`http://127.0.0.1:${appPort}/health`)).ok; } catch { return false; }
  }, 'server health');

  const png = await readFile(join(root, 'static/icons/icon-192.png'));
  const proxy = http.createServer((req, res) => {
    if (statusFixturesEnabled && req.method === 'GET') {
      const path = new URL(req.url, 'http://localhost').pathname;
      const data = {
        '/api/subscriptions': [statusSubscription],
        [`/api/subscriptions/${statusSubscription.id}/status`]: statusDetail,
        [`/api/subscriptions/${statusSubscription.id}/pipeline`]: statusPipeline,
        '/api/drive/aria2/tasks': statusDownloads
      }[path];
      if (data) {
        statusFixtureRequests.push(path);
        res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'});
        res.end(JSON.stringify({ok: true, data}));
        return;
      }
    }
    if (req.url.startsWith('/?') && req.url.includes('_media_sub_retry')) emptyImageRequests.push(req.url);
    if (req.url.startsWith('/smoke-images/')) {
      imageRequests.push(req.url);
      const good = req.url.startsWith('/smoke-images/good.png');
      res.writeHead(good ? 200 : 404, {'content-type': good ? 'image/png' : 'text/plain'});
      res.end(good ? png : 'missing fixture image');
      return;
    }
    if (req.url === '/api/search' && req.method === 'POST') {
      let body = '';
      req.on('data', chunk => { body += chunk; });
      req.on('end', () => searches.push({body: JSON.parse(body), res}));
      return;
    }
    if (req.url === '/api/subscriptions/seasons' && req.method === 'POST') {
      req.resume();
      res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'});
      res.end(JSON.stringify({ok: true, data: {
        ok: true, message: '本地画质演示数据', qualities: mediaQualities, media_files: mediaFiles,
        seasons: [{season: 1, file_count: 3, episodes: [], sample_files: [], inferred: true}],
        total_file_count: 3, unspecified_file_count: 0, partial: false
      }}));
      return;
    }
    if (req.url === '/api/transfer' && req.method === 'POST') {
      let body = '';
      req.on('data', chunk => { body += chunk; });
      req.on('end', () => {
        mediaSaves.push(JSON.parse(body));
        res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'});
        res.end(JSON.stringify({ok: true, data: {success: true, job_id: 'local-movie-save', message: '本地保存流程演示'}}));
      });
      return;
    }
    if (req.url.startsWith('/api/metadata/search?') && req.method === 'GET') {
      const params = new URL(req.url, 'http://localhost').searchParams;
      metadataSearches.push(Object.fromEntries(params));
      const query = params.get('query');
      const matched = query === '星海旅人之远航篇';
      const attempts = matched ? [query, '星海旅人'] : [query, '未收录剧集'];
      setTimeout(() => {
        if (res.destroyed) return;
        res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'});
        res.end(JSON.stringify({
          ok: true,
          data: matched ? [{
            provider: 'tmdb', provider_id: '900001', title: '星海旅人', media_type: 'anime',
            release_date: '2024-01-01', number_of_episodes: 24,
            overview: '本地演示数据：虚构剧集，用于展示篇章标题未命中后回退到系列主名的效果。'
          }] : [],
          attempted_queries: attempts,
          matched_query: matched ? '星海旅人' : null,
          message: matched ? '已调整搜索剧名为「星海旅人」并匹配到元数据（尝试 2 个名称）'
            : '已尝试 2 个剧名，仍未匹配到元数据'
        }));
      }, 150);
      return;
    }
    // Chrome no longer supports embedded Basic credentials; inject them locally.
    const upstream = http.request({
      hostname: '127.0.0.1', port: appPort, path: req.url, method: req.method,
      headers: {...req.headers, authorization: 'Basic ' + Buffer.from('smoke-user:smoke-temporary-test-password').toString('base64')}
    }, response => {
      res.writeHead(response.statusCode, response.headers);
      response.pipe(res);
    });
    upstream.on('error', error => {
      if (!res.headersSent) res.writeHead(502);
      res.end(error.message);
    });
    res.on('close', () => upstream.destroy());
    req.pipe(upstream);
  });
  servers.push(proxy);
  proxy.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  const proxyPort = await listen(proxy);

  const chrome = spawn(process.env.CHROME_BIN || 'google-chrome', [
    '--headless=new', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--disable-background-networking', '--no-first-run', '--no-default-browser-check',
    `--user-data-dir=${join(temp, 'chrome')}`, '--remote-debugging-port=0', 'about:blank'
  ], {stdio: ['ignore', 'ignore', 'pipe']});
  children.push(chrome);
  chrome.on('error', error => { chromeLog += error.message; });
  chrome.stderr.on('data', data => { chromeLog = (chromeLog + data).slice(-20000); });
  let debugPort;
  await waitFor(async () => {
    try {
      debugPort = (await readFile(join(temp, 'chrome/DevToolsActivePort'), 'utf8')).split('\n')[0];
      return true;
    } catch { return false; }
  }, 'Chrome debugging endpoint');
  const pages = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();
  ws = new WebSocket(pages.find(page => page.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, {once: true});
    ws.addEventListener('error', reject, {once: true});
  });
  let sequence = 0;
  const pending = new Map();
  function cdp(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
      pending.set(id, {resolve, reject, timer});
      ws.send(JSON.stringify({id, method, params}));
    });
  }
  ws.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const entry = pending.get(message.id);
      if (!entry) return;
      pending.delete(message.id);
      clearTimeout(entry.timer);
      if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
      else entry.resolve(message.result);
    } else if (message.method === 'Runtime.exceptionThrown') {
      exceptions.push(message.params.exceptionDetails);
    } else if (message.method === 'Fetch.requestPaused') {
      secureImageRequests += 1;
      cdp('Fetch.fulfillRequest', {
        requestId: message.params.requestId, responseCode: 200,
        responseHeaders: [{name: 'Content-Type', value: 'image/png'}], body: png.toString('base64')
      }).catch(error => exceptions.push(error.message));
    }
  });
  async function evaluate(expression) {
    const result = await cdp('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  const store = "Alpine.$data(document.querySelector('[x-data]'))";
  const onStore = code => evaluate(`(() => {const store = ${store}; ${code}})()`);
  await cdp('Page.enable');
  await cdp('Runtime.enable');
  await cdp('Network.enable');
  await cdp('Network.setBypassServiceWorker', {bypass: true});
  await cdp('Fetch.enable', {patterns: [{urlPattern: 'https://artwork.example.test/*'}]});
  await cdp('Emulation.setDeviceMetricsOverride', {width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false});
  await cdp('Page.navigate', {url: `http://127.0.0.1:${proxyPort}/?tab=search`});
  await waitFor(() => evaluate(`!!(window.Alpine && document.querySelector('[x-data]')._x_dataStack && ${store}.settingsLoaded)`), 'Alpine hydration');
  const fixtures = [
    {unique_id: 'empty', note: 'No artwork', images: []},
    {unique_id: 'fallback', note: 'Fallback artwork', images: ['/smoke-images/bad-first.png', '/smoke-images/good.png']},
    {unique_id: 'exhausted', note: 'Unavailable artwork', images: ['/smoke-images/never.png']},
    {unique_id: 'secure', note: 'Secure artwork', images: ['http://artwork.example.test/poster.png']}
  ].map(item => ({...item, url: `https://pan.quark.cn/s/${item.unique_id}`, cloud_type: 'quark', is_valid: true}));
  await onStore(`store.searchResults = ${JSON.stringify(fixtures)}.map(item => store.prepareSearchResult(item)); store.searchHasRun = true;`);
  await waitFor(() => evaluate("document.querySelectorAll('.search-result-card').length === 4"), 'result cards');
  await evaluate("document.querySelector('.search-result-grid').scrollIntoView()");
  await waitFor(() => evaluate(`(() => {
    const cards = [...document.querySelectorAll('.search-result-card')];
    return cards.filter(card => card.classList.contains('is-text-only')).length === 2
      && cards.filter(card => card.querySelector('img')?.naturalWidth > 0).length === 2;
  })()`), 'image fallback and collapsed empty artwork');
  assert.ok(imageRequests.some(path => path.startsWith('/smoke-images/bad-first.png')));
  assert.ok(imageRequests.includes('/smoke-images/good.png'));
  assert.ok(secureImageRequests > 0, 'HTTP artwork must be requested over HTTPS');
  assert.deepEqual(emptyImageRequests, [], 'empty artwork must not request the HTML shell');
  assert.equal(await evaluate("document.querySelector('.search-result-list') === null"), true, 'inactive views must not mount images');

  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: 1100, deviceScaleFactor: 1, mobile: width < 500});
    for (const mode of ['poster', 'list']) {
      await onStore(`store.setSearchViewMode('${mode}');`);
      await waitFor(() => evaluate(`document.querySelectorAll('${mode === 'poster' ? '.search-result-card' : '.search-result-list-row'}').length === 4`), `${width}px ${mode} view`);
      assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, `${width}px ${mode} must not overflow`);
      if (mode === 'poster') {
        assert.equal(await evaluate(`Array.from(document.querySelectorAll('.search-result-card.is-text-only')).every(card => !card.querySelector('img') && card.querySelector('.search-result-poster').getBoundingClientRect().height < 100)`), true, 'empty cards must not reserve poster space');
      } else {
        assert.equal(await evaluate("document.querySelectorAll('.search-list-thumb').length"), 2, 'list must not mount empty thumbnails');
      }
    }
  }
  await onStore("store.openSearchResultPreview(store.searchResults.find(item => item.unique_id === 'empty'));");
  await waitFor(() => evaluate("!!document.querySelector('.search-preview-facts')"), 'empty artwork preview');
  assert.equal(await evaluate("document.querySelector('.search-preview-poster') === null"), true);
  assert.equal(await evaluate("document.querySelectorAll('.search-preview-facts > div').length"), 4, 'preview metadata must remain');
  await onStore("store.closeSearchResultPreview(); store.openSearchResultPreview(store.searchResults.find(item => item.unique_id === 'fallback'));");
  await waitFor(() => evaluate("document.querySelector('.search-preview-poster img')?.naturalWidth > 0"), 'preview fallback image');
  await onStore('store.closeSearchResultPreview();');

  async function pressEnter(keyword) {
    await onStore(`store.searchQuery = ${JSON.stringify(keyword)}; document.querySelector('[x-ref="globalSearchInput"]').dispatchEvent(new KeyboardEvent('keyup', {key: 'Enter', bubbles: true}));`);
  }
  await pressEnter('old query');
  await waitFor(() => searches.length === 1, 'first search request');
  await pressEnter('old query');
  await delay(150);
  assert.equal(searches.length, 1, 'repeated Enter must not duplicate a request');
  await pressEnter('new query');
  await waitFor(() => searches.length === 2, 'newer search request');
  assert.equal(searches[1].body.keyword, 'new query');
  const reply = (entry, note) => {
    if (entry.res.destroyed) return;
    entry.res.writeHead(200, {'content-type': 'application/json'});
    entry.res.end(JSON.stringify({ok: true, data: [{...fixtures[0], note}], partial: false}));
  };
  reply(searches[1], 'Newest result');
  await waitFor(() => onStore("return !store.searching && store.searchResults[0]?.note === 'Newest result'"), 'newer result');
  reply(searches[0], 'Stale result');
  await delay(200);
  assert.equal(await onStore('return store.searchResults[0].note'), 'Newest result');
  assert.equal(await onStore('return store.uiError'), '');

  // Normalization uses the actual local backend; only the TMDB response is a fixture.
  await onStore(`
    store.selectTab('subscriptions');
    store.settings.metadata_provider = 'tmdb';
    store.settings.tmdb_api_key_configured = true;
    store.openBlankSubscriptionDialog();
    store.newSubscription.title = '【本地演示】星海旅人之远航篇 (2024) 第2季 1080p';
    store.newSubscription.media_type = 'anime';
    store.newSubscription.season_input = '1,3';
    store.newSubscription.season_list = [1, 3];
    store.applyMagicTitleMatch({silent: true});
  `);
  await waitFor(() => onStore('return store.metadataSearching'), 'metadata loading state');
  assert.equal(await evaluate(`document.querySelector('[x-show="metadataSearching"]').checkVisibility()`), true);
  await waitFor(() => onStore("return !store.metadataSearching && store.newSubscription.metadata?.title === '星海旅人'"), 'adjusted title result');
  assert.equal(metadataSearches.length, 1);
  assert.equal(metadataSearches[0].query, '星海旅人之远航篇');
  assert.equal(metadataSearches[0].year, '2024');
  assert.match(metadataSearches[0].source_title, /1080p/);
  assert.equal(await onStore('return store.newSubscription.title'), '星海旅人');
  assert.equal(await onStore('return store.newSubscription.season_input'), '1,3');
  assert.deepEqual(await onStore('return [...store.metadataSearchQueries]'), ['星海旅人之远航篇', '星海旅人']);
  const dialog = `document.querySelector('[data-media-editor]')`;
  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: 1100, deviceScaleFactor: 1, mobile: width < 500});
    await waitFor(() => evaluate('innerWidth === ' + width), 'metadata viewport');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, `${width}px metadata page must not overflow`);
    assert.equal(await evaluate(`${dialog}.scrollWidth <= ${dialog}.clientWidth`), true, `${width}px metadata dialog must not overflow`);
    assert.match(await evaluate(`${dialog}.innerText`), /尝试剧名：星海旅人之远航篇 \/ 星海旅人/);
    if (width === 1440 && process.argv.includes('--metadata-screenshot')) {
      await waitFor(() => evaluate("document.querySelectorAll('#toastContainer .toast').length === 0"), 'toast expiry before screenshot');
      await evaluate('window.scrollTo(0, 0); document.fonts.ready');
      await evaluate(`document.querySelector('[x-show="metadataSearchAvailable()"]').scrollIntoView({block: 'center', behavior: 'instant'})`);
      assert.equal(await evaluate(`(() => {
        const notice = document.querySelector('[x-text="metadataSearchMessage"]').getBoundingClientRect();
        return notice.top >= ${dialog}.getBoundingClientRect().top && notice.bottom <= document.querySelector('.media-dialog-actions').getBoundingClientRect().top;
      })()`), true, 'metadata screenshot must show the actual title fallback result');
      const screenshot = await cdp('Page.captureScreenshot', {format: 'png'});
      const asset = join(root, 'docs/assets/metadata-match.png');
      await mkdir(dirname(asset), {recursive: true});
      await writeFile(asset, Buffer.from(screenshot.data, 'base64'));
    }
  }
  await onStore(`
    store.newSubscription.title = '未收录剧集之旅行篇';
    store.newSubscription.metadata = null;
    store.searchMetadataForSubscription(true);
  `);
  await waitFor(() => onStore('return !store.metadataSearching && store.metadataSearchQueries[0] === "未收录剧集之旅行篇"'), 'exhausted metadata attempts');
  assert.equal(await onStore('return store.newSubscription.title'), '未收录剧集之旅行篇');
  assert.equal(await onStore('return store.newSubscription.metadata'), null);
  assert.match(await evaluate(`${dialog}.innerText`), /仍未匹配到元数据/);
  assert.deepEqual(exceptions, [], 'Alpine must not raise runtime exceptions');

  const screenshotDir = process.argv.includes('--media-screenshots')
    ? await mkdtemp(join(tmpdir(), 'media-sub-ui-shots-')) : null;
  async function mediaScreenshot(name) {
    if (!screenshotDir) return;
    await waitFor(() => evaluate("document.querySelectorAll('#toastContainer .toast').length === 0"), 'toast expiry before media screenshot');
    await evaluate('document.fonts.ready');
    await delay(500);
    const screenshot = await cdp('Page.captureScreenshot', {format: 'png'});
    await writeFile(join(screenshotDir, name + '.png'), Buffer.from(screenshot.data, 'base64'));
  }
  async function hoverVisible(selector) {
    const point = await evaluate(`(() => {
      const element = Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(item => item.checkVisibility() && !item.disabled);
      element.scrollIntoView({block: 'nearest', inline: 'nearest', behavior: 'instant'});
      const rect = element.getBoundingClientRect();
      return {x: rect.left + rect.width / 2, y: rect.top + Math.min(20, rect.height / 2)};
    })()`);
    await cdp('Input.dispatchMouseEvent', {type: 'mouseMoved', ...point});
    await waitFor(() => evaluate(`Array.from(document.querySelectorAll(${JSON.stringify(selector)})).some(element => element.checkVisibility() && element.matches(':hover'))`), selector + ' is hovered');
  }
  async function assertPrimaryContrast(label) {
    const controls = await evaluate(`Array.from(document.querySelectorAll('.btn-primary, .segmented-control > button.is-active'))
      .filter(element => element.checkVisibility() && !element.disabled)
      .map(element => {
        const style = getComputedStyle(element);
        return {text: element.innerText, color: style.color, background: style.backgroundImage};
      })`);
    assert.ok(controls.length > 0, label + ' must exercise visible primary controls');
    for (const control of controls) {
      const stops = control.background.match(/rgba?\([^)]+\)/g) || [];
      assert.ok(stops.length >= 2, label + ' must retain the primary gradient');
      for (const stop of stops) {
        assert.ok(contrast(control.color, stop) >= 4.5, `${label}: ${control.text} must have at least 4.5:1 contrast at ${stop}`);
      }
    }
  }
  async function assertNavigationColors(selector) {
    const icons = await evaluate(`Array.from(document.querySelectorAll(${JSON.stringify(selector + ' .nav-button[data-tab]')}))
      .filter(button => button.checkVisibility())
      .map(button => {
        const icon = button.querySelector('.nav-icon');
        if (!icon) return null;
        const style = getComputedStyle(icon);
        return {color: style.color, background: style.backgroundImage, active: button.classList.contains('is-active')};
      })`);
    assert.equal(icons.length, 8, selector + ' must expose all navigation modules');
    assert.ok(icons.every(Boolean), selector + ' must render dedicated module icons');
    assert.ok(new Set(icons.map(icon => icon.color)).size >= 7, selector + ' must use distinct module colors');
    const active = icons.filter(icon => icon.active);
    assert.equal(active.length, 1, selector + ' must have one active module');
    const stops = active[0].background.match(/rgba?\([^)]+\)/g) || [];
    assert.ok(stops.length >= 2, selector + ' must retain the active icon gradient');
    for (const stop of stops) {
      assert.ok(contrast(active[0].color, stop) >= 3, selector + ' active icon must remain legible');
    }
  }
  await cdp('Emulation.setDeviceMetricsOverride', {width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false});
  await onStore(`
    store.showSubscriptionDialog = false;
    store.settings.metadata_provider = 'none';
    store.settings.quark_cookie_configured = true;
    store.settings.aria2_rpc_url = 'http://local-aria2.fixture/jsonrpc';
    store.settings.aria2_movie_dir = '/downloads/movies';
    store.subscriptionViewMode = 'poster';
    store.subscriptionsLastLoadedAt = Date.now();
    store.subscriptions = ['星海旅人', '夏日来信', '城市慢游', '风的另一边'].map((title, index) => ({
      id: 'local-show-' + index, title, media_type: index % 2 ? 'series' : 'anime', season: 1,
      status: 'active', current_episode_number: index + 2, total_episode_number: 12,
      last_checked_at: Math.floor(Date.now() / 1000),
      rules: {target_dir: '/片库/' + title, preferred_resolution: index % 2 ? '1080p' : '2160p'},
      metadata: {title, vote_average: 8.1 + index / 10}
    }));
    store.selectTab('subscriptions');
  `);
  await waitFor(() => evaluate("document.querySelectorAll('.library-poster-card').length === 4"), 'poster wall');
  assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true);
  await assertNavigationColors('.app-sidebar');
  await assertPrimaryContrast('light theme');
  await hoverVisible('.btn-primary');
  await assertPrimaryContrast('light theme hover');
  await cdp('Input.dispatchMouseEvent', {type: 'mouseMoved', x: 0, y: 0});
  await mediaScreenshot('library-desktop');
  await waitFor(() => evaluate("document.querySelector('.library-poster-card').getAnimations().every(animation => animation.playState === 'finished')"), 'poster arrival completes');
  await hoverVisible('.library-poster-card');
  await waitFor(() => evaluate("new DOMMatrixReadOnly(getComputedStyle(document.querySelector('.library-poster-card')).transform).m42 < -3.5"), 'poster hover lifts after arrival');
  await cdp('Emulation.setEmulatedMedia', {features: [{name: 'prefers-reduced-motion', value: 'reduce'}]});
  const motion = await evaluate(`Array.from(document.querySelectorAll('.library-poster-card')).map(card => {
    const style = getComputedStyle(card);
    return {delay: style.animationDelay, duration: style.animationDuration};
  })`);
  assert.ok(motion.every(style => parseFloat(style.delay) === 0 && parseFloat(style.duration) <= 0.001), 'reduced motion must remove staggered arrival delays');
  await cdp('Emulation.setEmulatedMedia', {features: []});
  await cdp('Input.dispatchMouseEvent', {type: 'mouseMoved', x: 0, y: 0});
  await onStore("store.applyTheme('dark', {persist: false});");
  assert.equal(await evaluate("document.documentElement.getAttribute('data-theme')"), 'dark');
  await assertPrimaryContrast('dark theme');
  await hoverVisible('.btn-primary');
  await assertPrimaryContrast('dark theme hover');
  await cdp('Input.dispatchMouseEvent', {type: 'mouseMoved', x: 0, y: 0});
  await mediaScreenshot('library-desktop-dark');
  await onStore("store.applyTheme('light', {persist: false});");
  await cdp('Emulation.setDeviceMetricsOverride', {width: 390, height: 844, deviceScaleFactor: 1, mobile: true});
  assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, 'mobile poster wall must not overflow');
  await assertNavigationColors('.mobile-tab-grid');
  await assertPrimaryContrast('mobile light theme');
  await mediaScreenshot('library-mobile');

  await onStore(`
    store.openBlankSubscriptionDialog();
    store.setSubscriptionMediaType('movie');
    store.newSubscription.title = '午后奇遇（本地演示）';
    store.newSubscription.url = 'https://pan.quark.cn/s/local-movie-versions';
    store.detectSubscriptionSeasons();
  `);
  await waitFor(() => onStore('return !store.subscriptionSeasonsDetecting && store.subscriptionQualities.length === 2'), 'quality choices');
  assert.equal(await onStore('return store.subscriptionMode'), 'once');
  assert.deepEqual(await onStore('return store.subscriptionWizardSteps.map(step => step.id)'), ['content', 'download']);
  await evaluate(`Array.from(document.querySelectorAll('[aria-label="选择资源画质"] button')).find(button => button.innerText.includes('4K')).click()`);
  await waitFor(() => evaluate("document.querySelectorAll('input[name=movie-resource]').length === 2"), '4K movie editions');
  await evaluate("document.querySelector('input[name=movie-resource][value=movie-cut]').click()");
  await waitFor(() => onStore("return store.newSubscription.movie_file_key === 'movie-cut'"), 'exact movie edition');
  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: width === 390 ? 844 : 1100, deviceScaleFactor: 1, mobile: width < 500});
    await waitFor(() => evaluate('innerWidth === ' + width), 'movie viewport');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, `${width}px movie page must not overflow`);
    assert.equal(await evaluate(`${dialog}.scrollWidth <= ${dialog}.clientWidth`), true, `${width}px movie editor must not overflow`);
    assert.equal(await evaluate(`${dialog}.getAttribute('aria-label')`), '电影下载保存');
    if (width === 390) await evaluate("document.querySelector('.quality-picker').scrollIntoView({block: 'center'})");
    else await evaluate(`${dialog}.scrollTop = 0`);
    await mediaScreenshot('movie-quality-' + width);
    await evaluate("document.querySelector('.movie-resource-option.is-selected').scrollIntoView({block: 'center'})");
    assert.equal(await evaluate(`(() => {
      const option = document.querySelector('.movie-resource-option.is-selected').getBoundingClientRect();
      const actions = document.querySelector('.media-dialog-actions').getBoundingClientRect();
      return option.top >= ${dialog}.getBoundingClientRect().top && option.bottom <= actions.top;
    })()`), true, `${width}px selected version must be scrollable above the sticky footer`);
  }
  await onStore('store.nextSubscriptionStep()');
  await waitFor(() => onStore("return store.subscriptionDialogTab === 'download'"), 'movie save step');
  await evaluate(`document.querySelectorAll('[aria-label="选择电影保存方式"] button')[1].click()`);
  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: width === 390 ? 844 : 1100, deviceScaleFactor: 1, mobile: width < 500});
    await evaluate(`${dialog}.scrollTop = 0`);
    assert.equal(await evaluate(`${dialog}.scrollWidth <= ${dialog}.clientWidth`), true, `${width}px movie download options must not overflow`);
    if (width === 390) {
      assert.equal(await evaluate("document.querySelector('.save-resource-summary > .min-w-0').getBoundingClientRect().width >= 180"), true, 'mobile movie title must not be squeezed by its badge');
    }
    assert.equal(await evaluate(`(() => {
      const actions = document.querySelector('.media-dialog-actions').getBoundingClientRect();
      return actions.bottom <= innerHeight && actions.top >= 0;
    })()`), true, `${width}px save actions must remain in the viewport`);
    await mediaScreenshot('movie-save-' + width);
  }
  await onStore('store.confirmSubscription()');
  await waitFor(() => mediaSaves.length === 1, 'one-time movie save request');
  assert.equal(mediaSaves[0].save_options.media_type, 'movie');
  assert.equal(mediaSaves[0].save_options.preferred_resolution, '2160p');
  assert.deepEqual(mediaSaves[0].save_options.file_ids, ['movie-cut']);
  assert.equal(mediaSaves[0].save_options.sync_download_enabled, true);
  assert.equal(mediaSaves[0].save_options.sync_download_dir.includes('Season'), false);
  await waitFor(() => onStore('return !store.showSubscriptionDialog'), 'saved movie editor closes');
  await onStore(`
    store.openBlankSubscriptionDialog();
    store.newSubscription.url = 'https://pan.quark.cn/s/local-series-versions';
    store.detectSubscriptionSeasons();
  `);
  await waitFor(() => onStore('return !store.subscriptionSeasonsDetecting && store.subscriptionQualities.length === 2'), 'series quality choices');
  await evaluate(`Array.from(document.querySelectorAll('[aria-label="选择资源画质"] button')).find(button => button.innerText.includes('1080P')).click()`);
  assert.equal(await onStore('return store.buildSubscriptionRules().preferred_resolution'), '1080p');
  assert.equal(await onStore('return store.subscriptionMode'), 'continuous');
  await onStore(`
    store.showSubscriptionDialog = false;
    store.sourceSwitchSubscriptionId = 'local-show-0';
    store.sourceSwitchSubscriptionTitle = '星海旅人';
    const candidate = {id: 'wrong-quality-source', source: '本地演示', note: '只有其他画质', url: 'https://pan.quark.cn/s/wrong-quality', quality: {score: 84, grade: '良好'}, probe_info: {ok: true, files: []}};
    store.sourceSwitchCandidates = [candidate];
    store.sourceSwitchPreview = {candidate, current_quality: {score: 80}, score_delta: 4, probe_ok: true, quality_matches: false, season_matches: true, covers_progress: false, can_apply: false, auto_eligible: false, warnings: ['候选不含所选画质']};
    store.showSourceSwitchDialog = true;
  `);
  await waitFor(() => evaluate(`Array.from(document.querySelectorAll('[aria-label="换源候选"] button')).some(button => button.innerText === '画质不匹配' && button.disabled)`), 'quality mismatch blocks source application');
  assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, 'source quality warning must not overflow');
  await onStore("store.showSourceSwitchDialog = false; store.selectTab('dashboard');");
  await waitFor(() => evaluate("document.querySelector('.dash-command').checkVisibility()"), 'dashboard visible');
  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: width === 390 ? 844 : 1100, deviceScaleFactor: 1, mobile: width < 500});
    for (const theme of ['light', 'dark']) {
      await onStore(`store.applyTheme('${theme}', {persist: false});`);
      assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, `${width}px ${theme} dashboard must not overflow`);
      await assertNavigationColors(width === 390 ? '.mobile-tab-grid' : '.app-sidebar');
      await assertPrimaryContrast(`${width}px ${theme} dashboard`);
      await mediaScreenshot(`dashboard-${width}-${theme}`);
    }
  }

  async function pressTab(shift = false) {
    const key = {key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9, modifiers: shift ? 8 : 0};
    await cdp('Input.dispatchKeyEvent', {type: 'keyDown', ...key});
    await cdp('Input.dispatchKeyEvent', {type: 'keyUp', ...key});
  }
  await onStore('store.openBlankSubscriptionDialog();');
  await waitFor(() => evaluate(`document.activeElement === ${dialog}`), 'media editor initial focus');
  await pressTab(true);
  assert.equal(await evaluate(`document.activeElement.closest('[data-media-editor]') !== null && document.activeElement.innerText === '下一步'`), true, 'Shift+Tab from the dialog must wrap to its last visible action');
  await onStore('store.showSubscriptionDialog = false; store.requestDangerConfirmation({title: "本地键盘检查"});');
  await waitFor(() => evaluate("document.activeElement.matches('[role=alertdialog]')"), 'confirmation initial focus');
  await pressTab(true);
  assert.equal(await evaluate('document.activeElement.innerText'), '确认执行');
  await pressTab();
  assert.equal(await evaluate('document.activeElement.innerText'), '取消', 'hidden phrase input must not break the focus loop');
  await pressTab(true);
  assert.equal(await evaluate('document.activeElement.innerText'), '确认执行');
  await onStore('store.resolveDangerConfirmation(false);');
  await evaluate(`document.querySelector('button[aria-label="刷新当前页面"]').focus()`);
  await onStore('store.showNewFolderModal = true;');
  await waitFor(() => evaluate(`document.activeElement.matches('[role=dialog][aria-label="新建文件夹"]')`), 'new folder receives focus');
  await pressTab();
  assert.equal(await evaluate('document.activeElement.id'), 'field-newfoldername');
  await onStore('store.showNewFolderModal = false; store.selectTab("settings");');
  async function activeMobileTabIsVisible() {
    return evaluate(`(() => {
      const navigation = document.querySelector('.mobile-tab-grid');
      const active = navigation.querySelector('[aria-current="page"]');
      if (!active || active.dataset.tab !== ${store}.currentTab) return false;
      const viewport = navigation.getBoundingClientRect();
      const item = active.getBoundingClientRect();
      return item.left >= viewport.left - 1 && item.right <= viewport.right + 1;
    })()`);
  }
  await waitFor(activeMobileTabIsVisible, 'settings stays visible in the mobile navigation');
  await onStore('store.selectTab("dashboard");');
  await waitFor(activeMobileTabIsVisible, 'navigation scrolls back to dashboard');
  await evaluate('history.back()');
  await waitFor(() => onStore('return store.currentTab === "settings"'), 'history restores settings');
  await waitFor(activeMobileTabIsVisible, 'history restores the visible navigation item');
  await cdp('Emulation.setDeviceMetricsOverride', {width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false});
  await evaluate('document.querySelector(".mobile-tab-grid").scrollLeft = 0');
  await cdp('Emulation.setDeviceMetricsOverride', {width: 390, height: 844, deviceScaleFactor: 1, mobile: true});
  await waitFor(activeMobileTabIsVisible, 'resizing reveals the active navigation item');


  // ---- 可用性二期：待处理入口、布局编辑、手机工具区、设置概览、连接状态 ----
  const setViewport = width => cdp('Emulation.setDeviceMetricsOverride', {width, height: width === 390 ? 844 : 1100, deviceScaleFactor: 1, mobile: width < 500});
  const noOverflow = async label => assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, label + ' must not overflow');
  const visible = selector => evaluate(`Array.from(document.querySelectorAll(${JSON.stringify(selector)})).some(item => item.checkVisibility())`);
  // 轮询与事件流会用后端的空数据覆盖演示状态，因此「写入状态、等待渲染、
  // 读取或点击」放在同一次求值里完成。
  const withState = (setup, read) => evaluate(`(async () => {
    const store = ${store};
    // Alpine 可能把 x-show 推迟到下一帧，只等 nextTick 会读到尚未显示的元素。
    const settle = async () => { await Alpine.nextTick(); await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); };
    ${setup}
    await settle();
    ${read}
  })()`);
  const attentionState = `
    store.subscriptionsLastLoadedAt = Date.now();
    store.subscriptions = store.subscriptions.map((sub, index) => ({...sub, status: index === 0 ? 'invalid' : 'active', last_error: index === 0 ? '分享链接已失效' : ''}));
    store.jobs = [{id: 'local-failed-job', kind: 'subscription_transfer', status: 'failed', title: '转存失败', error: '本地演示'}, {id: 'local-push-job', kind: 'push_dispatch', status: 'failed', title: '推送派发'}];
    store.notifications = [];
    store.applyTheme('light', {persist: false});
    if (store.currentTab !== 'dashboard') store.selectTab('dashboard');
  `;
  await setViewport(390);
  const attention = await withState(attentionState, `
    return Array.from(document.querySelectorAll('.dash-attention-item')).filter(item => item.checkVisibility()).map(item => ({
      text: item.innerText.replace(/\\s+/g, ' ').trim(), height: item.getBoundingClientRect().height,
      color: getComputedStyle(item).color
    }));
  `);
  assert.deepEqual(attention.map(item => item.text), ['1 失效订阅 →', '1 失败任务 →'], 'only categories with pending items are offered');
  assert.ok(attention.every(item => item.height >= 44), 'attention entries must be touch sized on phones');
  for (const item of attention) {
    assert.ok(contrast(item.color, 'rgb(255, 255, 255)') >= 4.5, `${item.text} must stay readable in the light theme`);
  }
  await noOverflow('390px dashboard attention entries');
  await mediaScreenshot('attention-390-light');
  assert.deepEqual(await withState(attentionState, `
    document.querySelector('.dash-attention-item.is-danger').click();
    return [store.currentTab, store.subscriptionStatusTab, store.filteredSubscriptions.map(sub => sub.id)];
  `), ['subscriptions', 'invalid', ['local-show-0']], 'invalid entry opens the filtered subscription list');
  assert.deepEqual(await withState(attentionState, `
    store.activityQuery = '不相关的关键词';
    document.querySelector('.dash-attention-item.is-warning').click();
    // 切页会向后端重新拉取任务（本地后端没有任务），因此同步读取筛选结果。
    return [store.currentTab, store.activityFilter, store.filteredActivityItems.map(item => item.id)];
  `), ['notifications', 'failed', ['job:local-failed-job']], 'failed entry opens the activity centre filtered to the counted failures');

  // 布局编辑：没有无法兑现的排序按钮，最后一张卡片不能隐藏。
  await setViewport(1440);
  assert.deepEqual(await withState(`store.selectTab('dashboard'); store.startDashboardEdit();`, `
    const buttons = () => Array.from(document.querySelectorAll('.dash-card-edit-actions button')).filter(item => item.checkVisibility());
    const initial = buttons().map(item => [item.getAttribute('aria-label'), item.disabled]);
    buttons()[0].click();
    await settle();
    const afterHide = buttons().map(item => [item.getAttribute('aria-label'), item.disabled]);
    buttons()[0].click();
    await settle();
    const result = {initial, afterHide, layout: [...store.dashboardLayout], hidden: store.dashboardHiddenCards().map(card => card.id)};
    store.cancelDashboardEdit();
    return result;
  `), {
    initial: [['隐藏概览与操作', false], ['隐藏更新日历', false]],
    afterHide: [['隐藏更新日历', true]],
    layout: ['calendar'], hidden: ['command']
  }, 'the last visible dashboard card cannot be hidden');

  // 手机订阅页：批量操作收进「更多」，片单进入首屏。
  const subscriptionState = `
    store.subscriptionsLastLoadedAt = Date.now();
    store.subscriptions = store.subscriptions.map(sub => ({...sub, status: 'active', last_error: ''}));
    store.setSubscriptionStatusTab('active');
    store.subscriptionViewMode = 'poster';
    store.selectedSubscriptionIds = [];
    store.subscriptionToolsOpen = false;
    if (store.currentTab !== 'subscriptions') store.selectTab('subscriptions');
  `;
  await setViewport(390);
  const phoneLibrary = await withState(subscriptionState, `
    const rect = selector => document.querySelector(selector).getBoundingClientRect();
    return {
      toolbar: rect('.subs-toolbar').height, firstCardTop: rect('.library-poster-card').top,
      toggle: document.querySelector('.subs-tools-toggle').checkVisibility(),
      tools: document.querySelector('.subs-tools').checkVisibility(),
      expanded: document.querySelector('.subs-tools-toggle').getAttribute('aria-expanded'),
      add: Array.from(document.querySelectorAll('.subs-toolbar button')).some(item => item.checkVisibility() && item.innerText.trim() === '添加媒体')
    };
  `);
  assert.deepEqual([phoneLibrary.toggle, phoneLibrary.tools, phoneLibrary.expanded, phoneLibrary.add], [true, false, 'false', true], 'phone toolbar keeps only the primary action expanded');
  assert.ok(phoneLibrary.toolbar <= 150, `phone subscription toolbar must stay compact, got ${phoneLibrary.toolbar}px`);
  assert.ok(phoneLibrary.firstCardTop <= 844 * 0.45, `posters must start in the upper half of the first phone screen, got ${phoneLibrary.firstCardTop}px`);
  await noOverflow('390px subscriptions');
  await mediaScreenshot('subscriptions-390-collapsed');
  const phoneTools = await withState(subscriptionState, `
    document.querySelector('.subs-tools-toggle').click();
    await settle();
    const tools = document.querySelector('.subs-tools');
    return {
      expanded: document.querySelector('.subs-tools-toggle').getAttribute('aria-expanded'),
      labels: Array.from(tools.querySelectorAll('button, label')).filter(item => item.checkVisibility()).map(item => item.innerText.trim()),
      smallest: Math.min(...Array.from(tools.querySelectorAll('button')).filter(item => item.checkVisibility()).map(item => item.getBoundingClientRect().height)),
      overflow: document.documentElement.scrollWidth > innerWidth
    };
  `);
  assert.equal(phoneTools.expanded, 'true');
  assert.deepEqual(phoneTools.labels, ['批量检查', '刷新元数据', '选择当前显示', '表格', '海报墙'], 'every batch tool stays reachable on phones');
  assert.ok(phoneTools.smallest >= 44, 'expanded phone tools must be touch sized');
  assert.equal(phoneTools.overflow, false, 'expanded phone tools must not overflow');
  await mediaScreenshot('subscriptions-390-expanded');
  await setViewport(1440);
  assert.deepEqual(await withState(subscriptionState, `
    return [document.querySelector('.subs-tools-toggle').checkVisibility(), document.querySelector('.subs-tools').checkVisibility(), document.querySelector('.subs-toolbar-desc').checkVisibility()];
  `), [false, true, true], 'desktop keeps the full subscription toolbar');
  await noOverflow('1440px subscriptions');
  await mediaScreenshot('subscriptions-1440');

  // 手机设置页：概览收起，只列未完成的必需项，表单进入首屏。
  const settingsState = `
    store.settings.app_password_configured = true;
    store.settings.quark_cookie_configured = true;
    store.settings.quark_cookie = '';
    store.settings.quark_save_movie_dir = '';
    store.settings.quark_save_series_dir = '';
    store.settings.quark_save_anime_dir = '';
    store.settings.custom_categories = [];
    store.settings.subscription_scheduler_enabled = true;
    store.settingsOverviewOpen = false;
    if (store.currentTab !== 'settings') store.selectTab('settings');
  `;
  const overviewRead = `
    const overview = document.querySelector('.settings-overview');
    return {
      height: overview.getBoundingClientRect().height,
      navTop: document.querySelector('.settings-nav').getBoundingClientRect().top,
      items: Array.from(overview.querySelectorAll('.settings-overview-item')).filter(item => item.checkVisibility()).map(item => item.querySelector('strong').innerText),
      summary: overview.querySelector('.settings-overview-summary').innerText,
      toggle: overview.querySelector('.settings-overview-toggle').checkVisibility(),
      actions: overview.querySelector('.settings-overview-actions').checkVisibility(),
      expanded: overview.querySelector('.settings-overview-toggle').getAttribute('aria-expanded')
    };
  `;
  await setViewport(390);
  const collapsed = await withState(settingsState, overviewRead);
  assert.deepEqual([collapsed.items, collapsed.summary, collapsed.toggle, collapsed.actions, collapsed.expanded],
    [['媒体目录'], '还有 1 项必需配置待完成', true, false, 'false'], 'collapsed overview lists only unfinished required items');
  assert.ok(collapsed.navTop <= 844 * 0.5, `settings form must start in the upper half of the first phone screen, got ${collapsed.navTop}px`);
  await noOverflow('390px settings');
  await mediaScreenshot('settings-390-collapsed');
  const expandedOverview = await withState(settingsState + `
    await settle();
    document.querySelector('.settings-overview-toggle').click();
  `, overviewRead);
  assert.equal(expandedOverview.items.length, 7, 'expanding restores every overview item');
  assert.deepEqual([expandedOverview.actions, expandedOverview.expanded], [true, 'true']);
  assert.ok(expandedOverview.height > collapsed.height);
  const complete = await withState(settingsState + `store.settings.quark_save_series_dir = '/连续剧';`, overviewRead);
  assert.deepEqual([complete.items, complete.summary], [[], '必需配置已全部完成'], 'a finished setup collapses to a single summary row');
  assert.ok(complete.height <= 90, `finished overview must collapse to one row, got ${complete.height}px`);
  await setViewport(1440);
  const desktopOverview = await withState(settingsState, overviewRead);
  assert.deepEqual([desktopOverview.items.length, desktopOverview.toggle, desktopOverview.actions], [7, false, true], 'desktop always shows the full overview');
  await noOverflow('1440px settings');

  // 连接状态：取自真实请求结果。拦截接口后必须显示未响应，恢复后回到在线。
  const connection = () => evaluate(`(() => {
    const store = ${store};
    const status = document.querySelector('.brand-status');
    return {
      state: store.connectionState, label: status.innerText.trim(), tone: status.className,
      header: document.querySelector('.header-health').innerText.trim(),
      banner: Array.from(document.querySelectorAll('.pwa-status-banner')).filter(item => item.checkVisibility()).map(item => item.innerText.replace(/\\s+/g, ' ').trim())
    };
  })()`);
  await onStore('return store.loadNotifications()');
  let status = await connection();
  assert.deepEqual([status.state, status.label, status.banner], ['online', '服务在线', []]);
  assert.ok(await onStore('return store.serverLastOkAt > 0'), 'a successful response records when the service last answered');
  await cdp('Network.setBlockedURLs', {urls: ['*/api/notifications*']});
  // 失败请求返回后同步读取，避免事件流的下一条消息先把状态改回在线。
  assert.equal(await onStore('return store.loadNotifications().then(() => store.connectionState)'), 'unreachable', 'a failed request must be reflected in the status');
  await cdp('Network.setBlockedURLs', {urls: []});
  assert.deepEqual(await withState(`store.markServerReachable(false);`, `
    const status = document.querySelector('.brand-status');
    return [status.innerText.trim(), status.classList.contains('is-unreachable'), document.querySelector('.header-health').innerText.trim(),
      Array.from(document.querySelectorAll('.pwa-status-banner')).filter(item => item.checkVisibility()).map(item => item.innerText.replace(/\\s+/g, ' ').trim())];
  `), ['服务未响应', true, '服务未响应', ['无法连接服务，当前显示的可能是旧数据 重试']], 'an unreachable service is shown in the sidebar, header and banner');
  if (screenshotDir) {
    await onStore('store.markServerReachable(false)');
    const screenshot = await cdp('Page.captureScreenshot', {format: 'png'});
    await writeFile(join(screenshotDir, 'connection-unreachable-1440.png'), Buffer.from(screenshot.data, 'base64'));
  }
  await evaluate(`Array.from(document.querySelectorAll('.pwa-status-banner button')).find(item => item.checkVisibility() && item.innerText.trim() === '重试')?.click()`);
  await onStore('return store.loadNotifications()');
  await waitFor(async () => (await connection()).state === 'online', 'a successful request restores the online status');
  // Alpine 在下一帧才收起 x-show 元素；等界面跟上状态再断言，但横幅必须真的消失。
  await waitFor(async () => (await connection()).banner.length === 0, 'the unreachable banner to disappear once the service answers again', 3000);
  status = await connection();
  assert.deepEqual([status.label, status.banner], ['服务在线', []]);
  assert.deepEqual(await withState(`store.jobEventsReconnecting = true;`, `
    const status = document.querySelector('.brand-status');
    const result = [status.innerText.trim(), status.classList.contains('is-reconnecting')];
    store.jobEventsReconnecting = false;
    return result;
  `), ['正在重连', true], 'a dropped event stream is shown as reconnecting');

  // Read populated detail and download views through the same loaders used by the UI.
  statusFixturesEnabled = true;
  await onStore('return store.loadSubscriptions()');
  await onStore(`return store.openSubscriptionDetail('${statusSubscription.id}')`);
  await waitFor(() => evaluate("document.querySelectorAll('.subscription-episode-cell').length === 6"), 'multi-season episode states');
  assert.equal(await evaluate("Array.from(document.querySelectorAll('.subscription-detail-badges > span')).find(item => item.innerText.includes('季')).innerText"), '第 1,3 季', 'detail must preserve the skip-season set without enriched list fields');
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('.subscription-episode-grid')).map(grid => grid.previousElementSibling.innerText)"), ['第 1 季', '第 3 季']);
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('.subscription-pipeline-step > div:nth-child(2) span')).map(item => item.innerText)"), ['完成', '运行中', '需处理', '失败', '未启用', '等待中']);
  for (const [filter, count] of [['missing', 1], ['pending', 2], ['ready', 2], ['recent', 1], ['all', 6]]) {
    // Match the action attribute directly to avoid escaping its punctuation in CSS.
    await evaluate(`Array.from(document.querySelectorAll('.subscription-episode-filters button')).find(button => button.getAttribute('@click') === "setSubscriptionEpisodeFilter('${filter}')").click()`);
    await waitFor(() => evaluate(`document.querySelectorAll('.subscription-episode-cell').length === ${count}`), filter + ' episode filter');
  }
  for (const width of [1440, 390]) {
    await setViewport(width);
    for (const theme of ['light', 'dark']) {
      await withState(`store.applyTheme('${theme}', {persist: false});`, 'return true;');
      await noOverflow(`${width}px ${theme} populated subscription detail`);
      await mediaScreenshot(`subscription-detail-${width}-${theme}`);
    }
  }
  await onStore("store.settings.aria2_rpc_url = 'http://local-aria2.fixture/jsonrpc'; store.selectTab('downloads'); return store.loadDownloads();");
  await waitFor(() => onStore('return !store.downloadsLoading && !store.downloadsRefreshing && store.allDownloadTasks.length === 6'), 'download state fixtures');
  assert.equal(await onStore('return store.downloadsError'), '');
  assert.equal(await onStore('return store.downloadAutomationStats.linked'), 5);
  const downloadActionStates = setup => withState(setup, `
    return Array.from(document.querySelectorAll('.data-table--downloads .data-table-row')).filter(row => row.checkVisibility()).map(row =>
      Array.from(row.querySelectorAll('.btn-primary, .btn-secondary, .btn-danger')).filter(button => button.checkVisibility()).map(button => button.disabled));
  `);
  let actionStates = await downloadActionStates("store.selectDownloadCategory('downloading'); store.downloadTaskActions = {};");
  assert.equal(actionStates.length, 3);
  assert.ok(actionStates.every(row => row.length > 0 && row.every(disabled => !disabled)), 'idle download actions must be enabled; an empty action string is not a boolean disabled value');
  actionStates = await downloadActionStates("store.downloadTaskActions = {'local-active': 'pause'};");
  assert.ok(actionStates[0].every(disabled => disabled), 'only the task with an action in flight is locked');
  assert.ok(actionStates.slice(1).every(row => row.every(disabled => !disabled)), 'other download tasks remain operable');
  await onStore('store.downloadTaskActions = {};');
  const activeDownloadTabIsVisible = () => evaluate(`(() => {
    const navigation = document.querySelector('.download-tabs');
    const active = navigation.querySelector('[aria-selected="true"]');
    const viewport = navigation.getBoundingClientRect();
    const item = active.getBoundingClientRect();
    return item.left >= viewport.left - 1 && item.right <= viewport.right + 1;
  })()`);
  for (const width of [1440, 390]) {
    await setViewport(width);
    for (const [category, count] of [['downloading', 3], ['queued', 1], ['completed', 1], ['failed', 1]]) {
      const rows = await withState(`store.selectDownloadCategory('${category}');`, `
        return Array.from(document.querySelectorAll('.data-table--downloads .data-table-row')).filter(row => row.checkVisibility()).length;
      `);
      assert.equal(rows, count, `${width}px ${category} download rows`);
      await waitFor(activeDownloadTabIsVisible, `${width}px ${category} download tab stays visible`);
      await noOverflow(`${width}px populated ${category} downloads`);
      await mediaScreenshot(`downloads-${category}-${width}`);
    }
  }
  await evaluate("Array.from(document.querySelectorAll('.download-automation-strip > button')).find(button => button.checkVisibility()).click()");
  await waitFor(() => onStore(`return store.currentTab === 'subscriptions' && store.selectedSubscriptionId === '${statusSubscription.id}' && !store.subscriptionDetailLoading`), 'download opens its subscription detail');
  for (const path of ['/api/subscriptions', `/api/subscriptions/${statusSubscription.id}/status`, `/api/subscriptions/${statusSubscription.id}/pipeline`, '/api/drive/aria2/tasks']) {
    assert.ok(statusFixtureRequests.includes(path), `${path} must be loaded through the API`);
  }
  await setViewport(1440);
  console.log('populated detail and download smoke passed: multi-season filters, six pipeline states, four download categories, subscription navigation, 1440px and 390px');

  // 强调色只做装饰：承载文字的组件必须用可读的文字色，浅色和深色主题都要达到 4.5:1。
  // 渐变、半透明叠层、海报和祖先 opacity 都会改变实际底色，所以不按样式推算，而是
  // 截两张图逐像素比较：一张隐藏全部文字（真实底色），一张把元素铺满它的文字色。
  await evaluate(`(() => {
    const style = document.createElement('style');
    style.id = 'contrast-probe';
    style.textContent = '*, *::before, *::after { transition: none !important; }'
      + 'html.contrast-hide-text *, html.contrast-hide-text *::before, html.contrast-hide-text *::after'
      + ' { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }'
      // Freeze the pulsing status number at its faintest point, if it still animates.
      + '.subscription-pipeline-index { animation-delay: -.75s !important; animation-play-state: paused !important; }'
      // 海报可以是任意图片：把它换成纯黑或纯白，量出叠在上面的徽标的最差情况。
      + 'html[data-poster-probe="black"] .search-result-poster { background: #000 !important; }'
      + 'html[data-poster-probe="white"] .search-result-poster { background: #fff !important; }'
      + 'html[data-poster-probe="black"] .search-result-poster > :is(img, .search-result-poster-fallback),'
      + 'html[data-poster-probe="white"] .search-result-poster > :is(img, .search-result-poster-fallback) { visibility: hidden !important; }';
    document.head.appendChild(style);
  })()`);
  const probe = (id, body) => evaluate(`(async () => {
    const item = document.querySelector('[data-contrast-id="${id}"]');
    const frames = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const box = () => { const rect = item.getBoundingClientRect(); return [rect.left, rect.top, rect.width, rect.height].map(value => Math.round(value * 10) / 10).join(','); };
    if (!item || !item.checkVisibility()) return null;
    ${body}
  })()`);
  async function measureContrast(id) {
    const hidden = await probe(id, `
      item.scrollIntoView({block: 'center', inline: 'center'});
      const color = getComputedStyle(item).color;
      item.dataset.contrastStyle = item.getAttribute('style') || '';
      item.style.setProperty('pointer-events', 'auto', 'important');
      document.documentElement.classList.add('contrast-hide-text');
      await frames();
      const rect = item.getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(item);
      const points = [];
      // 只取文字所在、且确实由该元素绘制的像素：被裁剪、被遮挡或落在圆角外的不算。
      for (const line of range.getClientRects()) {
        const right = Math.min(line.right, rect.right) - 1;
        const bottom = Math.min(line.bottom, rect.bottom) - 1;
        for (let y = Math.ceil(Math.max(line.top, rect.top) + 1); y < bottom; y += 2) {
          for (let x = Math.ceil(Math.max(line.left, rect.left) + 1); x < right; x += 2) {
            const hit = document.elementFromPoint(x + 0.5, y + 0.5);
            if (hit && item.contains(hit)) points.push([x, y]);
          }
        }
      }
      const step = Math.ceil(points.length / 400) || 1;
      return {color, box: box(), points: points.filter((_, index) => index % step === 0),
        clip: {x: Math.floor(rect.left + scrollX), y: Math.floor(rect.top + scrollY), width: Math.ceil(rect.width) + 1, height: Math.ceil(rect.height) + 1, scale: 1},
        origin: [Math.floor(rect.left), Math.floor(rect.top)]};
    `);
    if (!hidden) return {stale: 'element is no longer visible'};
    const backdrop = decodePng(Buffer.from((await cdp('Page.captureScreenshot', {format: 'png', clip: hidden.clip})).data, 'base64'));
    const inked = await probe(id, `
      item.style.setProperty('box-shadow', 'inset 0 0 0 999px ${hidden.color}', 'important');
      await frames();
      return box();
    `);
    const ink = inked === hidden.box
      ? decodePng(Buffer.from((await cdp('Page.captureScreenshot', {format: 'png', clip: hidden.clip})).data, 'base64'))
      : null;
    const restored = await evaluate(`(() => {
      document.documentElement.classList.remove('contrast-hide-text');
      const item = document.querySelector('[data-contrast-id="${id}"]');
      if (!item) return null;
      if (item.dataset.contrastStyle) item.setAttribute('style', item.dataset.contrastStyle); else item.removeAttribute('style');
      delete item.dataset.contrastStyle;
      return item.checkVisibility();
    })()`);
    // 轮询或事件流在两次截图之间改了界面：这次测量作废，由调用方重试。
    if (!ink || !restored) return {stale: !restored ? 'element disappeared during the measurement' : `element moved from ${hidden.box} to ${inked}`};
    if (!hidden.points.length) return {stale: 'no text pixels are painted by the element'};
    let worst = Infinity;
    let pair = '';
    for (const [x, y] of hidden.points) {
      const background = backdrop.pixel(x - hidden.origin[0], y - hidden.origin[1]);
      const foreground = ink.pixel(x - hidden.origin[0], y - hidden.origin[1]);
      const ratio = contrast(foreground, background);
      if (ratio < worst) { worst = ratio; pair = `${foreground} on ${background}`; }
    }
    return {worst, pair, samples: hidden.points.length};
  }
  async function accentText(setup, selectors) {
    for (let attempt = 0; ; attempt++) {
      const items = await withState(setup, `
        const selectors = ${JSON.stringify(selectors)};
        const shown = selector => Array.from(document.querySelectorAll(selector)).filter(item => item.checkVisibility() && item.innerText.trim());
        // x-if 模板要多等几帧才挂载。
        for (let wait = 0; wait < 40 && !selectors.every(selector => shown(selector).length); wait++) await new Promise(resolve => setTimeout(resolve, 50));
        // 入场动画从透明开始：等有限时长的动画播完再量，循环动画（状态圆点等）不等。
        await Promise.all(document.getAnimations().filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime)).map(animation => animation.finished.catch(() => {})));
        document.querySelectorAll('[data-contrast-id]').forEach(item => item.removeAttribute('data-contrast-id'));
        let next = 0;
        return selectors.map(selector => {
          // 同一种外观只量一次：按状态类、文字色和所在卡片（有无海报）去重。
          const seen = new Set();
          const picked = [];
          for (const item of shown(selector)) {
            const state = item.closest('[class*="is-"]');
            const card = item.closest('.search-result-card');
            const signature = [state && state.className, card && card.className, getComputedStyle(item).color].join('|');
            if (seen.has(signature)) continue;
            seen.add(signature);
            // 同一元素可能被多个选择器选中，沿用已分配的编号。
            if (!item.hasAttribute('data-contrast-id')) item.setAttribute('data-contrast-id', String(++next));
            picked.push({id: item.getAttribute('data-contrast-id'), label: item.innerText.trim().replace(/\\s+/g, ' ').slice(0, 20) + (state ? ' [' + state.className + ']' : '')});
          }
          return {selector, items: picked.slice(0, 10)};
        });
      `);
      const results = [];
      let stale = '';
      for (const entry of items) {
        const measured = [];
        for (const item of entry.items) {
          const result = await measureContrast(item.id);
          if (result.stale) { stale = `${entry.selector} "${item.label}": ${result.stale}`; break; }
          measured.push({...item, ...result});
        }
        if (stale) break;
        results.push({selector: entry.selector, measured});
      }
      if (!stale) return results;
      assert.ok(attempt < 4, 'contrast measurement kept failing: ' + stale);
    }
  }
  // 覆盖各档画质徽标：有海报与无海报各一组，分数由真实的前端评分逻辑算出。
  const gradedFixtures = [
    {id: 'excellent', note: '本地演示 2160p HDR H.265 蓝光 Atmos', is_valid: true},
    {id: 'good', note: '本地演示 1080p HDR H.265 蓝光', is_valid: true},
    {id: 'fair', note: '本地演示 1080p', is_valid: true},
    {id: 'muted', note: '本地演示 1080p', is_valid: null},
    {id: 'danger', note: '本地演示 1080p 广告 加群', is_valid: false}
  ].flatMap(item => [[], ['/smoke-images/good.png']].map((images, index) => ({
    unique_id: item.id + '-' + index, note: item.note, images, is_valid: item.is_valid,
    url: 'https://pan.quark.cn/s/' + item.id + index, cloud_type: 'quark'
  })));
  const gradedState = `store.searchResults = ${JSON.stringify(gradedFixtures)}.map(item => store.prepareSearchResult(item)); store.searchHasRun = true; store.searchViewMode = 'poster'; store.searchResultFilter = 'all'; if (store.currentTab !== 'search') store.selectTab('search');`;
  const accentTextCases = [
    {setup: `store.searchResults = ${JSON.stringify(fixtures)}.map(item => store.prepareSearchResult(item)); store.searchHasRun = true; store.searchViewMode = 'poster'; store.searchResultFilter = 'all'; store.selectTab('search');`,
      selectors: ['.search-result-tags > span', '.search-quality-pill']},
    {setup: gradedState,
      selectors: ['.search-quality-pill.is-excellent', '.search-quality-pill.is-good', '.search-quality-pill.is-fair', '.search-quality-pill.is-danger', '.search-quality-pill.is-muted',
        '.search-card-badges .badge-success', '.search-card-badges .badge-danger', '.search-card-badges .badge-muted', '.search-result-risk p']},
    // 同一组徽标压在纯黑、纯白的海报上各量一遍：可读性不能取决于图片内容。
    ...['black', 'white'].map(poster => ({poster, setup: gradedState,
      selectors: ['.search-result-card:not(.is-text-only) .search-card-badges > span']})),
    {setup: `${settingsState} store.settings.quark_cookie_configured = false; store.settingsOverviewOpen = true; store.markServerReachable(false);`,
      selectors: ['.settings-overview-score strong', '.settings-overview-item.is-ready .settings-overview-state', '.settings-overview-item.is-required .settings-overview-state', '.settings-overview-item.is-optional strong', '.settings-overview-item.is-optional small', '.settings-overview-item strong', '.settings-overview-item small', '.pwa-status-banner.is-offline > span']},
    {setup: `store.markServerReachable(true); store.selectTab('dashboard'); store.startDashboardEdit(); store.toggleDashboardCard('calendar');`,
      selectors: ['.dashboard-chip']},
    {setup: `store.cancelDashboardEdit(); await store.openSubscriptionDetail('${statusSubscription.id}'); store.setSubscriptionEpisodeFilter('all');`,
      selectors: ['.subscription-detail-badges > span', '.subscription-detail-progress strong', '.subscription-detail-actions small', '.subscription-detail-metrics .has-warning strong',
        '.subscription-episode-number', '.subscription-episode-cell > strong', '.subscription-episode-cell > em', '.subscription-episode-filters button.is-active',
        '.subscription-pipeline-index', '.subscription-pipeline-step strong', '.subscription-pipeline-step > div:nth-child(2) span', '.subscription-pipeline-step p',
        '.subscription-activity-item > div > div > span', '.automation-event-panel .subscription-activity-item strong']},
    ...['downloading', 'queued', 'completed', 'failed'].map(category => ({
      setup: `if (store.currentTab !== 'downloads') store.selectTab('downloads'); store.selectDownloadCategory('${category}');`,
      selectors: ['.download-tab.is-active > em', '.data-table--downloads .badge', '.data-table--downloads .btn-primary:not(:disabled), .data-table--downloads .btn-danger:not(:disabled)',
        '.download-task-origin', '.download-automation-source > span', '.download-automation-source > small', '.download-pipeline-step > span']
    }))
  ];
  const contrastFloor = {};
  const contrastFailures = [];
  for (const theme of ['light', 'dark']) {
    contrastFloor[theme] = {worst: Infinity, count: 0};
    for (const [index, {setup, selectors, poster = ''}] of accentTextCases.entries()) {
      const results = await accentText(`document.documentElement.dataset.posterProbe = '${poster}'; store.applyTheme('${theme}', {persist: false}); ${setup}`, selectors);
      await mediaScreenshot(`accent-text-${index + 1}-${theme}`);
      const backdropNote = poster ? ` over a ${poster} poster` : '';
      for (const result of results) {
        assert.ok(result.measured.length > 0, `${theme} ${result.selector} must be rendered for the contrast check`);
        if (poster) assert.ok(result.measured.length >= 8, `${theme} ${result.selector} must cover every validity badge and quality pill, got ${result.measured.length}`);
        for (const item of result.measured) {
          if (item.worst < 4.5) contrastFailures.push(`${theme} ${result.selector} "${item.label}"${backdropNote} needs 4.5:1 text contrast, got ${item.worst.toFixed(2)}:1 (${item.pair})`);
          contrastFloor[theme].worst = Math.min(contrastFloor[theme].worst, item.worst);
          contrastFloor[theme].count += 1;
        }
      }
    }
  }
  assert.deepEqual(contrastFailures, [], 'all measured state text must have at least 4.5:1 contrast');
  await evaluate("document.getElementById('contrast-probe').remove(); delete document.documentElement.dataset.posterProbe; document.querySelectorAll('[data-contrast-id]').forEach(item => item.removeAttribute('data-contrast-id'));");
  console.log('accent text contrast: ' + Object.entries(contrastFloor).map(([theme, floor]) => `${theme} ${floor.count} elements, lowest ${floor.worst.toFixed(2)}:1`).join('; '));
  await onStore("store.cancelDashboardEdit(); store.applyTheme('light', {persist: false});");

  // 按钮文字不能贴着边框：只写 class="btn-primary" 而不带 px-* 的按钮也要有内边距。
  const flushButtons = setup => withState(setup, `
    return Array.from(document.querySelectorAll('.btn-primary, .btn-secondary, .btn-danger')).filter(button => button.checkVisibility() && button.innerText.trim()).map(button => {
      const range = document.createRange();
      range.selectNodeContents(button);
      const text = range.getBoundingClientRect();
      const box = button.getBoundingClientRect();
      return {label: button.innerText.trim().replace(/\\s+/g, ' ').slice(0, 16), gap: Math.min(text.left - box.left, box.right - text.right)};
    }).filter(item => item.gap < 6).map(item => item.label + ' (' + item.gap.toFixed(1) + 'px)');
  `);
  let paddedButtonPages = 0;
  for (const width of [1440, 390]) {
    await setViewport(width);
    for (const tab of ['dashboard', 'search', 'subscriptions', 'drive', 'downloads', 'notifications', 'diagnostics', 'settings']) {
      const flush = await flushButtons(`
        if (store.currentTab !== '${tab}') store.selectTab('${tab}');
        ${tab === 'diagnostics' ? 'await store.loadDiagnostics();' : ''}
        ${tab === 'dashboard' ? "store.calendarLoading = false; store.calendarError = ''; store.calendarItems = [];" : ''}
      `);
      assert.deepEqual(flush, [], `${width}px ${tab}: button text must keep clear of the border`);
      paddedButtonPages += 1;
    }
  }
  assert.equal(paddedButtonPages, 16);

  // 手机上的周视图：条目和整块视图都不能伸出卡片右缘，过长的剧名要省略而不是被裁掉。
  const weekOverflow = await withState(`
    if (store.currentTab !== 'dashboard') store.selectTab('dashboard');
    // 作废切页触发的在途日历请求，免得它用后端的空数据盖掉夹具。
    store.calendarRequestId += 1;
    const today = store.calendarTodayKey();
    store.calendarCursor = today; store.calendarView = 'week'; store.calendarLoading = false; store.calendarError = '';
    store.calendar = {today, source_alerts: [], summary: {total: 2, subscriptions: 2, source_alerts: 0, by_status: {today: 2}, by_media_type: {}},
      items: ['星海旅人：一段足够长、在手机上必须省略的本地演示剧名', '短剧名'].map((title, index) => ({
        id: 'local-week-' + index, subscription_id: '${statusSubscription.id}', subscription_title: title, media_type: 'anime',
        primary_status: index ? 'ready' : 'aired_undiscovered', scheduled_date: today, episode: 3, actions: {}
      }))};
  `, `
    const edge = element => element.getBoundingClientRect().right;
    const card = document.querySelector('.dash-card[data-card="calendar"]');
    const shell = document.querySelector('.calendar-week-shell');
    const items = Array.from(document.querySelectorAll('.calendar-item.is-row')).filter(item => item.checkVisibility());
    const title = items[0] && items[0].querySelector('strong');
    const result = {
      count: items.length,
      shell: edge(shell) - edge(card),
      items: items.map(item => edge(item) - edge(item.closest('.calendar-day-items'))),
      truncated: Boolean(title) && title.scrollWidth > title.clientWidth
    };
    store.calendar = null;
    return result;
  `);
  assert.equal(weekOverflow.count, 2, 'phone week view must render the fixture items');
  assert.ok(weekOverflow.shell <= 0.5, `phone week view must stay inside the calendar card, overflows by ${weekOverflow.shell}px`);
  assert.ok(weekOverflow.items.every(overflow => overflow <= 0.5), `phone week items must stay inside their day row, overflow ${weekOverflow.items}`);
  assert.equal(weekOverflow.truncated, true, 'a long title must be ellipsized inside the phone week item');
  await setViewport(1440);

  assert.deepEqual(exceptions, [], 'media selectors must not raise Alpine runtime exceptions');
  if (screenshotDir) console.log('UI screenshots:', screenshotDir);
  console.log('search browser smoke passed: images, concurrent searches, metadata, light/dark controls and navigation, poster hover and reduced motion, movie file/quality/download/save, series quality, source quality gate, dialog keyboard focus, mobile navigation and history, dashboard attention entries and layout editing, compact phone toolbars, connection status, populated detail/download states, accent text contrast, button padding, phone week view, 1440px and 390px views');
} catch (error) {
  console.error(serverLog, chromeLog);
  throw error;
} finally {
  if (ws) ws.close();
  for (const socket of sockets) socket.destroy();
  for (const server of servers) { server.closeAllConnections(); server.close(); }
  for (const child of children.reverse()) {
    if (child.exitCode !== null || child.signalCode !== null || !child.pid) continue;
    child.kill('SIGTERM');
    await Promise.race([new Promise(resolve => child.once('exit', resolve)), delay(2000)]);
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  }
  await rm(temp, {recursive: true, force: true, maxRetries: 3});
}
