(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubRouter = moduleApi;
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

  const SETTINGS_TAB_ALIASES = Object.freeze({
    basic: 'connections',
    quark: 'connections',
    advanced: 'maintenance',
    update: 'maintenance',
    push: 'notifications',
    rules: 'naming'
  });

  function normalizeSettingsTab(tabId) {
    return SETTINGS_TAB_ALIASES[tabId] || tabId;
  }

  function normalizeRoute(route = {}, validTabs = [], validSettingsTabs = []) {
    const tab = validTabs.includes(route.tab) ? route.tab : 'dashboard';
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
      {id: 'calendar', name: '更新日历', description: '查看播出排期与缺集状态', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 2v4m8-4v4M3 10h18M5 4h14a2 2 0 012 2v14H3V6a2 2 0 012-2zm3 10h3m2 0h3m-8 3h3"/></svg>'},
      {id: 'search', name: '资源搜索', description: '搜索影视资源并添加订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>'},
      {id: 'drive', name: '我的网盘', description: '管理夸克网盘文件', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>'},
      {id: 'downloads', name: '下载任务', description: '查看 Aria2 实时进度', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1M8 12l4 4m0 0l4-4m-4 4V4"/></svg>'},
      {id: 'subscriptions', name: '订阅管理', description: '管理媒体订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"/></svg>'},
      {id: 'transferHistory', name: '后台日志', description: '查看后台任务和执行记录', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v6h6M20 20v-6h-6M5 19A9 9 0 0019 5M19 5h-5M5 19h5"/></svg>'},
      {id: 'notifications', name: '通知中心', description: '查看用户通知和推送记录', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg>'},
      {id: 'diagnostics', name: '系统诊断', description: '备份、指标与安全状态', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 19h16M5 16l4-5 4 3 6-8M5 5v11"/></svg>'},
      {id: 'settings', name: '系统设置', description: '配置系统参数', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>'}
    ],

    settingsTabs: [
      {id: 'connections', name: '连接', icon: '⌁'},
      {id: 'automation', name: '自动化', icon: '⏱'},
      {id: 'naming', name: '命名规则', icon: '✦'},
      {id: 'notifications', name: '通知', icon: '↗'},
      {id: 'maintenance', name: '维护', icon: '⌘'}
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
      return this.tabs.some(tab => tab.id === tabId);
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

    runCurrentTabEffects() {
      if (this.currentTab !== 'search') this.stopSearchProgressTimer();
      if (this.currentTab !== 'settings' || this.currentSettingsTab !== 'maintenance') {
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
        this.loadNotifications();
        this.loadAutomationSummary();
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

      if (this.currentTab === 'calendar' && !this.calendarLoading) {
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

  return {SETTINGS_TAB_ALIASES, normalizeSettingsTab, normalizeRoute, routeFromSearch, routeUrl, createStore};
});
