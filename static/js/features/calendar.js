(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  root.MediaSubCalendar = api;
})(typeof globalThis !== 'undefined' ? globalThis : window, function () {
  'use strict';

  const STATUS_LABELS = Object.freeze({
    today: '今日更新',
    this_week: '本周待更新',
    aired_undiscovered: '已播未发现',
    discovered_pending_transfer: '已发现待转存',
    transferred_pending_download: '已转存待下载',
    completed_missing: '完结缺集',
    ready: '已就绪',
    scheduled: '已排期',
    unknown_schedule: '排期未知'
  });

  const SOURCE_LABELS = Object.freeze({
    manual: '手动排期',
    metadata_episode: '逐集元数据',
    metadata_next_episode: '下一集元数据',
    metadata_release_date: '发布日期',
    inferred_cadence: '周期推断',
    unknown: '未知'
  });

  const CONFIDENCE_LABELS = Object.freeze({
    high: '高可信',
    medium: '中可信',
    low: '推断',
    unknown: '未知'
  });

  function parseDate(value) {
    const match = String(value || '').match(/^(\d{4})-(\d{2})-(\d{2})$/);
    if (!match) return null;
    const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])));
    return Number.isNaN(date.getTime()) ? null : date;
  }

  function dateKey(value) {
    const date = value instanceof Date ? value : parseDate(value);
    return date ? date.toISOString().slice(0, 10) : '';
  }

  function addDays(value, days) {
    const date = value instanceof Date ? new Date(value.getTime()) : parseDate(value);
    if (!date) return null;
    date.setUTCDate(date.getUTCDate() + Number(days || 0));
    return date;
  }

  function startOfWeek(value) {
    const date = value instanceof Date ? new Date(value.getTime()) : parseDate(value);
    if (!date) return null;
    const day = date.getUTCDay() || 7;
    return addDays(date, 1 - day);
  }

  function endOfMonth(value) {
    const date = value instanceof Date ? value : parseDate(value);
    if (!date) return null;
    return new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0));
  }

  function viewRange(view, cursor) {
    const date = parseDate(cursor) || parseDate(dateKey(new Date()));
    if (view === 'week') {
      const from = startOfWeek(date);
      return {from: dateKey(from), to: dateKey(addDays(from, 6))};
    }
    if (view === 'list') {
      const from = addDays(date, -14);
      return {from: dateKey(from), to: dateKey(addDays(date, 60))};
    }
    const from = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1));
    return {from: dateKey(from), to: dateKey(endOfMonth(date))};
  }

  function shiftCursor(cursor, view, direction) {
    const date = parseDate(cursor);
    if (!date) return cursor;
    const amount = Number(direction || 0);
    if (view === 'week') return dateKey(addDays(date, amount * 7));
    if (view === 'list') return dateKey(addDays(date, amount * 30));
    const day = date.getUTCDate();
    const shifted = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + amount, 1));
    shifted.setUTCDate(Math.min(day, endOfMonth(shifted).getUTCDate()));
    return dateKey(shifted);
  }

  function groupByDate(items) {
    return (Array.isArray(items) ? items : []).reduce((groups, item) => {
      const key = item && item.scheduled_date ? item.scheduled_date : 'unknown';
      if (!groups[key]) groups[key] = [];
      groups[key].push(item);
      return groups;
    }, {});
  }

  function monthCells(cursor, items, today) {
    const date = parseDate(cursor);
    if (!date) return [];
    const monthStart = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1));
    const gridStart = startOfWeek(monthStart);
    const groups = groupByDate(items);
    return Array.from({length: 42}, (_, index) => {
      const current = addDays(gridStart, index);
      const key = dateKey(current);
      return {
        key,
        day: current.getUTCDate(),
        inMonth: current.getUTCMonth() === date.getUTCMonth(),
        isToday: key === today,
        items: groups[key] || []
      };
    });
  }

  function weekDays(cursor, items, today) {
    const start = startOfWeek(cursor);
    if (!start) return [];
    const groups = groupByDate(items);
    return Array.from({length: 7}, (_, index) => {
      const current = addDays(start, index);
      const key = dateKey(current);
      return {key, day: current.getUTCDate(), isToday: key === today, items: groups[key] || []};
    });
  }

  function listGroups(items) {
    const groups = groupByDate(items);
    return Object.keys(groups)
      .sort((left, right) => {
        if (left === 'unknown') return 1;
        if (right === 'unknown') return -1;
        return left.localeCompare(right);
      })
      .map(key => ({key, items: groups[key]}));
  }

  function rangeLabel(view, cursor, from, to) {
    if (view === 'month') {
      const date = parseDate(cursor);
      return date ? `${date.getUTCFullYear()} 年 ${date.getUTCMonth() + 1} 月` : '';
    }
    return `${from || ''} — ${to || ''}`;
  }

  function statusLabel(status) {
    return STATUS_LABELS[status] || status || '未知';
  }

  function sourceLabel(source) {
    return SOURCE_LABELS[source] || source || '未知';
  }

  function confidenceLabel(confidence) {
    return CONFIDENCE_LABELS[confidence] || confidence || '未知';
  }

  return Object.freeze({
    addDays,
    confidenceLabel,
    dateKey,
    groupByDate,
    listGroups,
    monthCells,
    parseDate,
    rangeLabel,
    shiftCursor,
    sourceLabel,
    startOfWeek,
    statusLabel,
    viewRange,
    weekDays
  });
});
