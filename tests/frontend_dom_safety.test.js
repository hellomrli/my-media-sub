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
  assert.match(html, /x-for="\(task, taskIndex\) in visibleDownloadCategoryTasks\(category\.id\)"/);
  assert.match(html, /x-for="\(item, itemIndex\) in day\.items"/);
});

// 该文件名承诺的是「DOM 安全」。此前它只断言了图片 error hack、资源版本号与
// x-for key，**完全没有检查 HTML 注入面**，于是新增 innerHTML / x-html 不会
// 触发任何门禁。下面补上真正的 sink 白名单断言。
test('no HTML injection sinks are introduced in the frontend', () => {
  const files = [];
  const walk = directory => {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
      const target = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(target);
      else if (entry.name.endsWith('.js')) files.push(target);
    }
  };
  walk(path.join(__dirname, '../static/js'));

  const sinks = /\.(?:innerHTML|outerHTML)\s*=|insertAdjacentHTML\s*\(|document\.write\s*\(|\beval\s*\(|new\s+Function\s*\(/;
  for (const file of files) {
    const source = fs.readFileSync(file, 'utf8');
    source.split('\n').forEach((line, index) => {
      // 允许注释里提到这些 API（例如说明为何不使用它们）
      const code = line.split('//')[0];
      assert.equal(
        sinks.test(code),
        false,
        `${path.relative(path.join(__dirname, '..'), file)}:${index + 1} 引入了 HTML 注入 sink：${line.trim()}`
      );
    });
  }
});

// x-html 会把字符串当 HTML 解析；CSP 含 'unsafe-eval'，一旦注入即等价 RCE。
// 因此把它限制为固定白名单（当前只用于内联 SVG 图标）。
test('x-html usage stays on the approved allowlist', () => {
  const sites = [...html.matchAll(/x-html="([^"]*)"/g)].map(match => match[1]);
  // 这些表达式都从 router.js 的硬编码 tab 表里取内联 SVG，不接受服务端数据。
  const allowed = new Set([
    'tab.icon',
    "tabs.find(tab => tab.id === 'settings')?.icon",
  ]);
  for (const expression of sites) {
    assert.ok(
      allowed.has(expression),
      `x-html 出现未登记表达式 "${expression}"：请改用 x-text，或把该表达式加入白名单并说明理由`
    );
  }
});

// 属性值里可能含 `>`（Alpine 表达式中的 `=>`），因此标签内部必须按「引号外的
// 非 > 字符，或整段双引号串」来吃，不能写 [^>]*——否则 `@input="... v => v.trim()"`
// 会把标签截断，后面的 id= 就被看不见了（v2.7.2 曾因此把 id 误插进表达式内部）。
const TAG_ATTRS = String.raw`((?:[^>"]|"[^"]*")*)`;

// 表单控件的可访问名称：每个 <label> 要么通过 for= 关联控件，要么把控件包在内部。
test('every label is associated with a form control', () => {
  const labelPattern = new RegExp(String.raw`<label\b${TAG_ATTRS}>([\s\S]*?)<\/label>`, 'g');
  let match;
  let checked = 0;
  while ((match = labelPattern.exec(html)) !== null) {
    const [, attrs, inner] = match;
    checked += 1;
    const wrapsControl = /<(?:input|select|textarea)\b/.test(inner);
    const hasFor = /\bfor="/.test(attrs);
    assert.ok(
      wrapsControl || hasFor,
      `存在既没有 for= 也没有包裹控件的 <label>：${match[0].slice(0, 120)}`
    );
  }
  assert.ok(checked >= 130, `只检查到 ${checked} 个 label，选择器可能失效`);
});

// 非隐藏控件必须有可访问名称：id 被某个 label 引用，或有 aria-label/aria-labelledby。
test('every visible form control has an accessible name', () => {
  const labelledIds = new Set(
    [...html.matchAll(new RegExp(String.raw`<label\b(?:[^>"]|"[^"]*")*?\bfor="([^"]+)"`, 'g'))].map(match => match[1])
  );
  const controlPattern = new RegExp(String.raw`<(input|select|textarea)\b${TAG_ATTRS}>`, 'g');
  let match;
  let checked = 0;
  while ((match = controlPattern.exec(html)) !== null) {
    const [, tag, attrs] = match;
    if (/\btype="(?:hidden|submit|button|reset|image)"/.test(attrs)) continue;
    const explicit = /\baria-label(?:ledby)?="/.test(attrs);
    // 控件被 label 包裹时也具备可访问名称
    const before = html.slice(Math.max(0, match.index - 200), match.index);
    const wrapped = new RegExp(String.raw`<label\b(?:[^>"]|"[^"]*")*>[^<]*$`).test(before);
    const idMatch = attrs.match(/\bid="([^"]+)"/);
    const referenced = idMatch ? labelledIds.has(idMatch[1]) : false;
    checked += 1;
    assert.ok(
      explicit || wrapped || referenced,
      `<${tag}> 缺少可访问名称（无 aria-label、未被 label 包裹、id 也未被 for= 引用）：${match[0].slice(0, 120)}`
    );
  }
  assert.ok(checked >= 100, `只检查到 ${checked} 个控件，选择器可能失效`);
});
