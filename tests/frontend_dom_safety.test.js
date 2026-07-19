const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const html = fs.readFileSync(path.join(__dirname, '../static/index.html'), 'utf8');

test('remote images recover after transient load failures', () => {
  assert.equal(html.includes("@error=\"$el.style.display = 'none'\""), false);
  assert.equal(html.includes("@error=\"$el.style.display='none'\""), false);
  assert.equal(html.includes('@error="$el.hidden = true"'), false);
  const recoverableImages = html.match(/@error="handleRemoteImageError\(\$event\)" @load="handleRemoteImageLoad\(\$event\)"/g) || [];
  assert.ok(recoverableImages.length >= 12);
});

test('rapidly refreshed Alpine lists use collision-resistant render keys', () => {
  for (const prefix of [
    'dashboard-event-', 'calendar-week-', 'calendar-month-', 'calendar-list-',
    'search-result-', 'drive-item-', 'download-task-', 'subscription-',
    'subscription-event-', 'subscription-activity-', 'job-', 'notification-'
  ]) {
    assert.ok(html.includes(prefix), `missing stable render key prefix ${prefix}`);
  }
  assert.match(html, /x-for="\(task, taskIndex\) in allDownloadTasks"/);
  assert.match(html, /x-for="\(item, itemIndex\) in day\.items"/);
});
