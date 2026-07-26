const test = require('node:test');
const assert = require('node:assert/strict');

const {
  normalizeSettingsTab,
  normalizeRoute,
  routeFromSearch,
  routeUrl
} = require('../static/js/core/router.js');

const tabs = ['dashboard', 'calendar', 'search', 'drive', 'downloads', 'subscriptions', 'notifications', 'settings'];
const settingsTabs = ['quark', 'library', 'automation', 'naming', 'notifications', 'advanced', 'update'];

test('router normalizes legacy settings aliases and rejects unknown tabs', () => {
  assert.equal(normalizeSettingsTab('rules'), 'naming');
  // v2.2.18 拆分后 advanced 本身就是标签页；旧的 connections / maintenance 才需要映射
  assert.equal(normalizeSettingsTab('advanced'), 'advanced');
  assert.equal(normalizeSettingsTab('connections'), 'library');
  assert.equal(normalizeSettingsTab('basic'), 'library');
  assert.equal(normalizeSettingsTab('maintenance'), 'advanced');
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

test('router maps legacy background-log routes to the unified activity center', () => {
  assert.equal(normalizeRoute({tab: 'transferHistory'}, tabs, settingsTabs).tab, 'notifications');
  assert.equal(routeFromSearch('?tab=transferHistory', tabs, settingsTabs).tab, 'notifications');
  // Keep a rolling-upgrade caller with only the old tab list functional.
  assert.equal(normalizeRoute({tab: 'transferHistory'}, ['dashboard', 'transferHistory'], settingsTabs).tab, 'transferHistory');
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
