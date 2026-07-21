const test = require('node:test');
const assert = require('node:assert/strict');

const subscriptions = require('../static/js/stores/subscriptions.js');

function store() {
  return subscriptions.createStore();
}

test('persisted completion remains authoritative when display progress lags', () => {
  const state = store();
  const completed = {
    status: 'completed',
    completed: true,
    current_episode_number: 11,
    total_episode_number: 12,
    rules: {finish_after_episode: 12}
  };
  assert.equal(state.subscriptionStatusKey(completed), 'completed');
  assert.equal(state.subscriptionStatusLabel(completed), '已完结');
});

test('invalid state takes precedence and plain active subscriptions stay active', () => {
  const state = store();
  assert.equal(state.subscriptionStatusKey({status: 'completed', completed: true, invalid_since: 1}), 'invalid');
  assert.equal(state.subscriptionStatusKey({status: 'active', completed: false}), 'active');
});

test('subscription wizard defaults to automatic scheduling and omits the schedule step', () => {
  const state = store();
  state.subscriptionMode = 'continuous';
  assert.deepEqual(state.subscriptionWizardSteps.map(step => step.id), ['content', 'rename', 'download']);
  assert.equal(state.manualSchedulePayload(), null);
});

test('edit preserves existing manual schedule and omits null payload when disabled without schedule', () => {
  const state = store();
  state.settings = {subscription_check_interval_minutes: 60};
  state.previewSubscriptionRename = async () => {};
  state.showNotification = () => {};
  state.apiErrorMessage = (_e, fallback) => fallback;
  state.openEditSubscriptionDialog({
    id: 'sub-1',
    title: 'Show',
    url: 'https://pan.quark.cn/s/x',
    password: '',
    media_type: 'series',
    season: 1,
    rules: {},
    manual_schedule: {
      start_date: '2026-01-01',
      weekdays: [1, 4],
      air_time: '20:00',
      interval_weeks: 1,
      first_episode_number: 1,
      total_episodes: 12
    }
  });
  assert.equal(state.newSubscription.manual_schedule_enabled, true);
  assert.equal(state.newSubscription.manual_schedule_start_date, '2026-01-01');
  assert.deepEqual(state.manualSchedulePayload().weekdays, [1, 4]);
  assert.equal(state.shouldSendManualSchedule(), true);

  // 关闭排期开关后不再发送字段，避免 PUT null 清空服务端排期
  state.newSubscription.manual_schedule_enabled = false;
  assert.equal(state.shouldSendManualSchedule(), false);
  assert.equal(state.manualSchedulePayload(), null);
});

test('buildSubscriptionRules keeps per-subscription check interval', () => {
  const state = store();
  state.settings = {subscription_check_interval_minutes: 60};
  state.newSubscription.check_interval_minutes = 15;
  state.newSubscription.custom_dir = false;
  state.newSubscription.media_type = 'series';
  state.newSubscription.season = 1;
  state.newSubscription.title = 'Show';
  const rules = state.buildSubscriptionRules();
  assert.equal(rules.check_interval_minutes, 15);
});

test('rename preview preserves nested probe paths and defaults to eligible files', () => {
  const state = store();
  const result = {probe_info: {files: [
    {name: 'Season 3', is_dir: true, parent_path: ''},
    {name: 'Show.S03E01.mkv', fid: 'episode-1', is_dir: false, parent_path: '合集/Season 3', size: 10}
  ]}};
  assert.equal(state.sampleFilesFromSearchResult(result), '合集/Season 3/Show.S03E01.mkv');
  assert.deepEqual(state.previewFilesFromSearchResult(result), [{
    name: 'Show.S03E01.mkv', fid: 'episode-1', is_dir: false, size: 10,
    parent_path: '合集/Season 3', updated_at: null
  }]);

  state.renamePreview = {items: [
    {source_name: 'Show.S03E01.mkv', source_parent_path: '合集/Season 3', action: 'transfer'},
    {source_name: 'Show.S02E01.mkv', source_parent_path: '合集/Season 2', action: 'skip'},
    {source_name: 'Season 4', source_parent_path: '合集', action: 'skip', skip_reason: '目录暂不规划转存'}
  ]};
  assert.equal(state.visibleRenamePreviewItems().length, 1);
  assert.equal(state.renamePreviewSourceLabel(state.renamePreview.items[0]), '合集/Season 3/Show.S03E01.mkv');
  state.renamePreviewScope = 'all';
  assert.equal(state.visibleRenamePreviewItems().length, 2);
});
