const test = require('node:test');
const assert = require('node:assert/strict');

// subscriptions.js 在模块作用域一次性解构 MediaSubApi，桩必须在 require 之前就位。
// 默认桩返回空对象：未显式配置桩的用例里，偶发的后台请求保持无害。
let apiDataStub = async () => ({});
global.MediaSubApi = {apiData: (...args) => apiDataStub(...args)};

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
});

test('edit ignores the retired manual_schedule field on existing subscriptions', () => {
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
  // 手动排期已整体下线：回填时既不读取该字段，也不再往表单里塞任何排期状态。
  assert.equal(state.newSubscription.title, 'Show');
  assert.equal(
    Object.keys(state.newSubscription).some(key => key.startsWith('manual_schedule')),
    false
  );
});

test('parseSeasonSpec supports ranges and multi-season target dirs skip Season suffix', () => {
  const state = store();
  state.settings = {
    subscription_check_interval_minutes: 60,
    quark_save_series_dir: '/连续剧',
    default_rename_template: ''
  };
  assert.deepEqual(state.parseSeasonSpec('1-4'), {start: 1, end: 4, label: '1-4', season_spec: '1-4', multi_season: true, seasons: [1, 2, 3, 4]});
  assert.deepEqual(state.parseSeasonSpec('2'), {start: 2, end: null, label: '2', season_spec: '2', multi_season: false, seasons: [2]});
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
  assert.equal(state.inferSubscriptionTitle('凡人修仙传 4K 高码率'), '凡人修仙传');
});

test('magic title matching waits for the cleaned title before TMDB lookup', async () => {
  const state = store();
  state.newSubscription.title = '凡人修仙传 4K 高码率';
  state.normalizeTitleRemote = async original => ({original, normalized: '凡人修仙传', changed: true});
  state.metadataSearchAvailable = () => true;
  let searchedTitle = '';
  let searchFinished = false;
  state.searchMetadataForSubscription = async () => {
    searchedTitle = state.newSubscription.title;
    await Promise.resolve();
    searchFinished = true;
  };

  await state.applyMagicTitleMatch({silent: true});
  assert.equal(state.newSubscription.title, '凡人修仙传');
  assert.equal(searchedTitle, '凡人修仙传');
  assert.equal(searchFinished, true);
});

