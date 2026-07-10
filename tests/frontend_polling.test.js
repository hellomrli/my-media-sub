const test = require('node:test');
const assert = require('node:assert/strict');

const {createPollingRegistry} = require('../static/js/core/polling.js');

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
