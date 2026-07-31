(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDownloads = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiData} = api;
  const mediaFormatters = root.MediaSubFormatters || {};
  const FULL_STOPPED_LIMIT = 1000;
  const POLL_STOPPED_LIMIT = 0;

  function normalizeDownloadGroups(value) {
    const source = value && typeof value === 'object' ? value : {};
    return {
      active: Array.isArray(source.active) ? source.active : [],
      waiting: Array.isArray(source.waiting) ? source.waiting : [],
      stopped: Array.isArray(source.stopped) ? source.stopped : []
    };
  }

  function hasPollableDownloadTasks(value) {
    const groups = normalizeDownloadGroups(value);
    return [...groups.active, ...groups.waiting].some(task => {
      const status = String((task && task.status) || '').trim().toLowerCase();
      return !status || status === 'active' || status === 'waiting';
    });
  }

  function flattenDownloadTasks(value) {
    const groups = normalizeDownloadGroups(value);
    const tasks = [...groups.active, ...groups.waiting, ...groups.stopped];
    const seen = new Set();
    return tasks.filter(task => {
      const gid = String((task && task.gid) || '').trim();
      // Aria2 can briefly expose the same gid in adjacent status snapshots. Duplicate
      // x-for keys make Alpine's DOM mover lose its anchor and throw while polling.
      if (!gid) return true;
      if (seen.has(gid)) return false;
      seen.add(gid);
      return true;
    });
  }

  function mergeStoppedTasks(current, incoming, limit = FULL_STOPPED_LIMIT) {
    const merged = [];
    const seen = new Set();
    for (const task of [...(incoming || []), ...(current || [])]) {
      const gid = String((task && task.gid) || '').trim();
      if (gid && seen.has(gid)) continue;
      if (gid) seen.add(gid);
      merged.push(task);
      if (merged.length >= limit) break;
    }
    return merged;
  }

  function mergeDownloadGroups(current, incoming) {
    const previous = normalizeDownloadGroups(current);
    const next = normalizeDownloadGroups(incoming);
    return {
      active: next.active,
      waiting: next.waiting,
      stopped: mergeStoppedTasks(previous.stopped, next.stopped)
    };
  }

  function categorizeDownloadTasks(value) {
    const groups = normalizeDownloadGroups(value);
    const seen = new Set();
    const unique = items => (Array.isArray(items) ? items : []).filter(task => {
      const gid = String((task && task.gid) || '').trim();
      if (!gid) return true;
      if (seen.has(gid)) return false;
      seen.add(gid);
      return true;
    });
    const active = unique(groups.active);
    const waiting = unique(groups.waiting);
    const stopped = unique(groups.stopped);
    const statusOf = task => String((task && task.status) || '').trim().toLowerCase();
    return {
      completed: stopped.filter(task => statusOf(task) === 'complete'),
      downloading: active.filter(task => !task || ['active', 'paused'].includes(statusOf(task))),
      queued: waiting.filter(task => !task || ['waiting', 'paused'].includes(statusOf(task))),
      failed: stopped.filter(task => ['error', 'removed'].includes(statusOf(task)))
    };
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
    const status = String((task && task.status) || '').trim().toLowerCase();
    return {
      pause: ['active', 'waiting'].includes(status),
      resume: status === 'paused',
      stop: ['active', 'waiting', 'paused'].includes(status),
      retry: ['error', 'removed'].includes(status)
    };
  }

  function createStore() {
    return {
    downloads: {active: [], waiting: [], stopped: []},
    downloadsLoading: false,
    downloadsRefreshing: false,
    downloadsError: '',
    downloadsUpdatedAt: null,
    downloadsHistoryLoadedAt: 0,
    downloadsAutoRefresh: true,
    downloadsPoller: null,
    downloadsFullRefreshPending: false,
    downloadsBulkAction: '',
    downloadTaskActions: {},

    // 在线更新
    get allDownloadTasks() {
      return flattenDownloadTasks(this.downloads);
    },

    get categorizedDownloads() {
      return categorizeDownloadTasks(this.downloads);
    },

    downloadCategory: 'downloading',
    downloadCategoryTouched: false,
    downloadVisibleLimit: 100,

    get downloadCategoryList() {
      return [
        {id: 'downloading', name: '正在下载', description: '当前正在传输或已暂停的任务'},
        {id: 'queued', name: '队列中', description: '等待 Aria2 调度的任务'},
        {id: 'completed', name: '已完成', description: '最近完成的下载任务'},
        {id: 'failed', name: '下载失败', description: '失败或被移除，可直接重试'}
      ];
    },

    /// 当前分页；分类为空时不强行停留在空页，自动落到有任务的那一页。
    get downloadActiveCategory() {
      const list = this.downloadCategoryList;
      const current = list.find(item => item.id === this.downloadCategory);
      if (current && this.downloadCategoryTasks(current.id).length > 0) return current.id;
      if (current && this.downloadCategoryTouched) return current.id;
      const firstWithTasks = list.find(item => this.downloadCategoryTasks(item.id).length > 0);
      return (firstWithTasks || list[0]).id;
    },

    selectDownloadCategory(id) {
      if (!this.downloadCategoryList.some(item => item.id === id)) return;
      this.downloadCategory = id;
      this.downloadCategoryTouched = true;
    },

    downloadCategoryTasks(category) {
      return (this.categorizedDownloads && this.categorizedDownloads[category]) || [];
    },

    visibleDownloadCategoryTasks(category) {
      const tasks = this.downloadCategoryTasks(category);
      return ['completed', 'failed'].includes(category)
        ? tasks.slice(0, this.downloadVisibleLimit)
        : tasks;
    },

    hasMoreDownloadCategoryTasks(category) {
      return this.downloadCategoryTasks(category).length > this.visibleDownloadCategoryTasks(category).length;
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

    async loadSettledDownloadTask(gid) {
      try {
        return await apiData(`/api/drive/aria2/tasks/${encodeURIComponent(gid)}`);
      } catch (error) {
        // 保留旧任务到下一轮再次确认；瞬时 RPC 失败不应把任务从界面静默丢掉。
        console.warn(`确认 Aria2 任务最终状态失败 ${gid}:`, error);
        return null;
      }
    },

    async loadDownloads(silent = false, options = {}) {
      if (!this.aria2Configured()) {
        if (typeof this.stopPolling === 'function') this.stopDownloadsPolling();
        else this.downloadsPoller = null;
        this.downloads = {active: [], waiting: [], stopped: []};
        this.downloadsError = '';
        this.downloadsLoading = false;
        this.downloadsRefreshing = false;
        this.downloadsHistoryLoadedAt = 0;
        this.downloadsFullRefreshPending = false;
        return;
      }
      const fullHistory = options.fullHistory === true
        || !silent
        || !this.downloadsHistoryLoadedAt;
      if (this.downloadsLoading || this.downloadsRefreshing) {
        if (fullHistory) this.downloadsFullRefreshPending = true;
        return;
      }
      this.downloadsLoading = !silent;
      this.downloadsRefreshing = silent;
      try {
        const now = Date.now();
        const stoppedLimit = fullHistory ? FULL_STOPPED_LIMIT : POLL_STOPPED_LIMIT;
        const data = await apiData(`/api/drive/aria2/tasks?stopped_limit=${stoppedLimit}`);
        const previous = normalizeDownloadGroups(this.downloads);
        const next = normalizeDownloadGroups(data);
        if (fullHistory) {
          this.downloads = next;
        } else {
          const nextRunningGids = new Set([...next.active, ...next.waiting]
            .map(task => String((task && task.gid) || '').trim())
            .filter(Boolean));
          const disappearedGids = new Set();
          const disappeared = [...previous.active, ...previous.waiting]
            .filter(task => {
              const gid = String((task && task.gid) || '').trim();
              if (!gid || nextRunningGids.has(gid) || disappearedGids.has(gid)) return false;
              disappearedGids.add(gid);
              return true;
            });
          const settled = await Promise.all(disappeared.map(async previousTask => ({
            previousTask,
            task: await this.loadSettledDownloadTask(previousTask.gid)
          })));

          for (const result of settled) {
            const task = result.task;
            if (!task) {
              const group = result.previousTask.status === 'waiting' || result.previousTask.status === 'paused'
                ? next.waiting
                : next.active;
              group.push(result.previousTask);
              continue;
            }
            if (['complete', 'error', 'removed'].includes(task.status)) next.stopped.push(task);
            else if (task.status === 'active') next.active.push(task);
            else next.waiting.push(task);
          }
          this.downloads = mergeDownloadGroups(previous, next);
        }
        if (fullHistory) this.downloadsHistoryLoadedAt = now;
        this.downloadsError = '';
        this.downloadsUpdatedAt = now;
        this.syncDownloadsPolling();
      } catch (error) {
        console.error('加载 Aria2 任务失败:', error);
        this.downloadsError = this.apiErrorMessage(error, '加载 Aria2 任务失败');
      } finally {
        this.downloadsLoading = false;
        this.downloadsRefreshing = false;
        if (this.downloadsFullRefreshPending) {
          this.downloadsFullRefreshPending = false;
          await this.loadDownloads(true, {fullHistory: true});
        }
      }
    },

    async controlAllDownloads(action) {
      const labels = {
        pause: '暂停全部下载任务',
        stop: '停止全部下载任务',
        purge: '清空已停止的下载记录'
      };
      if (action === 'stop' && !await this.requestDangerConfirmation({title:'停止全部下载', message:'全部活动和排队中的 Aria2 下载任务将停止。'})) return;
      if (action === 'purge' && !await this.requestDangerConfirmation({title:'清空已停止记录', message:'将删除 Aria2 中已完成、已出错和已移除的任务记录，清空后无法重试这些任务。', phrase:'CLEAR'})) return;
      this.downloadsBulkAction = action;
      try {
        const data = await apiData(`/api/drive/aria2/tasks/${action}-all`, {method: 'POST'});
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
          return;
        }
        this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
        await this.loadDownloads(true, {fullHistory: true});
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
      if (action === 'stop' && !await this.requestDangerConfirmation({title:'停止下载任务', message:`将停止 ${task.file_name || task.gid}。`})) return;
      if (action === 'delete' && !await this.requestDangerConfirmation({title:'删除下载记录', message:`将删除 ${task.file_name || task.gid} 的任务记录。`, phrase:'DELETE'})) return;

      this.downloadTaskActions = {...this.downloadTaskActions, [task.gid]: action};
      try {
        const data = await apiData(`/api/drive/aria2/tasks/${encodeURIComponent(task.gid)}/${action}`, {method: 'POST'});
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
          return;
        }
        this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
        await this.loadDownloads(true, {fullHistory: true});
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, `${labels[action] || '操作'}失败`));
      } finally {
        const next = {...this.downloadTaskActions};
        delete next[task.gid];
        this.downloadTaskActions = next;
      }
    },

    async retryDownloadTask(task) {
      if (!task || !this.canRetryDownloadTask(task)) return;
      const gid = String(task.gid || '').trim();
      if (!gid) return;
      this.downloadTaskActions = {...this.downloadTaskActions, [gid]: 'retry'};
      try {
        const data = await apiData(`/api/drive/aria2/tasks/${encodeURIComponent(gid)}/retry`, {method: 'POST'});
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '重试下载失败');
          return;
        }
        this.showNotification('success', data.message || '已重新加入下载队列');
        await this.loadDownloads(true, {fullHistory: true});
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, '重试下载失败'));
      } finally {
        const next = {...this.downloadTaskActions};
        delete next[gid];
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

    hasStoppedDownloadTasks() {
      return (this.downloads.stopped || []).length > 0;
    },

    syncDownloadsPolling() {
      if (this.currentTab !== 'downloads' && this.currentTab !== 'dashboard') {
        this.stopDownloadsPolling();
        return;
      }
      if (hasPollableDownloadTasks(this.downloads)) this.startDownloadsPolling();
      else this.stopDownloadsPolling();
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

    canRetryDownloadTask(task) {
      return downloadTaskCapabilities(task).retry;
    },

    startDownloadsPolling() {
      if (this.downloadsPoller) return;
      if (!this.aria2Configured()
        || !this.downloadsAutoRefresh
        || (this.currentTab !== 'downloads' && this.currentTab !== 'dashboard')
        || !hasPollableDownloadTasks(this.downloads)) return;
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

  return {
    FULL_STOPPED_LIMIT,
    POLL_STOPPED_LIMIT,
    normalizeDownloadGroups,
    mergeStoppedTasks,
    mergeDownloadGroups,
    hasPollableDownloadTasks,
    flattenDownloadTasks,
    categorizeDownloadTasks,
    summarizeActiveDownloads,
    downloadTaskCapabilities,
    createStore
  };
});
