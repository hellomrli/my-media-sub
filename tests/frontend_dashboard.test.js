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
  const app = state({settings: {dashboard_widgets: ['quick_actions', 'hero', 'kpis', 'library', 'operations']}});
  // hero → command，operations → cloud/automation/activity，顺序保持用户原有习惯
  assert.deepEqual(app.dashboardLayout, [
    'quick_actions', 'command', 'kpis', 'library', 'cloud', 'automation', 'activity'
  ]);
  assert.deepEqual(app.dashboardCards('compact').map(card => card.id), ['quick_actions', 'command', 'kpis']);
  assert.deepEqual(app.dashboardCards('panel').map(card => card.id), ['library', 'cloud', 'automation', 'activity']);
  assert.equal(app.dashboardHiddenCards().length, 0);

  const empty = state({settings: {dashboard_widgets: []}});
  assert.equal(empty.dashboardLayout.length, 7, '空配置视为全部显示');

  const unknown = state({settings: {dashboard_widgets: ['library', 'not-a-card', 'library']}});
  assert.deepEqual(unknown.dashboardLayout, ['library'], '未知与重复 id 会被丢弃');
});

test('dashboard edit mode reorders, hides and restores cards on a draft', () => {
  const app = state({settings: {dashboard_widgets: ['command', 'kpis', 'library']}});
  app.startDashboardEdit();
  assert.deepEqual(app.dashboardLayoutDraft, ['command', 'kpis', 'library']);

  app.moveDashboardCard('library', -1);
  assert.deepEqual(app.dashboardLayout, ['command', 'library', 'kpis'], '编辑态所见即所得');

  app.toggleDashboardCard('kpis');
  assert.deepEqual(app.dashboardLayout, ['command', 'library']);
  assert.ok(app.dashboardHiddenCards().some(card => card.id === 'kpis'));
  app.toggleDashboardCard('kpis');
  assert.deepEqual(app.dashboardLayout, ['command', 'library', 'kpis'], '再次点击恢复显示');

  // 越界移动不应改变顺序
  app.moveDashboardCard('command', -1);
  assert.deepEqual(app.dashboardLayout, ['command', 'library', 'kpis']);

  app.cancelDashboardEdit();
  assert.deepEqual(app.dashboardLayout, ['command', 'kpis', 'library'], '取消后回到已保存布局');
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
