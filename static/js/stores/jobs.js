(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubJobs = moduleApi;
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
    jobs: [],
    jobEvents: null,
    backgroundJobFilterKind: 'all',
    backgroundJobFilterStatus: 'all',
    backgroundJobQuery: '',
    selectedJob: null,
    showJobDetailDialog: false,

    // 网盘
    get backgroundLogStats() {
      const jobs = this.backgroundJobs;
      const success = jobs.filter(job => job.status === 'succeeded').length;
      const failed = jobs.filter(job => job.status === 'failed').length;
      const canceled = jobs.filter(job => job.status === 'canceled').length;
      const active = jobs.filter(job => ['queued', 'running'].includes(job.status)).length;
      const saved = jobs.reduce((sum, job) => sum + Number((job.result || {}).saved_count || (job.result || {}).transferred_count || 0), 0);
      return {total: jobs.length, active, success, failed, canceled, saved};
    },

    get backgroundJobs() {
      return this.jobs.filter(job => ['manual_transfer', 'subscription_transfer', 'metadata_scrape', 'push_dispatch'].includes(job.kind));
    },

    get recentBackgroundJobs() {
      return this.filteredBackgroundJobs.slice(0, 80);
    },

    get filteredBackgroundJobs() {
      const query = this.backgroundJobQuery.trim().toLowerCase();
      return this.backgroundJobs.filter(job => {
        if (this.backgroundJobFilterKind !== 'all' && job.kind !== this.backgroundJobFilterKind) return false;
        if (this.backgroundJobFilterStatus !== 'all' && job.status !== this.backgroundJobFilterStatus) return false;
        if (!query) return true;
        return [
          job.title,
          job.message,
          job.error,
          job.kind,
          job.status,
          JSON.stringify(job.payload || {}),
          JSON.stringify(job.result || {})
        ].some(value => String(value || '').toLowerCase().includes(query));
      });
    },

    get backgroundJobKinds() {
      return [
        {id: 'all', name: '全部类型'},
        {id: 'manual_transfer', name: '手动转存'},
        {id: 'subscription_transfer', name: '自动订阅'},
        {id: 'metadata_scrape', name: '元数据刮削'},
        {id: 'push_dispatch', name: '推送派发'}
      ];
    },

    get backgroundJobStatuses() {
      return [
        {id: 'all', name: '全部状态'},
        {id: 'queued', name: '排队中'},
        {id: 'running', name: '执行中'},
        {id: 'succeeded', name: '成功'},
        {id: 'failed', name: '失败'},
        {id: 'canceled', name: '已取消'}
      ];
    },

    async loadJobs() {
      try {
        const response = await apiFetch('/api/jobs');
        const data = await response.json();
        this.jobs = data.data || [];
      } catch (error) {
        console.error('加载任务失败:', error);
      }
    },

    jobStatusLabel(status) {
      const labels = {
        queued: '排队中',
        running: '执行中',
        succeeded: '成功',
        failed: '失败',
        canceled: '已取消'
      };
      return labels[status] || status;
    },

    jobKindLabel(kind) {
      const labels = {
        manual_transfer: '手动转存',
        subscription_transfer: '自动订阅',
        metadata_scrape: '元数据刮削',
        push_dispatch: '推送派发'
      };
      return labels[kind] || kind || '后台任务';
    },

    jobStatusClass(status) {
      const classes = {
        queued: 'bg-warning/20 text-warning',
        running: 'bg-primary/20 text-primary',
        succeeded: 'bg-success/20 text-success',
        failed: 'bg-danger/20 text-danger',
        canceled: 'bg-muted/30 text-text/80'
      };
      return classes[status] || 'bg-muted/30 text-text/80';
    },

    jobStatusBadgeClass(status) {
      const classes = {
        queued: 'badge badge-warning',
        running: 'badge badge-primary',
        succeeded: 'badge badge-success',
        failed: 'badge badge-danger',
        canceled: 'badge badge-muted'
      };
      return classes[status] || 'badge badge-muted';
    },

    resetBackgroundJobFilters() {
      this.backgroundJobFilterKind = 'all';
      this.backgroundJobFilterStatus = 'all';
      this.backgroundJobQuery = '';
    },

    openJobDetail(job) {
      this.selectedJob = job || null;
      this.showJobDetailDialog = !!job;
    },

    jobDurationLabel(job) {
      if (!job) return '-';
      const start = Number(job.started_at || job.created_at || 0);
      const end = Number(job.finished_at || job.updated_at || 0);
      if (!start || !end || end < start) return '-';
      const seconds = end - start;
      if (seconds < 60) return `${seconds}秒`;
      const minutes = Math.floor(seconds / 60);
      const rest = seconds % 60;
      if (minutes < 60) return rest ? `${minutes}分${rest}秒` : `${minutes}分钟`;
      const hours = Math.floor(minutes / 60);
      return `${hours}小时${minutes % 60}分钟`;
    },

    jobSummaryText(job) {
      if (!job) return '';
      const lines = [
        `任务：${job.title || '-'}`,
        `类型：${this.jobKindLabel(job.kind)}`,
        `状态：${this.jobStatusLabel(job.status)}`,
        `进度：${job.progress || 0}%`,
        `创建：${this.formatTime(job.created_at)}`,
        `开始：${this.formatTime(job.started_at)}`,
        `结束：${this.formatTime(job.finished_at)}`,
        `耗时：${this.jobDurationLabel(job)}`,
        `消息：${job.message || '-'}`,
      ];
      if (job.error) lines.push(`错误：${job.error}`);
      lines.push('', 'Payload:', JSON.stringify(job.payload || {}, null, 2));
      lines.push('', 'Result:', JSON.stringify(job.result || {}, null, 2));
      return lines.join('\n');
    },

    async copySelectedJobDetail() {
      if (!this.selectedJob) return;
      await this.copyText(this.jobSummaryText(this.selectedJob));
    },

    jobPayloadPretty(job) {
      return JSON.stringify((job && job.payload) || {}, null, 2);
    },

    jobResultPretty(job) {
      return JSON.stringify((job && job.result) || {}, null, 2);
    },

    canCancelJob(job) {
      return job && ['queued', 'running'].includes(job.status);
    },

    canRetryJob(job) {
      return job && ['failed', 'canceled'].includes(job.status);
    },

    async cancelJob(job) {
      if (!job || !this.canCancelJob(job)) return;
      try {
        const response = await apiFetch(`/api/jobs/${job.id}/cancel`, {method: 'POST'});
        const data = await response.json();
        if (response.ok) {
          this.upsertJob(data.data);
          this.showNotification('success', '任务已取消');
        } else {
          this.showNotification('error', data.message || '取消任务失败');
        }
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, '取消任务失败'));
      }
    },

    async retryJob(job) {
      if (!job || !this.canRetryJob(job)) return;
      try {
        const response = await apiFetch(`/api/jobs/${job.id}/retry`, {method: 'POST'});
        const data = await response.json();
        if (response.ok) {
          this.upsertJob(data.data);
          this.showNotification('success', '重试任务已创建');
        } else {
          this.showNotification('error', data.message || '重试任务失败');
        }
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, '重试任务失败'));
      }
    },

    upsertJob(job) {
      if (!job) return;
      const index = this.jobs.findIndex(item => item.id === job.id);
      if (index >= 0) {
        this.jobs.splice(index, 1, job);
      } else {
        this.jobs.unshift(job);
      }
    },

    setupJobEvents() {
      if (this.jobEvents || typeof EventSource === 'undefined') return;

      const source = new EventSource('/api/jobs/events');
      source.addEventListener('snapshot', (event) => {
        try {
          this.jobs = JSON.parse(event.data || '[]');
        } catch (error) {
          console.error('解析任务快照失败:', error);
        }
      });
      source.addEventListener('job', async (event) => {
        try {
          const job = JSON.parse(event.data);
          this.upsertJob(job);
          if (['succeeded', 'failed', 'canceled'].includes(job.status)) {
            await this.loadNotifications();
          }
          if (job.kind === 'metadata_scrape' && job.status === 'succeeded') {
            await this.loadSubscriptions();
          }
        } catch (error) {
          console.error('解析任务事件失败:', error);
        }
      });
      source.onerror = () => {
        console.warn('任务事件连接异常，浏览器会自动重连');
      };
      this.jobEvents = this.ownLifecycle('jobs-event-source', source, eventSource => eventSource.close());
    },

    };
  }

  return {createStore};
});
