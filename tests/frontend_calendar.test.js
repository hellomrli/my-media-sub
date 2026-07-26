const test = require('node:test');
const assert = require('node:assert/strict');

const calendar = require('../static/js/features/calendar.js');

test('week range uses Sunday through Saturday across year boundary', () => {
  // 2027-01-01 是周五，所在周从 2026-12-27（周日）到 2027-01-02（周六）
  assert.deepEqual(calendar.viewRange('week', '2027-01-01'), {
    from: '2026-12-27',
    to: '2027-01-02'
  });
});

test('month cells are Sunday-based and always contain six weeks', () => {
  const cells = calendar.monthCells('2026-07-10', [{scheduled_date: '2026-07-10', id: 'a'}], '2026-07-10');
  assert.equal(cells.length, 42);
  // 2026-07-01 是周三，网格从上一个周日 2026-06-28 开始
  assert.equal(cells[0].key, '2026-06-28');
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
