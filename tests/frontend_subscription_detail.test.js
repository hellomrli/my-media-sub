const test = require('node:test');
const assert = require('node:assert/strict');

const {
  activityTone,
  buildSubscriptionActivity,
  episodeStage,
  episodeStageLabel,
  filterEpisodes
} = require('../static/js/features/subscription-detail.js');

test('episode stages follow the automation progression', () => {
  assert.equal(episodeStage({missing: true}), 'missing');
  assert.equal(episodeStage({discovered: true, transferred: false}), 'discovered');
  assert.equal(episodeStage({discovered: true, transferred: true, download_status: 'pending'}), 'transferred');
  assert.equal(episodeStage({transferred: true, strm_status: 'generated'}), 'strm');
  assert.equal(episodeStage({transferred: true, download_status: 'completed'}), 'downloaded');
  assert.equal(episodeStage({transferred: true, download_status: 'completed', strm_status: 'generated'}), 'complete');
  assert.equal(episodeStageLabel({missing: true}), '缺集');
});

test('episode filters expose missing, pending, ready and recent groups', () => {
  const episodes = [
    {episode: 1, discovered: true, transferred: true, download_status: 'completed', strm_status: 'generated'},
    {episode: 2, discovered: true, transferred: false, download_status: 'not_started', strm_status: 'not_started', recent: true},
    {episode: 3, missing: true},
    {episode: 4, discovered: true, transferred: true, download_status: 'queued', strm_status: 'failed'}
  ];
  assert.deepEqual(filterEpisodes(episodes, 'missing').map(item => item.episode), [3]);
  assert.deepEqual(filterEpisodes(episodes, 'pending').map(item => item.episode), [2, 4]);
  assert.deepEqual(filterEpisodes(episodes, 'ready').map(item => item.episode), [1]);
  assert.deepEqual(filterEpisodes(episodes, 'recent').map(item => item.episode), [2]);
});

test('activity combines jobs, notifications and checks in time order', () => {
  const activity = buildSubscriptionActivity({
    subscription: {check_history: [{time: 10, state: 'success', summary: '检查完成'}]},
    recent_jobs: [{id: 'j1', title: '转存', status: 'running', updated_at: 30}],
    recent_notifications: [{id: 'n1', title: '已发现更新', level: 'info', created_at: 20}]
  });
  assert.deepEqual(activity.map(item => item.kind), ['job', 'notification', 'check']);
  assert.equal(activityTone(activity[0]), 'active');
  assert.equal(activityTone({status: 'failed'}), 'error');
});
