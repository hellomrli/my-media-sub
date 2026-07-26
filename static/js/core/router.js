(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubRouter = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  // 老书签与 PWA 快捷方式要继续可用：v2.2.18 把「连接」拆成夸克/下载与元数据，
  // 把「维护」拆成高级与安全/版本更新，这里保留旧 id 的映射。
  const SETTINGS_TAB_ALIASES = Object.freeze({
    basic: 'library',
    connections: 'library',
    maintenance: 'advanced',
    push: 'notifications',
    rules: 'naming'
  });

  const TAB_ALIASES = Object.freeze({
    // v2.2 and earlier exposed background jobs as a separate page. Keep old
    // bookmarks and PWA shortcuts working while rendering the unified center.
    transferHistory: 'notifications',
    // v2.2.19 把更新日历并入工作台，日历不再是独立页面。
    calendar: 'dashboard'
  });

  function normalizeTab(tabId) {
    return TAB_ALIASES[tabId] || tabId;
  }

  function normalizeSettingsTab(tabId) {
    return SETTINGS_TAB_ALIASES[tabId] || tabId;
  }

  function normalizeRoute(route = {}, validTabs = [], validSettingsTabs = []) {
    const legacyTab = route.tab;
    const requestedTab = normalizeTab(legacyTab);
    // During a rolling upgrade a caller may still expose the old tab list. If
    // the canonical tab is not available there, keep the legacy value rather
    // than silently sending an old bookmark to the dashboard.
    const tab = validTabs.includes(requestedTab)
      ? requestedTab
      : (validTabs.includes(legacyTab) ? legacyTab : 'dashboard');
    const requestedSettingsTab = normalizeSettingsTab(route.settingsTab || route.settings);
    const settingsTab = validSettingsTabs.includes(requestedSettingsTab) ? requestedSettingsTab : 'connections';
    return {
      appRoute: true,
      tab,
      settingsTab,
      subscriptionId: tab === 'subscriptions' ? String(route.subscriptionId || route.subscription || '') : ''
    };
  }

  function routeFromSearch(search, validTabs = [], validSettingsTabs = []) {
    const params = new URLSearchParams(search || '');
    return normalizeRoute({
      tab: params.get('tab'),
      settingsTab: params.get('settings'),
      subscriptionId: params.get('subscription')
    }, validTabs, validSettingsTabs);
  }

  function routeUrl(href, route, validTabs = [], validSettingsTabs = []) {
    const normalized = normalizeRoute(route, validTabs, validSettingsTabs);
    const url = new URL(href, 'http://localhost/');
    url.searchParams.set('tab', normalized.tab);
    if (normalized.tab === 'settings') url.searchParams.set('settings', normalized.settingsTab);
    else url.searchParams.delete('settings');
    if (normalized.tab === 'subscriptions' && normalized.subscriptionId) {
      url.searchParams.set('subscription', normalized.subscriptionId);
    } else {
      url.searchParams.delete('subscription');
    }
    return `${url.pathname}${url.search}${url.hash}`;
  }

  function createStore() {
    return {
    currentTab: 'dashboard',
    currentSettingsTab: 'connections',
    tabs: [
      {id: 'dashboard', name: '工作台', description: '', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 13h8V3H3v10zm10 8h8V11h-8v10zM3 21h8v-6H3v6zm10-12h8V3h-8v6z"/></svg>'},
      {id: 'search', name: '资源搜索', description: '搜索影视资源并添加订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>'},
      {id: 'drive', name: '我的网盘', description: '管理夸克网盘文件', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>'},
      {id: 'downloads', name: '下载任务', description: '查看 Aria2 实时进度', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1M8 12l4 4m0 0l4-4m-4 4V4"/></svg>'},
      {id: 'subscriptions', name: '订阅管理', description: '管理媒体订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"/></svg>'},
      {id: 'notifications', name: '活动中心', description: '统一查看后台任务、通知和失败重试', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v6h6M20 20v-6h-6M5 19A9 9 0 0019 5M15 17h5l-1.4-1.4A2 2 0 0118 14v-3"/></svg>'},
      {id: 'diagnostics', name: '系统诊断', description: '备份、指标与安全状态', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 19h16M5 16l4-5 4 3 6-8M5 5v11"/></svg>'},
      {id: 'settings', name: '系统设置', description: '配置系统参数', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>'}
    ],

    // 按「接入 → 运行 → 维护」的实际使用顺序排列，每个标签页只放一类事，
    // 避免此前「连接」一页塞进夸克、Aria2、TMDB、账号和目录五件事。
    settingsTabs: [
      {id: 'quark', name: '夸克网盘', icon: '☁'},
      {id: 'library', name: '下载与元数据', icon: '⌁'},
      {id: 'automation', name: '自动化', icon: '⏱'},
      {id: 'naming', name: '命名规则', icon: '✦'},
      {id: 'notifications', name: '通知', icon: '↗'},
      {id: 'advanced', name: '高级与安全', icon: '⌘'},
      {id: 'update', name: '版本更新', icon: '↑'}
    ],

    initNavigation() {
      this.applyRouteFromUrl({runEffects: false});
      this.replaceRouteState();
      this.listenLifecycle('router-popstate', window, 'popstate', event => {
        if (event.state && event.state.appRoute) {
          this.applyRouteState(event.state, {runEffects: true});
        } else {
          this.applyRouteFromUrl({runEffects: true});
        }
      });
    },

    isValidTab(tabId) {
      const normalized = normalizeTab(tabId);
      return this.tabs.some(tab => tab.id === normalized || tab.id === tabId);
    },

    normalizeSettingsTab(tabId) {
      return normalizeSettingsTab(tabId);
    },

    isValidSettingsTab(tabId) {
      const normalized = this.normalizeSettingsTab(tabId);
      return this.settingsTabs.some(tab => tab.id === normalized);
    },

    routeUrl(tabId = this.currentTab, settingsTab = this.currentSettingsTab, subscriptionId = this.selectedSubscriptionId) {
      return routeUrl(window.location.href, {tab: tabId, settingsTab, subscriptionId},
        this.tabs.map(tab => tab.id), this.settingsTabs.map(tab => tab.id));
    },

    routeState(tabId = this.currentTab, settingsTab = this.currentSettingsTab, subscriptionId = this.selectedSubscriptionId) {
      return normalizeRoute({tab: tabId, settingsTab, subscriptionId},
        this.tabs.map(tab => tab.id), this.settingsTabs.map(tab => tab.id));
    },

    pushRouteState() {
      history.pushState(this.routeState(), '', this.routeUrl());
    },

    replaceRouteState() {
      history.replaceState(this.routeState(), '', this.routeUrl());
    },

    applyRouteFromUrl(options = {}) {
      const route = routeFromSearch(window.location.search,
        this.tabs.map(tab => tab.id), this.settingsTabs.map(tab => tab.id));
      this.applyRouteState(route, options);
    },

    applyRouteState(state, options = {}) {
      const previousSubscriptionId = this.selectedSubscriptionId;
      const route = normalizeRoute(state, this.tabs.map(tab => tab.id), this.settingsTabs.map(tab => tab.id));
      this.currentTab = route.tab;
      this.currentSettingsTab = route.settingsTab;
      this.selectedSubscriptionId = route.subscriptionId;
      if (previousSubscriptionId !== this.selectedSubscriptionId) {
        this.subscriptionDetail = null;
        this.subscriptionDetailError = '';
        this.subscriptionEpisodeFilter = 'all';
      }
      if (options.runEffects !== false) {
        this.runCurrentTabEffects();
      }
    },

    runCurrentTabEffects(options = {}) {
      if (this.currentTab !== 'search') this.stopSearchProgressTimer();
      if (this.currentTab !== 'settings' || this.currentSettingsTab !== 'update') {
        this.stopUpdateProgressPolling();
      }

      if (this.currentTab === 'downloads' || this.currentTab === 'dashboard') {
        if (this.aria2Configured()) {
          this.loadDownloads(this.currentTab === 'dashboard');
          this.startDownloadsPolling();
        } else {
          this.stopDownloadsPolling();
          this.downloads = {active: [], waiting: [], stopped: []};
          this.downloadsError = '';
          this.downloadsUpdatedAt = 0;
        }
      } else {
        this.stopDownloadsPolling();
      }

      if (this.currentTab === 'dashboard') {
        if (!options.initialDataLoaded) {
          this.loadNotifications();
        }
        this.startNotificationsPolling();
      } else if (this.currentTab === 'notifications') {
        if (!options.initialDataLoaded) this.loadActivity();
        this.startNotificationsPolling();
      } else {
        this.stopNotificationsPolling();
      }

      if (this.currentTab === 'drive') {
        if (!this.driveLastLoadedAt && !this.driveLoading && !this.driveRefreshing) {
          this.loadDrive();
        }
        if (this.aria2Configured()) this.loadDownloads(true);
      }

      // 更新日历已并入工作台：进入工作台就刷新排期，避免看到上次的旧数据。
      if (this.currentTab === 'dashboard' && !this.calendarLoading) {
        this.loadCalendar();
      }

      if (this.currentTab === 'subscriptions' && this.selectedSubscriptionId) {
        if (!this.subscriptionDetailLoading
          && (!this.subscriptionDetail || this.subscriptionDetail.subscription.id !== this.selectedSubscriptionId)) {
          this.loadSubscriptionDetail(this.selectedSubscriptionId);
        }
      }

      if (this.currentTab === 'settings' && this.currentSettingsTab === 'maintenance') {
        if (!this.updateInfo && !this.updateLoading) this.checkUpdate(true);
        if (!this.updateReleases.length && !this.updateReleasesLoading) this.loadUpdateReleases(true);
        this.loadUpdateProgress().then(progress => {
          if (progress && progress.running && !this.updateProgressTimer) {
            this.startUpdateProgressPolling();
          }
          if (progress && progress.stage === 'restart_required') {
            this.showUpdateProgressDialog = true;
          }
        });
      }
    },

    selectTab(tabId, pushHistory = true) {
      const normalized = normalizeTab(tabId);
      if (this.tabs.some(tab => tab.id === normalized)) tabId = normalized;
      if (!this.isValidTab(tabId)) return;
      if (tabId === 'subscriptions' && this.currentTab === 'subscriptions' && this.selectedSubscriptionId) {
        this.closeSubscriptionDetail(pushHistory);
        return;
      }
      const changed = this.currentTab !== tabId;
      this.currentTab = tabId;
      if (tabId !== 'subscriptions') {
        this.selectedSubscriptionId = '';
        this.subscriptionDetail = null;
      }
      this.runCurrentTabEffects();
      if (pushHistory && changed) {
        this.pushRouteState();
      }
    },

    selectSettingsTab(tabId, pushHistory = true) {
      tabId = this.normalizeSettingsTab(tabId);
      if (!this.isValidSettingsTab(tabId)) return;
      const changed = this.currentSettingsTab !== tabId;
      this.currentSettingsTab = tabId;
      this.runCurrentTabEffects();
      if (pushHistory && changed) {
        this.pushRouteState();
      }
    },

    openRuleCenter() {
      this.selectTab('settings', false);
      this.selectSettingsTab('naming');
    },

    };
  }

  return {SETTINGS_TAB_ALIASES, TAB_ALIASES, normalizeSettingsTab, normalizeTab, normalizeRoute, routeFromSearch, routeUrl, createStore};
});
