const test = require('node:test');
const assert = require('node:assert/strict');

// drive.js / downloads.js 在**模块加载时**就从 global.MediaSubApi 解构 apiData，
// 因此必须在 require 之前装好一个可替换的转发桩，测试内再改 apiDataStub。
let apiDataStub = async () => ({});
global.MediaSubApi = {
  apiData: (...args) => apiDataStub(...args),
  apiFetch: async () => new Response('{}', {status: 200})
};

require('../static/js/core/formatters.js');
const downloads = require('../static/js/stores/downloads.js');
const drive = require('../static/js/stores/drive.js');
const updates = require('../static/js/features/updates.js');
const notifications = require('../static/js/core/notifications.js');

test('downloads store normalizes groups, summaries, and task capabilities', () => {
  const groups = downloads.normalizeDownloadGroups({
    active: [{status: 'active', download_speed: '12', completed_length: 30, total_length: 100}],
    waiting: null,
    stopped: [{status: 'complete'}]
  });
  assert.deepEqual(groups.waiting, []);
  assert.equal(downloads.flattenDownloadTasks(groups).length, 2);
  assert.deepEqual(downloads.summarizeActiveDownloads(groups), {speed: 12, completed: 30, total: 100});
  assert.deepEqual(downloads.downloadTaskCapabilities({status: 'paused'}), {
    pause: false,
    resume: true,
    stop: true,
    retry: false
  });
  assert.deepEqual(
    downloads.flattenDownloadTasks({
      active: [{gid: 'same', status: 'active'}],
      waiting: [{gid: 'same', status: 'waiting'}, {gid: 'next', status: 'waiting'}]
    }).map(task => task.gid),
    ['same', 'next']
  );
});

test('downloads are split into completed, active, queued and failed groups', () => {
  const categorized = downloads.categorizeDownloadTasks({
    active: [
      {gid: 'active-1', status: 'active'},
      {gid: 'paused-active', status: 'paused'}
    ],
    waiting: [
      {gid: 'queued-1', status: 'waiting'},
      {gid: 'paused-waiting', status: 'paused'}
    ],
    stopped: [
      {gid: 'done', status: 'complete'},
      {gid: 'failed', status: 'error'},
      {gid: 'removed', status: 'removed'},
      // A stale duplicate snapshot must stay in the first group it appears in.
      {gid: 'active-1', status: 'error'}
    ]
  });
  assert.deepEqual(categorized.downloading.map(task => task.gid), ['active-1', 'paused-active']);
  assert.deepEqual(categorized.queued.map(task => task.gid), ['queued-1', 'paused-waiting']);
  assert.deepEqual(categorized.completed.map(task => task.gid), ['done']);
  assert.deepEqual(categorized.failed.map(task => task.gid), ['failed', 'removed']);
  assert.equal(downloads.downloadTaskCapabilities({status: 'error'}).retry, true);
});

test('downloads high-frequency polling only needs active or waiting tasks', () => {
  assert.equal(downloads.hasPollableDownloadTasks({
    active: [],
    waiting: [],
    stopped: [{gid: 'done', status: 'complete'}]
  }), false);
  assert.equal(downloads.hasPollableDownloadTasks({
    active: [{gid: 'active', status: 'active'}],
    waiting: [],
    stopped: []
  }), true);
  assert.equal(downloads.hasPollableDownloadTasks({
    active: [],
    waiting: [{gid: 'waiting', status: 'waiting'}],
    stopped: []
  }), true);
  assert.equal(downloads.hasPollableDownloadTasks({
    active: [],
    waiting: [{gid: 'paused', status: 'paused'}],
    stopped: []
  }), false);
});

test('drive store filters videos and keeps folders first while sorting', () => {
  const items = [
    {fid: '2', file: true, file_name: 'Episode 10.mkv', size: 20},
    {fid: '1', file: false, file_name: 'Season 1', size: 0},
    {fid: '3', file: true, file_name: 'Episode 2.mp4', size: 10},
    {fid: '4', file: true, file_name: 'notes.txt', size: 1}
  ];
  assert.equal(drive.isDriveVideo(items[0]), true);
  assert.deepEqual(
    drive.filterAndSortDriveItems(items, {filterType: 'video', sortBy: 'name'}).map(item => item.fid),
    ['3', '2']
  );
  assert.deepEqual(
    drive.filterAndSortDriveItems(items, {sortBy: 'size', direction: 'desc'}).map(item => item.fid),
    ['1', '2', '3', '4']
  );
});

