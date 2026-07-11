const test = require('node:test');
const assert = require('node:assert/strict');

const dashboard = require('../static/js/features/dashboard.js');

function state(overrides = {}) {
  return Object.assign(dashboard.createStore(), {
    subscriptions: [], jobs: [], unreadNotifications: 0,
    downloadStats: {speed: 0},
    subscriptionStatusKey: sub => sub.status || 'active',
    ...overrides
  });
}

test('dashboard summary reports actionable state instead of promotional copy', () => {
  const empty = state();
  assert.match(empty.dashboardStatusSummary(), /没有订阅/);

  const healthy = state({subscriptions: [{status: 'active'}, {status: 'completed'}]});
  assert.match(healthy.dashboardStatusSummary(), /当前没有失效订阅、失败任务或未读通知/);

  const attention = state({
    subscriptions: [{status: 'invalid'}],
    jobs: [{status: 'failed'}],
    unreadNotifications: 2
  });
  assert.equal(attention.dashboardAttentionCount, 4);
  assert.match(attention.dashboardStatusSummary(), /4 项状态需要处理/);
});

test('dashboard attention links set the relevant filter before navigation', () => {
  const calls = [];
  const app = state({
    setSubscriptionStatusTab: value => calls.push(['subscription', value]),
    selectTab: value => calls.push(['tab', value])
  });
  app.openDashboardAttention('subscriptions');
  app.openDashboardAttention('jobs');
  app.openDashboardAttention('notifications');
  assert.deepEqual(calls, [
    ['subscription', 'invalid'], ['tab', 'subscriptions'],
    ['tab', 'transferHistory'], ['tab', 'notifications']
  ]);
  assert.equal(app.backgroundJobFilterStatus, 'failed');
  assert.equal(app.notificationFilter, 'unread');
});
