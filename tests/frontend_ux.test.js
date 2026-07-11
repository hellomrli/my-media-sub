const test = require('node:test');
const assert = require('node:assert/strict');
const values = new Map();
global.localStorage = {getItem: key => values.has(key) ? values.get(key) : null, setItem: (key,value) => values.set(key,value)};
const ux = require('../static/js/core/ux.js');
test('UX preferences are namespaced, validated and resilient', () => {
  ux.writePreference('view', 'poster');
  assert.equal(ux.readPreference('view', 'table', ['table','poster']), 'poster');
  assert.equal(ux.readPreference('view', 'table', ['table']), 'table');
});
test('large lists are windowed and pooled work keeps every item', async () => {
  assert.deepEqual(ux.visibleWindow([1,2,3,4], 2), [1,2]);
  const output = await ux.runPool([1,2,3,4], 2, async value => value * 2);
  assert.deepEqual(output.sort((a,b)=>a-b), [2,4,6,8]);
});
test('timeline sorts events and safe JSON never throws', () => {
  assert.deepEqual(ux.timeline([{id:'b',updated_at:2},{id:'a',updated_at:1}]).map(x=>x.id), ['a','b']);
  const cyclic={}; cyclic.self=cyclic; assert.equal(ux.safeJson(cyclic), '{}');
});
