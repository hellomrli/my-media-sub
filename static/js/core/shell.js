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

  function createStore() {
    return {
    theme: 'dark',

    async init() {
      this.setupLifecycleCleanup();
      this.initPwa();
      this.applyTheme(this.resolveInitialTheme(), {persist: false});
      this.calendarCursor = this.calendarTodayKey();
      this.loadSearchPreferences();
      this.initNavigation();
      await this.loadSubscriptions();
      await this.loadNotifications();
      await this.loadJobs();
      await this.loadAutomationSummary();
      await this.loadSettings();
      await this.loadSettingsSchema();
      this.setupJobEvents();
      this.setupGlobalShortcuts();
      this.loadSearchHistory();
      this.runCurrentTabEffects();
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
      });
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
