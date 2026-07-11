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
  function timeline(events, limit = 100) {
    return [...(events || [])].sort((a,b) => Number(a.updated_at || a.created_at || 0) - Number(b.updated_at || b.created_at || 0)).slice(-limit);
  }
  function safeJson(value) {
    try { return JSON.stringify(value, null, 2); } catch (_) { return '{}'; }
  }
  return Object.freeze({readPreference, runPool, safeJson, timeline, visibleWindow, writePreference});
});