test('updates and notification helpers clamp unsafe input and preserve immutable filtering', () => {
  assert.equal(updates.normalizeUpdateProgress({running: false, stage: 'idle', percent: 0}), null);
  assert.deepEqual(updates.normalizeUpdateProgress({
    running: true,
    stage: 'downloading',
    percent: 140,
    downloaded_bytes: '12',
    total_bytes: '20'
  }), {
    running: true,
    stage: 'downloading',
    percent: 100,
    downloaded_bytes: 12,
    total_bytes: 20
  });
  assert.equal(notifications.normalizeNotificationType('unknown'), 'info');
  assert.equal(notifications.toastIcon('error'), '✕');
});

test('activity polling drops the redundant job fetch while SSE is healthy', () => {
  const calls = [];
  const store = notifications.createStore();
  store.currentTab = 'dashboard';
  store.loadJobs = () => { calls.push('jobs'); return Promise.resolve(); };
  store.loadNotifications = () => { calls.push('notifications'); return Promise.resolve(); };
  let polled = null;
  store.startPolling = (_name, callback) => { polled = callback; return 1; };
  store.stopPolling = () => true;

  // SSE 正常：任务列表由推送维护，轮询只需要补没有 SSE 通道的通知。
  store.jobEventsHealthy = true;
  store.startNotificationsPolling();
  polled();
  assert.deepEqual(calls, ['notifications']);

  // SSE 断线：回落到任务 + 通知的全量刷新。
  calls.length = 0;
  store.jobEventsHealthy = false;
  polled();
  assert.deepEqual(calls.sort(), ['jobs', 'notifications']);
});

test('Docker deployments expose manual image upgrade instructions', () => {
  const store = updates.createStore();
  store.updateInfo = {
    runtime: 'docker',
    online_update_supported: false,
    update_available: true
  };
  store.updateReleases = [{
    tag: 'v2.2.13',
    asset: {size: 1},
    is_current: false,
    is_newer: true
  }];
  store.selectedUpdateTag = 'v2.2.13';

  assert.equal(store.onlineUpdateSupported(), false);
  assert.equal(store.canApplySelectedUpdate(), false);
  assert.equal(store.dockerUpdateCommand(), 'docker compose pull && docker compose up -d');
  assert.match(store.selectedUpdateDescription(), /Docker/);
});

test('managed Docker deployments can use persistent online updates', () => {
  const store = updates.createStore();
  store.updateInfo = {
    runtime: 'docker',
    online_update_supported: true,
    update_available: true
  };
  store.updateReleases = [{
    tag: 'v2.2.14',
    asset: {size: 1},
    is_current: false,
    is_newer: true
  }];
  store.selectedUpdateTag = 'v2.2.14';

  assert.equal(store.onlineUpdateSupported(), true);
  assert.equal(store.canApplySelectedUpdate(), true);
  assert.equal(store.updateRuntimeLabel(), 'Docker（可在线更新）');
  assert.match(store.updateMethodDescription(), /runtime/);
});

test('disabled binary updates use manual instructions instead of Docker copy', () => {
  const store = updates.createStore();
  store.updateInfo = {runtime: 'binary', online_update_supported: false, update_available: true};

  assert.equal(store.unsupportedUpdateTitle(), '手工升级');
  assert.match(store.unsupportedUpdateMessage(), /static/);
  assert.doesNotMatch(store.unsupportedUpdateMessage(), /Docker/);
});

test('activity merge sorts jobs and notifications and supports source filters', () => {
  const items = notifications.mergeActivityItems([
    {id: 'job-old', kind: 'manual_transfer', status: 'succeeded', updated_at: 10, title: '旧任务'},
    {id: 'job-new', kind: 'subscription_transfer', status: 'failed', updated_at: 30, title: '失败任务'}
  ], [
    {id: 'notice-mid', event: 'subscription_updated', level: 'info', read: false, created_at: 20, title: '未读通知'},
    {id: 'notice-new', event: 'push_sent', level: 'success', read: true, created_at: 30, title: '推送'}
  ]);
  assert.deepEqual(items.map(item => item.id), [
    'notification:notice-new', 'job:job-new', 'notification:notice-mid', 'job:job-old'
  ]);
  assert.deepEqual(items.map(item => item.source), ['notification', 'job', 'notification', 'job']);

  const store = notifications.createStore();
  store.backgroundJobs = [
    {id: 'job-1', kind: 'manual_transfer', status: 'failed', updated_at: 4, title: '任务失败'}
  ];
  store.notifications = [
    {id: 'notice-1', event: 'subscription_updated', level: 'info', read: false, created_at: 5, title: '未读'}
  ];
  store.activityFilter = 'failed';
  assert.deepEqual(store.filteredActivityItems.map(item => item.id), ['job:job-1']);
  store.activityFilter = 'unread';
  assert.deepEqual(store.filteredActivityItems.map(item => item.id), ['notification:notice-1']);
});

