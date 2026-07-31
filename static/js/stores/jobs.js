(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubJobs = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiFetch} = api;

  function createStore() {
    return {
    jobs: [],
    jobEvents: null,
    /// SSE 是否在正常推送。为 true 时活动轮询不再重复拉整个任务列表，
    /// 只补通知——通知没有 SSE 通道。断线后回落到全量轮询。
    jobEventsHealthy: false,
    backgroundJobFilterStatus: 'all',
    selectedJob: null,
    showJobDetailDialog: false,

    // 网盘
    get backgroundJobs() {
      return this.jobs.filter(job => ['manual_transfer', 'subscription_transfer', 'metadata_scrape', 'push_dispatch'].includes(job.kind));
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

    jobPriorityLabel(priority) {
      const labels = {high: '高', normal: '普通', low: '低'};
      return labels[priority || 'normal'] || priority;
    },

    jobErrorClassLabel(errorClass) {
      const labels = {
        rate_limited: '上游限流', transient: '临时故障', authentication: '认证失败',
        validation: '参数错误', not_found: '资源不存在', permanent: '永久失败',
        internal: '内部错误', timed_out: '执行超时'
      };
      return labels[errorClass] || errorClass || '-';
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
        `优先级：${this.jobPriorityLabel(job.priority)}`,
        `执行次数：${job.attempt || 1}`,
        `错误分类：${this.jobErrorClassLabel(job.error_class)}`,
        `下次重试：${job.next_attempt_at ? this.formatTime(job.next_attempt_at) : '-'}`,
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

    async setJobPriority(job, priority) {
      if (!job || job.status !== 'queued' || !['high', 'normal', 'low'].includes(priority)) return;
      try {
        const response = await apiFetch(`/api/jobs/${job.id}/priority`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({priority})
        });
        const data = await response.json();
        if (response.ok) {
          this.upsertJob(data.data);
          this.showNotification('success', `任务优先级已调整为${this.jobPriorityLabel(priority)}`);
        } else {
          this.showNotification('error', data.message || '调整任务优先级失败');
          await this.loadJobs();
        }
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, '调整任务优先级失败'));
        await this.loadJobs();
      }
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
      if (this.selectedJob && this.selectedJob.id === job.id) this.selectedJob = job;
    },

    setupJobEvents() {
      if (this.jobEvents || typeof EventSource === 'undefined') return;

      const source = new EventSource('/api/jobs/events');
      // 快照是周期性全量，job 事件是增量。job 处理器是 async 的，可能在 await
      // 联动刷新时被快照打断；若快照直接整体替换 this.jobs，会把刚 upsert 的
      // 任务冲掉（或让旧快照“复活”已成功/失败的任务）。因此在途期间快照被
      // 暂存，待处理完后应用，并重新叠加在途期间 upsert 的最新任务。
      let snapshotPending = null;
      let jobHandlersInFlight = 0;
      const upsertedDuringPending = new Map();

      const applySnapshot = (jobs) => {
        const list = Array.isArray(jobs) ? jobs.slice() : [];
        for (const job of upsertedDuringPending.values()) {
          const index = list.findIndex(item => item.id === job.id);
          if (index >= 0) list.splice(index, 1, job);
          else list.unshift(job);
        }
        this.jobs = list;
      };

      source.addEventListener('snapshot', (event) => {
        this.jobEventsHealthy = true;
        let jobs;
        try {
          jobs = JSON.parse(event.data || '[]');
        } catch (error) {
          console.error('解析任务快照失败:', error);
          return;
        }
        if (jobHandlersInFlight > 0) {
          snapshotPending = jobs;
          return;
        }
        applySnapshot(jobs);
      });
      source.addEventListener('job', async (event) => {
        this.jobEventsHealthy = true;
        jobHandlersInFlight += 1;
        try {
          const job = JSON.parse(event.data);
          upsertedDuringPending.set(job.id, job);
          this.upsertJob(job);
          if (['succeeded', 'failed', 'canceled'].includes(job.status)) {
            await this.loadNotifications();
          }
          if (job.kind === 'metadata_scrape' && job.status === 'succeeded') {
            await this.loadSubscriptions();
          }
        } catch (error) {
          console.error('解析任务事件失败:', error);
        } finally {
          jobHandlersInFlight -= 1;
          if (jobHandlersInFlight === 0) {
            if (snapshotPending) {
              const pending = snapshotPending;
              snapshotPending = null;
              applySnapshot(pending);
            }
            upsertedDuringPending.clear();
          }
        }
      });
      source.onerror = () => {
        // 标记不健康，让活动轮询接管全量刷新直到 SSE 自行重连成功。
        this.jobEventsHealthy = false;
        console.warn('任务事件连接异常，浏览器会自动重连');
      };
      this.jobEvents = this.ownLifecycle('jobs-event-source', source, eventSource => eventSource.close());
    },

    };
  }

  return {createStore};
});