test('Aria2 directory preview preserves explicit seasons and marks dynamic multi-season paths', () => {
  const state = store();
  state.settings = {aria2_series_dir: '/downloads/series'};
  state.newSubscription.media_type = 'series';
  state.newSubscription.title = '凡人修仙传';
  state.newSubscription.season_input = '1-4';
  state.newSubscription.sync_download_dir = '/downloads/custom/凡人修仙传';
  assert.equal(
    state.resolvedSubscriptionAria2Dir(),
    '/downloads/custom/凡人修仙传/Season N（按文件识别）'
  );
  state.newSubscription.sync_download_dir = '/downloads/custom/凡人修仙传/Season 2';
  assert.equal(
    state.resolvedSubscriptionAria2Dir(),
    '/downloads/custom/凡人修仙传/Season 2'
  );
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

test('season detection selection supports skip-season sets', () => {
  const state = store();
  state.newSubscription.url = 'https://pan.quark.cn/s/multi';
  state.subscriptionSeasons = [
    {season: 1, file_count: 12, episodes: [1, 2], sample_files: ['S01E01.mkv']},
    {season: 2, file_count: 8, episodes: [1], sample_files: ['S02E01.mkv']},
    {season: 3, file_count: 5, episodes: [1], sample_files: ['S03E01.mkv']}
  ];
  state.subscriptionSeasonsDetected = true;
  state.subscriptionSeasonsMessage = '检测到 S01、S02、S03';
  state.applyDefaultSeasonSelection();
  // 当前 season=1 且 season_end 为空：只预勾选 S1
  assert.deepEqual(state.selectedSubscriptionSeasons, [1]);

  // 勾选 S3：集合语义只选 S1+S3（跳季，不补 S2），回写 season_input/season_list
  state.toggleSubscriptionSeason(3);
  assert.deepEqual(state.selectedSubscriptionSeasons, [1, 3]);
  assert.equal(state.newSubscription.season, 1);
  assert.equal(state.newSubscription.season_end, 3);
  assert.equal(state.newSubscription.season_input, '1,3');
  assert.deepEqual(state.newSubscription.season_list, [1, 3]);

  // seasonPayload：跳季集合透传 season_list
  const payload = state.seasonPayload();
  assert.deepEqual(payload.season_list, [1, 3]);
  assert.equal(payload.season, 1);
  assert.equal(payload.season_end, 3);

  // 连续勾选 S2：集合折叠为区间语义
  state.toggleSubscriptionSeason(2);
  assert.deepEqual(state.selectedSubscriptionSeasons, [1, 2, 3]);
  assert.equal(state.newSubscription.season_input, '1-3');
  assert.equal(state.newSubscription.season_list, null);

  // 取消 S3 与 S2：回到单季
  state.toggleSubscriptionSeason(3);
  state.toggleSubscriptionSeason(2);
  assert.deepEqual(state.selectedSubscriptionSeasons, [1]);
  assert.equal(state.newSubscription.season, 1);
  assert.equal(state.newSubscription.season_end, null);
  assert.equal(state.newSubscription.season_input, '1');
  assert.equal(state.newSubscription.season_list, null);

  // 全部取消：保留手填值不动
  state.newSubscription.season_input = '2';
  state.newSubscription.season = 2;
  state.toggleSubscriptionSeason(1);
  assert.deepEqual(state.selectedSubscriptionSeasons, []);
  assert.equal(state.newSubscription.season_input, '2');
});

test('single season detection is pre-selected without user action', () => {
  const state = store();
  state.newSubscription.season = 1;
  state.newSubscription.season_end = null;
  state.subscriptionSeasons = [
    {season: 1, file_count: 12, episodes: [1, 2], sample_files: ['S01E01.mkv']}
  ];
  state.subscriptionSeasonsDetected = true;
  state.applyDefaultSeasonSelection();

  // 唯一季默认勾选，无需用户操作
  assert.deepEqual(state.selectedSubscriptionSeasons, [1]);
});

test('unmarked share is detected as an inferred season one and pre-selected', async () => {
  // 资源不标季度（文件名只有 01/02）时后端按第一季处理并回传 inferred：
  // 前端要透出该标记（UI 据此提示用户核对）并自动勾选 S1，省掉手填季号。
  apiDataStub = async (url) => {
    assert.equal(url, '/api/subscriptions/seasons');
    return {
      seasons: [
        {season: 1, file_count: 3, episodes: [1, 2, 3], sample_files: ['01.mkv'], inferred: true}
      ],
      message: '未检测到季度标记，已按第一季处理（共 3 个视频文件）',
      unspecified_file_count: 0,
      total_file_count: 3
    };
  };
  const state = store();
  state.newSubscription.url = 'https://pan.quark.cn/s/plain';
  await state.detectSubscriptionSeasons();
  apiDataStub = async () => ({});

  assert.equal(state.subscriptionSeasonsDetected, true);
  assert.equal(state.subscriptionSeasonsInferred, true);
  assert.equal(state.subscriptionSeasonsUnspecified, 0);
  // 唯一季（推断出的第一季）同样自动勾选
  assert.deepEqual(state.selectedSubscriptionSeasons, [1]);
});

test('explicitly marked seasons are not flagged as inferred', () => {
  const state = store();
  state.subscriptionSeasons = [
    {season: 1, file_count: 2, episodes: [1, 2], sample_files: ['S01E01.mkv'], inferred: false}
  ];
  state.subscriptionSeasonsInferred = state.subscriptionSeasons.some(item => item && item.inferred);
  assert.equal(state.subscriptionSeasonsInferred, false);
});

test('editing an unrelated field preserves the skip-season set', () => {
  // 回归：已存 [1,3] 的订阅重开编辑时输入框显示 '1,3'；
  // 只改标题等其他字段保存，season_list 不得静默退化为 1-3 全区间。
  const state = store();
  state.settings = {subscription_check_interval_minutes: 60};
  state.previewSubscriptionRename = async () => {};
  state.showNotification = () => {};
  state.apiErrorMessage = (_e, fallback) => fallback;
  state.detectSubscriptionSeasons = async () => {};
  state.openEditSubscriptionDialog({
    id: 'sub-roundtrip',
    title: 'Show',
    url: 'https://pan.quark.cn/s/x',
    password: '',
    media_type: 'series',
    season: 1,
    season_end: 3,
    season_list: [1, 3],
    rules: {}
  });
  assert.equal(state.newSubscription.season_input, '1,3');

  const payload = state.seasonPayload();
  assert.deepEqual(payload.season_list, [1, 3]);
  assert.equal(payload.season, 1);
  assert.equal(payload.season_end, 3);
  assert.equal(payload.season_spec, '1,3');
});

test('edit dialog restores stored skip-season set', () => {
  const state = store();
  state.settings = {subscription_check_interval_minutes: 60};
  state.previewSubscriptionRename = async () => {};
  state.showNotification = () => {};
  state.apiErrorMessage = (_e, fallback) => fallback;
  state.detectSubscriptionSeasons = async () => {};
  state.openEditSubscriptionDialog({
    id: 'sub-skip',
    title: 'Show',
    url: 'https://pan.quark.cn/s/x',
    password: '',
    media_type: 'series',
    season: 1,
    season_end: 3,
    season_list: [1, 3],
    rules: {}
  });

  // 编辑回填：season_input 显示跳季语法（'1,3'），集合原样保留
  assert.equal(state.newSubscription.season_input, '1,3');
  assert.deepEqual(state.newSubscription.season_list, [1, 3]);

  // 探测结果返回后按已存集合精确预勾选（不把 S2 误勾上）
  state.subscriptionSeasons = [
    {season: 1, file_count: 12, episodes: [1], sample_files: []},
    {season: 2, file_count: 8, episodes: [1], sample_files: []},
    {season: 3, file_count: 5, episodes: [1], sample_files: []}
  ];
  state.subscriptionSeasonsDetected = true;
  state.applyDefaultSeasonSelection();
  assert.deepEqual(state.selectedSubscriptionSeasons, [1, 3]);
});
