(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDashboard = moduleApi;
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

  function createStore() {
    return {
    dashboardWidgetEnabled(id) {
      const widgets = Array.isArray(this.settings.dashboard_widgets) ? this.settings.dashboard_widgets : [];
      return widgets.length === 0 || widgets.includes(id);
    },

    get dashboardStats() {
      const activeSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'active').length;
      const invalidSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'invalid').length;
      const completedSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'completed').length;
      const runningJobs = this.jobs.filter(job => ['queued', 'running'].includes(job.status)).length;
      const failedJobs = this.jobs.filter(job => job.status === 'failed').length;
      return {
        activeSubs,
        invalidSubs,
        completedSubs,
        runningJobs,
        failedJobs,
        unreadNotifications: this.unreadNotifications,
        downloadSpeed: this.downloadStats.speed
      };
    },

    get dashboardLibraryProgress() {
      const episodic = this.subscriptions.filter(sub => sub.media_type !== 'movie');
      if (!episodic.length) return this.subscriptions.length ? 100 : 0;
      const values = episodic.map(sub => this.subscriptionProgressPercent(sub)).filter(value => Number.isFinite(value));
      return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0;
    },

    dashboardDateLabel() {
      return new Intl.DateTimeFormat('zh-CN', {
        month: 'long',
        day: 'numeric',
        weekday: 'long'
      }).format(new Date());
    },

    get dashboardRecentSubscriptions() {
      return [...this.subscriptions]
        .sort((a, b) => Number(b.last_checked_at || b.updated_at || 0) - Number(a.last_checked_at || a.updated_at || 0))
        .slice(0, 9);
    },

    get dashboardRecentJobs() {
      return [...this.backgroundJobs]
        .sort((a, b) => Number(b.updated_at || b.created_at || 0) - Number(a.updated_at || a.created_at || 0))
        .slice(0, 6);
    },

    get dashboardRecentNotifications() {
      return this.notificationCenterNotifications.slice(0, 6);
    },

    };
  }

  return {createStore};
});
