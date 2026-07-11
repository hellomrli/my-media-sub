(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDiagnostics = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';
  const {apiData, apiFetch, getApiErrorMessage} = root.MediaSubApi || {};

  function downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function createStore() {
    return {
      diagnostics: null,
      storageLifecycle: null,
      diagnosticsLoading: false,
      storedBackups: [],
      backupArchive: null,
      backupPreview: null,
      backupVerification: null,
      backupVerifying: false,
      restoreConfirmation: '',
      logFilter: 'info',
      logFilterSaving: false,

      async loadDiagnostics() {
        this.diagnosticsLoading = true;
        try {
          const [diagnostics, backups, logFilter, backupVerification, storageLifecycle] = await Promise.all([
            apiData('/api/diagnostics'),
            apiData('/api/backups'),
            apiData('/api/observability/log-filter'),
            apiData('/api/backups/verification'),
            apiData('/api/storage/cleanup')
          ]);
          this.diagnostics = diagnostics;
          this.storedBackups = backups || [];
          this.logFilter = (logFilter && logFilter.filter) || 'info';
          this.backupVerification = backupVerification || null;
          this.storageLifecycle = storageLifecycle || null;
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '加载诊断信息失败'));
        } finally {
          this.diagnosticsLoading = false;
        }
      },

      async updateLogFilter() {
        this.logFilterSaving = true;
        try {
          const result = await apiData('/api/observability/log-filter', {
            method: 'PUT', headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({filter: this.logFilter})
          });
          this.logFilter = result.filter;
          this.showNotification('success', '运行时日志过滤规则已更新');
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '更新日志过滤规则失败'));
        } finally {
          this.logFilterSaving = false;
        }
      },

      async downloadProtectedFile(path, filename) {
        const response = await apiFetch(path);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        downloadBlob(await response.blob(), filename);
      },

      async exportDiagnostics() {
        try {
          await this.downloadProtectedFile('/api/diagnostics/export', 'my-media-sub-diagnostics.json');
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '导出诊断包失败'));
        }
      },

      async copyDiagnostics() {
        if (!this.diagnostics) return;
        const ux = root.MediaSubUx || {};
        await this.copyText(ux.safeJson ? ux.safeJson(this.diagnostics) : JSON.stringify(this.diagnostics, null, 2));
      },

      async exportBackup() {
        try {
          await this.downloadProtectedFile('/api/backups/export', `my-media-sub-backup-${Date.now()}.json`);
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '下载备份失败'));
        }
      },

      async compactStorage() {
        if (this.requestDangerConfirmation && !await this.requestDangerConfirmation({title:'按保留策略清理 Store', message:'系统会先创建并验证备份，再按预览中的独立保留策略删除历史数据。', phrase:'CLEANUP DATA'})) return;
        try {
          const result = await apiData('/api/storage/cleanup', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({confirmation: 'CLEANUP DATA'})});
          this.showNotification('success', result.message || 'Store 生命周期清理完成');
          await this.loadDiagnostics();
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, 'Store 生命周期清理失败'));
        }
      },

      async verifyStoredBackup() {
        this.backupVerifying = true;
        try {
          this.backupVerification = await apiData('/api/backups/verification', {method: 'POST'});
          this.showNotification('success', '备份已通过隔离恢复验证');
          await this.loadDiagnostics();
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '备份可恢复性验证失败'));
        } finally {
          this.backupVerifying = false;
        }
      },

      async createStoredBackup() {
        try {
          await apiData('/api/backups', {method: 'POST'});
          this.showNotification('success', '服务器备份已创建');
          await this.loadDiagnostics();
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '创建备份失败'));
        }
      },

      async selectBackupFile(event) {
        const file = event.target.files && event.target.files[0];
        this.backupArchive = null;
        this.backupPreview = null;
        this.restoreConfirmation = '';
        if (!file) return;
        try {
          this.backupArchive = JSON.parse(await file.text());
          this.backupPreview = await apiData('/api/backups/preview', {
            method: 'POST', headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(this.backupArchive)
          });
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '备份预览失败'));
        }
      },

      async restoreBackup() {
        if (!this.backupArchive || !this.backupPreview) return;
        if (this.restoreConfirmation !== 'RESTORE DATA') {
          this.showNotification('warning', '请输入 RESTORE DATA 以确认恢复');
          return;
        }
        if (this.requestDangerConfirmation && !await this.requestDangerConfirmation({title:'恢复完整备份', message:'恢复会覆盖业务数据并要求立即重启。', phrase:'RESTORE DATA'})) return;
        try {
          const result = await apiData('/api/backups/restore', {
            method: 'POST', headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({archive: this.backupArchive, confirmation: this.restoreConfirmation})
          });
          this.showNotification('success', result.message || '恢复完成，请重启服务');
        } catch (error) {
          this.showNotification('error', getApiErrorMessage(error, '恢复失败'));
        }
      },

      diagnosticBytes(value) {
        const bytes = Number(value || 0);
        if (bytes < 1024) return `${bytes} B`;
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
        return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
      },

      diagnosticAverage(total, count) {
        const samples = Number(count || 0);
        if (!samples) return '-';
        const milliseconds = Number(total || 0) / samples / 1000;
        return `${milliseconds.toFixed(2)} ms`;
      }
    };
  }
  return {createStore};
});
