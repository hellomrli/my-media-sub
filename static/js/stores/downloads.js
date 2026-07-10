(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDownloads = moduleApi;
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

  function normalizeDownloadGroups(value) {
    const source = value && typeof value === 'object' ? value : {};
    return {
      active: Array.isArray(source.active) ? source.active : [],
      waiting: Array.isArray(source.waiting) ? source.waiting : [],
      stopped: Array.isArray(source.stopped) ? source.stopped : []
    };
  }

  function flattenDownloadTasks(value) {
    const groups = normalizeDownloadGroups(value);
    return [...groups.active, ...groups.waiting, ...groups.stopped];
  }

  function summarizeActiveDownloads(value) {
    const active = normalizeDownloadGroups(value).active;
    return {
      speed: active.reduce((sum, item) => sum + Number(item.download_speed || 0), 0),
      completed: active.reduce((sum, item) => sum + Number(item.completed_length || 0), 0),
      total: active.reduce((sum, item) => sum + Number(item.total_length || 0), 0)
    };
  }

  function downloadTaskCapabilities(task) {
    const status = task && task.status;
    return {
      pause: ['active', 'waiting'].includes(status),
      resume: status === 'paused',
      stop: ['active', 'waiting', 'paused'].includes(status)
    };
  }

  function createStore() {
    return {
    downloads: {active: [], waiting: [], stopped: []},
    downloadsLoading: false,
    downloadsRefreshing: false,
    downloadsError: '',
    downloadsUpdatedAt: null,
    downloadsAutoRefresh: true,
    downloadsPoller: null,
    downloadsBulkAction: '',
    downloadTaskActions: {},

    // 在线更新
    get allDownloadTasks() {
      return flattenDownloadTasks(this.downloads);
    },

    get downloadAutomationStats() {
      const tasks = this.allDownloadTasks;
      const linked = tasks.filter(task => task.automation);
      return {
        linked: linked.length,
        manual: tasks.length - linked.length,
        activeLinked: linked.filter(task => ['active', 'waiting', 'paused'].includes(task.status)).length,
        strmReady: linked.filter(task => task.automation.strm_status === 'generated').length
      };
    },

    get downloadStats() {
      return summarizeActiveDownloads(this.downloads);
    },

    aria2Configured() {
      return Boolean(String((this.settings && this.settings.aria2_rpc_url) || '').trim());
    },

    async loadDownloads(silent = false) {
      if (!this.aria2Configured()) {
        this.downloads = {active: [], waiting: [], stopped: []};
        this.downloadsError = '';
        this.downloadsLoading = false;
        this.downloadsRefreshing = false;
        return;
      }
      if (this.downloadsLoading || this.downloadsRefreshing) return;
      this.downloadsLoading = !silent;
      this.downloadsRefreshing = silent;
      try {
        const data = await apiData('/api/drive/aria2/tasks?stopped_limit=10');
        this.downloads = normalizeDownloadGroups(data);
        this.downloadsError = '';
        this.downloadsUpdatedAt = Date.now();
      } catch (error) {
        console.error('加载 Aria2 任务失败:', error);
        this.downloadsError = this.apiErrorMessage(error, '加载 Aria2 任务失败');
      } finally {
        this.downloadsLoading = false;
        this.downloadsRefreshing = false;
      }
    },

    async controlAllDownloads(action) {
      const labels = {
        pause: '暂停全部下载任务',
        stop: '停止全部下载任务'
      };
      if (action === 'stop' && !confirm('确定停止全部活动和排队中的 Aria2 下载任务？')) return;
      this.downloadsBulkAction = action;
      try {
        const data = await apiData(`/api/drive/aria2/tasks/${action}-all`, {method: 'POST'});
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
          return;
        }
        this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
        await this.loadDownloads(true);
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, `${labels[action] || '操作'}失败`));
      } finally {
        this.downloadsBulkAction = '';
      }
    },

    async controlDownloadTask(task, action) {
      if (!task || !task.gid) return;
      const labels = {
        pause: '暂停下载任务',
        resume: '继续下载任务',
        stop: '停止下载任务',
        delete: '删除下载任务记录'
      };
      if (action === 'stop' && !confirm(`确定停止下载任务 ${task.file_name || task.gid}？`)) return;
      if (action === 'delete' && !confirm(`确定删除下载任务记录 ${task.file_name || task.gid}？`)) return;

      this.downloadTaskActions = {...this.downloadTaskActions, [task.gid]: action};
      try {
        const data = await apiData(`/api/drive/aria2/tasks/${encodeURIComponent(task.gid)}/${action}`, {method: 'POST'});
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
          return;
        }
        this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
        await this.loadDownloads(true);
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, `${labels[action] || '操作'}失败`));
      } finally {
        const next = {...this.downloadTaskActions};
        delete next[task.gid];
        this.downloadTaskActions = next;
      }
    },

    downloadTaskOriginLabel(task) {
      if (!task || !task.automation) return '网盘手动任务';
      const episode = task.automation.episode ? ` · E${String(task.automation.episode).padStart(2, '0')}` : '';
      return `${task.automation.subscription_title || '订阅自动化'}${episode}`;
    },

    downloadTaskAutomationSteps(task) {
      if (!task || !task.automation) return [];
      const aria2Status = {
        active: 'active', waiting: 'active', paused: 'warning', complete: 'success',
        error: 'error', removed: 'error'
      }[task.status] || 'idle';
      const strmStatus = {generated: 'success', failed: 'error', not_recorded: 'idle'}[task.automation.strm_status] || 'idle';
      return [
        {id: 'transfer', label: '转存', status: task.automation.transfer_status === 'completed' ? 'success' : 'idle'},
        {id: 'rename', label: '重命名', status: task.automation.rename_status === 'completed' ? 'success' : 'idle'},
        {id: 'strm', label: 'STRM', status: strmStatus},
        {id: 'aria2', label: 'Aria2', status: aria2Status}
      ];
    },

    downloadTaskStepClass(step) {
      return `download-pipeline-step is-${(step && step.status) || 'idle'}`;
    },

    openDownloadTaskSubscription(task) {
      if (!task || !task.automation || !task.automation.subscription_id) return;
      this.openSubscriptionDetail(task.automation.subscription_id);
    },

    hasRunningDownloadTasks() {
      return [...(this.downloads.active || []), ...(this.downloads.waiting || [])]
        .some(task => ['active', 'waiting', 'paused'].includes(task.status));
    },

    downloadTaskActionLoading(task) {
      return task && task.gid ? this.downloadTaskActions[task.gid] || '' : '';
    },

    canPauseDownloadTask(task) {
      return downloadTaskCapabilities(task).pause;
    },

    canResumeDownloadTask(task) {
      return downloadTaskCapabilities(task).resume;
    },

    canStopDownloadTask(task) {
      return downloadTaskCapabilities(task).stop;
    },

    startDownloadsPolling() {
      this.stopDownloadsPolling();
      if (!this.aria2Configured()
        || !this.downloadsAutoRefresh
        || (this.currentTab !== 'downloads' && this.currentTab !== 'dashboard')) return;
      this.downloadsPoller = this.startPolling('downloads', () => this.loadDownloads(true), 2000);
    },

    stopDownloadsPolling() {
      this.stopPolling('downloads');
      this.downloadsPoller = null;
    },

    downloadStatusLabel(status) {
      const labels = {
        active: '下载中',
        waiting: '排队中',
        paused: '已暂停',
        complete: '已完成',
        error: '失败',
        removed: '已移除'
      };
      return labels[status] || status || '-';
    },

    downloadStatusClass(status) {
      if (status === 'active') return 'bg-primary/20 text-primary';
      if (status === 'waiting') return 'bg-warning/20 text-warning';
      if (status === 'complete') return 'bg-success/20 text-success';
      if (status === 'error') return 'bg-danger/20 text-danger';
      return 'bg-muted/20 text-text/80';
    },

    downloadStatusBadgeClass(status) {
      if (status === 'active') return 'badge badge-primary';
      if (status === 'waiting') return 'badge badge-warning';
      if (status === 'complete') return 'badge badge-success';
      if (status === 'error') return 'badge badge-danger';
      return 'badge badge-muted';
    },

    downloadProgressStyle(task) {
      const value = Math.max(0, Math.min(100, Number(task && task.progress ? task.progress : 0)));
      return `width: ${value}%`;
    },

    formatPercent(value) {
      return mediaFormatters.formatPercent(value);
    },

    formatDownloadSize(bytes) {
      return mediaFormatters.formatBytes(bytes);
    },

    formatSpeed(bytes) {
      return mediaFormatters.formatSpeed(bytes);
    },

    formatDuration(seconds) {
      return mediaFormatters.formatDuration(seconds);
    },

    };
  }

  return {normalizeDownloadGroups, flattenDownloadTasks, summarizeActiveDownloads, downloadTaskCapabilities, createStore};
});
