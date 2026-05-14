const keywordInput = document.querySelector('#keyword');
const searchBtn = document.querySelector('#searchBtn');
const statusBox = document.querySelector('#status');
const resultsBox = document.querySelector('#results');
const selectedPanel = document.querySelector('#selected');
const selectedBody = document.querySelector('#selectedBody');
const checkLinksInput = document.querySelector('#checkLinks');
const probeFilesInput = document.querySelector('#probeFiles');
const filterBadLinksInput = document.querySelector('#filterBadLinks');
const cloudTypesBox = document.querySelector('#cloudTypes');
const settingsCloudTypesBox = document.querySelector('#settingsCloudTypes');
const saveSettingsBtn = document.querySelector('#saveSettingsBtn');
const testAria2Btn = document.querySelector('#testAria2Btn');
const openlistLink = document.querySelector('#openlistLink');
const setUsername = document.querySelector('#setUsername');
const setPassword = document.querySelector('#setPassword');
const setPansou = document.querySelector('#setPansou');
const setOpenlist = document.querySelector('#setOpenlist');
const setAria2Rpc = document.querySelector('#setAria2Rpc');
const setAria2Secret = document.querySelector('#setAria2Secret');
const setAria2Dir = document.querySelector('#setAria2Dir');
const downloadsBody = document.querySelector('#downloadsBody');
const subscriptionsBody = document.querySelector('#subscriptionsBody');
const checkAllSubsBtn = document.querySelector('#checkAllSubsBtn');
const notificationsBody = document.querySelector('#notificationsBody');
const markAllReadBtn = document.querySelector('#markAllReadBtn');

const chatId = `webui-${Math.random().toString(36).slice(2)}`;
let appSettings = null;
const downloadLogs = [];

const CLOUD_TYPE_NAMES_FALLBACK = {
  quark: '夸克网盘',
  baidu: '百度网盘',
  aliyun: '阿里云盘',
  uc: 'UC网盘',
  tianyi: '天翼云盘',
  mobile: '移动云盘',
  '115': '115网盘',
  pikpak: 'PikPak',
  xunlei: '迅雷网盘',
  '123': '123网盘',
  magnet: '磁力链接',
  ed2k: '电驴链接',
  others: '其他资源',
};

function cloudTypeName(type) {
  return appSettings?.cloud_type_names?.[type] || CLOUD_TYPE_NAMES_FALLBACK[type] || type || '未知网盘';
}

function showPage(pageId) {
  document.querySelectorAll('.page').forEach(p => p.classList.toggle('active', p.id === pageId));
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.page === pageId));
}

document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => showPage(tab.dataset.page));
});

function setStatus(message, type = '') {
  if (!message) {
    statusBox.className = 'status hidden';
    statusBox.textContent = '';
    return;
  }
  statusBox.className = `status ${type}`;
  statusBox.textContent = message;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

async function requestJson(url, options = {}) {
  const res = await fetch(url, options);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(data.detail || data.message || `请求失败：${res.status}`);
  }
  return data;
}

