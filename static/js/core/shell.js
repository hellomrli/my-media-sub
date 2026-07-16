(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubShell = moduleApi;
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
  const ux = root.MediaSubUx || {};

  function createStore() {
    return {
    theme: 'dark',
    uiError: '',
    showDangerConfirmDialog: false,
    dangerConfirmTitle: '', dangerConfirmMessage: '', dangerConfirmPhrase: '', dangerConfirmInput: '',
    _dangerConfirmResolve: null,

    get uiBusy() {
      return !!(this.subscriptionDetailLoading || this.subscriptionBatchLoading || this.driveLoading || this.downloadsLoading || this.diagnosticsLoading || this.calendarLoading || this.searchLoading || this.checkingAllSubscriptions);
    },

    async init() {
      this.setupLifecycleCleanup();
      this.initPwa();
      this.applyTheme(this.resolveInitialTheme(), {persist: false});
      this.calendarCursor = this.calendarTodayKey();
      this.loadSearchPreferences();
      if (this.loadSubscriptionPreferences) this.loadSubscriptionPreferences();
      this.restoreUiPreferences();
      this.setupErrorBoundary();
      this.initNavigation();
      await Promise.all([
        this.loadSubscriptions(),
        this.loadNotifications(),
        this.loadJobs(),
        this.loadAutomationSummary(),
        this.loadSettings(),
        this.loadSettingsSchema()
      ]);
      this.setupJobEvents();
      this.setupGlobalShortcuts();
      this.loadSearchHistory();
      this.runCurrentTabEffects({initialDataLoaded: true});
      await this.handlePwaShortcut();
    },

    setupGlobalShortcuts() {
      this.listenLifecycle('global-shortcuts', window, 'keydown', event => {
        const target = event.target;
        const isTyping = target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
        if (event.key === '/' && !isTyping && !event.metaKey && !event.ctrlKey && !event.altKey) {
          event.preventDefault();
          this.selectTab('search');
          this.$nextTick(() => this.$refs.globalSearchInput && this.$refs.globalSearchInput.focus());
        }
        if (event.key === 'Escape' && this.showDangerConfirmDialog) this.resolveDangerConfirmation(false);
      });
    },

    restoreUiPreferences() {
      if (!ux.readPreference) return;
      this.backgroundJobFilterKind = ux.readPreference('jobs.kind', 'all');
      this.backgroundJobFilterStatus = ux.readPreference('jobs.status', 'all');
      this.driveFilterType = ux.readPreference('drive.filter', 'all', ['all','folder','video','other']);
      this.driveViewMode = ux.readPreference('drive.view', 'list', ['list','grid']);
      this.notificationFilter = ux.readPreference('notifications.filter', 'all', ['all','unread']);
      if (this.$watch) {
        for (const [property,key] of [['backgroundJobFilterKind','jobs.kind'],['backgroundJobFilterStatus','jobs.status'],['driveFilterType','drive.filter'],['driveViewMode','drive.view'],['notificationFilter','notifications.filter']]) {
          this.$watch(property, value => {
            ux.writePreference(key, value);
            if (property.startsWith('backgroundJob')) this.backgroundJobVisibleLimit = 80;
            if (property.startsWith('drive')) this.driveVisibleLimit = 200;
            if (property === 'notificationFilter') this.notificationVisibleLimit = 100;
          });
        }
      }
    },

    setupErrorBoundary() {
      this.listenLifecycle('ui-error', window, 'error', event => { this.uiError = event.message || '页面组件发生错误'; });
      this.listenLifecycle('ui-rejection', window, 'unhandledrejection', event => { this.uiError = this.apiErrorMessage(event.reason, '后台操作发生未处理错误'); });
    },

    requestDangerConfirmation({title='确认危险操作', message='', phrase=''}) {
      if (this._dangerConfirmResolve) this._dangerConfirmResolve(false);
      this.dangerConfirmTitle = title; this.dangerConfirmMessage = message; this.dangerConfirmPhrase = phrase; this.dangerConfirmInput = '';
      this.showDangerConfirmDialog = true;
      return new Promise(resolve => { this._dangerConfirmResolve = resolve; this.$nextTick(() => this.$refs.dangerConfirmInput && this.$refs.dangerConfirmInput.focus()); });
    },
    dangerConfirmationReady() { return !this.dangerConfirmPhrase || this.dangerConfirmInput === this.dangerConfirmPhrase; },
    resolveDangerConfirmation(approved) {
      if (approved && !this.dangerConfirmationReady()) return;
      this.showDangerConfirmDialog = false; const resolve = this._dangerConfirmResolve; this._dangerConfirmResolve = null;
      if (resolve) resolve(!!approved);
    },

    resolveInitialTheme() {
      const current = document.documentElement.getAttribute('data-theme');
      if (current === 'light' || current === 'dark') return current;
      try {
        const stored = localStorage.getItem('theme');
        if (stored === 'light' || stored === 'dark') return stored;
      } catch (_) {
        // Ignore storage access failures and fall back to system preference.
      }
      return window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
    },

    applyTheme(theme, options = {}) {
      const nextTheme = theme === 'light' ? 'light' : 'dark';
      this.theme = nextTheme;
      document.documentElement.setAttribute('data-theme', nextTheme);
      document.documentElement.classList.toggle('dark', nextTheme === 'dark');
      if (options.persist !== false) {
        try {
          localStorage.setItem('theme', nextTheme);
        } catch (_) {
          // Theme switching still works for this page load when storage is unavailable.
        }
      }
    },

    toggleTheme() {
      this.applyTheme(this.theme === 'dark' ? 'light' : 'dark');
    },

    themeToggleLabel() {
      return this.theme === 'dark' ? '切换到浅色主题' : '切换到深色主题';
    },

    trapDialogFocus(event) {
      const dialog = event.currentTarget;
      const focusable = Array.from(dialog.querySelectorAll([
        'a[href]',
        'button:not([disabled])',
        'input:not([disabled])',
        'select:not([disabled])',
        'textarea:not([disabled])',
        '[tabindex]:not([tabindex="-1"])'
      ].join(','))).filter(element => {
        const style = window.getComputedStyle(element);
        return style.display !== 'none' && style.visibility !== 'hidden';
      });
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    },

    async refresh() {
      if (this.currentTab === 'dashboard') {
        await Promise.all([
          this.loadSubscriptions(),
          this.loadNotifications(),
          this.loadJobs(),
          this.loadDownloads(true)
        ]);
      }
      else if (this.currentTab === 'calendar') await this.loadCalendar();
      else if (this.currentTab === 'subscriptions') {
        await this.loadSubscriptions();
        if (this.selectedSubscriptionId) await this.loadSubscriptionDetail(this.selectedSubscriptionId);
      }
      else if (this.currentTab === 'transferHistory') {
        await this.loadJobs();
      }
      else if (this.currentTab === 'notifications') await this.loadNotifications();
      else if (this.currentTab === 'diagnostics') await this.loadDiagnostics();
      else if (this.currentTab === 'settings') {
        if (this.currentSettingsTab === 'maintenance') {
          await Promise.all([this.loadSettings(), this.checkUpdate(true)]);
        } else {
          await this.loadSettings();
        }
      }
      else if (this.currentTab === 'drive') await this.loadDrive();
      else if (this.currentTab === 'downloads') await this.loadDownloads();
    },

    async copyText(value) {
      if (!value) return;
      try {
        await navigator.clipboard.writeText(value);
        this.showNotification('success', '已复制');
      } catch (error) {
        this.showNotification('error', '复制失败');
      }
    },

    apiErrorMessage(error, fallback = '请求失败') {
      return getApiErrorMessage(error, fallback);
    },

    };
  }

  return {createStore};
});
