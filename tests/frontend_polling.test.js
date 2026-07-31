const test = require('node:test');
const assert = require('node:assert/strict');

const {createPollingRegistry, createStore} = require('../static/js/core/polling.js');

function fakeDocument() {
  const listeners = new Map();
  return {
    listeners,
    hidden: false,
    addEventListener(name, handler) { listeners.set(name, handler); },
    removeEventListener(name, handler) {
      if (listeners.get(name) === handler) listeners.delete(name);
    }
  };
}

function pollingStore(scheduler, doc) {
  globalThis.document = doc;
  const store = createStore();
  store.pollingRegistry = createPollingRegistry(scheduler);
  return store;
}

function fakeScheduler() {
  let nextId = 0;
  const active = new Map();
  return {
    active,
    setInterval(callback, delay) {
      const id = ++nextId;
      active.set(id, {callback, delay});
      return id;
    },
    clearInterval(id) {
      active.delete(id);
    }
  };
}

test('polling registry replaces named timers and stops every lifecycle resource', () => {
  const scheduler = fakeScheduler();
  const registry = createPollingRegistry(scheduler);
  const first = registry.startInterval('downloads', () => {}, 2000);
  const second = registry.startInterval('downloads', () => {}, 1000);
  assert.notEqual(first, second);
  assert.equal(scheduler.active.has(first), false);
  assert.equal(scheduler.active.has(second), true);
  assert.deepEqual(registry.names(), ['downloads']);

  let closed = 0;
  registry.own('events', {close() { closed += 1; }});
  registry.stopAll();
  assert.equal(scheduler.active.size, 0);
  assert.equal(closed, 1);
  assert.equal(registry.size, 0);
});

test('polling registry removes event listeners without accumulating duplicates', () => {
  const listeners = new Map();
  const target = {
    addEventListener(name, handler) { listeners.set(name, handler); },
    removeEventListener(name, handler) {
      if (listeners.get(name) === handler) listeners.delete(name);
    }
  };
  const registry = createPollingRegistry(fakeScheduler());
  const first = () => {};
  const second = () => {};
  registry.listen('navigation', target, 'popstate', first);
  registry.listen('navigation', target, 'popstate', second);
  assert.equal(listeners.get('popstate'), second);
  registry.stop('navigation');
  assert.equal(listeners.has('popstate'), false);
});

test('polling skips hidden pages and refreshes immediately on return', () => {
  const scheduler = fakeScheduler();
  const doc = fakeDocument();
  const store = pollingStore(scheduler, doc);
  try {
    let runs = 0;
    const timer = store.startPolling('activity', () => { runs += 1; }, 30000);
    const tick = () => scheduler.active.get(timer).callback();

    tick();
    assert.equal(runs, 1, '可见时正常轮询');

    doc.hidden = true;
    tick();
    tick();
    assert.equal(runs, 1, '隐藏时不再发请求');

    doc.hidden = false;
    doc.listeners.get('visibilitychange')();
    assert.equal(runs, 2, '切回前台立刻补一次，不等满一个周期');

    store.stopPolling('activity');
    assert.equal(scheduler.active.size, 0);
    assert.equal(doc.listeners.has('visibilitychange'), false, 'visibility 监听器随轮询一起清理');
  } finally {
    delete globalThis.document;
  }
});

test('pauseWhenHidden false keeps upgrade progress polling in the background', () => {
  const scheduler = fakeScheduler();
  const doc = fakeDocument();
  const store = pollingStore(scheduler, doc);
  try {
    let runs = 0;
    const timer = store.startPolling('update-progress', () => { runs += 1; }, 800, {pauseWhenHidden: false});
    doc.hidden = true;
    scheduler.active.get(timer).callback();
    assert.equal(runs, 1, '升级进度切走也要继续拉，否则会错过重启窗口');
    assert.equal(doc.listeners.has('visibilitychange'), false);
  } finally {
    delete globalThis.document;
  }
});
