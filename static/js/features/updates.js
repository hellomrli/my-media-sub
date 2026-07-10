(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubUpdates = moduleApi;
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

  function normalizeUpdateProgress(progress) {
    if (!progress) return null;
    const percent = Math.max(0, Math.min(100, Number(progress.percent) || 0));
    if (!progress.running && !progress.error && progress.stage === 'idle' && percent === 0) return null;
    return {
      ...progress,
      percent,
      downloaded_bytes: Number(progress.downloaded_bytes) || 0,
      total_bytes: progress.total_bytes ? Number(progress.total_bytes) : null
    };
  }

  function createStore() {
    return {
    updateInfo: null,
    updateLoading: false,
    updateApplying: false,
    updateError: '',
    updateReleases: [],
    updateReleasesLoading: false,
    selectedUpdateTag: '',
    updateProgress: null,
    updateProgressTimer: null,
    showUpdateProgressDialog: false,
    updateRestarting: false,
    updateRestartError: '',

    // 转存相关
    async checkUpdate(silent = false) {
      this.updateLoading = true;
      this.updateError = '';
      try {
        const [checkResponse] = await Promise.all([
          apiFetch('/api/update/check'),
          this.loadUpdateReleases(true)
        ]);
        const response = checkResponse;
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateInfo = result.data;
          if (!silent) {
            this.showNotification(
              result.data.update_available ? 'success' : 'info',
              result.data.update_available ? `发现新版本 ${result.data.latest_tag}` : '当前已是最新版本'
            );
          }
        } else {
          this.updateError = result.message || result.error || '检查更新失败';
          if (!silent) this.showNotification('error', this.updateError);
        }
      } catch (error) {
        this.updateError = this.apiErrorMessage(error, '检查更新失败');
        if (!silent) this.showNotification('error', this.updateError);
      } finally {
        this.updateLoading = false;
      }
    },

    async loadUpdateReleases(silent = false) {
      this.updateReleasesLoading = true;
      try {
        const response = await apiFetch('/api/update/releases', {cache: 'no-store'});
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateReleases = result.data || [];
          if (!this.selectedUpdateTag) {
            const latest = this.updateReleases.find(item => !item.is_current && item.asset) || this.updateReleases[0];
            this.selectedUpdateTag = latest ? latest.tag : '';
          }
        } else if (!silent) {
          this.updateError = result.message || result.error || '读取版本列表失败';
          this.showNotification('error', this.updateError);
        }
      } catch (error) {
        if (!silent) {
          this.updateError = this.apiErrorMessage(error, '读取版本列表失败');
          this.showNotification('error', this.updateError);
        }
      } finally {
        this.updateReleasesLoading = false;
      }
    },

    selectedUpdateRelease() {
      return this.updateReleases.find(item => item.tag === this.selectedUpdateTag) || null;
    },

    selectedUpdateActionLabel() {
      const item = this.selectedUpdateRelease();
      if (!item) return '选择版本';
      if (item.is_current) return '当前版本';
      return item.is_newer ? '升级到所选版本' : '回退到所选版本';
    },

    selectedUpdateDescription() {
      const item = this.selectedUpdateRelease();
      if (!item) return '请选择一个 Release 版本。';
      if (!item.asset) return '该版本没有 Linux x86_64 二进制包，不能在线切换。';
      if (item.is_current) return '当前服务已经运行该版本。';
      const direction = item.is_newer ? '升级' : '回退';
      return `将${direction}到 ${item.tag}，替换二进制和 WebUI 静态资源，完成后需要重启。`;
    },

    canApplySelectedUpdate() {
      const item = this.selectedUpdateRelease();
      return Boolean(item && item.asset && !item.is_current && !this.updateApplying);
    },

    async applySelectedUpdate() {
      const item = this.selectedUpdateRelease();
      if (!item) {
        this.showNotification('info', '请先选择版本');
        return;
      }
      if (item.is_current) {
        this.showNotification('info', '当前已经是所选版本');
        return;
      }
      await this.applyUpdate(item.tag);
    },

    async applyUpdate(targetTag = null) {
      if (!targetTag && (!this.updateInfo || !this.updateInfo.update_available)) {
        this.showNotification('info', '当前已是最新版本');
        return;
      }
      this.updateApplying = true;
      this.updateError = '';
      this.updateRestartError = '';
      this.updateRestarting = false;
      this.showUpdateProgressDialog = true;
      this.setLocalUpdateProgress(1, '正在准备升级');
      this.startUpdateProgressPolling();
      try {
        const body = targetTag ? JSON.stringify({tag: targetTag}) : '{}';
        const response = await apiFetch('/api/update/apply', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body
        });
        const result = await response.json().catch(() => ({}));
        await this.loadUpdateProgress();
        if (response.ok && result.data) {
          const restartRequired = Boolean(result.data.restart_required);
          this.setLocalUpdateProgress(
            100,
            result.data.message || (restartRequired ? '升级完成，请重启服务后生效' : '升级完成'),
            restartRequired ? 'restart_required' : 'completed',
            false
          );
          this.stopUpdateProgressPolling();
          this.showNotification('success', result.data.message || '升级完成');
          if (this.updateInfo) {
            this.updateInfo.current_version = result.data.new_version || this.updateInfo.current_version;
            this.updateInfo.update_available = false;
          }
          await this.loadUpdateReleases(true);
        } else {
          this.updateError = result.message || result.error || '升级失败';
          this.markUpdateProgressFailed(this.updateError);
          this.stopUpdateProgressPolling();
          this.showNotification('error', this.updateError);
        }
      } catch (error) {
        await this.loadUpdateProgress();
        if (this.updateRestartRequired()) {
          this.updateError = '';
          this.showNotification('success', '升级完成，请重启服务后生效');
        } else {
          this.updateError = this.apiErrorMessage(error, '升级失败');
          this.markUpdateProgressFailed(this.updateError);
          this.showNotification('error', this.updateError);
        }
        this.stopUpdateProgressPolling();
      } finally {
        this.updateApplying = false;
        if (!this.updateProgress || !this.updateProgress.running) {
          this.stopUpdateProgressPolling();
        }
      }
    },

    async restartAfterUpdate() {
      if (this.updateRestarting) return;
      this.updateRestarting = true;
      this.updateRestartError = '';
      this.updateError = '';
      this.showUpdateProgressDialog = true;
      this.setLocalUpdateProgress(100, '正在请求服务重启', 'restarting', false);
      try {
        const response = await apiFetch('/api/update/restart', {method: 'POST'});
        const result = await response.json().catch(() => ({}));
        if (!response.ok) {
          throw new Error(result.message || result.error || '请求重启失败');
        }
        this.setLocalUpdateProgress(100, result.data?.message || '服务正在重启，请稍后刷新页面', 'restarting', false);
        await this.waitForServiceRestart();
        window.location.reload();
      } catch (error) {
        if (error instanceof TypeError || error.isNetworkError) {
          try {
            await this.waitForServiceRestart();
            window.location.reload();
            return;
          } catch (pollError) {
            this.updateRestartError = this.apiErrorMessage(pollError, '等待服务恢复超时');
          }
        } else {
          this.updateRestartError = this.apiErrorMessage(error, '重启失败');
        }
        this.setLocalUpdateProgress(100, this.updateRestartError, 'restart_required', false, this.updateRestartError);
        this.showNotification('error', this.updateRestartError);
      } finally {
        this.updateRestarting = false;
      }
    },

    async waitForServiceRestart(timeoutMs = 60000) {
      await this.sleep(2000);
      const startedAt = Date.now();
      while (Date.now() - startedAt < timeoutMs) {
        try {
          const response = await fetch(`/health?restart=${Date.now()}`, {cache: 'no-store'});
          if (response.ok) return true;
        } catch (error) {
          // The connection is expected to fail while the process is restarting.
        }
        await this.sleep(1000);
      }
      throw new Error('服务重启超时，请稍后手动刷新页面');
    },

    sleep(ms) {
      return new Promise(resolve => setTimeout(resolve, ms));
    },

    async loadUpdateProgress(silent = true) {
      try {
        const response = await apiFetch('/api/update/progress', {cache: 'no-store'});
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateProgress = this.normalizeUpdateProgress(result.data);
          if (this.updateProgress && this.updateProgress.stage === 'restart_required') {
            this.showUpdateProgressDialog = true;
          }
          if (this.updateProgress && !this.updateProgress.running) {
            this.stopUpdateProgressPolling();
          }
          return this.updateProgress;
        }
        if (!silent) {
          this.updateError = result.message || result.error || '读取升级进度失败';
        }
      } catch (error) {
        if (!silent) {
          this.updateError = this.apiErrorMessage(error, '读取升级进度失败');
        }
      }
      return this.updateProgress;
    },

    normalizeUpdateProgress(progress) {
      return normalizeUpdateProgress(progress);
    },

    startUpdateProgressPolling() {
      this.stopUpdateProgressPolling();
      this.loadUpdateProgress();
      this.updateProgressTimer = this.startPolling('update-progress', () => this.loadUpdateProgress(), 800);
    },

    stopUpdateProgressPolling() {
      this.stopPolling('update-progress');
      this.updateProgressTimer = null;
    },

    setLocalUpdateProgress(percent, message, stage = 'starting', running = true, error = null) {
      const current = this.updateProgress || {};
      this.updateProgress = {
        running,
        percent: Math.max(0, Math.min(100, Number(percent) || 0)),
        stage,
        message,
        downloaded_bytes: Number(current.downloaded_bytes) || 0,
        total_bytes: current.total_bytes || null,
        error,
        updated_at: new Date().toISOString()
      };
    },

    markUpdateProgressFailed(message) {
      const currentPercent = this.updateProgress ? this.updateProgress.percent : 1;
      this.setLocalUpdateProgress(currentPercent, message, 'failed', false, message);
    },

    updateRestartRequired() {
      return Boolean(this.updateProgress && this.updateProgress.stage === 'restart_required' && !this.updateProgress.error);
    },

    updateDialogBusy() {
      return Boolean(this.updateApplying || this.updateRestarting || (this.updateProgress && this.updateProgress.running));
    },

    updateDialogCanClose() {
      return !this.updateDialogBusy() && !this.updateRestartRequired();
    },

    closeUpdateProgressDialog() {
      if (this.updateDialogCanClose()) {
        this.showUpdateProgressDialog = false;
      }
    },

    updateDialogTitle() {
      if (this.updateRestarting || (this.updateProgress && this.updateProgress.stage === 'restarting')) return '服务重启中';
      if (this.updateProgress && this.updateProgress.error) return '升级失败';
      if (this.updateRestartRequired()) return '升级完成';
      return '在线升级';
    },

    updateProgressPercent() {
      return this.updateProgress ? this.updateProgress.percent : 0;
    },

    updateProgressMessage() {
      return this.updateProgress && this.updateProgress.message ? this.updateProgress.message : '正在准备升级';
    },

    updateProgressBarClass() {
      if (this.updateProgress && this.updateProgress.error) return 'bg-danger';
      if (this.updateProgress && this.updateProgress.percent >= 100) return 'bg-success';
      return 'bg-primary';
    },

    updateStatusLabel() {
      if (!this.updateInfo) return '未检查';
      return this.updateInfo.update_available ? '可更新' : '已最新';
    },

    updateStatusClass() {
      if (!this.updateInfo) return 'text-muted';
      return this.updateInfo.update_available ? 'text-warning' : 'text-success';
    },

    updateRuntimeLabel() {
      if (!this.updateInfo) return '-';
      return this.updateInfo.runtime === 'docker' ? 'Docker' : '二进制';
    },

    assetSizeLabel(asset) {
      return asset && asset.size ? this.formatSize(asset.size) : '-';
    },

    formatUpdateTime(value) {
      return mediaFormatters.formatDateTime(value);
    },

    };
  }

  return {normalizeUpdateProgress, createStore};
});
