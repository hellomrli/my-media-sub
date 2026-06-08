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
const setAutoDownloadNewItems = document.querySelector('#setAutoDownloadNewItems');
const setSubscriptionScheduler = document.querySelector('#setSubscriptionScheduler');
const setSubscriptionInterval = document.querySelector('#setSubscriptionInterval');
const setQuarkSaveEnabled = document.querySelector('#setQuarkSaveEnabled');
const setQuarkCookie = document.querySelector('#setQuarkCookie');
const setQuarkSaveRoot = document.querySelector('#setQuarkSaveRoot');
const setOpenlistUser = document.querySelector('#setOpenlistUser');
const setOpenlistPass = document.querySelector('#setOpenlistPass');
const setNasSyncEnabled = document.querySelector('#setNasSyncEnabled');
const setNasSyncSource = document.querySelector('#setNasSyncSource');
const setNasSyncTarget = document.querySelector('#setNasSyncTarget');
const downloadsBody = document.querySelector('#downloadsBody');
const subscriptionsBody = document.querySelector('#subscriptionsBody');
const checkAllSubsBtn = document.querySelector('#checkAllSubsBtn');
const notificationsBody = document.querySelector('#notificationsBody');
const markAllReadBtn = document.querySelector('#markAllReadBtn');
const subscriptionModal = document.querySelector('#subscriptionModal');
const subscriptionForm = document.querySelector('#subscriptionForm');
const subEditId = document.querySelector('#subEditId');
const subEditTitle = document.querySelector('#subEditTitle');
const subEditSeason = document.querySelector('#subEditSeason');
const subEditTotal = document.querySelector('#subEditTotal');
const subEditMediaType = document.querySelector('#subEditMediaType');
const subEditEnabled = document.querySelector('#subEditEnabled');
const subEditCompleted = document.querySelector('#subEditCompleted');
const subEditOnlyLatest = document.querySelector('#subEditOnlyLatest');
const subEditInclude = document.querySelector('#subEditInclude');
const subEditExclude = document.querySelector('#subEditExclude');
const subEditRegex = document.querySelector('#subEditRegex');
const subEditTargetDir = document.querySelector('#subEditTargetDir');
const subEditRenameTemplate = document.querySelector('#subEditRenameTemplate');
const subEditRenameRegex = document.querySelector('#subEditRenameRegex');
const subEditRenameReplacement = document.querySelector('#subEditRenameReplacement');
const subEditAutoCreateDir = document.querySelector('#subEditAutoCreateDir');
const subEditSkipExisting = document.querySelector('#subEditSkipExisting');
const subEditIgnoreExt = document.querySelector('#subEditIgnoreExt');
const previewSubPlanBtn = document.querySelector('#previewSubPlanBtn');
const subPlanPreview = document.querySelector('#subPlanPreview');
const pageTitle = document.querySelector('#pageTitle');
const resultCount = document.querySelector('#resultCount');
const resultCloudTabs = document.querySelector('#resultCloudTabs');

const chatId = `webui-${Math.random().toString(36).slice(2)}`;
let appSettings = null;
const downloadLogs = [];
let currentResultCloud = 'all';
let lastSearchResults = [];

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
  const activeTab = document.querySelector(`.tab[data-page=\"${pageId}\"]`);
  if (pageTitle && activeTab) pageTitle.textContent = activeTab.textContent.trim();
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
  const requestUrl = new URL(url, window.location.origin);
  const res = await fetch(requestUrl, options);
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
  if (setAutoDownloadNewItems) setAutoDownloadNewItems.checked = !!settings.auto_download_new_subscription_items;
  if (setSubscriptionScheduler) setSubscriptionScheduler.checked = !!settings.subscription_scheduler_enabled;
  if (setSubscriptionInterval) setSubscriptionInterval.value = settings.subscription_check_interval_minutes || 60;
  if (setQuarkSaveEnabled) setQuarkSaveEnabled.checked = !!settings.quark_save_enabled;
  if (setQuarkCookie) setQuarkCookie.value = '';
  if (setQuarkSaveRoot) setQuarkSaveRoot.value = settings.quark_save_root || '';
  if (setOpenlistUser) setOpenlistUser.value = settings.openlist_username || '';
  if (setOpenlistPass) setOpenlistPass.value = '';
  if (setNasSyncEnabled) setNasSyncEnabled.checked = !!settings.nas_sync_enabled;
  if (setNasSyncSource) setNasSyncSource.value = settings.nas_sync_source || '';
  if (setNasSyncTarget) setNasSyncTarget.value = settings.nas_sync_target || '';
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


