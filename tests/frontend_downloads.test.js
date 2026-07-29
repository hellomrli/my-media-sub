const test = require('node:test');
const assert = require('node:assert/strict');

let apiDataStub = async () => { throw new Error('apiData stub not configured'); };
global.MediaSubApi = {apiData: (...args) => apiDataStub(...args)};
require('../static/js/core/formatters.js');
const downloads = require('../static/js/stores/downloads.js');

function storeHarness() {
  const store = downloads.createStore();
  const confirmations = [];
  const notifications = [];
  const refreshes = [];
  store.requestDangerConfirmation = async options => {
    confirmations.push(options);
    return true;
  };
  store.showNotification = (type, message) => notifications.push({type, message});
  store.apiErrorMessage = (_error, fallback) => fallback;
  store.loadDownloads = async (...args) => refreshes.push(args);
  return {store, confirmations, notifications, refreshes};
}

test('downloads load the latest thousand once and only resolve tasks that leave the active queue', async () => {
  const calls = [];
  apiDataStub = async url => {
    calls.push(url);
    if (url.endsWith('stopped_limit=1000')) {
      return {
        active: [{gid: 'active-1', status: 'active'}],
        waiting: [],
        stopped: [{gid: 'old-2', status: 'complete'}, {gid: 'old-1', status: 'complete'}]
      };
    }
    if (url.endsWith('stopped_limit=0')) return {active: [], waiting: [], stopped: []};
    if (url.endsWith('/active-1')) return {gid: 'active-1', status: 'complete'};
    throw new Error(`unexpected URL: ${url}`);
  };
  const store = downloads.createStore();
  store.settings = {aria2_rpc_url: 'http://127.0.0.1:6800/jsonrpc'};
  store.syncDownloadsPolling = () => {};

  await store.loadDownloads();
  await store.loadDownloads(true);

  assert.deepEqual(calls, [
    '/api/drive/aria2/tasks?stopped_limit=1000',
    '/api/drive/aria2/tasks?stopped_limit=0',
    '/api/drive/aria2/tasks/active-1'
  ]);
  assert.deepEqual(store.downloads.stopped.map(task => task.gid), ['active-1', 'old-2', 'old-1']);
  assert.equal(downloads.FULL_STOPPED_LIMIT, 1000);
  assert.equal(downloads.POLL_STOPPED_LIMIT, 0);
});

test('a transient final-status failure keeps the disappeared task for the next poll', async () => {
  const originalWarn = console.warn;
  apiDataStub = async url => {
    if (url.endsWith('stopped_limit=0')) return {active: [], waiting: [], stopped: []};
    throw new Error('temporary RPC failure');
  };
  const store = downloads.createStore();
  store.settings = {aria2_rpc_url: 'http://127.0.0.1:6800/jsonrpc'};
  store.downloadsHistoryLoadedAt = Date.now();
  store.downloads.active = [{gid: 'active-1', status: 'active'}];
  store.syncDownloadsPolling = () => {};

  console.warn = () => {};
  try {
    await store.loadDownloads(true);
  } finally {
    console.warn = originalWarn;
  }

  assert.deepEqual(store.downloads.active.map(task => task.gid), ['active-1']);
});

test('a manual refresh queued behind a fast poll still reloads full history', async () => {
  const calls = [];
  let resolvePoll;
  apiDataStub = url => {
    calls.push(url);
    if (url.endsWith('stopped_limit=0')) {
      return new Promise(resolve => { resolvePoll = resolve; });
    }
    if (url.endsWith('stopped_limit=1000')) {
      return Promise.resolve({active: [], waiting: [], stopped: [{gid: 'done-1', status: 'complete'}]});
    }
    throw new Error(`unexpected URL: ${url}`);
  };
  const store = downloads.createStore();
  store.settings = {aria2_rpc_url: 'http://127.0.0.1:6800/jsonrpc'};
  store.downloadsHistoryLoadedAt = Date.now();
  store.syncDownloadsPolling = () => {};

  const poll = store.loadDownloads(true);
  await Promise.resolve();
  await store.loadDownloads();
  resolvePoll({active: [], waiting: [], stopped: []});
  await poll;

  assert.deepEqual(calls, [
    '/api/drive/aria2/tasks?stopped_limit=0',
    '/api/drive/aria2/tasks?stopped_limit=1000'
  ]);
  assert.deepEqual(store.downloads.stopped.map(task => task.gid), ['done-1']);
});

