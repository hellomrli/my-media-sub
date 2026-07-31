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
test('safe JSON never throws and external URLs are scheme-constrained', () => {
  const cyclic = {}; cyclic.self = cyclic;
  assert.equal(ux.safeJson(cyclic), '{}');
  assert.equal(ux.safeExternalUrl('https://example.com/a b'), 'https://example.com/a%20b');
  assert.equal(ux.safeExternalUrl('javascript:alert(1)'), null);
  assert.equal(ux.safeExternalUrl('ftp://example.com'), null);
  assert.equal(ux.safeExternalUrl(42), null);
});