function postJson(url, payload) {
  return requestJson(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

function selectedCloudTypes(container = cloudTypesBox) {
  return [...container.querySelectorAll('input[data-cloud]:checked')].map(el => el.value);
}

function renderCloudTypeOptions(container, selected = ['quark']) {
  const supported = appSettings?.supported_cloud_types || ['quark'];
  container.innerHTML = supported.map(type => `
    <label><input type="checkbox" data-cloud value="${escapeHtml(type)}" ${selected.includes(type) ? 'checked' : ''} /> ${escapeHtml(cloudTypeName(type))}</label>
  `).join('');
}

function applySettingsToUi(settings) {
  appSettings = settings;
  setUsername.value = settings.app_username || '';
  setPassword.value = '';
  setPansou.value = settings.pansou_base_url || '';
  setOpenlist.value = settings.openlist_base_url || '';
  setAria2Rpc.value = settings.aria2_rpc_url || '';
  setAria2Secret.value = '';
  setAria2Dir.value = settings.aria2_dir || '';
  checkLinksInput.checked = settings.check_links !== false;
  probeFilesInput.checked = settings.probe_quark_files !== false;
  filterBadLinksInput.checked = settings.filter_bad_links !== false;
  if (settings.openlist_base_url) openlistLink.href = settings.openlist_base_url;
  renderCloudTypeOptions(cloudTypesBox, settings.cloud_types || ['quark']);
  renderCloudTypeOptions(settingsCloudTypesBox, settings.cloud_types || ['quark']);
}

async function loadSettings() {
  const settings = await requestJson('/api/settings');
  applySettingsToUi(settings);
}

function formatProbe(item) {
  const probe = item.probe || {};
  const check = item.link_check || {};
  const bits = [];
  if (item.cloud_name || item.cloud_type) bits.push(`网盘：${escapeHtml(item.cloud_name || cloudTypeName(item.cloud_type))}`);
  if (check.state) bits.push(`有效性：${escapeHtml(check.state)}${check.summary ? `（${escapeHtml(check.summary)}）` : ''}`);
  if (probe.file_count !== undefined) bits.push(`文件：${escapeHtml(probe.file_count)}`);
  if (probe.episode_count) bits.push(`疑似剧集：${escapeHtml(probe.episode_count)}集`);
  if (probe.message) bits.push(`嗅探：${escapeHtml(probe.message)}`);
  if (item.download_capability?.label) bits.push(`下载：${escapeHtml(item.download_capability.label)}`);
  const files = (probe.files || []).slice(0, 12);
  const fileHtml = files.length ? `<ol class="file-list">${files.map(f => `<li>${escapeHtml(f.name)}${f.is_dir ? ' <span class="badge">目录</span>' : ''}</li>`).join('')}</ol>` : '';
  return `<div class="meta">${bits.map(b => `<span>${b}</span>`).join('')}</div>${fileHtml}`;
}

function renderResults(results) {
  selectedPanel.classList.add('hidden');
  selectedBody.innerHTML = '';

  if (!results.length) {
    resultsBox.innerHTML = '<p class="empty">没有找到结果。</p>';
    return;
  }

  resultsBox.innerHTML = results.map(item => `
    <article class="result-card">
      <div class="index">${item.index}</div>
      <div>
        <h2 class="result-title">${escapeHtml(item.title)}</h2>
        <div class="meta">
          <span>来源：${escapeHtml(item.source || '未知')}</span>
          <span>时间：${escapeHtml(item.datetime || '未知')}</span>
        </div>
        ${formatProbe(item)}
        <div class="url">${escapeHtml(item.url)}</div>
      </div>
      <div class="card-actions">
        <button data-select="${item.index}">选择</button>
        ${(item.download_capability?.action === 'save_subscribe') ? `
          <select data-media-type="${item.index}" class="media-select">
            <option value="movie">电影</option>
            <option value="series" selected>连续剧</option>
            <option value="anime">动画</option>
          </select>
          <button class="secondary" data-subscribe="${item.index}">订阅</button>
          <button class="secondary" disabled>转存待接入</button>
        ` : ''}
        ${(item.download_capability?.direct_aria2) ? `<button class="secondary" data-aria2="${item.index}">Aria2</button>` : ''}
        ${(!item.download_capability?.direct_aria2 && item.download_capability?.action !== 'save_subscribe') ? `<button class="secondary" disabled>${escapeHtml(item.download_capability?.label || '复制链接')}</button>` : ''}
      </div>
    </article>
  `).join('');

  resultsBox.querySelectorAll('[data-select]').forEach(btn => {
    btn.addEventListener('click', () => selectResult(Number(btn.dataset.select)));
  });
  resultsBox.querySelectorAll('[data-aria2]').forEach(btn => {
    btn.addEventListener('click', () => sendToAria2(Number(btn.dataset.aria2)));
  });
  resultsBox.querySelectorAll('[data-subscribe]').forEach(btn => {
    btn.addEventListener('click', () => subscribeResult(Number(btn.dataset.subscribe)));
  });
}

async function search() {
  const keyword = keywordInput.value.trim();
  if (!keyword) {
    setStatus('请输入影视名称。', 'error');
    return;
  }

  searchBtn.disabled = true;
  setStatus('正在搜索 PanSou...');
  resultsBox.innerHTML = '';

  try {
    const data = await postJson('/api/search', {
      chat_id: chatId,
      keyword,
      limit: 12,
      cloud_types: selectedCloudTypes(cloudTypesBox),
      check_links: checkLinksInput?.checked ?? true,
      probe_files: probeFilesInput?.checked ?? true,
      filter_bad_links: filterBadLinksInput?.checked ?? true,
    });
    renderResults(data.results || []);
    setStatus(`找到 ${(data.results || []).length} 条可用结果，已过滤 ${data.filtered_count || 0} 条失效链接。`, 'ok');
  } catch (err) {
    setStatus(err.message, 'error');
  } finally {
    searchBtn.disabled = false;
  }
}

async function selectResult(index) {
  setStatus(`已选择第 ${index} 条，正在处理...`);
  try {
    const data = await postJson('/api/select', { chat_id: chatId, index });
    const item = data.selected || {};
    selectedBody.innerHTML = `
      <pre>${escapeHtml(data.reply || '')}</pre>
      <p class="url">${escapeHtml(item.url || '')}</p>
      <button id="selectedAria2Btn">发送到 Aria2</button>
    `;
    selectedPanel.classList.remove('hidden');
    document.querySelector('#selectedAria2Btn')?.addEventListener('click', () => sendToAria2(index));
    setStatus('选择成功。可发送到 Aria2；夸克转存和 OpenList/NAS 下载将在下一阶段接入。', 'ok');
  } catch (err) {
    setStatus(err.message, 'error');
  }
}

function formatNotificationTime(ts) {
  if (!ts) return '';
  return new Date(ts * 1000).toLocaleString();
}

function renderNotifications(items) {
  if (!items.length) {
    notificationsBody.innerHTML = '<p class="empty">暂无通知。</p>';
    return;
  }
  notificationsBody.innerHTML = items.slice(0, 20).map(item => `
    <article class="sub-card ${item.read ? 'read' : ''}">
      <div>
        <h3>${item.level === 'warning' ? '⚠️ ' : 'ℹ️ '}${escapeHtml(item.title)}</h3>
        <div class="meta"><span>${escapeHtml(formatNotificationTime(item.created_at))}</span><span>${escapeHtml(item.event)}</span></div>
        <p>${escapeHtml(item.message)}</p>
      </div>
      <div class="card-actions">
        <button class="secondary" data-read-notification="${item.id}">已读</button>
      </div>
    </article>
  `).join('');
  notificationsBody.querySelectorAll('[data-read-notification]').forEach(btn => {
    btn.addEventListener('click', async () => {
      await postJson('/api/notifications/read', { notification_id: btn.dataset.readNotification });
      await loadNotifications();
    });
  });
}

async function loadNotifications() {
  const data = await requestJson('/api/notifications?include_read=true');
  renderNotifications(data.notifications || []);
}

async function markAllNotificationsRead() {
  await postJson('/api/notifications/read', {});
  await loadNotifications();
  setStatus('通知已全部标记为已读。', 'ok');
}

async function subscribeResult(index) {
  const mediaType = document.querySelector(`[data-media-type=\"${index}\"]`)?.value || 'series';
  if (mediaType === 'movie') {
    setStatus('电影不会创建追更订阅；你可以直接选择或发送到 Aria2。', 'error');
    return;
  }
  setStatus(`正在订阅第 ${index} 条...`);
  try {
    await postJson('/api/subscriptions', { chat_id: chatId, index, media_type: mediaType, notify_only: true });
    await loadSubscriptions();
    setStatus('订阅已创建。以后可在“我的订阅”里手动检查更新。', 'ok');
  } catch (err) {
    setStatus(`订阅失败：${err.message}`, 'error');
  }
}

function formatTime(ts) {
  if (!ts) return '未知';
  return new Date(ts * 1000).toLocaleString();
}

function renderSubscriptions(subs) {
  if (!subs.length) {
    subscriptionsBody.innerHTML = '<p class="empty">还没有订阅。搜索连续剧后点击“订阅”。</p>';
    return;
  }
  subscriptionsBody.innerHTML = subs.map(sub => {
    const newFiles = sub.last_new_files || [];
    const files = (sub.known_files || []).slice(-12);
    return `<article class="sub-card">
      <div>
        <h3>${escapeHtml(sub.title)}</h3>
        <div class="meta">
          <span>类型：${sub.media_type === 'anime' ? '动画' : '连续剧'}</span>
          <span>第 ${escapeHtml(sub.season || 1)} 季</span>
          <span>进度：${escapeHtml(sub.current_episode_number || 0)} / ${escapeHtml(sub.total_episode_number || '*')}</span>
          <span>状态：${sub.status === 'invalid' ? '链接疑似失效' : '正常'}</span>
          <span>网盘：${escapeHtml(cloudTypeName(sub.cloud_type))}</span>
          <span>已知文件：${escapeHtml((sub.known_files || []).length)}</span>
          <span>最后检查：${escapeHtml(formatTime(sub.last_checked_at))}</span>
        </div>
        ${sub.status === 'invalid' ? `<p class="status error">链接疑似失效：${escapeHtml(sub.last_error || '分享不可访问')}</p>` : ''}
        ${newFiles.length ? `<p class="status ok">发现新文件：${escapeHtml(newFiles.join('、'))}</p>` : ''}
        <ol class="file-list">${files.map(name => `<li>${escapeHtml(name)}</li>`).join('')}</ol>
      </div>
      <div class="card-actions">
        <button class="secondary" data-check-sub="${sub.id}">检查更新</button>
        <button class="secondary" data-delete-sub="${sub.id}">删除</button>
      </div>
    </article>`;
  }).join('');
  subscriptionsBody.querySelectorAll('[data-check-sub]').forEach(btn => {
    btn.addEventListener('click', () => checkSubscription(btn.dataset.checkSub));
  });
  subscriptionsBody.querySelectorAll('[data-delete-sub]').forEach(btn => {
    btn.addEventListener('click', () => deleteSubscription(btn.dataset.deleteSub));
  });
}

async function loadSubscriptions() {
  const data = await requestJson('/api/subscriptions');
  renderSubscriptions(data.subscriptions || []);
}

async function checkSubscription(id) {
  setStatus('正在检查订阅更新...');
  try {
    const data = await postJson('/api/subscriptions/check', { subscription_id: id });
    await loadSubscriptions();
    await loadNotifications();
    const count = (data.new_files || []).length;
    setStatus(count ? `发现 ${count} 个新文件。` : '没有发现新文件。', count ? 'ok' : '');
  } catch (err) {
    setStatus(`检查失败：${err.message}`, 'error');
  }
}

async function checkAllSubscriptions() {
  setStatus('正在检查全部订阅...');
  try {
    const data = await postJson('/api/subscriptions/check-all', {});
    await loadSubscriptions();
    await loadNotifications();
    const count = (data.results || []).reduce((sum, r) => sum + ((r.new_files || []).length), 0);
    setStatus(count ? `发现 ${count} 个新文件。` : '全部订阅都没有新文件。', count ? 'ok' : '');
  } catch (err) {
    setStatus(`检查失败：${err.message}`, 'error');
  }
}

async function deleteSubscription(id) {
  if (!confirm('确定删除这个订阅吗？')) return;
  await postJson('/api/subscriptions/delete', { subscription_id: id });
  await loadSubscriptions();
  setStatus('订阅已删除。', 'ok');
}

function renderDownloadLogs() {
  if (!downloadLogs.length) {
    downloadsBody.innerHTML = '<p class="empty">暂无下载记录。</p>';
    return;
  }
  downloadsBody.innerHTML = downloadLogs.map(item => `
    <article class="sub-card">
      <div>
        <h3>${escapeHtml(item.title || 'Aria2 任务')}</h3>
        <div class="meta"><span>${escapeHtml(item.time)}</span><span>GID：${escapeHtml(item.gid || '-')}</span></div>
        <p class="url">${escapeHtml(item.url || '')}</p>
      </div>
    </article>
  `).join('');
}

async function sendToAria2(index) {
  setStatus(`正在把第 ${index} 条发送到 Aria2...`);
  try {
    const data = await postJson('/api/download/aria2', { chat_id: chatId, index });
    downloadLogs.unshift({ gid: data.gid, url: data.url, title: data.selected?.title, time: new Date().toLocaleString() });
    renderDownloadLogs();
    setStatus(`已发送到 Aria2，GID：${data.gid}`, 'ok');
  } catch (err) {
    setStatus(`Aria2 失败：${err.message}`, 'error');
  }
}

async function saveSettings() {
  saveSettingsBtn.disabled = true;
  const payload = {
    app_username: setUsername.value.trim(),
    pansou_base_url: setPansou.value.trim(),
    openlist_base_url: setOpenlist.value.trim(),
    cloud_types: selectedCloudTypes(settingsCloudTypesBox),
    check_links: checkLinksInput.checked,
    probe_quark_files: probeFilesInput.checked,
    filter_bad_links: filterBadLinksInput.checked,
    aria2_rpc_url: setAria2Rpc.value.trim(),
    aria2_dir: setAria2Dir.value.trim(),
  };
  if (setPassword.value) payload.app_password = setPassword.value;
  if (setAria2Secret.value) payload.aria2_secret = setAria2Secret.value;
  try {
    const settings = await postJson('/api/settings', payload);
    applySettingsToUi(settings);
    setStatus('设置已保存。', 'ok');
  } catch (err) {
    setStatus(err.message, 'error');
  } finally {
    saveSettingsBtn.disabled = false;
  }
}

async function testAria2() {
  setStatus('正在测试 Aria2...');
  try {
    const data = await postJson('/api/aria2/test', {});
    setStatus(`Aria2 可用：${JSON.stringify(data.version)}`, 'ok');
  } catch (err) {
    setStatus(`Aria2 测试失败：${err.message}`, 'error');
  }
}

searchBtn.addEventListener('click', search);
keywordInput.addEventListener('keydown', event => {
  if (event.key === 'Enter') search();
});
saveSettingsBtn.addEventListener('click', saveSettings);
testAria2Btn.addEventListener('click', testAria2);

checkAllSubsBtn.addEventListener('click', checkAllSubscriptions);
markAllReadBtn.addEventListener('click', markAllNotificationsRead);

loadSettings()
  .then(loadSubscriptions)
  .then(loadNotifications)
  .catch(err => setStatus(`加载设置失败：${err.message}`, 'error'));