test('removing the Aria2 configuration stops an existing download poller', async () => {
  const stopped = [];
  const store = downloads.createStore();
  store.settings = {aria2_rpc_url: ''};
  store.downloadsPoller = 1;
  store.stopPolling = name => stopped.push(name);

  await store.loadDownloads(true);

  assert.deepEqual(stopped, ['downloads']);
  assert.equal(store.downloadsPoller, null);
});

test('download polling runs only while active or waiting tasks exist', () => {
  const started = [];
  const stopped = [];
  const store = downloads.createStore();
  store.currentTab = 'downloads';
  store.settings = {aria2_rpc_url: 'http://127.0.0.1:6800/jsonrpc'};
  store.startPolling = (...args) => { started.push(args); return 1; };
  store.stopPolling = name => stopped.push(name);

  store.startDownloadsPolling();
  assert.deepEqual(started, []);

  store.downloads.active = [{gid: 'active-1', status: 'active'}];
  store.startDownloadsPolling();
  assert.equal(started.length, 1);
  assert.equal(started[0][0], 'downloads');
  assert.equal(started[0][2], 2000);

  store.downloads = {active: [], waiting: [], stopped: [{gid: 'done-1', status: 'complete'}]};
  store.syncDownloadsPolling();
  assert.deepEqual(stopped, ['downloads']);
  assert.equal(store.downloadsPoller, null);

  store.currentTab = 'settings';
  store.downloads.active = [{gid: 'active-2', status: 'active'}];
  store.downloadsPoller = 2;
  store.syncDownloadsPolling();
  assert.deepEqual(stopped, ['downloads', 'downloads']);
  assert.equal(store.downloadsPoller, null);
});

test('completed and failed history renders in bounded windows', () => {
  const store = downloads.createStore();
  store.downloads.stopped = Array.from({length: 150}, (_, index) => ({
    gid: `done-${index}`,
    status: 'complete'
  }));

  assert.equal(store.downloadCategoryTasks('completed').length, 150);
  assert.equal(store.visibleDownloadCategoryTasks('completed').length, 100);
  assert.equal(store.hasMoreDownloadCategoryTasks('completed'), true);
});

test('purging stopped downloads requires typed confirmation and refreshes the task list', async () => {
  const calls = [];
  apiDataStub = async (url, options) => {
    calls.push({url, method: options && options.method});
    return {success: true, message: '已清空已停止的下载记录'};
  };
  const {store, confirmations, notifications, refreshes} = storeHarness();
  store.downloads.stopped = [{gid: 'done', status: 'complete'}];

  assert.equal(store.hasStoppedDownloadTasks(), true);
  await store.controlAllDownloads('purge');

  assert.deepEqual(confirmations, [{
    title: '清空已停止记录',
    message: '将删除 Aria2 中已完成、已出错和已移除的任务记录，清空后无法重试这些任务。',
    phrase: 'CLEAR'
  }]);
  assert.deepEqual(calls, [{
    url: '/api/drive/aria2/tasks/purge-all',
    method: 'POST'
  }]);
  assert.deepEqual(notifications, [{type: 'success', message: '已清空已停止的下载记录'}]);
  assert.deepEqual(refreshes, [[true, {fullHistory: true}]]);
  assert.equal(store.downloadsBulkAction, '');
});

test('canceling purge confirmation leaves aria2 untouched', async () => {
  let called = false;
  apiDataStub = async () => { called = true; return {}; };
  const {store, notifications, refreshes} = storeHarness();
  store.requestDangerConfirmation = async () => false;

  assert.equal(store.hasStoppedDownloadTasks(), false);
  await store.controlAllDownloads('purge');

  assert.equal(called, false);
  assert.deepEqual(notifications, []);
  assert.deepEqual(refreshes, []);
  assert.equal(store.downloadsBulkAction, '');
});
