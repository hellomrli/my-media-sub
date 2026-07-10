const test = require('node:test');
const assert = require('node:assert/strict');
const {
  canApplyPreview,
  episodeRange,
  historyLabel,
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
  assert.equal(canApplyPreview({can_apply: true, candidate: {id: 'a'}}, 'a'), true);
  assert.equal(canApplyPreview({can_apply: false, candidate: {id: 'a'}}, 'a'), false);
  assert.equal(canApplyPreview({can_apply: true, candidate: {id: 'b'}}, 'a'), false);
});

test('history labels distinguish automatic manual failure and rollback', () => {
  assert.equal(historyLabel({status: 'succeeded', automatic: true}), '自动换源');
  assert.equal(historyLabel({status: 'succeeded', automatic: false}), '手动换源');
  assert.equal(historyLabel({status: 'failed'}), '失败');
  assert.equal(historyLabel({status: 'rolled_back'}), '已回滚');
});
