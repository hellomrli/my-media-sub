(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubNotifications = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiData, apiFetch, getApiErrorMessage} = api;
  const mediaFormatters = root.MediaSubFormatters || {};
  const searchResultTools = root.MediaSubSearchResults || {};
  const subscriptionDetailTools = root.MediaSubSubscriptionDetail || {};
  const calendarTools = root.MediaSubCalendar || {};
  const sourceSwitchTools = root.MediaSubSourceSwitch || {};
  const automationEventTools = root.MediaSubAutomationEvents || {};

  const TOAST_ICONS = Object.freeze({success: '✓', error: '✕', warning: '⚠', info: 'ℹ'});

  function normalizeNotificationType(type) {
    return Object.prototype.hasOwnProperty.call(TOAST_ICONS, type) ? type : 'info';
  }

  function toastIcon(type) {
    return TOAST_ICONS[normalizeNotificationType(type)];
  }

  function filterNotificationItems(items, filter = 'all') {
    const list = Array.isArray(items) ? items : [];
    return filter === 'unread' ? list.filter(item => !item.read) : list;
  }

  function createStore() {
    return {
    notifications: [],
    notificationsPoller: null,
    notificationFilter: 'all',
    notificationFilters: [
      {id: 'all', name: '全部'},
      {id: 'unread', name: '未读'}
    ],
    get unreadNotifications() {
      return this.notificationCenterNotifications.filter(n => !n.read).length;
    },

    get pushNotifications() {
      return this.notificationCenterNotifications.filter(n => this.notificationHasPush(n));
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

    get systemNotifications() {
      return this.notificationCenterNotifications;
    },

    get filteredNotifications() {
      return filterNotificationItems(this.notificationCenterNotifications, this.notificationFilter);
    },

    async loadNotifications() {
      try {
        const response = await apiFetch('/api/notifications');
        const data = await response.json();
        this.notifications = data.data || [];
      } catch (error) {
        console.error('加载通知失败:', error);
      }
    },

    startNotificationsPolling() {
      this.stopNotificationsPolling();
      if (this.currentTab !== 'dashboard') return;
      this.notificationsPoller = this.startPolling('notifications', () => this.loadNotifications(), 30000);
    },

    stopNotificationsPolling() {
      this.stopPolling('notifications');
      this.notificationsPoller = null;
    },

    notificationFilterCount(filterId) {
      if (filterId === 'unread') return this.unreadNotifications;
      return this.notificationCenterNotifications.length;
    },

    notificationLevelLabel(level) {
      const labels = {info: '信息', success: '成功', warning: '警告', error: '错误'};
      return labels[level] || level || '信息';
    },

    notificationLevelClass(level) {
      const classes = {
        info: 'bg-primary/20 text-primary',
        success: 'bg-success/20 text-success',
        warning: 'bg-warning/20 text-warning',
        error: 'bg-danger/20 text-danger'
      };
      return classes[level] || classes.info;
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

    notificationPushChannels(notif) {
      const statuses = this.notificationPushChannelStatuses(notif);
      if (statuses.length === 0) return '-';
      return statuses.map(item => item.name).join('、');
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
      if (!confirm('确定清空所有通知？')) return;
      try {
        await apiFetch('/api/notifications/clear', {method: 'POST'});
        this.showNotification('success', '已清空');
        await this.loadNotifications();
      } catch (error) {
        console.error('清空失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '清空通知失败'));
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

  return {TOAST_ICONS, normalizeNotificationType, toastIcon, filterNotificationItems, createStore};
});
