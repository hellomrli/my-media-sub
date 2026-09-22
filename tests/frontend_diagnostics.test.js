// 诊断页的备份/清理/恢复流程。
//
// 这些方法此前完全没有测试（`features/diagnostics.js` 的 13 个方法里只有一处
// `typeof === 'function'` 的形状断言），却包含**不可逆**操作：按保留策略删除历史
// 数据、以及覆盖业务数据的备份恢复。这里锁住三类边界：
//   1. 破坏性操作必须经过确认短语，用户取消时不得发出请求；
//   2. 恢复必须要求精确的 `RESTORE DATA` 短语；
//   3. 失败路径必须给出错误提示而不是静默吞掉。
const test = require('node:test');
const assert = require('node:assert/strict');

// apiData 在模块加载时解构，因此必须在 require 之前装好可替换的转发桩。
let apiDataStub = async () => ({});
global.MediaSubApi = {
  apiData: (...args) => apiDataStub(...args),
  apiFetch: async () => new Response('{}', {status: 200}),
  getApiErrorMessage: (error, fallback) => (error && error.message) || fallback
};
global.URL = global.URL || {createObjectURL: () => 'blob:x', revokeObjectURL: () => {}};
global.document = global.document || {createElement: () => ({click: () => {}})};

const diagnostics = require('../static/js/features/diagnostics.js');

function harness(apiDataImpl = async () => ({})) {
  apiDataStub = apiDataImpl;
  const calls = [];
  const notifications = [];
  const confirmations = [];
  const store = diagnostics.createStore();
  store.diagnostics = {storage: {}};
  store.loadDiagnostics = async () => { calls.push({url: 'reload'}); };
  store.showNotification = (type, message) => notifications.push({type, message});
  store.requestDangerConfirmation = async request => {
    confirmations.push(request);
    return true;
  };
  return {store, calls, notifications, confirmations};
}

test('storage cleanup requires a CLEANUP DATA confirmation and reloads diagnostics', async () => {
  const {store, notifications, confirmations} = harness(async (url, options) => {
    assert.equal(url, '/api/storage/cleanup');
    assert.deepEqual(JSON.parse(options.body), {confirmation: 'CLEANUP DATA'});
    return {message: '清理完成'};
  });

  await store.compactStorage();

  assert.deepEqual(confirmations, [{
    title: '按保留策略清理 Store',
    message: '系统会先创建并验证备份，再按预览中的独立保留策略删除历史数据。',
    phrase: 'CLEANUP DATA'
  }]);
  assert.deepEqual(notifications, [{type: 'success', message: '清理完成'}]);
});

test('canceling the storage cleanup confirmation sends nothing', async () => {
  let called = false;
  const {store} = harness(async () => { called = true; return {}; });
  store.requestDangerConfirmation = async () => false;

  await store.compactStorage();
  assert.equal(called, false, '取消确认后不得发起清理请求');
});

test('restore refuses to run without the exact RESTORE DATA phrase', async () => {
  let called = false;
  const {store, notifications} = harness(async () => { called = true; return {}; });
  store.backupArchive = {format: 'my-media-sub-backup', files: []};
  store.backupPreview = {files: []};
  store.restoreConfirmation = 'restore data'; // 大小写不符

  await store.restoreBackup();

  assert.equal(called, false, '短语不精确时绝不能提交恢复请求');
  assert.deepEqual(notifications, [{type: 'warning', message: '请输入 RESTORE DATA 以确认恢复'}]);
});

test('restore is a no-op before an archive and preview are selected', async () => {
  let called = false;
  const {store} = harness(async () => { called = true; return {}; });
  store.restoreConfirmation = 'RESTORE DATA';

  await store.restoreBackup();
  assert.equal(called, false, '未选择备份文件时不应发起请求');
});

test('restore posts the archive with the confirmation phrase', async () => {
  const archive = {format: 'my-media-sub-backup', files: [{path: 'settings.json'}]};
  let posted = null;
  const {store, notifications} = harness(async (url, options) => {
    posted = {url, body: JSON.parse(options.body)};
    return {message: '已暂存'};
  });
  store.backupArchive = archive;
  store.backupPreview = {files: [{path: 'settings.json'}]};
  store.restoreConfirmation = 'RESTORE DATA';

  await store.restoreBackup();

  assert.equal(posted.url, '/api/backups/restore');
  assert.deepEqual(posted.body, {archive, confirmation: 'RESTORE DATA'});
  assert.deepEqual(notifications, [{type: 'success', message: '已暂存'}]);
});

test('backup creation and verification surface failures instead of swallowing them', async () => {
  const {store, notifications} = harness(async () => {
    throw new Error('磁盘已满');
  });

  await store.createStoredBackup();
  await store.verifyStoredBackup();

  assert.deepEqual(notifications, [
    {type: 'error', message: '磁盘已满'},
    {type: 'error', message: '磁盘已满'}
  ]);
  assert.equal(store.backupVerifying, false, '失败后必须复位 loading 标志');
});

test('diagnostic byte and average formatters stay stable', () => {
  const {store} = harness();
  assert.equal(store.diagnosticBytes(512), '512 B');
  assert.equal(store.diagnosticBytes(2048), '2.0 KiB');
  assert.equal(store.diagnosticBytes(3 * 1024 * 1024), '3.0 MiB');
  assert.equal(store.diagnosticAverage(0, 0), '-');
  assert.equal(store.diagnosticAverage(2000, 2), '1.00 ms');
});