function groupResultsByCloud(results) {
  const groups = [];
  const byType = new Map();
  for (const item of results) {
    const type = item.cloud_type || 'unknown';
    if (!byType.has(type)) {
      const group = {
        type,
        name: item.cloud_name || cloudTypeName(type),
        items: [],
      };
      byType.set(type, group);
      groups.push(group);
    }
    byType.get(type).items.push(item);
  }
  return groups;
}

function renderResultRow(item) {
  return `
    <article class="result-card">
      <div><span class="index">${item.index}</span></div>
      <div>
        <h2 class="result-title">${escapeHtml(item.title)}</h2>
        <div class="url">${escapeHtml(item.url)}</div>
        ${formatProbe(item)}
      </div>
      <div class="meta"><span>${escapeHtml(item.source || '未知')}</span><span>${escapeHtml(item.datetime || '未知')}</span></div>
      <div class="meta"><span>${escapeHtml(item.cloud_name || cloudTypeName(item.cloud_type))}</span>${item.download_capability?.label ? `<span>${escapeHtml(item.download_capability.label)}</span>` : ''}</div>
      <div class="card-actions">
        <button data-select="${item.index}">选择</button>
        ${(item.download_capability?.action === 'save_subscribe') ? `
          <select data-media-type="${item.index}" class="media-select">
            <option value="movie">电影</option>
            <option value="series" selected>连续剧</option>
            <option value="anime">动画</option>
          </select>
          <button class="secondary" data-subscribe="${item.index}">订阅</button>
        ` : ''}
        ${(item.download_capability?.direct_aria2) ? `<button class="secondary" data-aria2="${item.index}">Aria2</button>` : ''}
      </div>
    </article>
  `;
}

function renderResultTabs(groups, total) {
  if (!resultCloudTabs) return;
  if (!groups.length) {
    resultCloudTabs.classList.add('hidden');
    resultCloudTabs.innerHTML = '';
    return;
  }
  const tabs = [
    { type: 'all', name: '全部', count: total },
    ...groups.map(group => ({ type: group.type, name: group.name, count: group.items.length })),
  ];
  resultCloudTabs.classList.remove('hidden');
  resultCloudTabs.innerHTML = tabs.map(tab => `
    <button class="cloud-tab ${currentResultCloud === tab.type ? 'active' : ''}" data-cloud-tab="${escapeHtml(tab.type)}">
      ${escapeHtml(tab.name)} <span>${tab.count}</span>
    </button>
  `).join('');
  resultCloudTabs.querySelectorAll('[data-cloud-tab]').forEach(btn => {
    btn.addEventListener('click', () => {
      currentResultCloud = btn.dataset.cloudTab || 'all';
      renderResults(lastSearchResults, { keepTab: true });
    });
  });
}

