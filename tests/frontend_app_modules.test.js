const test = require('node:test');
const assert = require('node:assert/strict');

for (const file of [
  '../static/js/core/api.js',
  '../static/js/core/formatters.js',
  '../static/js/features/search-results.js',
  '../static/js/features/subscription-detail.js',
  '../static/js/features/calendar.js',
  '../static/js/features/source-switch.js',
  '../static/js/features/automation-events.js',
  '../static/js/core/polling.js',
  '../static/js/core/router.js',
  '../static/js/core/notifications.js',
  '../static/js/stores/downloads.js',
  '../static/js/stores/drive.js',
  '../static/js/stores/jobs.js',
  '../static/js/stores/subscriptions.js',
  '../static/js/features/updates.js',
  '../static/js/features/settings.js',
  '../static/js/features/diagnostics.js',
  '../static/js/features/pwa.js',
  '../static/js/features/dashboard.js',
  '../static/js/features/search-page.js',
  '../static/js/features/calendar-page.js',
  '../static/js/core/shell.js'
]) require(file);

const {app} = require('../static/app.js');

test('app assembly preserves store getters and exposes every P4 domain module', () => {
  const store = app();
  assert.equal(store.currentTab, 'dashboard');
  assert.equal(typeof store.initNavigation, 'function');
  assert.equal(typeof store.showNotification, 'function');
  assert.equal(typeof store.loadDownloads, 'function');
  assert.equal(typeof store.loadDrive, 'function');
  assert.equal(typeof store.loadSubscriptions, 'function');
  assert.equal(typeof store.loadSettings, 'function');
  assert.equal(typeof store.checkUpdate, 'function');
  assert.equal(typeof store.loadDiagnostics, 'function');
  assert.equal(typeof store.initPwa, 'function');
  assert.equal(typeof store.destroy, 'function');
  assert.equal(typeof Object.getOwnPropertyDescriptor(store, 'filteredDriveItems').get, 'function');
  assert.deepEqual(store.filteredDriveItems, []);
});

test('page navigation stops page-bound pollers before applying the next page effects', () => {
  const store = app();
  const stopped = [];
  store.aria2Configured = () => false;
  store.stopDownloadsPolling = () => stopped.push('downloads');
  store.stopNotificationsPolling = () => stopped.push('notifications');
  // 真实实现会起一个 30s 定时器，测试进程会因此永不退出；这里只记录调用。
  store.startNotificationsPolling = () => stopped.push('notifications-start');
  store.stopSearchProgressTimer = () => stopped.push('search');
  store.stopUpdateProgressPolling = () => stopped.push('update');
  store.loadCalendar = () => stopped.push('calendar-load');
  store.loadDrive = () => {};
  store.loadDownloads = () => {};
  store.loadNotifications = () => {};
  store.checkUpdate = () => {};
  store.loadUpdateReleases = () => {};
  store.loadUpdateProgress = async () => null;

  // v2.2.19：更新日历并入工作台，calendar 现在会被路由到 dashboard；
  // 工作台会加载日历数据并重启通知轮询（startNotificationsPolling 先停后起）。
  store.currentTab = 'search';
  store.selectTab('calendar', false);
  assert.equal(store.currentTab, 'dashboard');
  assert.deepEqual(stopped, ['search', 'update', 'downloads', 'notifications-start', 'calendar-load']);

  stopped.length = 0;
  store.currentTab = 'settings';
  store.currentSettingsTab = 'maintenance';
  store.selectSettingsTab('connections', false);
  assert.deepEqual(stopped, ['search', 'update', 'downloads', 'notifications']);
  store.destroy();
});

test('dashboard refresh reloads the visible calendar and clears its previous error', async () => {
  const store = app();
  store.calendarError = '上次请求失败';
  store.calendarCursor = '2026-11-12';
  for (const method of ['loadSubscriptions', 'loadNotifications', 'loadJobs', 'loadDownloads']) {
    store[method] = async () => {};
  }
  const calendar = {items: [{id: 'new-episode'}], summary: {total: 1, by_status: {today: 1}}};
  const originalFetch = global.fetch;
  global.fetch = async url => {
    assert.match(url, /^\/api\/calendar\?/);
    return {ok: true, status: 200, json: async () => ({data: calendar})};
  };
  try {
    await store.refresh();
    assert.deepEqual(store.calendarItems, calendar.items);
    assert.equal(store.calendarSummary.total, 1);
    assert.equal(store.calendarError, '');
    assert.equal(store.calendarLoading, false);
  } finally {
    global.fetch = originalFetch;
  }
});

