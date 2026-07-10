(function (root, factory) {
  const tools = factory();
  if (typeof module === 'object' && module.exports) module.exports = tools;
  root.MediaSubAutomationEvents = tools;
})(typeof globalThis !== 'undefined' ? globalThis : window, function () {
  'use strict';
  const STAGES = {
    source_check: '来源检查', file_filter: '文件过滤', version_select: '版本选择',
    cloud_transfer: '云盘转存', rename: '重命名', strm: 'STRM', aria2: 'Aria2', notification: '通知'
  };
  const STATUSES = {
    pending: '等待', running: '执行中', succeeded: '成功', skipped: '跳过',
    failed: '失败', retrying: '重试中', canceled: '已取消'
  };
  function stageLabel(value) { return STAGES[value] || value || '未知阶段'; }
  function statusLabel(value) { return STATUSES[value] || value || '未知状态'; }
  function statusTone(value) {
    if (value === 'succeeded') return 'success';
    if (value === 'failed') return 'danger';
    if (value === 'running' || value === 'retrying') return 'warning';
    return 'muted';
  }
  function duration(event) {
    if (!event || !event.started_at) return '—';
    const end = event.finished_at || event.updated_at || event.started_at;
    const seconds = Math.max(0, Number(end) - Number(event.started_at));
    if (seconds < 60) return `${seconds}秒`;
    return `${Math.floor(seconds / 60)}分${seconds % 60}秒`;
  }
  function episodeGroups(events) {
    const groups = new Map();
    for (const event of Array.isArray(events) ? events : []) {
      if (event.episode === null || event.episode === undefined) continue;
      if (!groups.has(event.episode)) groups.set(event.episode, []);
      groups.get(event.episode).push(event);
    }
    return [...groups.entries()].sort((a,b)=>a[0]-b[0]).map(([episode, items])=>({episode, items}));
  }
  function canRetry(event) { return !!event && ['failed','canceled'].includes(event.status); }
  return Object.freeze({canRetry, duration, episodeGroups, stageLabel, statusLabel, statusTone});
});
