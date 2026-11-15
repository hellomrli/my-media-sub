const test = require('node:test');
const assert = require('node:assert/strict');

// 与页面加载顺序一致：工作台复用活动中心「哪些任务可见」的口径。
const notifications = require('../static/js/core/notifications.js');
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

test('dashboard attention entries count each category separately and skip empty ones', () => {
  const app = state({
    subscriptions: [{status: 'invalid'}, {status: 'invalid'}, {status: 'active'}],
    jobs: [{status: 'failed'}, {status: 'running'}],
    unreadNotifications: 0
  });
  assert.deepEqual(app.dashboardAttentionItems.map(item => [item.id, item.count]), [['invalid', 2], ['failed', 1]]);
  assert.equal(
    app.dashboardAttentionItems.reduce((sum, item) => sum + item.count, 0),
    app.dashboardAttentionCount,
    '分项之和必须等于摘要里的总数'
  );
  assert.deepEqual(state().dashboardAttentionItems, []);
});

test('dashboard failed-job count matches what the activity centre can show', () => {
  const jobs = [
    {id: 'a', kind: 'subscription_transfer', status: 'failed'},
    {id: 'b', kind: 'push_dispatch', status: 'failed'},
    {id: 'c', kind: 'metadata_scrape', status: 'succeeded'}
  ];
  const app = state({subscriptions: [{status: 'active'}], jobs});
  const listed = notifications.mergeActivityItems(jobs, [])
    .filter(item => item.source === 'job' && item.status === 'failed');
  assert.deepEqual(listed.map(item => item.raw.id), ['a'], '推送派发任务不在活动中心展示');
  assert.equal(app.dashboardStats.failedJobs, listed.length, '计入的失败任务必须能在落地页找到');
  assert.deepEqual(app.dashboardAttentionItems.map(item => [item.id, item.count]), [['failed', 1]]);
});

test('dashboard attention entries open the page that lists the problem, already filtered', () => {
  const calls = [];
  const app = state({
    activityFilter: 'jobs', activityQuery: '旧关键词', activityVisibleLimit: 300,
    setSubscriptionStatusTab(value) { calls.push(['status', value]); },
    selectTab(value) { calls.push(['tab', value]); }
  });

  app.openDashboardAttention('invalid');
  assert.deepEqual(calls, [['status', 'invalid'], ['tab', 'subscriptions']]);

  calls.length = 0;
  app.openDashboardAttention('failed');
  assert.deepEqual(calls, [['tab', 'notifications']]);
  assert.equal(app.activityFilter, 'failed');
  assert.equal(app.activityQuery, '', '残留的搜索词会把目标条目滤掉');
  assert.equal(app.activityVisibleLimit, 100);

  app.openDashboardAttention('unread');
  assert.equal(app.activityFilter, 'unread');

  calls.length = 0;
  app.openDashboardAttention('unknown');
  assert.deepEqual(calls, []);
  assert.equal(app.activityFilter, 'unread');
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

  const retired = state({settings: {dashboard_widgets: ['kpis', 'operations']}});
  assert.deepEqual(retired.dashboardLayout, ['command', 'calendar'], '只剩已下线卡片的旧布局回到默认');
});

test('dashboard edit mode hides and restores cards on a draft', () => {
  const app = state({settings: {dashboard_widgets: ['command', 'calendar']}});
  app.startDashboardEdit();
  assert.deepEqual(app.dashboardLayoutDraft, ['command', 'calendar']);

  app.toggleDashboardCard('command');
  assert.deepEqual(app.dashboardLayout, ['calendar'], '编辑态所见即所得');
  assert.ok(app.dashboardHiddenCards().some(card => card.id === 'command'));

  app.toggleDashboardCard('command');
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar'], '恢复时回到目录顺序，而不是追加到末尾');

  app.toggleDashboardCard('calendar');
  app.cancelDashboardEdit();
  assert.deepEqual(app.dashboardLayout, ['command', 'calendar'], '取消后回到已保存布局');
});

test('dashboard keeps at least one card because an empty layout cannot be persisted', async () => {
  const saved = [];
  const app = state({
    settings: {dashboard_widgets: ['command', 'calendar']},
    async saveSettings() { saved.push([...this.settings.dashboard_widgets]); }
  });
  app.startDashboardEdit();
  assert.equal(app.canHideDashboardCard('command'), true);

  app.toggleDashboardCard('command');
  assert.equal(app.canHideDashboardCard('calendar'), false, '最后一张卡片不可隐藏');
  assert.equal(app.canHideDashboardCard('command'), false, '已隐藏的卡片没有隐藏操作');

  app.toggleDashboardCard('calendar');
  assert.deepEqual(app.dashboardLayoutDraft, ['calendar'], '隐藏最后一张卡片是空操作');
  assert.deepEqual(app.dashboardLayout, ['calendar'], '草稿不会变空后被当作「全部显示」');

  await app.saveDashboardLayout();
  assert.deepEqual(saved, [['calendar']]);
  assert.equal(app.dashboardEditing, false);
  assert.deepEqual(app.dashboardLayout, ['calendar'], '保存后的布局与编辑时看到的一致');
});
