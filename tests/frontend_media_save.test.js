const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

let apiDataStub = async () => ({});
let apiFetchStub = async () => ({ok: true, json: async () => ({data: []})});
global.MediaSubApi = {
  apiData: (...args) => apiDataStub(...args),
  apiFetch: (...args) => apiFetchStub(...args)
};
global.MediaSubFormatters = require('../static/js/core/formatters.js');
global.MediaSubSourceSwitch = require('../static/js/features/source-switch.js');
const subscriptions = require('../static/js/stores/subscriptions.js');

const files = [
  {file_key: 'hd', name: 'Movie.mkv', parent_path: '1080P', resolution: '1080p', size: 5 * 1024 ** 3},
  {file_key: 'uhd', name: 'Movie.mkv', parent_path: '4K', resolution: '2160p', size: 20 * 1024 ** 3},
  {file_key: 'cut', name: 'Movie.Directors.Cut.mkv', parent_path: '4K', resolution: '2160p', size: 24 * 1024 ** 3}
];
const qualities = [
  {resolution: '2160p', label: '4K / 2160P', file_count: 2, total_size: 44 * 1024 ** 3, sample_files: ['4K/Movie.mkv']},
  {resolution: '1080p', label: '1080P', file_count: 1, total_size: 5 * 1024 ** 3, sample_files: ['1080P/Movie.mkv']}
];

function store() {
  const state = subscriptions.createStore();
  state.settings = {
    quark_cookie_configured: true, custom_categories: [],
    quark_save_movie_dir: '/Movies', quark_save_series_dir: '/Shows',
    aria2_movie_dir: '/downloads/movies', aria2_rpc_url: 'http://aria2.test/jsonrpc'
  };
  state.toasts = [];
  state.showNotification = (type, message) => state.toasts.push({type, message});
  state.apiErrorMessage = (error, fallback) => error.message || fallback;
  state.loadJobs = async () => {};
  state.loadNotifications = async () => {};
  state.newSubscription.title = 'Movie';
  state.newSubscription.url = 'https://pan.quark.cn/s/movie';
  return state;
}

function readyMovie() {
  const state = store();
  state.subscriptionMode = 'continuous';
  state.setSubscriptionMediaType('movie');
  state.subscriptionResourcesKey = JSON.stringify([state.newSubscription.url, '']);
  state.subscriptionSeasonsDetected = true;
  state.subscriptionQualities = qualities;
  state.subscriptionMediaFiles = files;
  return state;
}

test.afterEach(() => {
  apiDataStub = async () => ({});
  apiFetchStub = async () => ({ok: true, json: async () => ({data: []})});
});

test('movie category switches tracking into save steps and switching back restores series mode', () => {
  const state = store();
  state.subscriptionMode = 'continuous';
  state.newSubscription.notify_only = true;
  state.subscriptionDialogTab = 'rename';
  state.setSubscriptionMediaType('movie');
  assert.equal(state.subscriptionMode, 'once');
  assert.equal(state.newSubscription.notify_only, false);
  assert.equal(state.newSubscription.sync_download_enabled, true, 'configured movies default to local download');
  assert.equal(state.subscriptionDialogTab, 'content');
  assert.deepEqual(state.subscriptionWizardSteps.map(step => step.id), ['content', 'download']);
  state.setSubscriptionMode('continuous');
  assert.equal(state.subscriptionMode, 'once');
  state.setSubscriptionMediaType('series');
  assert.equal(state.subscriptionMode, 'continuous');
  assert.equal(state.newSubscription.sync_download_enabled, false, 'movie downloads must not silently enable series downloads');
});

test('multiple resolutions require a deliberate choice and movies require one exact version', async () => {
  const state = readyMovie();
  assert.equal(await state.validateSubscriptionResourceSelection(), false);
  state.setSubscriptionQuality('2160p');
  assert.equal(state.movieResourceCandidates.length, 2);
  assert.equal(await state.validateSubscriptionResourceSelection(), false);
  state.newSubscription.movie_file_key = 'cut';
  assert.equal(await state.validateSubscriptionResourceSelection(), true);
  state.setSubscriptionQuality('1080p');
  assert.equal(state.selectedMovieResource.file_key, 'hd');
  assert.equal(await state.validateSubscriptionResourceSelection(), true);
});

test('movie save sends the chosen file and local download options without creating a subscription', async () => {
  const state = readyMovie();
  state.setSubscriptionQuality('1080p');
  state.newSubscription.sync_download_enabled = true;
  state.newSubscription.metadata = {title: 'Movie', release_date: '2024-01-01'};
  state.createContinuousSubscription = async () => assert.fail('movie must not create a subscription');
  let request;
  apiDataStub = async (url, options) => {
    assert.equal(url, '/api/transfer');
    request = JSON.parse(options.body);
    return {success: true, job_id: 'movie-save'};
  };
  await state.confirmSubscription();
  assert.equal(request.save_options.media_type, 'movie');
  assert.equal(request.save_options.preferred_resolution, '1080p');
  assert.deepEqual(request.save_options.file_ids, ['hd']);
  assert.equal(request.save_options.target_dir, '/Movies/Movie (2024)');
  assert.equal(request.save_options.sync_download_enabled, true);
  assert.equal(request.save_options.sync_download_dir, '/downloads/movies/Movie (2024)');
  assert.equal(request.save_options.sync_download_dir.includes('Season'), false);
  assert.equal(state.subscriptionSaving, false);
});

