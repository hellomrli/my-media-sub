const test = require('node:test');
const assert = require('node:assert/strict');

const {
  formatBytes,
  formatDateTime,
  formatDuration,
  formatPercent,
  formatSpeed,
  parseTimestamp
} = require('../static/js/core/formatters.js');

test('timestamps accept Unix seconds, milliseconds and ISO strings', () => {
  assert.equal(parseTimestamp(1_700_000_000), 1_700_000_000_000);
  assert.equal(parseTimestamp(1_700_000_000_000), 1_700_000_000_000);
  assert.equal(parseTimestamp('2026-07-10T00:00:00Z'), Date.parse('2026-07-10T00:00:00Z'));
  assert.equal(parseTimestamp(''), 0);
  assert.notEqual(formatDateTime(1_700_000_000), '-');
});

test('bytes and speeds use consistent binary units', () => {
  assert.equal(formatBytes(1024), '1.00 KB');
  assert.equal(formatBytes(0), '-');
  assert.equal(formatSpeed(1024), '1.00 KB/s');
  assert.equal(formatSpeed(0), '0 B/s');
});

test('durations and percentages stay bounded and concise', () => {
  assert.equal(formatDuration(65), '1m 5s');
  assert.equal(formatDuration(3660), '1h 1m');
  assert.equal(formatDuration(0), '-');
  assert.equal(formatPercent(120), '100.0%');
  assert.equal(formatPercent(-3), '0.0%');
});
