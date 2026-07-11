const test = require('node:test');
const assert = require('node:assert/strict');
const tools = require('../static/js/features/automation-events.js');

test('automation event labels and tones are stable', () => {
  assert.equal(tools.stageLabel('cloud_transfer'), '云盘转存');
  assert.equal(tools.statusLabel('retrying'), '重试中');
  assert.equal(tools.statusTone('failed'), 'danger');
  assert.equal(tools.statusTone('succeeded'), 'success');
});

test('events are grouped by episode and retry is limited to terminal failures', () => {
  const groups = tools.episodeGroups([{episode: 2, id: 'b'}, {episode: 1, id: 'a'}, {episode: null}]);
  assert.deepEqual(groups.map(group => group.episode), [1, 2]);
  assert.equal(tools.canRetry({status: 'failed'}), true);
  assert.equal(tools.canRetry({status: 'running'}), false);
});

test('duration uses structured timestamps instead of notification text', () => {
  assert.equal(tools.duration({started_at: 100, finished_at: 165}), '1分5秒');
  assert.equal(tools.duration({}), '—');
});

test('automation timeline is chronological and bounded', () => {
  const events = [{id:'late',updated_at:3},{id:'early',updated_at:1},{id:'middle',updated_at:2}];
  assert.deepEqual(tools.timeline(events, 2).map(item => item.id), ['middle','late']);
});
