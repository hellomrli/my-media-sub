(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDrive = moduleApi;
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

  function driveTimestamp(value) {
    return mediaFormatters.parseTimestamp(value);
  }

  function isDriveVideo(item) {
    return !!(item && item.file && /\.(mp4|mkv|avi|mov|ts|m4v|wmv|flv|rmvb|webm)$/i.test(item.file_name || ''));
  }

  function filterAndSortDriveItems(items, options = {}) {
    const query = String(options.query || '').trim().toLowerCase();
    const filterType = options.filterType || 'all';
    const sortBy = options.sortBy || 'name';
    const direction = options.direction === 'desc' ? -1 : 1;
    let result = Array.isArray(items) ? [...items] : [];
    if (query) result = result.filter(item => String(item.file_name || '').toLowerCase().includes(query));
    if (filterType !== 'all') {
      result = result.filter(item => {
        if (filterType === 'folder') return !item.file;
        if (filterType === 'video') return isDriveVideo(item);
        if (filterType === 'other') return item.file && !isDriveVideo(item);
        return item.file;
      });
    }
    result.sort((a, b) => {
      if (!a.file && b.file) return -1;
      if (a.file && !b.file) return 1;
      let value = 0;
      if (sortBy === 'size') value = Number(a.size || 0) - Number(b.size || 0);
      else if (sortBy === 'time') value = driveTimestamp(a.updated_at) - driveTimestamp(b.updated_at);
      else value = String(a.file_name || '').localeCompare(String(b.file_name || ''), 'zh-CN', {numeric: true, sensitivity: 'base'});
      return value * direction;
    });
    return result;
  }

  function createStore() {
    return {
    driveItems: [],
    driveCurrentPath: '/',
    driveCurrentFid: '0',  // 当前目录的 fid
    driveFidStack: [{fid: '0', name: '根目录'}],  // 导航栈
    driveLoading: false,
    driveRefreshing: false,
    driveError: '',
    driveLastLoadedAt: null,
    driveSelectMode: false,
    driveSelectedItems: [],
    driveSortBy: 'name',
    driveSortDirection: 'asc',
    driveFilterType: 'all',
    driveSearchQuery: '',
    driveViewMode: 'list',
    driveVisibleLimit: 200,
    driveActionLoading: '',
    showNewFolderModal: false,
    newFolderName: '',

    // 下载任务
    get filteredDriveItems() {
      return filterAndSortDriveItems(this.driveItems, {
        query: this.driveSearchQuery,
        filterType: this.driveFilterType,
        sortBy: this.driveSortBy,
        direction: this.driveSortDirection
      });
    },

    get driveBreadcrumbs() {
      return this.driveFidStack.map((item, index) => ({
        ...item,
        index,
        label: index === 0 ? '根目录' : item.name
      }));
    },

    get visibleDriveItems() {
      return this.filteredDriveItems.slice(0, this.driveVisibleLimit);
    },

    get hasMoreDriveItems() {
      return this.filteredDriveItems.length > this.visibleDriveItems.length;
    },

    get driveStats() {
      const folders = this.driveItems.filter(item => !item.file).length;
      const files = this.driveItems.length - folders;
      const videos = this.driveItems.filter(item => this.isDriveVideo(item)).length;
      const totalSize = this.driveItems
        .filter(item => item.file)
        .reduce((sum, item) => sum + Number(item.size || 0), 0);
      return {folders, files, videos, totalSize};
    },

    get selectedDriveItems() {
      const selected = new Set(this.driveSelectedItems);
      return this.driveItems.filter(item => selected.has(item.fid));
    },

    get selectedDriveFileCount() {
      return this.selectedDriveItems.filter(item => item.file).length;
    },

    async loadDrive(forceRefresh = false) {
      if (this.driveLoading || this.driveRefreshing) return;
      const hadItems = this.driveItems.length > 0;
      this.driveLoading = !hadItems;
      this.driveRefreshing = hadItems;
      this.driveError = '';
      try {
        const params = new URLSearchParams({fid: this.driveCurrentFid});
        if (forceRefresh) params.set('refresh', 'true');
        const data = await apiData(`/api/drive?${params.toString()}`);
        this.driveItems = data.list || [];
        this.driveLastLoadedAt = Date.now();
        this.driveVisibleLimit = 200;
        const visibleFids = new Set(this.driveItems.map(item => item.fid));
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => visibleFids.has(fid));
      } catch (error) {
        console.error('加载网盘失败:', error);
        this.driveError = this.apiErrorMessage(error, '加载网盘失败');
        this.driveItems = [];
      } finally {
        this.driveLoading = false;
        this.driveRefreshing = false;
      }
    },

    updateDriveCurrentPath() {
      if (this.driveFidStack.length <= 1) {
        this.driveCurrentPath = '/';
        return;
      }
      this.driveCurrentPath = this.driveFidStack.slice(1).map(d => d.name).join(' / ');
    },

    async driveGoBack() {
      if (this.driveFidStack.length > 1) {
        this.driveFidStack.pop();
        const parent = this.driveFidStack[this.driveFidStack.length - 1];
        this.driveCurrentFid = parent.fid;
        this.updateDriveCurrentPath();
        this.driveSelectedItems = [];
        await this.loadDrive();
      }
    },

    async driveOpenBreadcrumb(index) {
      if (index < 0 || index >= this.driveFidStack.length) return;
      this.driveFidStack = this.driveFidStack.slice(0, index + 1);
      const current = this.driveFidStack[this.driveFidStack.length - 1];
      this.driveCurrentFid = current.fid;
      this.updateDriveCurrentPath();
      this.driveSelectedItems = [];
      await this.loadDrive();
    },

    async driveItemClick(item) {
      if (this.driveSelectMode) {
        this.toggleDriveItemSelection(item);
        return;
      }
      if (!item.file) {
        this.driveFidStack.push({fid: item.fid, name: item.file_name});
        this.driveCurrentFid = item.fid;
        this.updateDriveCurrentPath();
        this.driveSelectedItems = [];
        await this.loadDrive();
      }
    },

    toggleSelectMode() {
      this.driveSelectMode = !this.driveSelectMode;
      if (!this.driveSelectMode) this.driveSelectedItems = [];
    },

    toggleDriveItemSelection(item) {
      if (!item || !item.fid) return;
      if (this.driveSelectedItems.includes(item.fid)) {
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => fid !== item.fid);
      } else {
        this.driveSelectedItems = [...this.driveSelectedItems, item.fid];
      }
    },

    isDriveItemSelected(item) {
      return item && this.driveSelectedItems.includes(item.fid);
    },

    driveAllVisibleSelected() {
      const visible = this.visibleDriveItems;
      return visible.length > 0 && visible.every(item => this.driveSelectedItems.includes(item.fid));
    },

    toggleVisibleDriveSelection() {
      const visibleFids = this.visibleDriveItems.map(item => item.fid);
      if (visibleFids.length === 0) return;
      if (this.driveAllVisibleSelected()) {
        const visible = new Set(visibleFids);
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => !visible.has(fid));
      } else {
        this.driveSelectedItems = [...new Set([...this.driveSelectedItems, ...visibleFids])];
      }
      this.driveSelectMode = this.driveSelectedItems.length > 0;
    },

    setDriveSort(sortBy) {
      if (this.driveSortBy === sortBy) {
        this.driveSortDirection = this.driveSortDirection === 'asc' ? 'desc' : 'asc';
      } else {
        this.driveSortBy = sortBy;
        this.driveSortDirection = sortBy === 'name' ? 'asc' : 'desc';
      }
      this.driveVisibleLimit = 200;
    },

    showMoreDriveItems() {
      this.driveVisibleLimit += 200;
    },

    driveTimestamp(value) {
      return driveTimestamp(value);
    },

    driveUpdatedLabel(item) {
      const timestamp = this.driveTimestamp(item && item.updated_at);
      if (!timestamp) return '-';
      return mediaFormatters.formatDateTime(timestamp, {seconds: false});
    },

    driveFileExtension(item) {
      const name = (item && item.file_name) || '';
      const match = name.match(/\.([^.]+)$/);
      return match ? match[1].toUpperCase() : 'FILE';
    },

    isDriveVideo(item) {
      return isDriveVideo(item);
    },

    driveItemTypeLabel(item) {
      if (!item) return '-';
      if (!item.file) return '文件夹';
      if (this.isDriveVideo(item)) return '视频';
      return this.driveFileExtension(item);
    },

    driveItemIconClass(item) {
      if (!item || !item.file) return 'bg-warning/15 text-warning border-warning/20';
      if (this.isDriveVideo(item)) return 'bg-success/15 text-success border-success/20';
      return 'bg-primary/15 text-primary border-primary/20';
    },

    async createFolder() {
      if (!this.newFolderName.trim()) return;
      this.driveActionLoading = 'mkdir';
      try {
        const data = await apiData('/api/drive/mkdir', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({parent_fid: this.driveCurrentFid, name: this.newFolderName})
        });
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '创建失败');
          return;
        }
        this.showNotification('success', data.message || '创建成功');
        this.showNewFolderModal = false;
        this.newFolderName = '';
        await this.loadDrive(true);
      } catch (error) {
        console.error('创建失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '创建失败'));
      } finally {
        this.driveActionLoading = '';
      }
    },

    async deleteDriveItem(item) {
      if (!confirm(`确定删除 ${item.file_name}？`)) return;
      this.driveActionLoading = `delete:${item.fid}`;
      try {
        const data = await apiData('/api/drive/delete', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids: [item.fid]})
        });
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '删除失败');
          return;
        }
        this.showNotification('success', data.message || '已删除');
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => fid !== item.fid);
        await this.loadDrive(true);
      } catch (error) {
        console.error('删除失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '删除失败'));
      } finally {
        this.driveActionLoading = '';
      }
    },

    async renameDriveItem(item) {
      const newName = prompt('新名称:', item.file_name);
      if (!newName || newName === item.file_name) return;
      this.driveActionLoading = `rename:${item.fid}`;
      try {
        const data = await apiData('/api/drive/rename', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            fid: item.fid,
            name: newName,
            parent_fid: this.driveCurrentFid
          })
        });
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '重命名失败');
          return;
        }
        this.showNotification('success', data.message || '重命名成功');
        await this.loadDrive(true);
      } catch (error) {
        console.error('重命名失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '重命名失败'));
      } finally {
        this.driveActionLoading = '';
      }
    },

    driveItemDownloadTask(item) {
      if (!item || !item.file) return null;
      const name = String(item.file_name || '').trim().toLowerCase();
      if (!name) return null;
      return this.allDownloadTasks.find(task => {
        if (String(task.file_name || '').trim().toLowerCase() === name) return true;
        return (task.files || []).some(file => String(file.file_name || '').trim().toLowerCase() === name);
      }) || null;
    },

    driveItemAutomationLabel(item) {
      const task = this.driveItemDownloadTask(item);
      if (!task) return '';
      return `Aria2 ${this.downloadStatusLabel(task.status)}`;
    },

    driveItemAutomationClass(item) {
      const task = this.driveItemDownloadTask(item);
      return task ? this.downloadStatusBadgeClass(task.status) : 'badge badge-muted';
    },

    async sendDriveItemToAria2(item) {
      if (!item || !item.file) return;
      await this.submitDriveFidsToAria2([item.fid]);
    },

    async sendSelectedDriveItemsToAria2() {
      const selectedFiles = this.selectedDriveItems
        .filter(item => item.file)
        .map(item => item.fid);
      if (selectedFiles.length === 0) {
        this.showNotification('warning', '请选择要发送到 Aria2 的文件');
        return;
      }
      await this.submitDriveFidsToAria2(selectedFiles);
    },

    async submitDriveFidsToAria2(fids) {
      this.driveActionLoading = 'aria2';
      try {
        const data = await apiData('/api/drive/aria2', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids})
        });
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '提交 Aria2 失败');
          return;
        }
        this.showNotification('success', data.message || `已提交 ${fids.length} 个 Aria2 任务`);
        this.selectTab('downloads');
      } catch (error) {
        console.error('提交 Aria2 失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '提交 Aria2 失败'));
      } finally {
        this.driveActionLoading = '';
      }
    },

    // ===== 下载任务 =====
    sortDriveItems() {
      // 列表排序由 filteredDriveItems 统一派生。
    },

    filterDriveItems() {
      // 由 computed 属性处理
    },

    async batchDeleteDrive() {
      if (this.driveSelectedItems.length === 0) return;
      if (!confirm(`确定删除选中的 ${this.driveSelectedItems.length} 个项目？`)) return;

      this.driveActionLoading = 'batch-delete';
      try {
        const fids = [...this.driveSelectedItems];
        const data = await apiData('/api/drive/delete', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids})
        });
        if (data.success === false) {
          this.showNotification('error', data.message || data.error || '批量删除失败');
          return;
        }
        this.showNotification('success', data.message || `已删除 ${fids.length} 项`);
        this.driveSelectedItems = [];
        this.driveSelectMode = false;
        await this.loadDrive(true);
      } catch (error) {
        console.error('批量删除失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '批量删除失败'));
      } finally {
        this.driveActionLoading = '';
      }
    },

    formatSize(bytes) {
      return mediaFormatters.formatBytes(bytes);
    },

    // ===== 设置 =====
    };
  }

  return {driveTimestamp, isDriveVideo, filterAndSortDriveItems, createStore};
});
