const test = require('node:test');
const assert = require('node:assert/strict');

require('../static/js/core/formatters.js');
const downloads = require('../static/js/stores/downloads.js');
const drive = require('../static/js/stores/drive.js');
const updates = require('../static/js/features/updates.js');
const notifications = require('../static/js/core/notifications.js');

test('downloads store normalizes groups, summaries, and task capabilities', () => {
  const groups = downloads.normalizeDownloadGroups({
    active: [{status: 'active', download_speed: '12', completed_length: 30, total_length: 100}],
    waiting: null,
    stopped: [{status: 'complete'}]
  });
  assert.deepEqual(groups.waiting, []);
  assert.equal(downloads.flattenDownloadTasks(groups).length, 2);
  assert.deepEqual(downloads.summarizeActiveDownloads(groups), {speed: 12, completed: 30, total: 100});
  assert.deepEqual(downloads.downloadTaskCapabilities({status: 'paused'}), {
    pause: false,
    resume: true,
    stop: true
  });
  assert.deepEqual(
    downloads.flattenDownloadTasks({
      active: [{gid: 'same', status: 'active'}],
      waiting: [{gid: 'same', status: 'waiting'}, {gid: 'next', status: 'waiting'}]
    }).map(task => task.gid),
    ['same', 'next']
  );
});

test('drive store filters videos and keeps folders first while sorting', () => {
  const items = [
    {fid: '2', file: true, file_name: 'Episode 10.mkv', size: 20},
    {fid: '1', file: false, file_name: 'Season 1', size: 0},
    {fid: '3', file: true, file_name: 'Episode 2.mp4', size: 10},
    {fid: '4', file: true, file_name: 'notes.txt', size: 1}
  ];
  assert.equal(drive.isDriveVideo(items[0]), true);
  assert.deepEqual(
    drive.filterAndSortDriveItems(items, {filterType: 'video', sortBy: 'name'}).map(item => item.fid),
    ['3', '2']
  );
  assert.deepEqual(
    drive.filterAndSortDriveItems(items, {sortBy: 'size', direction: 'desc'}).map(item => item.fid),
    ['1', '2', '3', '4']
  );
});

test('updates and notification helpers clamp unsafe input and preserve immutable filtering', () => {
  assert.equal(updates.normalizeUpdateProgress({running: false, stage: 'idle', percent: 0}), null);
  assert.deepEqual(updates.normalizeUpdateProgress({
    running: true,
    stage: 'downloading',
    percent: 140,
    downloaded_bytes: '12',
    total_bytes: '20'
  }), {
    running: true,
    stage: 'downloading',
    percent: 100,
    downloaded_bytes: 12,
    total_bytes: 20
  });
  const items = [{id: 1, read: false}, {id: 2, read: true}];
  assert.deepEqual(notifications.filterNotificationItems(items, 'unread'), [items[0]]);
  assert.equal(notifications.normalizeNotificationType('unknown'), 'info');
  assert.equal(notifications.toastIcon('error'), '✕');
});
