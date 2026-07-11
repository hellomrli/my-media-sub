(function (root, factory) {
  const policy = factory();
  if (typeof module === 'object' && module.exports) module.exports = policy;
  root.MediaSubPwaPolicy = policy;
})(typeof globalThis !== 'undefined' ? globalThis : self, function () {
  'use strict';

  const STATIC_EXTENSIONS = /\.(?:css|js|mjs|png|jpg|jpeg|webp|svg|ico|woff2?|ttf|webmanifest)$/i;

  function normalizedPath(input, origin = 'http://localhost') {
    try { return new URL(typeof input === 'string' ? input : input.url, origin).pathname; }
    catch (_) { return ''; }
  }

  function classifyRequest(request, origin = 'http://localhost') {
    const url = new URL(typeof request === 'string' ? request : request.url, origin);
    if (url.origin !== origin) return 'network-only';
    const path = url.pathname;
    if (path === '/api' || path.startsWith('/api/') || path === '/strm' || path.startsWith('/strm/')) {
      return 'network-only';
    }
    if (path === '/health') return 'network-only';
    const mode = typeof request === 'string' ? '' : request.mode;
    const destination = typeof request === 'string' ? '' : request.destination;
    if (mode === 'navigate' || destination === 'document' || path === '/' || path === '/index.html') {
      return 'network-first';
    }
    if (STATIC_EXTENSIONS.test(path) || ['script', 'style', 'image', 'font', 'manifest', 'worker'].includes(destination)) {
      return 'stale-while-revalidate';
    }
    return 'network-only';
  }

  function obsoleteCacheNames(names, activeNames, prefix = 'media-sub-') {
    const active = new Set(Array.isArray(activeNames) ? activeNames : []);
    return (Array.isArray(names) ? names : []).filter(name => name.startsWith(prefix) && !active.has(name));
  }

  function isCacheableResponse(response) {
    if (!response || response.status !== 200 || !response.ok) return false;
    const cacheControl = response.headers && response.headers.get
      ? String(response.headers.get('cache-control') || '').toLowerCase() : '';
    return !cacheControl.includes('no-store') && !cacheControl.includes('private');
  }

  function cacheKey(input, origin = 'http://localhost') {
    const url = new URL(typeof input === 'string' ? input : input.url, origin);
    url.hash = '';
    return url.toString();
  }

  return {normalizedPath, classifyRequest, obsoleteCacheNames, isCacheableResponse, cacheKey};
});
