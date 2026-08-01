const test = require('node:test');
const assert = require('node:assert/strict');

const calendar = require('../static/js/features/calendar.js');
const calendarPage = require('../static/js/features/calendar-page.js');

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

test('calendar progress and stale-source reminders use the highest persisted episodes', () => {
  assert.equal(calendar.transferProgressLabel({media_type: 'series', latest_transferred_episode: 6}), '最高已转存 E6');
  assert.equal(calendar.transferProgressLabel({media_type: 'series', latest_transferred_episode: 6}, true), '已存 E6');
  assert.equal(calendar.transferProgressLabel({media_type: 'series'}), '尚未转存');
  assert.equal(calendar.transferProgressLabel({media_type: 'movie', transferred: true}), '已转存');
  assert.equal(calendar.sourceAlertLabel({latest_aired_episode: 8, latest_discovered_episode: 6, overdue_days: 10}), 'E8 已播 10 天，当前来源最高 E6');
});

test('calendar card lines: bold title data with day episode and saved episodes', () => {
  assert.equal(calendar.cardDayLabel({season: 1, episode: 13}), '今天 S1 E13');
  assert.equal(calendar.cardDayLabel({season: 2, episode: 1}), '今天 S2 E1');
  assert.equal(calendar.cardDayLabel({}), '上映 / 开播');
  assert.equal(calendar.cardSavedLabel({media_type: 'series', latest_transferred_episode: 10}), '已存 E10');
  assert.equal(calendar.cardSavedLabel({media_type: 'series'}), '尚未转存');
  assert.equal(calendar.cardSavedLabel({media_type: 'movie', transferred: true}), '已转存');
});

test('calendar stale-source action opens the existing source switch workflow', async () => {
  const fullSubscription = {id: 'sub-1', title: '完整订阅', source_candidates: [{id: 'candidate-1'}]};
  const opened = [];
  const store = Object.assign(calendarPage.createStore(), {
    subscriptions: [fullSubscription],
    openSourceSwitchDialog: async subscription => opened.push(subscription)
  });

  await store.openCalendarSourceSwitch({subscription_id: 'sub-1', subscription_title: '日历标题'});
  assert.equal(opened[0], fullSubscription);

  store.subscriptions = [];
  await store.openCalendarSourceSwitch({
    subscription_id: 'sub-2',
    subscription_title: '备用标题',
    media_type: 'series'
  });
  assert.deepEqual(opened[1], {
    id: 'sub-2',
    title: '备用标题',
    media_type: 'series',
    source_candidates: []
  });
});
