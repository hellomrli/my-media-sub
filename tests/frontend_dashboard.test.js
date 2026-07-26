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
    ['tab', 'notifications'], ['tab', 'notifications']
  ]);
  assert.equal(app.backgroundJobFilterStatus, 'failed');
  assert.equal(app.notificationFilter, 'unread');
});

test('dashboard cards keep legacy layouts working and stay ordered', () => {
  // v2.2.21：指标行移除（kpis 映射为空），现役卡片只剩概览与日历；
  // 旧 id hero→command、library→calendar，其余静默丢弃。
  const app = state({settings: {dashboard_widgets: ['quick_actions', 'hero', 'kpis', 'library', 'operations']}});
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar']);
  assert.deepEqual(app.dashboardCards('compact').map(card => card.id), ['command']);
  assert.deepEqual(app.dashboardCards('panel').map(card => card.id), ['calendar']);
  assert.equal(app.dashboardHiddenCards().length, 0);

  const empty = state({settings: {dashboard_widgets: []}});
  assert.equal(empty.dashboardLayout.length, 2, '空配置视为全部显示');

  const unknown = state({settings: {dashboard_widgets: ['calendar', 'not-a-card', 'calendar']}});
  assert.deepEqual(unknown.dashboardLayout, ['calendar'], '未知与重复 id 会被丢弃');
});

test('dashboard edit mode reorders, hides and restores cards on a draft', () => {
  const app = state({settings: {dashboard_widgets: ['command', 'calendar']}});
  app.startDashboardEdit();
  assert.deepEqual(app.dashboardLayoutDraft, ['command', 'calendar']);

  app.moveDashboardCard('calendar', -1);
  assert.deepEqual(app.dashboardLayout, ['calendar', 'command'], '编辑态所见即所得');

  app.toggleDashboardCard('command');
  assert.deepEqual(app.dashboardLayout, ['calendar']);
  assert.ok(app.dashboardHiddenCards().some(card => card.id === 'command'));
  app.toggleDashboardCard('command');
  assert.deepEqual(app.dashboardLayout, ['calendar', 'command'], '再次点击恢复显示');

  app.moveDashboardCard('calendar', -1);
  assert.deepEqual(app.dashboardLayout, ['calendar', 'command'], '越界移动不改顺序');

  app.cancelDashboardEdit();
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar'], '取消后回到已保存布局');
});
