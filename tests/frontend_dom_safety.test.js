const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const html = fs.readFileSync(path.join(__dirname, '../static/index.html'), 'utf8');
const subscriptionsSource = fs.readFileSync(path.join(__dirname, '../static/js/stores/subscriptions.js'), 'utf8');
const cargo = fs.readFileSync(path.join(__dirname, '../Cargo.toml'), 'utf8');

test('remote images recover after transient load failures', () => {
  assert.equal(html.includes("@error=\"$el.style.display = 'none'\""), false);
  assert.equal(html.includes("@error=\"$el.style.display='none'\""), false);
  assert.equal(html.includes('@error="$el.hidden = true"'), false);
  const recoverableImages = html.match(/@error="handleRemoteImageError\(\$event\)" @load="handleRemoteImageLoad\(\$event\)"/g) || [];
  assert.ok(recoverableImages.length >= 12);
  assert.match(subscriptionsSource, /this\.subscriptions = data\.data \|\| \[\];[\s\S]*recoverRemoteImagesAfterDataRefresh\(\)/);
  assert.equal(html.includes(':src="item.thumbnail_url"'), false);
  assert.equal(html.includes(':src="item.poster_url"'), false);
  assert.ok((html.match(/remoteImageUrl\(/g) || []).length >= 8);
});

test('critical browser assets carry the current application version', () => {
  const version = cargo.match(/^version = "([^"]+)"/m)[1];
  const references = [...html.matchAll(/(?:src|href)="((?:js\/|vendor\/|styles\.css|app\.js)[^"]+)"/g)]
    .map(match => match[1]);
  assert.ok(references.length >= 25);
  assert.ok(references.every(reference => reference.endsWith(`?v=${version}`)));
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
