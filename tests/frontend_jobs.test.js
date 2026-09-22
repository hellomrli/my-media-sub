const test = require('node:test');
const assert = require('node:assert/strict');

let request;
global.MediaSubApi = {
  apiFetch: async (url, init) => {
    request = {url, init};
    return new Response(JSON.stringify({
      ok: true,
      data: {id: 'job-1', status: 'queued', priority: 'high'}
    }), {status: 200, headers: {'Content-Type': 'application/json'}});
  }
};

const jobsModule = require('../static/js/stores/jobs.js');

test('jobs store labels legacy priority as normal and includes it in copied detail', () => {
  const store = jobsModule.createStore();
  store.formatTime = value => String(value || '-');

  assert.equal(store.jobPriorityLabel(undefined), '普通');
  assert.equal(store.jobPriorityLabel('high'), '高');
  assert.match(store.jobSummaryText({
    id: 'job-1', title: '测试', kind: 'metadata_scrape', status: 'queued',
    progress: 0, created_at: 1, updated_at: 1, payload: {}, result: {}
  }), /优先级：普通/);
  assert.equal(store.jobErrorClassLabel('timed_out'), '执行超时');
});

test('jobs store updates queued priority through the stable API contract', async () => {
  const store = jobsModule.createStore();
  store.jobs = [{id: 'job-1', status: 'queued', priority: 'normal'}];
  store.showNotification = () => {};
  store.apiErrorMessage = (_error, fallback) => fallback;

  await store.setJobPriority(store.jobs[0], 'high');

  assert.equal(request.url, '/api/jobs/job-1/priority');
  assert.equal(request.init.method, 'POST');
  assert.deepEqual(JSON.parse(request.init.body), {priority: 'high'});
  assert.equal(store.jobs[0].priority, 'high');
});

// ─── 作业 SSE 事件流的并发交错 ───────────────────────────────────────────────
//
// setupJobEvents 是前端并发最敏感的一段（快照是全量、job 事件是增量，而 job
// 处理器是 async 的，可能在 await 联动刷新时被快照打断），此前**零测试**。
// 下面用一个可手动驱动的假 EventSource 复现三种交错。

function fakeEventSource() {
  const listeners = new Map();
  return {
    closed: false,
    addEventListener(name, handler) {
      listeners.set(name, handler);
    },
    fire(name, payload) {
      return listeners.get(name)({data: JSON.stringify(payload)});
    },
    close() {
      this.closed = true;
    }
  };
}

function sseStore(source, overrides = {}) {
  const store = jobsModule.createStore();
  store.jobEvents = null;
  store.jobEventsHealthy = false;
  store.loadNotifications = async () => {};
  store.loadSubscriptions = async () => {};
  store.showNotification = () => {};
  store.ownLifecycle = (name, resource) => resource;
  Object.assign(store, overrides);
  global.EventSource = function () { return source; };
  store.setupJobEvents();
  return store;
}

test('job snapshot arriving during an in-flight handler does not clobber the upsert', async () => {
  const source = fakeEventSource();
  let releaseNotifications;
  const blocked = new Promise(resolve => { releaseNotifications = resolve; });
  const store = sseStore(source, {
    loadNotifications: () => blocked,
    loadSubscriptions: async () => {}
  });

  // 一个 job 事件进入，处理器卡在 await loadNotifications 上
  const job = {id: 'job-1', kind: 'subscription_transfer', status: 'succeeded', title: '转存'};
  const handler = source.fire('job', job);

  // 在途期间到达一个**不含该任务**的旧快照
  source.fire('snapshot', [{id: 'job-0', kind: 'metadata_scrape', status: 'queued', title: '旧任务'}]);
  assert.equal(
    store.jobs.some(item => item.id === 'job-1'),
    true,
    '快照在途时应被暂存，不能立刻冲掉刚 upsert 的任务'
  );

  releaseNotifications();
  await handler;

  assert.equal(
    store.jobs.some(item => item.id === 'job-1'),
    true,
    '快照应用后必须重新叠加在途期间 upsert 的任务'
  );
  assert.equal(store.jobs.some(item => item.id === 'job-0'), true);
});

test('terminal job event refreshes notifications and subscriptions for transfers', async () => {
  const source = fakeEventSource();
  const calls = [];
  const store = sseStore(source, {
    loadNotifications: async () => { calls.push('notifications'); },
    loadSubscriptions: async () => { calls.push('subscriptions'); }
  });

  await source.fire('job', {id: 'j1', kind: 'subscription_transfer', status: 'succeeded'});
  await source.fire('job', {id: 'j2', kind: 'metadata_scrape', status: 'succeeded'});
  await source.fire('job', {id: 'j3', kind: 'subscription_transfer', status: 'running'});

  // 每条终态任务都刷新通知；其中 subscription_transfer / metadata_scrape 还要刷新订阅
  assert.deepEqual(calls, [
    'notifications', 'subscriptions',
    'notifications', 'subscriptions'
  ]);
});

test('SSE error marks the stream unhealthy so polling can take over', () => {
  const source = fakeEventSource();
  const store = sseStore(source);
  store.jobEventsHealthy = true;
  source.onerror();
  assert.equal(store.jobEventsHealthy, false);
});

test('setupJobEvents is idempotent and does not open a second stream', () => {
  const source = fakeEventSource();
  const store = sseStore(source);
  const first = store.jobEvents;
  store.setupJobEvents();
  assert.equal(store.jobEvents, first, '已有连接时不应重复建立 SSE');
});