// ─── 网盘破坏性操作：确认短语与拒绝路径 ──────────────────────────────────────
//
// drive store 的删除是**不可恢复**操作（服务端要求 `DELETE <fid>` 确认短语），
// 而它此前 36 个方法里只有 3 个被测试引用，删除/重命名/批量删除全部零覆盖。

function driveHarness(apiDataImpl) {
  const confirmations = [];
  const notifications = [];
  const refreshes = [];
  const store = drive.createStore();
  store.settings = {};
  store.requestDangerConfirmation = async request => { confirmations.push(request); return true; };
  store.showNotification = (type, message) => notifications.push({type, message});
  store.apiErrorMessage = (error, fallback) => (error && error.message) || fallback;
  store.loadDrive = async full => { refreshes.push(full); };
  store.mediaFormatters = {formatBytes: value => String(value)};
  apiDataStub = apiDataImpl;
  return {store, confirmations, notifications, refreshes};
}

test('deleting a drive item requires a DELETE confirmation and sends the exact phrase', async () => {
  const calls = [];
  const {store, confirmations, notifications, refreshes} = driveHarness(async (url, options) => {
    calls.push({url, body: JSON.parse(options.body)});
    return {success: true, message: '已删除'};
  });

  await store.deleteDriveItem({fid: 'fid-1', file_name: '旧剧集.mkv'});

  assert.deepEqual(confirmations, [{
    title: '删除网盘项目',
    message: '将永久删除 旧剧集.mkv。',
    phrase: 'DELETE'
  }]);
  assert.deepEqual(calls, [{
    url: '/api/drive/delete',
    body: {fids: ['fid-1'], confirmation: 'DELETE fid-1'}
  }]);
  assert.deepEqual(notifications, [{type: 'success', message: '已删除'}]);
  assert.deepEqual(refreshes, [true], '删除后必须刷新列表');
});

test('canceling the drive delete confirmation sends nothing', async () => {
  let called = false;
  const {store} = driveHarness(async () => { called = true; return {}; });
  store.requestDangerConfirmation = async () => false;

  await store.deleteDriveItem({fid: 'fid-1', file_name: 'x.mkv'});
  assert.equal(called, false, '用户在确认框取消后不得发起删除请求');
});

test('batch delete uses the count phrase and clears the selection', async () => {
  const calls = [];
  const {store, confirmations} = driveHarness(async (url, options) => {
    calls.push(JSON.parse(options.body));
    return {success: true, message: '已删除 2 项'};
  });
  store.driveSelectedItems = ['a', 'b'];
  store.driveSelectMode = true;

  await store.batchDeleteDrive();

  assert.equal(confirmations[0].phrase, 'DELETE');
  assert.deepEqual(calls, [{fids: ['a', 'b'], confirmation: 'DELETE 2'}]);
  assert.deepEqual(store.driveSelectedItems, [], '删除后应清空选择');
  assert.equal(store.driveSelectMode, false);
});

test('renaming a drive item to the same name is a no-op', async () => {
  let called = false;
  const {store} = driveHarness(async () => { called = true; return {}; });
  global.prompt = () => '同名.mkv';

  await store.renameDriveItem({fid: 'fid-1', file_name: '同名.mkv'});
  assert.equal(called, false, '名称未变化时不应发起请求');
});

test('renaming a drive item sends fid, name and parent fid', async () => {
  const calls = [];
  const {store} = harnessRename(calls);
  global.prompt = () => '新名字.mkv';
  store.driveCurrentFid = 'parent-9';

  await store.renameDriveItem({fid: 'fid-2', file_name: '旧名字.mkv'});
  assert.deepEqual(calls, [{
    url: '/api/drive/rename',
    body: {fid: 'fid-2', name: '新名字.mkv', parent_fid: 'parent-9'}
  }]);
});

function harnessRename(calls) {
  return driveHarness(async (url, options) => {
    calls.push({url, body: JSON.parse(options.body)});
    return {success: true, message: '已重命名'};
  });
}
