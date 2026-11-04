#!/usr/bin/env node
// Exercise real Alpine rendering with deterministic image and search responses.
// Requires Node 22+ and Chrome; no third-party browser driver or live media account.
import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import {spawn} from 'node:child_process';
import {mkdir, mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
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
const exceptions = [];
let serverLog = '';
let chromeLog = '';
let ws;
let secureImageRequests = 0;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve(server.address().port)));

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
  const dialog = `document.querySelector('[aria-label="订阅配置"]')`;
  for (const width of [1440, 390]) {
    await cdp('Emulation.setDeviceMetricsOverride', {width, height: 1100, deviceScaleFactor: 1, mobile: width < 500});
    await waitFor(() => evaluate('innerWidth === ' + width), 'metadata viewport');
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, `${width}px metadata page must not overflow`);
    assert.equal(await evaluate(`${dialog}.scrollWidth <= ${dialog}.clientWidth`), true, `${width}px metadata dialog must not overflow`);
    assert.match(await evaluate(`${dialog}.innerText`), /尝试剧名：星海旅人之远航篇 \/ 星海旅人/);
    if (width === 1440 && process.argv.includes('--metadata-screenshot')) {
      await waitFor(() => evaluate("document.querySelectorAll('#toastContainer .toast').length === 0"), 'toast expiry before screenshot');
      await evaluate(`window.scrollTo(0, 0); ${dialog}.scrollTop = 0; document.fonts.ready`);
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
  console.log('search browser smoke passed: images, preview, concurrent searches, metadata fallback/empty results, 1440px and 390px views');
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
