/* global MediaSubPwaPolicy */
importScripts('/js/pwa-policy.js?v=2.8.5');

const ASSET_VERSION = '2.8.5';
const CACHE_VERSION = 'v2.8.5-shell-1';
const SHELL_CACHE = `media-sub-shell-${CACHE_VERSION}`;
const STATIC_CACHE = `media-sub-static-${CACHE_VERSION}`;
const CACHE_PREFIX = 'media-sub-';
const ASSET_REVISIONS = {
  '/styles.css': 'calendar-styles-1',
  '/js/theme-init.js': 'media-save-1',
  '/js/core/api.js': 'ui-usability-2',
  '/js/features/search-results.js': 'search-images-1',
  '/js/features/source-switch.js': 'media-save-1',
  '/js/core/polling.js': 'ui-usability-2',
  '/js/core/router.js': 'ui-usability-1',
  '/js/core/notifications.js': 'ui-usability-2',
  '/js/stores/jobs.js': 'ui-usability-2',
  '/js/stores/downloads.js': 'ui-usability-3',
  '/js/stores/subscriptions.js': 'ui-usability-3',
  '/js/features/settings.js': 'ui-usability-2',
  '/js/features/pwa.js': 'ui-usability-2',
  '/js/features/dashboard.js': 'ui-usability-2',
  '/js/features/search-page.js': 'search-images-1',
  '/js/core/shell.js': 'ui-usability-2'
};
const PRECACHE_ASSETS = [
  '/',
  '/manifest.webmanifest',
  '/styles.css',
  '/fonts/space-grotesk-latin.woff2',
  '/vendor/alpine.min.js',
  '/js/theme-init.js',
  '/js/pwa-policy.js',
  '/js/core/api.js',
  '/js/core/formatters.js',
  '/js/core/ux.js',
  '/js/features/search-results.js',
  '/js/features/subscription-detail.js',
  '/js/features/calendar.js',
  '/js/features/source-switch.js',
  '/js/features/automation-events.js',
  '/js/core/polling.js',
  '/js/core/router.js',
  '/js/core/notifications.js',
  '/js/stores/downloads.js',
  '/js/stores/drive.js',
  '/js/stores/jobs.js',
  '/js/stores/subscriptions.js',
  '/js/features/updates.js',
  '/js/features/settings.js',
  '/js/features/diagnostics.js',
  '/js/features/pwa.js',
  '/js/features/dashboard.js',
  '/js/features/search-page.js',
  '/js/features/calendar-page.js',
  '/js/core/shell.js',
  '/app.js',
  '/api-docs.html',
  '/openapi.json',
  '/js/openapi-docs.js',
  '/icons/icon-192.png',
  '/icons/icon-512.png',
  '/icons/icon-maskable-512.png',
].map(asset => {
  if (!/\.(?:css|js)$/i.test(asset)) return asset;
  const revision = ASSET_REVISIONS[asset] ? `&rev=${ASSET_REVISIONS[asset]}` : '';
  return `${asset}?v=${ASSET_VERSION}${revision}`;
});

function sanitizedRequest(request) {
  return new Request(MediaSubPwaPolicy.cacheKey(request, self.location.origin), {
    method: 'GET', credentials: 'same-origin', redirect: 'follow'
  });
}

function isShellUrl(value) {
  const url = new URL(value, self.location.origin);
  return url.origin === self.location.origin && (url.pathname === '/' || url.pathname === '/index.html');
}

function canCache(request, response) {
  return MediaSubPwaPolicy.isCacheableResponse(response) && (!isShellUrl(request.url) ||
    (String(response.headers.get('content-type') || '').includes('text/html') &&
      (!response.redirected || isShellUrl(response.url))));
}

async function putIfCacheable(cacheName, request, response) {
  if (!canCache(request, response)) return;
  const cache = await caches.open(cacheName);
  await cache.put(sanitizedRequest(request), response.clone());
}

