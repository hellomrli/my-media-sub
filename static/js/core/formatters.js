(function (root, factory) {
  const formatters = factory();

  if (typeof module === 'object' && module.exports) {
    module.exports = formatters;
  }

  root.MediaSubFormatters = formatters;
})(typeof globalThis !== 'undefined' ? globalThis : window, function () {
  'use strict';

  function numeric(value, fallback = 0) {
    const number = Number(value);
    return Number.isFinite(number) ? number : fallback;
  }

  function parseTimestamp(value) {
    if (value === null || value === undefined || value === '') return 0;
    if (typeof value === 'number' || /^\d+(?:\.\d+)?$/.test(String(value).trim())) {
      const number = numeric(value);
      if (!number) return 0;
      return number < 1e12 ? number * 1000 : number;
    }
    const parsed = Date.parse(String(value));
    return Number.isFinite(parsed) ? parsed : 0;
  }

  function formatDateTime(value, options = {}) {
    const timestamp = parseTimestamp(value);
    if (!timestamp) return options.fallback || '-';
    const date = new Date(timestamp);
    if (options.locale === false) return date.toISOString();
    return new Intl.DateTimeFormat(options.locale || 'zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: options.seconds === false ? undefined : '2-digit',
      hour12: false
    }).format(date);
  }

  function formatBytes(value, options = {}) {
    const bytes = numeric(value);
    if (bytes <= 0) return options.zero || '-';
    const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
    let size = bytes;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit += 1;
    }
    const decimals = options.decimals === undefined ? 2 : Math.max(0, Number(options.decimals));
    return `${size.toFixed(unit === 0 && options.compact ? 0 : decimals)} ${units[unit]}`;
  }

  function formatSpeed(value) {
    const bytes = numeric(value);
    return bytes > 0 ? `${formatBytes(bytes)}/s` : '0 B/s';
  }

  function formatDuration(value, options = {}) {
    const seconds = Math.max(0, Math.floor(numeric(value)));
    if (!seconds) return options.zero || '-';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    if (minutes > 0) return `${minutes}m ${secs}s`;
    return `${secs}s`;
  }

  function formatPercent(value, decimals = 1) {
    return `${Math.max(0, Math.min(100, numeric(value))).toFixed(decimals)}%`;
  }

  return Object.freeze({
    formatBytes,
    formatDateTime,
    formatDuration,
    formatPercent,
    formatSpeed,
    parseTimestamp
  });
});
