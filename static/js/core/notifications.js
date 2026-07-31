(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubNotifications = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiData, apiFetch} = api;

  const TOAST_ICONS = Object.freeze({success: '✓', error: '✕', warning: '⚠', info: 'ℹ'});

  function normalizeNotificationType(type) {
    return Object.prototype.hasOwnProperty.call(TOAST_ICONS, type) ? type : 'info';
  }

  function toastIcon(type) {
    return TOAST_ICONS[normalizeNotificationType(type)];
  }

  function activityTimestamp(item) {
    const value = item && (item.timestamp ?? item.updated_at ?? item.created_at);
    if (typeof value === 'number' && Number.isFinite(value)) return value;
    const numeric = Number(value);
    if (Number.isFinite(numeric) && numeric !== 0) return numeric;
    if (typeof value === 'string' && value.trim()) {
      const parsed = Date.parse(value);
      if (Number.isFinite(parsed)) return parsed / 1000;
    }
    return 0;
  }

  function jobActivityLevel(status) {
    if (status === 'succeeded') return 'success';
    if (status === 'failed') return 'error';
    if (status === 'queued' || status === 'running') return 'warning';
    return 'info';
  }

  /**
   * Convert jobs and user-facing notifications to one stable timeline shape.
   * Background notifications are intentionally supplied by the caller after
   * filtering; this keeps one event from being rendered twice (once as a Job
   * and once as the notification emitted by that Job).
   */
  /// 推送派发任务只是通知的投递动作，本身没有阅读价值，却因为每条通知都会产生一条
  /// 而迅速淹没活动中心（线上实测占任务总数的 88%）。通知本体照常展示。
  const HIDDEN_ACTIVITY_JOB_KINDS = Object.freeze(['push_dispatch']);

  function isNoisyActivityJob(job) {
    return HIDDEN_ACTIVITY_JOB_KINDS.includes(String((job && job.kind) || ''));
  }

  function mergeActivityItems(jobs, notifications) {
    const jobItems = (Array.isArray(jobs) ? jobs : [])
      .filter(job => !isNoisyActivityJob(job))
      .map(job => ({
      id: `job:${job.id || 'missing'}`,
      source: 'job',
      kind: 'job',
      title: job.title || '后台任务',
      message: job.message || job.error || '',
      level: jobActivityLevel(job.status),
      event: job.kind || 'job',
      status: job.status || '',
      read: true,
      timestamp: activityTimestamp(job),
      raw: job
    }));
    const notificationItems = (Array.isArray(notifications) ? notifications : []).map(notification => ({
      id: `notification:${notification.id || 'missing'}`,
      source: 'notification',
      kind: 'notification',
      title: notification.title || '系统通知',
      message: notification.message || '',
      level: normalizeNotificationType(notification.level),
      event: notification.event || '',
      status: notification.read ? 'read' : 'unread',
      read: !!notification.read,
      timestamp: activityTimestamp(notification),
      raw: notification
    }));

    return [...jobItems, ...notificationItems].sort((left, right) => {
      const time = activityTimestamp(right) - activityTimestamp(left);
      if (time !== 0) return time;
      return String(right.id).localeCompare(String(left.id));
    });
  }

  function createStore() {
    return {
    notifications: [],
    notificationsPoller: null,
    /// 请求序号：轮询、SSE 联动、标记已读并发时丢弃过期响应，避免旧数据覆盖新数据。
    notificationsRequestId: 0,
    notificationFilter: 'all',
    activityFilter: 'all',
    activityQuery: '',
    activityVisibleLimit: 100,
    activityFilters: [
      {id: 'all', name: '全部活动'},
      {id: 'unread', name: '未读通知'},
      {id: 'jobs', name: '后台任务'},
      {id: 'notifications', name: '系统通知'},
      {id: 'failed', name: '失败项'}
    ],
    get unreadNotifications() {
      return this.notificationCenterNotifications.filter(n => !n.read).length;
    },

    get backgroundNotificationEvents() {
      return [
        'subscription_transferred',
        'subscription_transfer_failed',
        'manual_transfer_succeeded',
        'manual_transfer_failed',
        'metadata_scrape_completed'
      ];
    },

    get notificationCenterNotifications() {
      return this.notifications.filter(n => !this.backgroundNotificationEvents.includes(n.event));
    },

    get activityItems() {
      const jobs = Array.isArray(this.backgroundJobs) ? this.backgroundJobs : [];
      return mergeActivityItems(jobs, this.notificationCenterNotifications);
    },

    get filteredActivityItems() {
      const query = String(this.activityQuery || '').trim().toLowerCase();
      return this.activityItems.filter(item => {
        if (this.activityFilter === 'unread' && (item.source !== 'notification' || item.read)) return false;
        if (this.activityFilter === 'jobs' && item.source !== 'job') return false;
        if (this.activityFilter === 'notifications' && item.source !== 'notification') return false;
        if (this.activityFilter === 'failed' && !(item.status === 'failed' || item.level === 'error')) return false;
        if (!query) return true;
        const rawSearch = item.source === 'job'
          ? [item.raw && item.raw.payload, item.raw && item.raw.result, item.raw && item.raw.error]
          : [item.raw && item.raw.meta];
        return [item.title, item.message, item.event, item.status, item.source, ...rawSearch]
          .some(value => {
            const text = value && typeof value === 'object' ? JSON.stringify(value) : String(value || '');
            return text.toLowerCase().includes(query);
          });
      });
    },

    get visibleActivityItems() {
      return this.filteredActivityItems.slice(0, this.activityVisibleLimit);
    },

    get activityStats() {
      const items = this.activityItems;
      return {
        total: items.length,
        jobs: items.filter(item => item.source === 'job').length,
        notifications: items.filter(item => item.source === 'notification').length,
        unread: items.filter(item => item.source === 'notification' && !item.read).length,
        failed: items.filter(item => item.status === 'failed' || item.level === 'error').length,
        running: items.filter(item => item.source === 'job' && ['queued', 'running'].includes(item.status)).length
      };
    },

    async loadNotifications() {
      const requestId = ++this.notificationsRequestId;
      try {
        const response = await apiFetch('/api/notifications');
        const data = await response.json();
        if (requestId !== this.notificationsRequestId) return;
        this.notifications = data.data || [];
      } catch (error) {
        if (requestId !== this.notificationsRequestId) return;
        console.error('加载通知失败:', error);
      }
    },

    startNotificationsPolling() {
      this.stopNotificationsPolling();
      if (!['dashboard', 'notifications'].includes(this.currentTab)) return;
      // SSE 正常推送时任务列表已经是最新的，轮询只补通知（通知没有 SSE 通道）；
      // SSE 断线时回落到任务 + 通知的全量刷新。
      const refresh = () => {
        if (this.jobEventsHealthy) return this.loadNotifications();
        return typeof this.loadActivity === 'function'
          ? this.loadActivity()
          : this.loadNotifications();
      };
      this.notificationsPoller = this.startPolling('notifications', refresh, 30000);
    },

    stopNotificationsPolling() {
      this.stopPolling('notifications');
      this.notificationsPoller = null;
    },

    activityFilterCount(filterId) {
      if (filterId === 'unread') return this.activityStats.unread;
      if (filterId === 'jobs') return this.activityStats.jobs;
      if (filterId === 'notifications') return this.activityStats.notifications;
      if (filterId === 'failed') return this.activityStats.failed;
      return this.activityStats.total;
    },

    activitySourceLabel(source) {
      return source === 'job' ? '后台任务' : '系统通知';
    },

    activityStatusLabel(item) {
      if (!item) return '-';
      if (item.source === 'job' && typeof this.jobStatusLabel === 'function') return this.jobStatusLabel(item.status);
      return item.read ? '已读' : '未读';
    },

    activityLevelBadgeClass(item) {
      if (!item) return 'badge badge-muted';
      if (item.source === 'job' && typeof this.jobStatusBadgeClass === 'function') return this.jobStatusBadgeClass(item.status);
      return typeof this.notificationLevelBadgeClass === 'function'
        ? this.notificationLevelBadgeClass(item.level)
        : 'badge badge-muted';
    },

    activityEventLabel(item) {
      if (!item) return '-';
      if (item.source === 'job' && typeof this.jobKindLabel === 'function') return this.jobKindLabel(item.event);
      return this.notificationEventLabel(item.event);
    },

    activityTimeLabel(item) {
      return this.formatTime(activityTimestamp(item));
    },

    resetActivityFilters() {
      this.activityFilter = 'all';
      this.activityQuery = '';
      this.activityVisibleLimit = 100;
    },

    async loadActivity() {
      await Promise.all([
        typeof this.loadJobs === 'function' ? this.loadJobs() : Promise.resolve(),
        this.loadNotifications()
      ]);
    },

    openActivityItem(item) {
      if (!item) return;
      if (item.source === 'job' && typeof this.openJobDetail === 'function') {
        this.openJobDetail(item.raw);
      } else if (item.source === 'notification' && !item.read && item.raw && item.raw.id) {
        this.markRead(item.raw.id);
      }
    },

    notificationLevelBadgeClass(level) {
      const classes = {
        info: 'badge badge-primary',
        success: 'badge badge-success',
        warning: 'badge badge-warning',
        error: 'badge badge-danger'
      };
      return classes[level] || classes.info;
    },

    notificationEventLabel(event) {
      const labels = {
        push_sent: '推送记录',
        push_test: '推送测试',
        subscription_updated: '订阅更新',
        subscription_invalid: '订阅失效',
        subscription_completed: '订阅完结',
        subscription_transferred: '自动转存',
        download_completed: '下载完成',
        quark_signin: '夸克签到',
        subscription_transfer_failed: '转存失败',
        manual_transfer_succeeded: '手动转存',
        manual_transfer_failed: '转存失败',
        metadata_scrape_completed: '元数据刮削'
      };
      return labels[event] || '系统通知';
    },

    notificationPushChannelStatuses(notif) {
      const meta = this.notificationPushMeta(notif);
      const results = meta.results || {};
      const attempts = meta.attempts || {};
      const channels = Array.isArray(meta.channels) ? meta.channels : Object.keys(results);
      return channels.map(channel => ({
        channel,
        name: this.pushChannelName(channel),
        success: results[channel] === true,
        attempts: Number(attempts[channel] || 0)
      }));
    },

    notificationPushErrors(notif) {
      const errors = this.notificationPushMeta(notif).errors || {};
      return Object.entries(errors)
        .filter(([_, error]) => !!error)
        .map(([channel, error]) => ({channel, error}));
    },

    notificationHasPush(notif) {
      return Object.keys(this.notificationPushMeta(notif).results || {}).length > 0;
    },

    notificationPushMeta(notif) {
      const meta = notif && notif.meta ? notif.meta : {};
      if (meta.push) return meta.push;
      if (notif && notif.event === 'push_sent') return meta;
      return {};
    },

    async markRead(id) {
      try {
        await apiFetch(`/api/notifications/${id}/read`, {method: 'POST'});
        await this.loadNotifications();
      } catch (error) {
        console.error('标记失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '标记通知失败'));
      }
    },

    async markAllRead() {
      try {
        await apiFetch('/api/notifications/read-all', {method: 'POST'});
        this.showNotification('success', '全部已读');
        await this.loadNotifications();
      } catch (error) {
        console.error('操作失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '标记全部已读失败'));
      }
    },

    async clearNotifications() {
      if (this.requestDangerConfirmation && !await this.requestDangerConfirmation({title:'清空所有通知', message:'此操作会删除全部通知历史。', phrase:'CLEAR'})) return;
      try {
        await apiFetch('/api/notifications/clear', {method: 'POST'});
        this.showNotification('success', '已清空');
        await this.loadNotifications();
      } catch (error) {
        console.error('清空失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '清空通知失败'));
      }
    },

    /// 清除任务日志：只删已结束的记录，排队中和运行中的任务不受影响。
    async clearJobLogs() {
      if (this.requestDangerConfirmation && !await this.requestDangerConfirmation({
        title: '清除任务日志',
        message: '会删除全部已完成、失败和已取消的任务记录（含归档），排队中与运行中的任务保留。',
        phrase: 'CLEAR'
      })) return;
      try {
        const result = await apiData('/api/jobs/clear', {method: 'POST'});
        const removed = (result && result.removed) || 0;
        this.showNotification('success', `已清除 ${removed} 条任务日志`);
        await this.loadActivity();
      } catch (error) {
        console.error('清除任务日志失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '清除任务日志失败'));
      }
    },

    // ===== 网盘 =====
    showNotification(type, message) {
      const container = document.getElementById('toastContainer');
      if (!container) {
        console[type === 'error' ? 'error' : 'info'](`[${type}] ${message}`);
        return;
      }

      type = normalizeNotificationType(type);
      const toast = document.createElement('div');
      toast.className = `toast toast-${type}`;

      const icon = toastIcon(type);

      const iconEl = document.createElement('span');
      iconEl.className = 'toast-icon';
      iconEl.textContent = icon;

      const messageEl = document.createElement('span');
      messageEl.className = 'toast-message';
      messageEl.textContent = String(message || '');

      toast.appendChild(iconEl);
      toast.appendChild(messageEl);

      container.appendChild(toast);

      setTimeout(() => {
        toast.style.transition = 'all 0.3s ease-out';
        toast.style.opacity = '0';
        toast.style.transform = 'translateX(400px)';
        setTimeout(() => toast.remove(), 300);
      }, 3000);
    }
    };
  }

  return {TOAST_ICONS, normalizeNotificationType, toastIcon, activityTimestamp, mergeActivityItems, createStore};
});
