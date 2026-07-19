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
  store.stopSearchProgressTimer = () => stopped.push('search');
  store.stopUpdateProgressPolling = () => stopped.push('update');
  store.loadCalendar = () => stopped.push('calendar-load');
  store.loadDrive = () => {};
  store.loadDownloads = () => {};
  store.loadNotifications = () => {};
  store.loadAutomationSummary = () => {};
  store.checkUpdate = () => {};
  store.loadUpdateReleases = () => {};
  store.loadUpdateProgress = async () => null;

  store.currentTab = 'search';
  store.selectTab('calendar', false);
  assert.deepEqual(stopped, ['search', 'update', 'downloads', 'notifications', 'calendar-load']);

  stopped.length = 0;
  store.currentTab = 'settings';
  store.currentSettingsTab = 'maintenance';
  store.selectSettingsTab('connections', false);
  assert.deepEqual(stopped, ['search', 'update', 'downloads', 'notifications']);
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