function renderResults(results, options = {}) {
  selectedPanel.classList.add('hidden');
  selectedBody.innerHTML = '';
  lastSearchResults = results;

  if (!results.length) {
    currentResultCloud = 'all';
    if (resultCloudTabs) {
      resultCloudTabs.classList.add('hidden');
      resultCloudTabs.innerHTML = '';
    }
    resultsBox.className = 'results empty-card';
    resultsBox.innerHTML = '<p class="empty">没有找到结果。</p>';
    if (resultCount) resultCount.textContent = '0 条结果';
    return;
  }

  const groups = groupResultsByCloud(results);
  if (!options.keepTab || !['all', ...groups.map(g => g.type)].includes(currentResultCloud)) {
    currentResultCloud = 'all';
  }
  renderResultTabs(groups, results.length);

  const visibleResults = currentResultCloud === 'all'
    ? results
    : (groups.find(group => group.type === currentResultCloud)?.items || []);

  resultsBox.className = 'results';
  if (resultCount) {
    const groupText = groups.map(group => `${group.name} ${group.items.length}`).join(' / ');
    resultCount.textContent = currentResultCloud === 'all'
      ? `${results.length} 条结果 · ${groupText}`
      : `${visibleResults.length} 条结果 · ${cloudTypeName(currentResultCloud)}`;
  }
  resultsBox.innerHTML = visibleResults.map(renderResultRow).join('');

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
  resultsBox.className = 'results empty-card';
  resultsBox.innerHTML = '<p class="empty">正在搜索，请稍候...</p>';
  if (resultCount) resultCount.textContent = '搜索中';
  if (resultCloudTabs) { resultCloudTabs.classList.add('hidden'); resultCloudTabs.innerHTML = ''; }

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
      <div><span class="badge ${item.level === 'warning' ? 'orange' : 'green'}">${escapeHtml(item.level)}</span></div>
      <div class="muted">${item.read ? '已读' : '未读'}</div>
      <div class="muted">${escapeHtml(formatNotificationTime(item.created_at))}</div>
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

function splitKeywords(value) {
  return String(value || '').split(/[,，]/).map(x => x.trim()).filter(Boolean);
}

function renderSubscriptions(subs) {
  if (!subs.length) {
    subscriptionsBody.innerHTML = '<p class="empty">还没有订阅。搜索连续剧后点击“订阅”。</p>';
    return;
  }
  subscriptionsBody.innerHTML = subs.map(sub => {
    const newFiles = sub.last_new_files || [];
    const files = (sub.known_files || []).slice(-6);
    const statusText = sub.status === 'invalid' ? '链接疑似失效' : (sub.completed ? '已完结' : (sub.enabled === false ? '已停用' : '正常'));
    return `<article class="sub-card">
      <div>
        <h3>${escapeHtml(sub.title)}</h3>
        <div class="meta"><span>${sub.media_type === 'anime' ? '动画' : '连续剧'}</span><span>${escapeHtml(cloudTypeName(sub.cloud_type))}</span><span>已知 ${escapeHtml((sub.known_files || []).length)} 个文件</span></div>
        <div class="rule-summary">
          ${sub.rules?.target_dir ? `<span>目录：${escapeHtml(sub.rules.target_dir)}</span>` : ''}
          ${sub.rules?.match_regex ? `<span>过滤：${escapeHtml(sub.rules.match_regex)}</span>` : ''}
          ${sub.rules?.rename_template ? `<span>模板：${escapeHtml(sub.rules.rename_template)}</span>` : ''}
          ${sub.rules?.rename_regex ? `<span>替换：${escapeHtml(sub.rules.rename_regex)} → ${escapeHtml(sub.rules.rename_replacement || '')}</span>` : ''}
          ${sub.rules?.skip_existing_transferred ? '<span>跳过已转存</span>' : ''}
        </div>
        ${files.length ? `<ol class="file-list">${files.map(name => `<li>${escapeHtml(name)}</li>`).join('')}</ol>` : ''}
      </div>
      <div><span class="badge ${sub.completed ? 'green' : ''}">${escapeHtml(sub.current_episode_number || 0)} / ${escapeHtml(sub.total_episode_number || '*')}</span></div>
      <div><span class="badge ${sub.status === 'invalid' ? 'red' : sub.enabled === false ? 'orange' : 'green'}">${statusText}</span>${newFiles.length ? `<p class="status ok">新增 ${newFiles.length} 个</p>` : ''}</div>
      <div class="muted">${escapeHtml(formatTime(sub.last_checked_at))}</div>
      <div class="card-actions">
        <button class="secondary" data-check-sub="${sub.id}">刷新</button>
        <button class="secondary" data-edit-sub="${sub.id}">编辑</button>
        <button class="secondary" data-toggle-sub="${sub.id}">${sub.enabled === false ? '启用' : '停用'}</button>
        <button class="danger" data-delete-sub="${sub.id}">删除</button>
      </div>
    </article>`;
  }).join('');
  subscriptionsBody.querySelectorAll('[data-check-sub]').forEach(btn => {
    btn.addEventListener('click', () => checkSubscription(btn.dataset.checkSub));
  });
  subscriptionsBody.querySelectorAll('[data-delete-sub]').forEach(btn => {
    btn.addEventListener('click', () => deleteSubscription(btn.dataset.deleteSub));
  });
  subscriptionsBody.querySelectorAll('[data-toggle-sub]').forEach(btn => {
    btn.addEventListener('click', () => toggleSubscription(btn.dataset.toggleSub));
  });
  subscriptionsBody.querySelectorAll('[data-edit-sub]').forEach(btn => {
    btn.addEventListener('click', () => editSubscription(btn.dataset.editSub));
  });
}

async function loadSubscriptions() {
  const data = await requestJson('/api/subscriptions');
  renderSubscriptions(data.subscriptions || []);
}

async function toggleSubscription(id) {
  const data = await requestJson('/api/subscriptions');
  const sub = (data.subscriptions || []).find(x => x.id === id);
  if (!sub) return;
  await postJson('/api/subscriptions/update', { subscription_id: id, enabled: !(sub.enabled !== false) });
  await loadSubscriptions();
}

async function editSubscription(id) {
  const data = await requestJson('/api/subscriptions');
  const sub = (data.subscriptions || []).find(x => x.id === id);
  if (!sub) return;
  openSubscriptionModal(sub);
}

function openSubscriptionModal(sub) {
  const rules = sub.rules || {};
  subEditId.value = sub.id || '';
  subEditTitle.value = sub.title || '';
  subEditSeason.value = sub.season || 1;
  subEditTotal.value = sub.total_episode_number || '';
  subEditMediaType.value = sub.media_type || 'series';
  subEditEnabled.checked = sub.enabled !== false;
  subEditCompleted.checked = !!sub.completed;
  subEditOnlyLatest.checked = !!rules.only_latest;
  subEditInclude.value = (rules.include_keywords || []).join(', ');
  subEditExclude.value = (rules.exclude_keywords || []).join(', ');
  subEditRegex.value = rules.match_regex || '';
  subEditTargetDir.value = rules.target_dir || '';
  subEditRenameTemplate.value = rules.rename_template || '';
  subEditRenameRegex.value = rules.rename_regex || '';
  subEditRenameReplacement.value = rules.rename_replacement || '';
  subEditAutoCreateDir.checked = rules.auto_create_target_dir !== false;
  subEditSkipExisting.checked = rules.skip_existing_transferred !== false;
  subEditIgnoreExt.checked = !!rules.ignore_extensions;
  if (subPlanPreview) {
    subPlanPreview.className = 'plan-preview empty';
    subPlanPreview.textContent = '点击“预览规划”查看哪些文件会转存、跳过以及重命名结果。';
  }
  subscriptionModal?.classList.remove('hidden');
  subEditTitle?.focus();
}

function closeSubscriptionModal() {
  subscriptionModal?.classList.add('hidden');
}

function collectSubscriptionRulesFromModal() {
  return {
    include_keywords: splitKeywords(subEditInclude.value),
    exclude_keywords: splitKeywords(subEditExclude.value),
    match_regex: subEditRegex.value.trim(),
    only_latest: subEditOnlyLatest.checked,
    target_dir: subEditTargetDir.value.trim(),
    auto_create_target_dir: subEditAutoCreateDir.checked,
    skip_existing_transferred: subEditSkipExisting.checked,
    rename_template: subEditRenameTemplate.value.trim(),
    rename_regex: subEditRenameRegex.value.trim(),
    rename_replacement: subEditRenameReplacement.value,
    ignore_extensions: subEditIgnoreExt.checked,
  };
}

function renderPlanPreview(plan) {
  if (!subPlanPreview) return;
  const items = plan.items || [];
  if (!items.length) {
    subPlanPreview.className = 'plan-preview empty';
    subPlanPreview.textContent = '当前没有可预览的文件。请先检查订阅或搜索时开启文件嗅探。';
    return;
  }
  subPlanPreview.className = 'plan-preview';
  subPlanPreview.innerHTML = `
    <div class="plan-summary">
      <span>${escapeHtml(plan.summary || '')}</span>
      <span>待转存：${escapeHtml(plan.transfer_count || 0)}</span>
      <span>跳过：${escapeHtml(plan.skip_count || 0)}</span>
    </div>
    <div class="plan-table">
      <div class="plan-row plan-head"><span>动作</span><span>源文件</span><span>目标文件</span><span>原因</span></div>
      ${items.slice(0, 80).map(item => `
        <div class="plan-row ${item.action === 'transfer' ? 'will-transfer' : 'will-skip'}">
          <span>${item.action === 'transfer' ? '转存' : '跳过'}</span>
          <span>${escapeHtml(item.source_name || '')}</span>
          <span>${escapeHtml(item.target_name || '')}</span>
          <span>${escapeHtml(item.skip_reason || '-')}</span>
        </div>
      `).join('')}
    </div>
  `;
}

async function previewSubscriptionPlan() {
  const id = subEditId.value;
  if (!id) return;
  previewSubPlanBtn.disabled = true;
  if (subPlanPreview) {
    subPlanPreview.className = 'plan-preview empty';
    subPlanPreview.textContent = '正在生成规划...';
  }
  try {
    const data = await postJson('/api/subscriptions/plan', {
      subscription_id: id,
      rules: collectSubscriptionRulesFromModal(),
    });
    renderPlanPreview(data.plan || {});
  } catch (err) {
    if (subPlanPreview) {
      subPlanPreview.className = 'plan-preview empty error-text';
      subPlanPreview.textContent = `规划失败：${err.message}`;
    }
  } finally {
    previewSubPlanBtn.disabled = false;
  }
}

async function saveSubscriptionModal(event) {
  event.preventDefault();
  const id = subEditId.value;
  if (!id) return;
  await postJson('/api/subscriptions/update', {
    subscription_id: id,
    title: subEditTitle.value.trim(),
    media_type: subEditMediaType.value,
    season: Number(subEditSeason.value || 1),
    total_episode_number: subEditTotal.value ? Number(subEditTotal.value) : null,
    enabled: subEditEnabled.checked,
    completed: subEditCompleted.checked,
rules: collectSubscriptionRulesFromModal()
  });
  closeSubscriptionModal();
  await loadSubscriptions();
  setStatus('订阅规则已保存。自动转存/重命名执行器将在下一阶段接入。', 'ok');
}

function addDownloadLogsFromSubscription(downloads, fallbackTitle = '订阅自动投递') {
  for (const item of downloads || []) {
    if (!item || item.status === 'skipped') continue;
    downloadLogs.unshift({
      gid: item.gid || item.result?.gid || item.error || item.status || '-',
      url: item.url || item.source_url || '',
      title: item.title || fallbackTitle,
      time: new Date().toLocaleString(),
    });
  }
  renderDownloadLogs();
}

async function checkSubscription(id) {
  setStatus('正在检查订阅更新...');
  try {
    const data = await postJson('/api/subscriptions/check', { subscription_id: id });
    await loadSubscriptions();
    await loadNotifications();
    addDownloadLogsFromSubscription(data.downloads || []);
    const count = (data.new_files || []).length;
    const downloadCount = (data.downloads || []).filter(item => item && item.status !== 'skipped').length;
    const quarkCount = (data.quark_saves || []).length;
    const nasSyncCount = (data.nas_syncs || []).length;
    const parts = [];
    if (count) parts.push(`发现 ${count} 个新文件`);
    if (downloadCount) parts.push(`Aria2 提交 ${downloadCount} 个`);
    if (quarkCount) parts.push(`夸克转存 ${quarkCount} 个`);
    if (nasSyncCount) parts.push(`NAS 同步 ${nasSyncCount} 个`);
    setStatus(parts.length ? parts.join('，') + '。' : '没有发现新文件。', count ? 'ok' : '');
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
    for (const result of data.results || []) addDownloadLogsFromSubscription(result.downloads || [], result.title || '订阅自动投递');
    const count = (data.results || []).reduce((sum, r) => sum + ((r.new_files || []).length), 0);
    const downloadCount = (data.results || []).reduce((sum, r) => sum + ((r.downloads || []).filter(item => item && item.status !== 'skipped').length), 0);
    const quarkCount = (data.results || []).reduce((sum, r) => sum + ((r.quark_saves || []).length), 0);
    const nasSyncCount = (data.results || []).reduce((sum, r) => sum + ((r.nas_syncs || []).length), 0);
    const parts = [];
    if (count) parts.push(`发现 ${count} 个新文件`);
    if (downloadCount) parts.push(`Aria2 提交 ${downloadCount} 个`);
    if (quarkCount) parts.push(`夸克转存 ${quarkCount} 个`);
    if (nasSyncCount) parts.push(`NAS 同步 ${nasSyncCount} 个`);
    setStatus(parts.length ? parts.join('，') + '。' : '全部订阅都没有新文件。', count ? 'ok' : '');
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
    auto_download_new_subscription_items: !!setAutoDownloadNewItems?.checked,
    subscription_scheduler_enabled: !!setSubscriptionScheduler?.checked,
    subscription_check_interval_minutes: Number(setSubscriptionInterval?.value || 60),
    quark_save_enabled: !!setQuarkSaveEnabled?.checked,
    quark_save_root: setQuarkSaveRoot?.value.trim() || '',
    openlist_username: setOpenlistUser?.value.trim() || '',
    nas_sync_enabled: !!setNasSyncEnabled?.checked,
    nas_sync_source: setNasSyncSource?.value.trim() || '',
    nas_sync_target: setNasSyncTarget?.value.trim() || '',
  };
  if (setPassword.value) payload.app_password = setPassword.value;
  if (setAria2Secret.value) payload.aria2_secret = setAria2Secret.value;
  if (setQuarkCookie?.value) payload.quark_cookie = setQuarkCookie.value;
  if (setOpenlistPass?.value) payload.openlist_password = setOpenlistPass.value;
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
subscriptionForm?.addEventListener('submit', saveSubscriptionModal);
previewSubPlanBtn?.addEventListener('click', previewSubscriptionPlan);
document.querySelectorAll('[data-close-sub-modal]').forEach(el => el.addEventListener('click', closeSubscriptionModal));
document.addEventListener('keydown', event => { if (event.key === 'Escape') closeSubscriptionModal(); });

loadSettings()
  .then(loadSubscriptions)
  .then(loadNotifications)
  .catch(err => setStatus(`加载设置失败：${err.message}`, 'error'));
