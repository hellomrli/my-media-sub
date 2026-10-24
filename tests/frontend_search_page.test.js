const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

let responseBody;
let failure;
global.MediaSubApi = {
  async apiFetch() {
    if (failure) throw failure;
    return {json: async () => responseBody};
  }
};
require('../static/js/features/search-results.js');
const searchPage = require('../static/js/features/search-page.js');

function harness(filterBad = false) {
  const store = searchPage.createStore();
  store.searchQuery = 'test';
  store.searchOptions = {probeFiles: true, filterBad};
  store.addSearchHistory = () => {};
  store.startPolling = () => null;
  store.stopPolling = () => {};
  store.apiErrorMessage = error => error.message;
  const notifications = [];
  store.showNotification = (level, message) => notifications.push({level, message});
  return {store, notifications};
}

async function runSearch(store) {
  const original = global.setTimeout;
  global.setTimeout = () => 0;
  try {
    await store.search();
  } finally {
    global.setTimeout = original;
  }
}

test('search retains unknown results and warns rather than claiming full success', async () => {
  failure = null;
  responseBody = {
    ok: true, partial: true, timed_out: true, total_count: 2, probed_count: 0,
    data: [{unique_id: 'a', note: 'Test'}, {unique_id: 'b', note: 'Test'}]
  };
  const {store, notifications} = harness();
  await runSearch(store);
  assert.equal(store.searchResults.length, 2);
  assert.equal(store.searchPartial, true);
  assert.equal(store.searchResultStats.checked, 0);
  assert.match(store.searchWarning, /超时.*0\/2/);
  assert.match(store.searchWarning, /有效性未知/);
  assert.equal(store.searchProgress.label, '探测未完成');
  assert.equal(notifications.at(-1).level, 'warning');
  assert.ok(notifications.every(item => item.level !== 'success'));
});

test('zero completed filtered probes never report no matches', async () => {
  failure = null;
  responseBody = {ok: true, partial: true, timed_out: true, total_count: 2, probed_count: 0, data: []};
  const {store, notifications} = harness(true);
  await runSearch(store);
  assert.equal(store.searchPartial, true);
  assert.equal(store.searchHasRun, true);
  assert.equal(store.searchResults.length, 0);
  assert.match(store.searchWarning, /暂未展示/);
  assert.match(store.searchWarning, /重试/);
  assert.doesNotMatch(notifications.at(-1).message, /未找到|没有匹配/);
});

test('partial probe failures do not incorrectly claim a timeout', async () => {
  failure = null;
  responseBody = {ok: true, partial: true, timed_out: false, total_count: 2, probed_count: 1, data: []};
  const {store} = harness(true);
  await runSearch(store);
  assert.match(store.searchWarning, /未完成.*1\/2/);
  assert.doesNotMatch(store.searchWarning, /超时/);
});

test('retry clears stale incomplete state and supports previous API responses', async () => {
  failure = null;
  responseBody = {ok: true, partial: true, timed_out: true, total_count: 2, probed_count: 0, data: []};
  const {store, notifications} = harness(true);
  await runSearch(store);
  responseBody = {ok: true, data: [{unique_id: 'a', note: 'Test'}]};
  await runSearch(store);
  assert.equal(store.searchPartial, false);
  assert.equal(store.searchWarning, '');
  assert.equal(notifications.at(-1).level, 'success');
  assert.equal(store.searchProgress.label, '搜索完成');
});

test('complete empty search still reports no matches', async () => {
  failure = null;
  responseBody = {ok: true, partial: false, timed_out: false, total_count: 0, probed_count: 0, data: []};
  const {store, notifications} = harness();
  await runSearch(store);
  assert.equal(store.searchPartial, false);
  assert.equal(notifications.at(-1).message, '未找到结果');
});

test('incomplete warning persists after progress reset and has a retry action', () => {
  const source = fs.readFileSync(path.join(__dirname, '../static/partials/page-search.html'), 'utf8');
  assert.match(source, /x-show="searchPartial && !searching"/);
  assert.match(source, /x-text="searchWarning"/);
  assert.match(source, /@click="search\(\)"[^>]*>重新搜索/);
  assert.match(source, /x-show="searchHasRun && !searching && !searchPartial && searchResults.length === 0"/);
});
