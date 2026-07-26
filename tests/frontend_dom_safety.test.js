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
  // 工作台海报墙在 v2.2.18 换成了密度更高的订阅看板，可恢复图片相应少了一处。
  assert.ok(recoverableImages.length >= 11, `可恢复远程图片只剩 ${recoverableImages.length} 处`);
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
    // v2.2.19 移除了「自动化执行」和「最近活动」卡片，dashboard-event- 与
    // dashboard-job-（含 job- 子串）随之消失；任务与通知统一由活动中心承载。
    'calendar-week-', 'calendar-month-', 'calendar-list-',
    'search-result-', 'drive-item-', 'download-task-', 'subscription-',
    'subscription-event-', 'subscription-activity-', 'activity-'
  ]) {
    assert.ok(html.includes(prefix), `missing stable render key prefix ${prefix}`);
  }
  assert.match(html, /x-for="\(task, taskIndex\) in downloadCategoryTasks\(category\.id\)"/);
  assert.match(html, /x-for="\(item, itemIndex\) in day\.items"/);
});
