const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

let responseBody;
let failure;
let fetchImpl;
global.MediaSubApi = {
  async apiFetch(...args) {
    if (fetchImpl) return fetchImpl(...args);
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

function deferredSearches(t) {
  const requests = [];
  fetchImpl = (_url, options) => new Promise((resolve, reject) => {
    requests.push({options, resolve: data => resolve({json: async () => data}), reject});
  });
  t.after(() => { fetchImpl = null; });
  t.mock.method(global, 'setTimeout', () => 0);
  return requests;
}

test('a late search cannot overwrite the newest result even if abort is ignored', async t => {
  const requests = deferredSearches(t);
  const {store, notifications} = harness();
  store.searchQuery = 'first';
  const first = store.search();
  store.searchQuery = 'second';
  const second = store.search();
  assert.equal(requests[0].options.signal.aborted, true);
  requests[1].resolve({data: [{note: 'new result'}]});
  await second;
  const noticeCount = notifications.length;
  requests[0].resolve({data: [{note: 'old result'}], partial: true});
  await first;
  assert.equal(store.searchResults[0].note, 'new result');
  assert.equal(store.searchPartial, false);
  assert.equal(notifications.length, noticeCount);
});

test('superseded failures cannot stop the latest progress or raise an error', async t => {
  const requests = deferredSearches(t);
  const {store, notifications} = harness();
  const first = store.search();
  store.searchQuery = 'second';
  const second = store.search();
  requests[0].reject(new Error('old request failed'));
  await first;
  assert.equal(store.searching, true);
  assert.equal(notifications.some(item => item.level === 'error'), false);
  requests[1].resolve({data: []});
  await second;
  assert.equal(store.searching, false);
});

test('repeated Enter for the same pending query does not duplicate requests', async t => {
  const requests = deferredSearches(t);
  const {store} = harness();
  const first = store.search();
  const second = store.search();
  assert.equal(requests.length, 1);
  requests.forEach(request => request.resolve({data: []}));
  await Promise.all([first, second]);
});

test('completion messages use the submitted options rather than later edits', async t => {
  const requests = deferredSearches(t);
  const {store, notifications} = harness(false);
  const search = store.search();
  store.searchOptions.filterBad = true;
  requests[0].resolve({data: [{note: 'unknown'}], partial: true, total_count: 1, probed_count: 0});
  await search;
  assert.match(notifications.at(-1).message, /有效性未知/);
  assert.doesNotMatch(notifications.at(-1).message, /暂未展示/);
});

test('an old progress reset cannot clear the newest completed search', async t => {
  const requests = deferredSearches(t);
  const timers = [];
  t.mock.method(global, 'setTimeout', callback => { timers.push(callback); return timers.length; });
  const {store} = harness();
  const first = store.search();
  requests[0].resolve({data: []});
  await first;
  store.searchQuery = 'second';
  const second = store.search();
  requests[1].resolve({data: [{note: 'new'}]});
  await second;
  timers[0]();
  assert.equal(store.searchProgress.value, 100);
  timers.at(-1)();
  assert.equal(store.searchProgress.value, 0);
});

test('a superseded response body is ignored after its headers have already arrived', async t => {
  t.mock.method(global, 'setTimeout', () => 0);
  let finishBody;
  let bodyStarted;
  const started = new Promise(resolve => { bodyStarted = resolve; });
  fetchImpl = async () => ({json() {
    bodyStarted();
    return new Promise(resolve => { finishBody = resolve; });
  }});
  t.after(() => { fetchImpl = null; });
  const {store} = harness();
  const first = store.search();
  await started;
  store.searchQuery = 'second';
  fetchImpl = async () => ({json: async () => ({data: [{note: 'new'}]})});
  await store.search();
  finishBody({data: [{note: 'old'}]});
  await first;
  assert.equal(store.searchResults[0].note, 'new');
});

test('lifecycle cleanup aborts the pending search and prevents later state changes', async t => {
  const requests = deferredSearches(t);
  const {store} = harness();
  const registry = require('../static/js/core/polling.js').createPollingRegistry();
  store.ownLifecycle = registry.own;
  store.stopPolling = registry.stop;
  const search = store.search();
  registry.stopAll();
  assert.equal(requests[0].options.signal.aborted, true);
  requests[0].resolve({data: [{note: 'late'}]});
  await search;
  assert.equal(store.searching, false);
  assert.equal(store.searchResults.length, 0);
});
