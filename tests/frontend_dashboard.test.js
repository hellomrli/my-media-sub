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
  // v2.2.19：更新日历并入工作台顶替订阅看板；快捷入口/自动化/最近活动的内容
  // 已被指标格与活动中心覆盖，旧 id 映射后应当消失而不是变成未知卡片。
  // v2.2.20 起夸克状态常驻侧边栏，cloud 卡片不再存在
  const app = state({settings: {dashboard_widgets: ['quick_actions', 'hero', 'kpis', 'library', 'operations']}});
  assert.deepEqual(app.dashboardLayout, ['command', 'kpis', 'calendar']);
  assert.deepEqual(app.dashboardCards('compact').map(card => card.id), ['command', 'kpis']);
  assert.deepEqual(app.dashboardCards('panel').map(card => card.id), ['calendar']);
  assert.equal(app.dashboardHiddenCards().length, 0);

  const legacyCloud = state({settings: {dashboard_widgets: ['cloud', 'calendar']}});
  assert.deepEqual(legacyCloud.dashboardLayout, ['calendar'], '已下线的 cloud 卡片静默丢弃');

  const empty = state({settings: {dashboard_widgets: []}});
  assert.equal(empty.dashboardLayout.length, 3, '空配置视为全部显示');

  const unknown = state({settings: {dashboard_widgets: ['calendar', 'not-a-card', 'calendar']}});
  assert.deepEqual(unknown.dashboardLayout, ['calendar'], '未知与重复 id 会被丢弃');
});

test('dashboard edit mode reorders, hides and restores cards on a draft', () => {
  const app = state({settings: {dashboard_widgets: ['command', 'kpis', 'calendar']}});
  app.startDashboardEdit();
  assert.deepEqual(app.dashboardLayoutDraft, ['command', 'kpis', 'calendar']);

  app.moveDashboardCard('calendar', -1);
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar', 'kpis'], '编辑态所见即所得');

  app.toggleDashboardCard('kpis');
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar']);
  assert.ok(app.dashboardHiddenCards().some(card => card.id === 'kpis'));
  app.toggleDashboardCard('kpis');
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar', 'kpis'], '再次点击恢复显示');

  app.moveDashboardCard('command', -1);
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar', 'kpis'], '越界移动不改顺序');

  app.cancelDashboardEdit();
  assert.deepEqual(app.dashboardLayout, ['command', 'kpis', 'calendar'], '取消后回到已保存布局');
});

test('dashboard watchlist keeps only subscriptions still airing', () => {
  const app = state({
    subscriptions: [
      {id: 'a', status: 'completed', last_checked_at: 300},
      {id: 'b', status: 'active', last_checked_at: 100},
      {id: 'c', status: 'invalid', last_checked_at: 200},
      {id: 'd', status: 'active', last_checked_at: 250}
    ]
  });
  assert.deepEqual(app.dashboardWatchlist.map(sub => sub.id), ['d', 'b']);
});

test('dashboard tiles stay muted while the counters are zero', () => {
  const calm = state({subscriptions: [{status: 'active'}], formatSpeed: () => '0 B/s'});
  const tones = Object.fromEntries(calm.dashboardTiles().map(tile => [tile.id, tile.tone]));
  assert.equal(tones.invalid, 'idle');
  assert.equal(tones.failed, 'idle');
  assert.equal(tones.unread, 'idle');

  const busy = state({
    subscriptions: [{status: 'invalid'}], jobs: [{status: 'failed'}],
    unreadNotifications: 3, formatSpeed: () => '1 MB/s'
  });
  const busyTones = Object.fromEntries(busy.dashboardTiles().map(tile => [tile.id, tile.tone]));
  assert.equal(busyTones.invalid, 'danger');
  assert.equal(busyTones.failed, 'danger');
  assert.equal(busyTones.unread, 'warning');
});
