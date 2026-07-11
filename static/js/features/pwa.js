(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubPwa = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const SHORTCUTS = Object.freeze([
    {id: 'calendar-today', label: '今日更新', tab: 'calendar'},
    {id: 'calendar-missing', label: '缺集', tab: 'calendar'},
    {id: 'failed-jobs', label: '失败任务', tab: 'transferHistory'},
    {id: 'check-all', label: '检查全部', tab: 'subscriptions'},
    {id: 'downloads', label: '下载进度', tab: 'downloads'},
    {id: 'quark-signin', label: '夸克签到', tab: 'dashboard'}
  ]);

  function shortcutFromSearch(search) {
    const action = new URLSearchParams(search || '').get('pwa') || '';
    return SHORTCUTS.some(item => item.id === action) ? action : '';
  }

  function createStore() {
    return {
      pwaInstallPrompt: null,
      pwaInstallAvailable: false,
      pwaInstalled: false,
      pwaUpdateReady: false,
      pwaOffline: typeof navigator !== 'undefined' ? !navigator.onLine : false,
      pwaRegistration: null,
      pwaApplyingUpdate: false,
      browserPushEnabled: false,
      browserPushSupported: typeof PushManager !== 'undefined' && typeof Notification !== 'undefined',
      pwaShortcuts: SHORTCUTS,

      initPwa() {
        if (typeof window === 'undefined') return;
        this.listenLifecycle('pwa-beforeinstall', window, 'beforeinstallprompt', event => {
          event.preventDefault();
          this.pwaInstallPrompt = event;
          this.pwaInstallAvailable = true;
        });
        this.listenLifecycle('pwa-installed', window, 'appinstalled', () => {
          this.pwaInstallPrompt = null;
          this.pwaInstallAvailable = false;
          this.pwaInstalled = true;
          this.showNotification('success', 'MEDIA/SUB 已安装');
        });
        this.listenLifecycle('pwa-online', window, 'online', () => {
          this.pwaOffline = false;
          this.showNotification('success', '网络已恢复');
        });
        this.listenLifecycle('pwa-offline', window, 'offline', () => {
          this.pwaOffline = true;
          this.showNotification('warning', '当前离线，仅读取已缓存的应用壳层');
        });

        if (!('serviceWorker' in navigator)) return;
        this.listenLifecycle('pwa-controller-change', navigator.serviceWorker, 'controllerchange', () => {
          if (this.pwaApplyingUpdate) window.location.reload();
        });
        this.listenLifecycle('pwa-worker-message', navigator.serviceWorker, 'message', event => {
          if (event.data && event.data.type === 'PWA_ACTIVATED') {
            this.pwaUpdateReady = false;
          }
        });
        navigator.serviceWorker.register('/service-worker.js', {scope: '/'}).then(registration => {
          this.pwaRegistration = registration;
          this.refreshBrowserPushStatus();
          if (registration.waiting && navigator.serviceWorker.controller) this.pwaUpdateReady = true;
          registration.addEventListener('updatefound', () => {
            const worker = registration.installing;
            if (!worker) return;
            worker.addEventListener('statechange', () => {
              if (worker.state === 'installed' && navigator.serviceWorker.controller) {
                this.pwaUpdateReady = true;
              }
            });
          });
          const worker = registration.active || registration.waiting || registration.installing;
          if (worker) worker.postMessage({type: 'WARM_CACHE'});
          registration.update().catch(() => {});
        }).catch(error => {
          console.warn('Service Worker 注册失败:', error);
        });
      },

      browserPushKeyBytes(value) {
        const padding = '='.repeat((4 - value.length % 4) % 4);
        const raw = atob((value + padding).replace(/-/g, '+').replace(/_/g, '/'));
        return Uint8Array.from([...raw].map(character => character.charCodeAt(0)));
      },

      async refreshBrowserPushStatus() {
        if (!this.browserPushSupported || !this.pwaRegistration) return;
        const subscription = await this.pwaRegistration.pushManager.getSubscription();
        this.browserPushEnabled = !!subscription;
      },

      async toggleBrowserPush() {
        if (!this.browserPushSupported || !this.pwaRegistration) { this.showNotification('warning', '当前浏览器不支持 Push'); return; }
        try {
          const existing = await this.pwaRegistration.pushManager.getSubscription();
          if (existing) {
            await apiData('/api/push/browser', {method: 'DELETE', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({endpoint: existing.endpoint})});
            await existing.unsubscribe();
            this.browserPushEnabled = false;
            this.showNotification('success', '浏览器 Push 已关闭');
            return;
          }
          const permission = await Notification.requestPermission();
          if (permission !== 'granted') { this.showNotification('warning', '未授予通知权限'); return; }
          const status = await apiData('/api/push/browser');
          const subscription = await this.pwaRegistration.pushManager.subscribe({userVisibleOnly: true, applicationServerKey: this.browserPushKeyBytes(status.public_key)});
          const json = subscription.toJSON();
          await apiData('/api/push/browser', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({endpoint: subscription.endpoint, p256dh: json.keys.p256dh, auth: json.keys.auth, user_agent: navigator.userAgent})});
          this.browserPushEnabled = true;
          this.showNotification('success', '浏览器 Push 已启用');
        } catch (error) { this.showNotification('error', this.apiErrorMessage(error, '浏览器 Push 操作失败')); }
      },

      async installPwa() {
        if (!this.pwaInstallPrompt) return;
        const prompt = this.pwaInstallPrompt;
        this.pwaInstallPrompt = null;
        this.pwaInstallAvailable = false;
        await prompt.prompt();
        await prompt.userChoice.catch(() => null);
      },

      applyPwaUpdate() {
        const worker = this.pwaRegistration && this.pwaRegistration.waiting;
        if (!worker) {
          window.location.reload();
          return;
        }
        this.pwaApplyingUpdate = true;
        worker.postMessage({type: 'SKIP_WAITING'});
      },

      dismissPwaUpdate() {
        this.pwaUpdateReady = false;
      },

      async runPwaShortcut(action, options = {}) {
        if (!SHORTCUTS.some(item => item.id === action)) return false;
        if (action === 'calendar-today' || action === 'calendar-missing') {
          this.calendarView = 'list';
          this.calendarCursor = this.calendarTodayKey();
          this.calendarStatusFilter = action === 'calendar-today' ? 'today' : 'completed_missing';
          this.selectTab('calendar', options.pushHistory !== false);
          await this.loadCalendar();
        } else if (action === 'failed-jobs') {
          this.backgroundJobFilterStatus = 'failed';
          this.selectTab('transferHistory', options.pushHistory !== false);
          await this.loadJobs();
        } else if (action === 'check-all') {
          this.selectTab('subscriptions', options.pushHistory !== false);
          await this.checkAllSubscriptions();
        } else if (action === 'downloads') {
          this.selectTab('downloads', options.pushHistory !== false);
          await this.loadDownloads();
        } else if (action === 'quark-signin') {
          this.selectTab('dashboard', options.pushHistory !== false);
          await this.runQuarkSignin();
        }
        return true;
      },

      async handlePwaShortcut() {
        if (typeof window === 'undefined') return;
        const action = shortcutFromSearch(window.location.search);
        if (!action) return;
        const url = new URL(window.location.href);
        url.searchParams.delete('pwa');
        url.searchParams.delete('source');
        history.replaceState(this.routeState(), '', `${url.pathname}${url.search}${url.hash}`);
        await this.runPwaShortcut(action, {pushHistory: false});
      }
    };
  }

  return {SHORTCUTS, shortcutFromSearch, createStore};
});
