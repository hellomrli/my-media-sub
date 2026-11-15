(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubShell = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {getApiErrorMessage} = api;
  const ux = root.MediaSubUx || {};

  function createStore() {
    return {
    theme: 'light',
    uiError: '',
    showDangerConfirmDialog: false,
    dangerConfirmTitle: '', dangerConfirmMessage: '', dangerConfirmPhrase: '', dangerConfirmInput: '',
    _dangerConfirmResolve: null,
    /// 最近一次请求是否到达了服务端；初始乐观为 true，避免首屏闪现故障。
    serverReachable: true,
    serverLastOkAt: 0,

    get uiBusy() {
      return !!(this.subscriptionDetailLoading || this.subscriptionBatchLoading || this.driveLoading || this.downloadsLoading || this.diagnosticsLoading || this.calendarLoading || this.searching || this.checkingAllSubscriptions);
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
      this.setupConnectivityTracking();
      this.initNavigation();
      await Promise.all([
        this.loadSubscriptions(),
        this.loadNotifications(),
        this.loadJobs(),
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
      this.backgroundJobFilterStatus = ux.readPreference('jobs.status', 'all');
      this.driveFilterType = ux.readPreference('drive.filter', 'all', ['all','folder','video','other']);
      this.driveViewMode = ux.readPreference('drive.view', 'list', ['list','grid']);
      this.notificationFilter = ux.readPreference('notifications.filter', 'all', ['all','unread']);
      this.activityFilter = ux.readPreference('activity.filter', 'all', ['all','unread','jobs','notifications','failed']);
      this.settingsOverviewOpen = ux.readPreference('settings.overview', 'closed', ['open','closed']) === 'open';
      if (this.$watch) {
        for (const [property,key] of [['backgroundJobFilterStatus','jobs.status'],['driveFilterType','drive.filter'],['driveViewMode','drive.view'],['notificationFilter','notifications.filter'],['activityFilter','activity.filter']]) {
          this.$watch(property, value => {
            ux.writePreference(key, value);
            if (property.startsWith('drive')) this.driveVisibleLimit = 200;
            if (property === 'activityFilter') this.activityVisibleLimit = 100;
          });
        }
      }
    },

    setupErrorBoundary() {
      this.listenLifecycle('ui-error', window, 'error', event => { this.uiError = event.message || '页面组件发生错误'; });
      this.listenLifecycle('ui-rejection', window, 'unhandledrejection', event => { this.uiError = this.apiErrorMessage(event.reason, '后台操作发生未处理错误'); });
    },

    setupConnectivityTracking() {
      if (typeof api.subscribeConnectivity !== 'function') return;
      this.ownLifecycle(
        'api-connectivity',
        api.subscribeConnectivity(reachable => this.markServerReachable(reachable)),
        unsubscribe => unsubscribe()
      );
    },

    markServerReachable(reachable) {
      this.serverReachable = !!reachable;
      if (reachable) this.serverLastOkAt = Date.now();
    },

    /// online / reconnecting / unreachable / offline。
    /// 事件流断开但普通请求仍成功时只是「重连中」：数据还能靠轮询更新。
    get connectionState() {
      if (this.pwaOffline) return 'offline';
      if (!this.serverReachable) return 'unreachable';
      return this.jobEventsReconnecting ? 'reconnecting' : 'online';
    },

    connectionLabel() {
      return {
        online: '服务在线',
        reconnecting: '正在重连',
        unreachable: '服务未响应',
        offline: '网络离线'
      }[this.connectionState];
    },

    connectionDetail() {
      const lastOk = this.serverLastOkAt ? `最近成功响应：${this.formatTime(this.serverLastOkAt)}` : '尚未收到服务响应';
      if (this.connectionState === 'online') return lastOk;
      if (this.connectionState === 'reconnecting') return `实时任务推送已断开，正在自动重连；数据改由定时刷新。${lastOk}`;
      if (this.connectionState === 'offline') return `设备处于离线状态。${lastOk}`;
      return `无法连接服务，当前显示的可能是旧数据。${lastOk}`;
    },

    /// 顶栏在断连时让位给连接状态，否则「系统空闲」会被误读成一切正常。
    headerHealthLabel() {
      if (this.connectionState !== 'online') return this.connectionLabel();
      const running = this.dashboardStats.runningJobs;
      return running ? `${running} 项运行中` : '系统空闲';
    },

    async retryConnection() {
      await this.refresh();
      if (!this.jobEvents && typeof this.setupJobEvents === 'function') this.setupJobEvents();
    },

    requestDangerConfirmation({title='确认危险操作', message='', phrase=''}) {
      if (this._dangerConfirmResolve) this._dangerConfirmResolve(false);
      this.dangerConfirmTitle = title; this.dangerConfirmMessage = message; this.dangerConfirmPhrase = phrase; this.dangerConfirmInput = '';
      this.showDangerConfirmDialog = true;
      return new Promise(resolve => { this._dangerConfirmResolve = resolve; });
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
        // Theme switching remains usable when preference storage is unavailable.
      }
      return 'light';
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

    focusDialog(dialog, initialFocus = null) {
      // Alpine may defer x-show to the next animation frame. Focusing during
      // $nextTick alone can still target a hidden dialog and silently do nothing.
      this.$nextTick(() => window.requestAnimationFrame(() => {
        if (!dialog.isConnected || !dialog.getClientRects().length) return;
        const target = initialFocus && initialFocus.getClientRects().length ? initialFocus : dialog;
        target.focus({preventScroll: true});
      }));
    },

    trapDialogFocus(event) {
      const dialog = event.currentTarget;
      const focusable = Array.from(dialog.querySelectorAll([
        'a[href]',
        'button:not(:disabled)',
        'input:not(:disabled):not([type="hidden"])',
        'select:not(:disabled)',
        'textarea:not(:disabled)',
        '[tabindex]:not([tabindex="-1"])'
      ].join(','))).filter(element => {
        const style = window.getComputedStyle(element);
        return element.tabIndex >= 0 && !element.matches(':disabled')
          && !element.closest('[inert]') && element.getClientRects().length > 0
          && style.visibility !== 'hidden' && style.visibility !== 'collapse';
      });
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      // Dialogs receive focus on opening. Include that initial state, and skip
      // controls inside hidden wizard steps, when wrapping keyboard navigation.
      const outsideControls = !focusable.includes(document.activeElement);
      if (event.shiftKey && (document.activeElement === first || outsideControls)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || outsideControls)) {
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
          this.loadDownloads(true),
          this.loadCalendar()
        ]);
      }
      else if (this.currentTab === 'subscriptions') {
        await this.loadSubscriptions();
        if (this.selectedSubscriptionId) await this.loadSubscriptionDetail(this.selectedSubscriptionId);
      }
      else if (this.currentTab === 'notifications') await this.loadActivity();
      else if (this.currentTab === 'diagnostics') await this.loadDiagnostics();
      else if (this.currentTab === 'settings') {
        if (this.currentSettingsTab === 'update') {
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

    imageRetrySource(value) {
      const source = String(value || '').trim();
      if (!source) return '';
      try {
        const url = new URL(source, window.location.href);
        url.searchParams.delete('_media_sub_retry');
        return url.href;
      } catch (_) {
        return source;
      }
    },

    remoteImageUrl(value) {
      const source = String(value || '').trim();
      if (!source) return '';
      try {
        const base = typeof window !== 'undefined' ? window.location.href : 'http://localhost/';
        const origin = new URL(base).origin;
        const url = new URL(source, base);
        if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password) return '';
        // Keep CSP strict and avoid an arbitrary-URL proxy: remote HTTP artwork must use TLS.
        if (url.protocol === 'http:' && url.origin !== origin) url.protocol = 'https:';
        if (url.protocol === 'https:' && url.hostname.toLowerCase() === 'image.tmdb.org') {
          const match = url.pathname.match(/^\/t\/p\/(w(?:92|154|185|300|342|400|500|780|1280)|original)\/([A-Za-z0-9_-]+\.(?:jpe?g|png|webp|avif))$/i);
          if (match) {
            return `/api/images/tmdb/${encodeURIComponent(match[1].toLowerCase())}/${encodeURIComponent(match[2])}`;
          }
        }
        return url.origin === origin && !/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(source) ? source : url.href;
      } catch (_) {
        return '';
      }
    },

    handleRemoteImageLoad(event) {
      const element = event && event.currentTarget;
      if (!element) return;
      if (element._mediaSubRetryTimer) {
        clearTimeout(element._mediaSubRetryTimer);
        element._mediaSubRetryTimer = null;
      }
      element.dataset.imageRetryCount = '0';
      element.classList.remove('remote-image-retrying', 'remote-image-failed');
      element.hidden = false;
    },

    handleRemoteImageError(event, onFailure) {
      const element = event && event.currentTarget;
      if (!element || element._mediaSubRetryTimer) return;
      const rawSource = typeof element.getAttribute === 'function' ? element.getAttribute('src') : element.src;
      const currentSource = this.imageRetrySource(rawSource);
      if (!currentSource) return;
      if (element.currentSrc && this.imageRetrySource(element.currentSrc) !== currentSource) return;
      const previousSource = element.dataset.imageRetrySource || '';
      let retryCount = Number(element.dataset.imageRetryCount || 0);
      if (previousSource !== currentSource) retryCount = 0;
      element.dataset.imageRetrySource = currentSource;
      element.hidden = false;

      if (!/^https?:/i.test(currentSource) || retryCount >= 2) {
        element.classList.remove('remote-image-retrying');
        element.classList.add('remote-image-failed');
        if (typeof onFailure === 'function') onFailure(currentSource);
        return;
      }

      retryCount += 1;
      element.dataset.imageRetryCount = String(retryCount);
      element.classList.add('remote-image-retrying');
      element.classList.remove('remote-image-failed');
      const delay = retryCount === 1 ? 350 : 1200;
      element._mediaSubRetryTimer = setTimeout(() => {
        element._mediaSubRetryTimer = null;
        if (!element.isConnected) return;
        const latestSource = typeof element.getAttribute === 'function' ? element.getAttribute('src') : element.src;
        if (this.imageRetrySource(latestSource) !== currentSource) return;
        const retryUrl = new URL(currentSource);
        retryUrl.searchParams.set('_media_sub_retry', `${retryCount}-${Date.now()}`);
        element.src = retryUrl.href;
      }, delay);
    },

    recoverRemoteImages(rootNode) {
      const scope = rootNode || (typeof document !== 'undefined' ? document : null);
      if (!scope || typeof scope.querySelectorAll !== 'function') return 0;
      let recovered = 0;

      [...scope.querySelectorAll('img[src]')].forEach((element, index) => {
        const rawSource = typeof element.getAttribute === 'function' ? element.getAttribute('src') : element.src;
        const currentSource = this.imageRetrySource(rawSource);
        if (!/^https?:/i.test(currentSource)) return;
        const retryCount = Number(element.dataset.imageRetryCount || 0);
        const failed = element.classList.contains('remote-image-failed');
        const retrying = element.classList.contains('remote-image-retrying');
        const broken = element.complete && Number(element.naturalWidth || 0) === 0;
        if (!failed && !retrying && !element.hidden && retryCount <= 0 && !broken) return;

        if (element._mediaSubRetryTimer) {
          clearTimeout(element._mediaSubRetryTimer);
          element._mediaSubRetryTimer = null;
        }
        element.dataset.imageRetrySource = currentSource;
        element.dataset.imageRetryCount = '0';
        element.classList.remove('remote-image-retrying', 'remote-image-failed');
        element.hidden = false;

        const retryUrl = new URL(currentSource);
        retryUrl.searchParams.set('_media_sub_retry', `refresh-${Date.now()}-${index}`);
        element.src = retryUrl.href;
        recovered += 1;
      });

      return recovered;
    },

    recoverRemoteImagesAfterDataRefresh() {
      const recover = () => this.recoverRemoteImages();
      if (typeof this.$nextTick === 'function') {
        this.$nextTick(recover);
      } else {
        recover();
      }
    },

    apiErrorMessage(error, fallback = '请求失败') {
      return getApiErrorMessage(error, fallback);
    },

    };
  }

  return {createStore};
});
