(function (root, factory) {
  const tools = factory();

  if (typeof module === 'object' && module.exports) {
    module.exports = tools;
  }

  root.MediaSubSubscriptionDetail = tools;
})(typeof globalThis !== 'undefined' ? globalThis : window, function () {
  'use strict';

  function episodeStage(episode) {
    if (!episode) return 'unknown';
    if (episode.missing) return 'missing';
    const downloaded = episode.download_status === 'completed';
    const strmReady = episode.strm_status === 'generated';
    if (downloaded && strmReady) return 'complete';
    if (downloaded) return 'downloaded';
    if (strmReady) return 'strm';
    if (episode.transferred) return 'transferred';
    if (episode.discovered) return 'discovered';
    return 'unknown';
  }

  function episodeStageLabel(episode) {
    return {
      missing: '缺集',
      discovered: '已发现',
      transferred: '已转存',
      strm: 'STRM 就绪',
      downloaded: '已下载',
      complete: '已完成',
      unknown: '未知'
    }[episodeStage(episode)] || '未知';
  }

  function filterEpisodes(episodes, filter = 'all') {
    const items = Array.isArray(episodes) ? episodes : [];
    if (filter === 'missing') return items.filter(item => item.missing);
    if (filter === 'pending') {
      return items.filter(item => item.discovered && (
        !item.transferred
        || ['queued', 'pending'].includes(item.download_status)
        || item.strm_status === 'failed'
      ));
    }
    if (filter === 'ready') {
      return items.filter(item => ['complete', 'downloaded', 'strm'].includes(episodeStage(item)));
    }
    if (filter === 'recent') return items.filter(item => item.recent);
    return items;
  }

  function episodeFilterCount(episodes, filter) {
    return filterEpisodes(episodes, filter).length;
  }

  function pipelineStatusLabel(status) {
    return {
      success: '完成',
      active: '运行中',
      warning: '需处理',
      error: '失败',
      disabled: '未启用',
      idle: '等待中'
    }[status] || '等待中';
  }

  function activityTimestamp(item) {
    return Number(item && (item.timestamp || item.updated_at || item.created_at || item.time) || 0);
  }

  function buildSubscriptionActivity(detail) {
    if (!detail) return [];
    const jobs = (detail.recent_jobs || []).map(job => ({
      id: `job:${job.id}`,
      kind: 'job',
      title: job.title || '后台任务',
      message: job.error || job.message || '',
      status: job.status || 'idle',
      timestamp: Number(job.updated_at || job.created_at || 0),
      raw: job
    }));
    const notifications = (detail.recent_notifications || []).map(notification => ({
      id: `notification:${notification.id}`,
      kind: 'notification',
      title: notification.title || '通知',
      message: notification.message || '',
      status: notification.level || 'info',
      timestamp: Number(notification.created_at || 0),
      raw: notification
    }));
    const checks = (((detail.subscription || {}).check_history) || []).map((check, index) => ({
      id: `check:${check.time || index}`,
      kind: 'check',
      title: check.summary || '订阅检查',
      message: `扫描 ${Number(check.scanned_count || 0)} · 新增 ${Number(check.new_count || 0)} · 转存 ${Number(check.transfer_count || 0)}`,
      status: check.state === 'success' ? 'success' : (check.state || 'info'),
      timestamp: Number(check.time || 0),
      raw: check
    }));
    return [...jobs, ...notifications, ...checks]
      .sort((left, right) => activityTimestamp(right) - activityTimestamp(left))
      .slice(0, 60);
  }

  function activityTone(item) {
    const status = String(item && item.status || '');
    if (['failed', 'error'].includes(status)) return 'error';
    if (['running', 'queued', 'active', 'info'].includes(status)) return 'active';
    if (['warning', 'canceled'].includes(status)) return 'warning';
    if (['succeeded', 'success', 'completed'].includes(status)) return 'success';
    return 'idle';
  }

  return Object.freeze({
    activityTone,
    buildSubscriptionActivity,
    episodeFilterCount,
    episodeStage,
    episodeStageLabel,
    filterEpisodes,
    pipelineStatusLabel
  });
});
