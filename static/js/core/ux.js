(function (root, factory) {
  const api = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  root.MediaSubUx = api;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';
  function readPreference(key, fallback, allowed) {
    try {
      const value = root.localStorage && root.localStorage.getItem(`media-sub:${key}`);
      if (value !== null && (!allowed || allowed.includes(value))) return value;
    } catch (_) {}
    return fallback;
  }
  function writePreference(key, value) {
    try { if (root.localStorage) root.localStorage.setItem(`media-sub:${key}`, String(value)); } catch (_) {}
    return value;
  }
  function visibleWindow(items, limit, maximum = 500) {
    const list = Array.isArray(items) ? items : [];
    return list.slice(0, Math.min(Math.max(1, Number(limit) || 1), maximum));
  }
  async function runPool(items, concurrency, worker) {
    const queue = [...(items || [])]; const results = [];
    const runners = Array.from({length: Math.min(Math.max(1, concurrency || 1), queue.length)}, async () => {
      while (queue.length) { const item = queue.shift(); results.push(await worker(item)); }
    });
    await Promise.all(runners); return results;
  }
  function safeJson(value) {
    try { return JSON.stringify(value, null, 2); } catch (_) { return '{}'; }
  }
  function safeExternalUrl(value) {
    if (typeof value !== 'string') return null;
    try {
      const base = root.location && root.location.href;
      const url = new URL(value, base);
      if (url.protocol === 'https:' || url.protocol === 'http:') return url.href;
    } catch (_) {}
    return null;
  }
  function escapeHtml(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, (ch) => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    })[ch]);
  }
  const api = Object.freeze({escapeHtml, readPreference, runPool, safeExternalUrl, safeJson, visibleWindow, writePreference});
  // Alpine 模板表达式在 `new Function` 全局作用域内求值，裸函数名可直接调用。
  root.safeExternalUrl = safeExternalUrl;
  root.escapeHtml = escapeHtml;
  return api;
});
