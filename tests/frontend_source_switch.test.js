const test = require('node:test');
const assert = require('node:assert/strict');
const {
  applyButtonState,
  episodeRange,
  historyLabel,
  previewMatches,
  quality,
  sortCandidates
} = require('../static/js/features/source-switch.js');

test('candidates are sorted by authoritative backend score', () => {
  const sorted = sortCandidates([
    {id: 'low', quality: {score: 40}},
    {id: 'high', quality: {score: 90}},
    {id: 'middle', quality: {score: 70}}
  ]);
  assert.deepEqual(sorted.map(item => item.id), ['high', 'middle', 'low']);
});

test('candidate quality and episode range have compatibility fallbacks', () => {
  assert.equal(quality({}).score, 0);
  assert.equal(episodeRange({quality: {episode_start: 4, episode_end: 12}}), 'E4–E12');
  assert.equal(episodeRange({quality: {episode_count: 3}}), '3 集');
  assert.equal(episodeRange({}), '集数未知');
});

test('preview must belong to the candidate and pass backend safety checks', () => {
  assert.equal(previewMatches({candidate: {id: 'a'}}, 'a'), true);
  assert.equal(previewMatches({candidate: {id: 'b'}}, 'a'), false);
});

test('apply stays clickable before preview so搜索结果可以直接应用', () => {
  const fresh = applyButtonState(null, 'a', '', '');
  assert.equal(fresh.disabled, false);
  assert.equal(fresh.label, '确认应用');
  assert.equal(fresh.force, false);

  const otherCandidate = applyButtonState({candidate: {id: 'b'}, season_matches: false}, 'a', '', '');
  assert.equal(otherCandidate.disabled, false);
});

test('apply button reflects force, probing, applying and season gate', () => {
  assert.deepEqual(
    applyButtonState({candidate: {id: 'a'}, probe_ok: true, can_apply: false, season_matches: true}, 'a', '', ''),
    {disabled: false, label: '强制应用', force: true}
  );
  assert.equal(applyButtonState({candidate: {id: 'a'}, probe_ok: true, can_apply: true}, 'a', '', '').label, '确认应用');
  assert.equal(applyButtonState(null, 'a', 'a', '').label, '应用中');
  assert.equal(applyButtonState(null, 'a', 'a', '').disabled, true);
  assert.equal(applyButtonState(null, 'a', '', 'a').label, '探测中');
  assert.equal(applyButtonState(null, 'a', '', 'a').disabled, true);

  const seasonBlocked = applyButtonState({candidate: {id: 'a'}, probe_ok: true, season_matches: false}, 'a', '', '');
  assert.equal(seasonBlocked.disabled, true);
  assert.equal(seasonBlocked.label, '季度不匹配');
  assert.equal(seasonBlocked.force, false);
});

test('preview ownership is matched by candidate id', () => {
  assert.equal(previewMatches({candidate: {id: 'a'}}, 'a'), true);
  assert.equal(previewMatches({candidate: {id: 'b'}}, 'a'), false);
  assert.equal(previewMatches(null, 'a'), false);
});

test('history labels distinguish automatic manual failure and rollback', () => {
  assert.equal(historyLabel({status: 'succeeded', automatic: true}), '自动换源');
  assert.equal(historyLabel({status: 'succeeded', automatic: false}), '手动换源');
  assert.equal(historyLabel({status: 'failed'}), '失败');
  assert.equal(historyLabel({status: 'rolled_back'}), '已回滚');
});
