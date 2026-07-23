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
  assert.deepEqual(state.subscriptionWizardSteps.map(step => step.name), ['订阅内容', '高级规则', '下载']);
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

test('parseSeasonSpec supports ranges and multi-season target dirs skip Season suffix', () => {
  const state = store();
  state.settings = {
    subscription_check_interval_minutes: 60,
    quark_save_series_dir: '/连续剧',
    default_rename_template: ''
  };
  assert.deepEqual(state.parseSeasonSpec('1-4'), {start: 1, end: 4, label: '1-4', season_spec: '1-4', multi_season: true});
  assert.deepEqual(state.parseSeasonSpec('2'), {start: 2, end: null, label: '2', season_spec: '2', multi_season: false});
  state.newSubscription.media_type = 'series';
  state.newSubscription.season_input = '1-4';
  state.newSubscription.title = '庆余年';
  state.newSubscription.custom_dir = false;
  const multiDir = state.getDefaultTargetDir();
  assert.equal(multiDir.includes('Season'), false);
  state.newSubscription.season_input = '2';
  const singleDir = state.getDefaultTargetDir();
  assert.match(singleDir, /Season 2$/);
});

test('rename preview groups multi-season items into collapsible Season sections', () => {
  const state = store();
  state.newSubscription.media_type = 'series';
  state.newSubscription.season_input = '1-3';
  state.renamePreviewScope = 'all';
  state.renamePreview = {items: [
    {source_name: 'S01E01.mkv', source_parent_path: 'Season 1', season: 1, action: 'transfer', target_dir: '/show/Season 1', target_name: 'A.S01E01.mkv'},
    {source_name: 'S02E01.mkv', source_parent_path: 'Season 2', season: 2, action: 'transfer', target_dir: '/show/Season 2', target_name: 'A.S02E01.mkv'},
    {source_name: 'S02E02.mkv', source_parent_path: 'Season 2', season: 2, action: 'skip', skip_reason: '已转存', target_dir: '/show/Season 2', target_name: 'A.S02E02.mkv'},
    {source_name: 'extra.mkv', source_parent_path: '', season: null, action: 'skip', skip_reason: '多季订阅无法判定季号', target_name: 'extra.mkv'}
  ]};
  assert.equal(state.shouldGroupRenamePreviewBySeason(), true);
  const groups = state.groupedRenamePreviewSeasons();
  assert.deepEqual(groups.map(group => group.label), ['Season 1', 'Season 2', '未识别季']);
  assert.equal(groups[0].transferCount, 1);
  assert.equal(groups[1].items.length, 2);
  state.collapseAllRenamePreviewSeasons();
  assert.equal(state.isRenamePreviewSeasonCollapsed('1'), true);
  state.toggleRenamePreviewSeason('1');
  assert.equal(state.isRenamePreviewSeasonCollapsed('1'), false);
});

test('inferSubscriptionTitle strips fan-sub noise for metadata matching', () => {
  const state = store();
  assert.equal(state.inferSubscriptionTitle('【字幕组】庆余年 1080p S01-S04 全集'), '庆余年');
  assert.equal(state.inferSubscriptionTitle('庆余年（2024）[简中]'), '庆余年');
  assert.equal(state.inferSubscriptionTitle('🗄 庆余年'), '庆余年');
  assert.equal(state.inferSubscriptionTitle('📺庆余年 1080p'), '庆余年');
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