function isDocumentRequest(request) {
  return request.mode === 'navigate' || request.destination === 'document';
}

async function networkFirst(request) {
  const shell = isShellUrl(request.url);
  const cacheName = shell || isDocumentRequest(request) ? SHELL_CACHE : STATIC_CACHE;
  const key = shell ? new Request(`${self.location.origin}/`) : request;
  try {
    const response = await fetch(request);
    // A Basic Auth 401/403 is authoritative and must never fall back to a cached shell.
    if (response.status === 401 || response.status === 403) return response;
    await putIfCacheable(cacheName, key, response);
    return response;
  } catch (error) {
    const cache = await caches.open(cacheName);
    const cached = await cache.match(sanitizedRequest(key));
    if (cached) return cached;
    throw error;
  }
}

async function staleWhileRevalidate(request, event) {
  const cache = await caches.open(STATIC_CACHE);
  const key = sanitizedRequest(request);
  const cached = await cache.match(key);
  const refresh = fetch(request).then(async response => {
    await putIfCacheable(STATIC_CACHE, request, response);
    return response;
  }).catch(() => null);
  if (cached) {
    event.waitUntil(refresh);
    return cached;
  }
  const response = await refresh;
  if (response) return response;
  throw new Error('offline and static asset is not cached');
}

async function warmCache() {
  const staticCache = await caches.open(STATIC_CACHE);
  const shellCache = await caches.open(SHELL_CACHE);
  await Promise.allSettled(PRECACHE_ASSETS.map(async asset => {
    const request = new Request(asset, {credentials: 'same-origin'});
    const response = await fetch(request);
    if (canCache(request, response)) {
      const cache = asset === '/' || asset.endsWith('.html') ? shellCache : staticCache;
      await cache.put(sanitizedRequest(request), response);
    }
  }));
}

self.addEventListener('install', event => {
  event.waitUntil(warmCache());
});

self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    const obsolete = MediaSubPwaPolicy.obsoleteCacheNames(
      names, [SHELL_CACHE, STATIC_CACHE], CACHE_PREFIX
    );
    await Promise.all(obsolete.map(name => caches.delete(name)));
    await self.clients.claim();
    const clients = await self.clients.matchAll({type: 'window'});
    clients.forEach(client => client.postMessage({type: 'PWA_ACTIVATED', version: CACHE_VERSION}));
  })());
});

self.addEventListener('fetch', event => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const strategy = MediaSubPwaPolicy.classifyRequest(request, self.location.origin);
  if (strategy === 'network-only') {
    event.respondWith(fetch(request));
  } else if (strategy === 'network-first') {
    event.respondWith(networkFirst(request));
  } else if (strategy === 'stale-while-revalidate') {
    event.respondWith(staleWhileRevalidate(request, event));
  }
});

self.addEventListener('message', event => {
  if (!event.data) return;
  if (event.data.type === 'SKIP_WAITING') self.skipWaiting();
  if (event.data.type === 'WARM_CACHE') event.waitUntil(warmCache());
});

self.addEventListener('push', event => {
  let data = {};
  try { data = event.data ? event.data.json() : {}; } catch (_) { data = {body: event.data ? event.data.text() : ''}; }
  event.waitUntil(self.registration.showNotification(data.title || 'MEDIA/SUB', {
    body: data.body || '', icon: '/icons/icon-192.png', badge: '/icons/icon-192.png',
    tag: data.tag || 'media-sub-notification', data: {url: data.url || '/?tab=notifications'}
  }));
});

self.addEventListener('notificationclick', event => {
  event.notification.close();
  const target = new URL((event.notification.data && event.notification.data.url) || '/', self.location.origin).href;
  event.waitUntil(self.clients.matchAll({type: 'window', includeUncontrolled: true}).then(clients => {
    const existing = clients.find(client => client.url.startsWith(self.location.origin));
    if (existing) { existing.navigate(target); return existing.focus(); }
    return self.clients.openWindow(target);
  }));
});
