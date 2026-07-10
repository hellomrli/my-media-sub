const test = require('node:test');
const assert = require('node:assert/strict');

const calendar = require('../static/js/features/calendar.js');

test('week range uses Monday through Sunday across year boundary', () => {
  assert.deepEqual(calendar.viewRange('week', '2027-01-01'), {
    from: '2026-12-28',
    to: '2027-01-03'
  });
});

test('month cells are Monday-based and always contain six weeks', () => {
  const cells = calendar.monthCells('2026-07-10', [{scheduled_date: '2026-07-10', id: 'a'}], '2026-07-10');
  assert.equal(cells.length, 42);
  assert.equal(cells[0].key, '2026-06-29');
  assert.equal(cells.find(cell => cell.key === '2026-07-10').items.length, 1);
  assert.equal(cells.find(cell => cell.key === '2026-07-10').isToday, true);
});

test('cursor shifts by view scale', () => {
  assert.equal(calendar.shiftCursor('2026-07-10', 'week', 1), '2026-07-17');
  assert.equal(calendar.shiftCursor('2026-07-31', 'month', 1), '2026-08-31');
  assert.equal(calendar.shiftCursor('2026-07-10', 'list', -1), '2026-06-10');
});

test('list groups put unknown schedule last', () => {
  const groups = calendar.listGroups([
    {scheduled_date: null, id: 'unknown'},
    {scheduled_date: '2026-07-11', id: 'b'},
    {scheduled_date: '2026-07-10', id: 'a'}
  ]);
  assert.deepEqual(groups.map(group => group.key), ['2026-07-10', '2026-07-11', 'unknown']);
});

test('labels expose stable Chinese presentation', () => {
  assert.equal(calendar.statusLabel('completed_missing'), '完结缺集');
  assert.equal(calendar.sourceLabel('inferred_cadence'), '周期推断');
  assert.equal(calendar.confidenceLabel('low'), '推断');
});