test('double clicking save cannot submit two transfer jobs', async () => {
  const state = readyMovie();
  state.setSubscriptionQuality('1080p');
  let resolve;
  let calls = 0;
  apiDataStub = async () => {
    calls += 1;
    return new Promise(done => { resolve = done; });
  };
  const first = state.confirmSubscription();
  await state.confirmSubscription();
  await Promise.resolve();
  assert.equal(calls, 1);
  resolve({success: true});
  await first;
  assert.equal(state.subscriptionSaving, false);
});

test('movie download uses Aria2 default directory when no movie root is configured', () => {
  const state = readyMovie();
  state.settings.aria2_movie_dir = '';
  assert.equal(state.resolvedSubscriptionAria2Dir(), '');
  state.settings.aria2_movie_dir = '/';
  assert.equal(state.resolvedSubscriptionAria2Dir(), '/Movie');
  state.newSubscription.sync_download_dir = '/custom/movie/';
  assert.equal(state.resolvedSubscriptionAria2Dir(), '/custom/movie');
});

test('finishing an older save cannot close a newly opened editor', async () => {
  const state = readyMovie();
  state.setSubscriptionQuality('1080p');
  state.showSubscriptionDialog = true;
  let resolve;
  apiDataStub = async () => new Promise(done => { resolve = done; });
  const saving = state.doOneTimeTransfer();
  state.showSubscriptionDialog = false;
  state.openBlankSubscriptionDialog();
  state.newSubscription.title = 'Next show';
  resolve({success: true});
  await saving;
  assert.equal(state.showSubscriptionDialog, true);
  assert.equal(state.newSubscription.title, 'Next show');
});

test('quality probe returns choices and invalidates exact movie selections when a URL changes', async () => {
  const state = readyMovie();
  state.setSubscriptionQuality('1080p');
  state.newSubscription.url = 'https://pan.quark.cn/s/new-movie';
  apiDataStub = async () => ({ok: true, qualities, media_files: [files[1]], seasons: [], partial: true});
  await state.detectSubscriptionSeasons();
  assert.equal(state.subscriptionProbePartial, true);
  assert.equal(state.selectedMovieResource, null);
  assert.equal(state.newSubscription.movie_file_key, '');
  assert.equal(state.newSubscription.preferred_resolution, '1080p');
});

test('an unusable share does not restore stale quality or movie file choices', async () => {
  const state = readyMovie();
  state.setSubscriptionQuality('1080p');
  apiDataStub = async () => ({ok: false, message: '提取码错误', qualities, media_files: files});
  await state.detectSubscriptionSeasons();
  assert.equal(state.subscriptionSeasonsDetected, false);
  assert.equal(state.subscriptionSeasonsError, '提取码错误');
  assert.equal(state.subscriptionQualities.length, 0);
  assert.equal(state.selectedMovieResource, null);
});

test('series save persists the selected resolution independently from duplicate handling', () => {
  const state = store();
  state.setSubscriptionQuality('1080p');
  state.newSubscription.duplicate_episode_strategy = 'highest_quality';
  const rules = state.buildSubscriptionRules();
  assert.equal(rules.preferred_resolution, '1080p');
  assert.equal(rules.duplicate_episode_strategy, 'highest_quality');
  state.subscriptionQualities = [qualities[1]];
  state.setSubscriptionQuality('');
  assert.equal(state.newSubscription.preferred_resolution, '', 'explicit auto selection must not be overwritten by the only detected resolution');
});

test('a wrong-quality source cannot enter force confirmation or be submitted', async () => {
  const state = store();
  state.sourceSwitchSubscriptionId = 'show';
  state.sourceSwitchPreview = {
    candidate: {id: 'wrong-quality'}, probe_ok: true, quality_matches: false,
    season_matches: true, can_apply: false
  };
  state.requestDangerConfirmation = async () => assert.fail('quality mismatch cannot be forced');
  apiDataStub = async () => assert.fail('quality mismatch cannot be applied');
  await state.applySourceCandidate({id: 'wrong-quality'});
  assert.match(state.sourceSwitchError, /画质/);
});

test('a movie matched by metadata cannot slip back into the continuous creation branch', async () => {
  const state = store();
  state.subscriptionMode = 'continuous';
  state.applyMagicTitleMatch = async () => state.setSubscriptionMediaType('movie');
  state.validateSubscriptionResourceSelection = async () => true;
  let saves = 0;
  state.doOneTimeTransfer = async () => { saves += 1; };
  apiFetchStub = async () => assert.fail('movie must not POST /api/subscriptions');
  await state.createContinuousSubscription();
  assert.equal(saves, 1);
});

test('new theme defaults to light while preserving an explicitly saved dark preference', () => {
  const source = fs.readFileSync(require.resolve('../static/js/theme-init.js'), 'utf8');
  for (const [stored, expected] of [[null, 'light'], ['dark', 'dark'], ['light', 'light']]) {
    const attributes = {};
    const classes = {};
    vm.runInNewContext(source, {
      localStorage: {getItem: () => stored},
      document: {documentElement: {
        setAttribute: (name, value) => { attributes[name] = value; },
        classList: {toggle: (name, value) => { classes[name] = value; }, remove: () => {}}
      }}
    });
    assert.equal(attributes['data-theme'], expected);
    assert.equal(classes.dark, expected === 'dark');
  }
});

test('legacy movie records use save labels instead of tracking or season completion labels', () => {
  const state = store();
  assert.equal(state.subscriptionStatusLabel({media_type: 'movie', status: 'active'}), '待保存');
  assert.equal(state.subscriptionStatusLabel({media_type: 'movie', status: 'completed'}), '已保存');
  assert.equal(state.subscriptionMenuActions({media_type: 'movie', status: 'completed'}).find(action => action.id === 'reopen').label, '恢复待保存');
});