test('remote image failures retry with a cache-busting URL before falling back', () => {
  const store = app();
  const originalWindow = global.window;
  const originalSetTimeout = global.setTimeout;
  global.window = {location: {href: 'https://media.example.com/'}};
  global.setTimeout = callback => { callback(); return 1; };
  const classes = new Set();
  const element = {
    currentSrc: 'https://image.tmdb.org/t/p/w500/poster.jpg',
    src: 'https://image.tmdb.org/t/p/w500/poster.jpg',
    dataset: {},
    hidden: true,
    isConnected: true,
    classList: {
      add(...values) { values.forEach(value => classes.add(value)); },
      remove(...values) { values.forEach(value => classes.delete(value)); }
    }
  };

  try {
    store.handleRemoteImageError({currentTarget: element});
    assert.match(element.src, /_media_sub_retry=1-/);
    assert.equal(element.hidden, false);
    assert.equal(classes.has('remote-image-retrying'), true);
    store.handleRemoteImageLoad({currentTarget: element});
    assert.equal(classes.has('remote-image-retrying'), false);
    assert.equal(classes.has('remote-image-failed'), false);
  } finally {
    global.window = originalWindow;
    global.setTimeout = originalSetTimeout;
  }
});

test('TMDB artwork uses the authenticated same-origin image proxy', () => {
  const store = app();
  const originalWindow = global.window;
  global.window = {location: {href: 'https://media.example.com/'}};

  try {
    assert.equal(
      store.remoteImageUrl('https://image.tmdb.org/t/p/w500/Poster_123-abc.jpg'),
      '/api/images/tmdb/w500/Poster_123-abc.jpg'
    );
    assert.equal(
      store.subscriptionPoster({metadata: {poster_url: 'https://image.tmdb.org/t/p/w780/show.webp'}}),
      '/api/images/tmdb/w780/show.webp'
    );
    assert.equal(store.remoteImageUrl('https://cdn.example.com/poster.jpg'), 'https://cdn.example.com/poster.jpg');
  } finally {
    global.window = originalWindow;
  }
});

test('subscription data refresh recovers failed image nodes reused with the same URL', () => {
  const store = app();
  const originalWindow = global.window;
  global.window = {location: {href: 'https://media.example.com/'}};
  const failedClasses = new Set(['remote-image-failed']);
  const healthyClasses = new Set();
  const failed = {
    currentSrc: 'https://image.tmdb.org/t/p/w500/poster.jpg',
    src: 'https://image.tmdb.org/t/p/w500/poster.jpg',
    dataset: {imageRetryCount: '2', imageRetrySource: 'https://image.tmdb.org/t/p/w500/poster.jpg'},
    hidden: true,
    complete: true,
    naturalWidth: 0,
    classList: {
      add(...values) { values.forEach(value => failedClasses.add(value)); },
      remove(...values) { values.forEach(value => failedClasses.delete(value)); },
      contains(value) { return failedClasses.has(value); }
    }
  };
  const healthy = {
    currentSrc: 'https://image.tmdb.org/t/p/w500/healthy.jpg',
    src: 'https://image.tmdb.org/t/p/w500/healthy.jpg',
    dataset: {imageRetryCount: '0'},
    hidden: false,
    complete: true,
    naturalWidth: 500,
    classList: {
      add(...values) { values.forEach(value => healthyClasses.add(value)); },
      remove(...values) { values.forEach(value => healthyClasses.delete(value)); },
      contains(value) { return healthyClasses.has(value); }
    }
  };
  const root = {querySelectorAll() { return [failed, healthy]; }};

  try {
    assert.equal(store.recoverRemoteImages(root), 1);
    assert.equal(failed.hidden, false);
    assert.equal(failed.dataset.imageRetryCount, '0');
    assert.equal(failedClasses.has('remote-image-failed'), false);
    assert.match(failed.src, /_media_sub_retry=refresh-/);
    assert.equal(healthy.src, 'https://image.tmdb.org/t/p/w500/healthy.jpg');
  } finally {
    global.window = originalWindow;
  }
});

test('settings load auto-refreshes Quark health when a cookie is configured', async () => {
  const store = app();
  const refreshes = [];
  store.refreshQuarkHealth = async () => { refreshes.push('refresh'); };
  const originalFetch = global.fetch;
  global.fetch = async url => {
    if (url === '/api/settings') {
      return {ok: true, status: 200, json: async () => ({data: {quark_cookie_configured: true}})};
    }
    throw new Error(`unexpected fetch: ${url}`);
  };

  try {
    await store.loadSettings();
    assert.deepEqual(refreshes, ['refresh']);
    assert.equal(store.settingsLoaded, true);
  } finally {
    global.fetch = originalFetch;
  }
});

test('settings load skips Quark health refresh without a configured cookie', async () => {
  const store = app();
  const refreshes = [];
  store.refreshQuarkHealth = async () => { refreshes.push('refresh'); };
  const originalFetch = global.fetch;
  global.fetch = async url => {
    if (url === '/api/settings') {
      return {ok: true, status: 200, json: async () => ({data: {quark_cookie_configured: false, quark_cookie: ''}})};
    }
    throw new Error(`unexpected fetch: ${url}`);
  };

  try {
    await store.loadSettings();
    assert.deepEqual(refreshes, []);
  } finally {
    global.fetch = originalFetch;
  }
});

test('testTmdb warns when no TMDB API key is configured', async () => {
  const store = app();
  const notices = [];
  store.showNotification = (level, message) => notices.push([level, message]);

  await store.testTmdb();

  assert.deepEqual(notices, [['warning', '请先填写 TMDB API Key']]);
});

