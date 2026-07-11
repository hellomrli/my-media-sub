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
