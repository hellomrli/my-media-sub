const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const policy = require('../static/js/pwa-policy.js');
const pwa = require('../static/js/features/pwa.js');

const root = path.join(__dirname, '..');

function response(status, cacheControl = '') {
  return {
    status,
    ok: status >= 200 && status < 300,
    headers: {get(name) { return name.toLowerCase() === 'cache-control' ? cacheControl : ''; }}
  };
}

test('PWA policy keeps API, STRM and health network-only', () => {
  const origin = 'https://media.example.com';
  assert.equal(policy.classifyRequest({url: `${origin}/api/settings`, mode: 'cors', destination: ''}, origin), 'network-only');
  assert.equal(policy.classifyRequest({url: `${origin}/strm/quark/id/file.mkv`, mode: 'cors', destination: ''}, origin), 'network-only');
  assert.equal(policy.classifyRequest({url: `${origin}/health`, mode: 'cors', destination: ''}, origin), 'network-only');
  assert.equal(policy.classifyRequest({url: 'https://other.example/app.js', mode: 'cors', destination: 'script'}, origin), 'network-only');
});

test('PWA policy uses network-first HTML and stale-while-revalidate assets', () => {
  const origin = 'https://media.example.com';
  assert.equal(policy.classifyRequest({url: `${origin}/?tab=calendar`, mode: 'navigate', destination: 'document'}, origin), 'network-first');
  assert.equal(policy.classifyRequest({url: `${origin}/js/core/api.js`, mode: 'no-cors', destination: 'script'}, origin), 'stale-while-revalidate');
  assert.equal(policy.classifyRequest({url: `${origin}/styles.css`, mode: 'no-cors', destination: 'style'}, origin), 'stale-while-revalidate');
  assert.equal(policy.isCacheableResponse(response(200)), true);
  assert.equal(policy.isCacheableResponse(response(401)), false, 'Basic Auth challenges must never be cached');
  assert.equal(policy.isCacheableResponse(response(200, 'private, no-store')), false);
});

test('manifest contains install icons and all six required shortcuts', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'static/manifest.webmanifest'), 'utf8'));
  assert.equal(manifest.display, 'standalone');
  assert.ok(manifest.icons.some(icon => icon.sizes === '192x192'));
  assert.ok(manifest.icons.some(icon => icon.sizes === '512x512'));
  assert.ok(manifest.icons.some(icon => icon.purpose === 'maskable'));
  assert.deepEqual(manifest.shortcuts.map(item => item.short_name), ['今日', '缺集', '失败', '检查', '下载', '签到']);
});

test('service worker has version cleanup, offline shell and safe update flow', () => {
  const source = fs.readFileSync(path.join(root, 'static/service-worker.js'), 'utf8');
  assert.match(source, /CACHE_VERSION\s*=\s*'v1\.11\.0-p16-1'/);
  assert.match(source, /obsoleteCacheNames/);
  assert.match(source, /response\.status === 401 \|\| response\.status === 403/);
  assert.match(source, /cache\.match\(new Request\(`\$\{self\.location\.origin\}\/`\)\)/);
  assert.match(source, /SKIP_WAITING/);
  assert.match(source, /'\/app\.js'/);
  assert.match(source, /asset === '\/' \? shellCache : staticCache/);
});

test('shortcut parser only accepts declared actions', () => {
  assert.equal(pwa.shortcutFromSearch('?pwa=calendar-today'), 'calendar-today');
  assert.equal(pwa.shortcutFromSearch('?pwa=unknown'), '');
  assert.equal(pwa.SHORTCUTS.length, 6);
});

test('390px mobile contract and install hooks are present', () => {
  const css = fs.readFileSync(path.join(root, 'tailwind/input.css'), 'utf8');
  const html = fs.readFileSync(path.join(root, 'static/index.html'), 'utf8');
  assert.match(css, /@media \(max-width: 390px\)/);
  assert.match(css, /\.pwa-quick-grid/);
  assert.match(html, /width=device-width, initial-scale=1\.0/);
  assert.match(html, /manifest\.webmanifest/);
  assert.match(html, /pwaInstallAvailable/);
  assert.match(html, /pwaUpdateReady/);
});


test('cache upgrades keep the active pair and delete every older PWA generation', () => {
  const names = ['media-sub-shell-v1.9.0-p10-1', 'media-sub-static-v1.9.0-p10-1',
    'media-sub-shell-v1.9.0-p11-1', 'media-sub-static-v1.9.0-p11-1', 'unrelated-cache'];
  assert.deepEqual(policy.obsoleteCacheNames(names, [
    'media-sub-shell-v1.9.0-p11-1', 'media-sub-static-v1.9.0-p11-1'
  ]), ['media-sub-shell-v1.9.0-p10-1', 'media-sub-static-v1.9.0-p10-1']);
});