test('testTmdb reports a successful TMDB API connection', async () => {
  const store = app();
  const notices = [];
  store.showNotification = (level, message) => notices.push([level, message]);
  store.settings.tmdb_api_key_configured = true;
  const originalFetch = global.fetch;
  global.fetch = async url => {
    assert.equal(url, '/api/metadata/test');
    return {ok: true, status: 200, json: async () => ({data: {success: true, message: 'TMDB API 连接成功'}})};
  };

  try {
    await store.testTmdb();
    assert.deepEqual(notices, [
      ['info', '测试 TMDB API 连接中...'],
      ['success', 'TMDB API 连接成功']
    ]);
  } finally {
    global.fetch = originalFetch;
  }
});

test('testTmdb surfaces the backend failure message', async () => {
  const store = app();
  const notices = [];
  store.showNotification = (level, message) => notices.push([level, message]);
  store.settings.tmdb_api_key = 'secret-key';
  const originalFetch = global.fetch;
  global.fetch = async () => {
    return {ok: true, status: 200, json: async () => ({data: {success: false, message: 'TMDB API 测试失败: HTTP 401'}})};
  };

  try {
    await store.testTmdb();
    assert.deepEqual(notices[1], ['error', 'TMDB API 测试失败: HTTP 401']);
  } finally {
    global.fetch = originalFetch;
  }
});

test('connection status reflects request results and the job event stream', () => {
  const store = app();
  store.formatTime = () => '刚刚';
  assert.equal(store.connectionState, 'online');
  assert.equal(store.connectionLabel(), '服务在线');
  assert.match(store.connectionDetail(), /尚未收到服务响应/);

  store.jobEventsReconnecting = true;
  assert.equal(store.connectionState, 'reconnecting', '事件流断开但请求正常时只是重连中');
  assert.equal(store.headerHealthLabel(), '正在重连');

  store.markServerReachable(false);
  assert.equal(store.connectionState, 'unreachable');
  assert.equal(store.connectionLabel(), '服务未响应');
  assert.equal(store.serverLastOkAt, 0, '失败不能刷新最近成功时间');

  store.pwaOffline = true;
  assert.equal(store.connectionState, 'offline', '设备离线优先于服务状态');
  assert.equal(store.connectionLabel(), '网络离线');

  store.pwaOffline = false;
  store.jobEventsReconnecting = false;
  store.markServerReachable(true);
  assert.equal(store.connectionState, 'online');
  assert.ok(store.serverLastOkAt > 0);
  assert.match(store.connectionDetail(), /最近成功响应：刚刚/);
  assert.equal(store.headerHealthLabel(), '系统空闲');

  store.jobs = [{status: 'running'}, {status: 'queued'}];
  assert.equal(store.headerHealthLabel(), '2 项运行中');
});

test('job event stream marks the server reachable and clears the reconnecting state', () => {
  const store = app();
  const listeners = {};
  const previous = globalThis.EventSource;
  globalThis.EventSource = function EventSource() {
    this.addEventListener = (name, handler) => { listeners[name] = handler; };
    this.close = () => {};
  };
  try {
    store.ownLifecycle = (_name, resource) => resource;
    store.setupJobEvents();
    assert.equal(store.jobEventsReconnecting, false, '首次连接尚未收到快照不算故障');

    store.jobEvents.onerror();
    store.markServerReachable(false);
    assert.equal(store.jobEventsReconnecting, true);
    assert.equal(store.connectionState, 'unreachable');

    listeners.snapshot({data: '[]'});
    assert.equal(store.jobEventsHealthy, true);
    assert.equal(store.jobEventsReconnecting, false);
    assert.equal(store.connectionState, 'online', '收到快照即说明服务已恢复，不必等下一次请求');

    store.jobEvents.onerror();
    store.closeJobEventsForBfcache();
    assert.equal(store.jobEventsReconnecting, false, '主动关闭连接后不能停留在重连中');
  } finally {
    globalThis.EventSource = previous;
  }
});

test('settings overview summarises pending required items for the collapsed mobile view', () => {
  const store = app();
  store.quarkHealth = {message: ''};
  store.settings = {};
  const pending = store.settingsPendingRequiredItems.map(item => item.id);
  assert.deepEqual(pending, ['auth', 'quark', 'storage', 'automation'], '可选项不计入待完成');
  assert.equal(store.settingsOverviewSummary(), '还有 4 项必需配置待完成');
  assert.ok(store.settingsPendingRequiredItems.every(item => store.settingsCompletionItemClass(item) === 'is-required'),
    '收起态靠 is-required 类筛出这些条目');

  store.settings = {
    app_password_configured: true, quark_cookie_configured: true,
    quark_save_series_dir: '/连续剧', subscription_scheduler_enabled: true
  };
  assert.deepEqual(store.settingsPendingRequiredItems, []);
  assert.equal(store.settingsOverviewSummary(), '必需配置已全部完成');

  assert.equal(store.settingsOverviewOpen, false);
  store.toggleSettingsOverview();
  assert.equal(store.settingsOverviewOpen, true);
});
