const test = require('node:test');
const assert = require('node:assert/strict');

const {
  normalizeSettingsTab,
  normalizeRoute,
  routeFromSearch,
  routeUrl
} = require('../static/js/core/router.js');

const tabs = ['dashboard', 'calendar', 'search', 'drive', 'downloads', 'subscriptions', 'transferHistory', 'notifications', 'settings'];
const settingsTabs = ['connections', 'automation', 'naming', 'notifications', 'maintenance'];

test('router normalizes legacy settings aliases and rejects unknown tabs', () => {
  assert.equal(normalizeSettingsTab('rules'), 'naming');
  assert.equal(normalizeSettingsTab('advanced'), 'maintenance');
  assert.deepEqual(normalizeRoute({tab: 'missing', settingsTab: 'push'}, tabs, settingsTabs), {
    appRoute: true,
    tab: 'dashboard',
    settingsTab: 'notifications',
    subscriptionId: ''
  });
});

test('router parses subscription details only on the subscriptions page', () => {
  assert.deepEqual(
    routeFromSearch('?tab=subscriptions&subscription=sub-1&settings=rules', tabs, settingsTabs),
    {appRoute: true, tab: 'subscriptions', settingsTab: 'naming', subscriptionId: 'sub-1'}
  );
  assert.equal(
    routeFromSearch('?tab=dashboard&subscription=sub-1', tabs, settingsTabs).subscriptionId,
    ''
  );
});

test('router serializes route state without dropping unrelated query params or hashes', () => {
  assert.equal(
    routeUrl('https://example.test/app?lang=zh&settings=naming#section', {
      tab: 'subscriptions', settingsTab: 'connections', subscriptionId: 'a/b'
    }, tabs, settingsTabs),
    '/app?lang=zh&tab=subscriptions&subscription=a%2Fb#section'
  );
  assert.equal(
    routeUrl('https://example.test/app?lang=zh&subscription=old#section', {
      tab: 'settings', settingsTab: 'rules'
    }, tabs, settingsTabs),
    '/app?lang=zh&tab=settings&settings=naming#section'
  );
});
