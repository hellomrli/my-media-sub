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
const testQuarkBtn = document.querySelector('#testQuarkBtn');
const testNasSyncBtn = document.querySelector('#testNasSyncBtn');
const testPushBtn = document.querySelector('#testPushBtn');
const setUsername = document.querySelector('#setUsername');
const setPassword = document.querySelector('#setPassword');
const setAria2Rpc = document.querySelector('#setAria2Rpc');
const setAria2Secret = document.querySelector('#setAria2Secret');
const setAria2Dir = document.querySelector('#setAria2Dir');
const setAutoDownloadNewItems = document.querySelector('#setAutoDownloadNewItems');
const setSubscriptionScheduler = document.querySelector('#setSubscriptionScheduler');
const setSubscriptionInterval = document.querySelector('#setSubscriptionInterval');
const setQuarkSaveEnabled = document.querySelector('#setQuarkSaveEnabled');
const setQuarkCookie = document.querySelector('#setQuarkCookie');
const setQuarkSaveRoot = document.querySelector('#setQuarkSaveRoot');
const setQuarkSaveMovieDir = document.querySelector('#setQuarkSaveMovieDir');
const setQuarkSaveSeriesDir = document.querySelector('#setQuarkSaveSeriesDir');
const setQuarkSaveAnimeDir = document.querySelector('#setQuarkSaveAnimeDir');
const addCustomCategoryBtn = document.querySelector('#addCustomCategoryBtn');
const customCategoriesBox = document.querySelector('#customCategoriesBox');
const setNasSyncEnabled = document.querySelector('#setNasSyncEnabled');
const setNasSyncSource = document.querySelector('#setNasSyncSource');
const setNasSyncTarget = document.querySelector('#setNasSyncTarget');
const setPushOnUpdate = document.querySelector('#setPushOnUpdate');
const setPushOnFailed = document.querySelector('#setPushOnFailed');
const setPushOnCompleted = document.querySelector('#setPushOnCompleted');
const setPushOnSave = document.querySelector('#setPushOnSave');
const setPushSilent = document.querySelector('#setPushSilent');
const setWecomUrl = document.querySelector('#setWecomUrl');
const setWxpusherToken = document.querySelector('#setWxpusherToken');
const setWxpusherUids = document.querySelector('#setWxpusherUids');
const setTelegramToken = document.querySelector('#setTelegramToken');
const setTelegramChatId = document.querySelector('#setTelegramChatId');
const setBarkUrl = document.querySelector('#setBarkUrl');
const setGotifyUrl = document.querySelector('#setGotifyUrl');
const setGotifyToken = document.querySelector('#setGotifyToken');
const setPushplusToken = document.querySelector('#setPushplusToken');
const setServerchanKey = document.querySelector('#setServerchanKey');
const downloadsBody = document.querySelector('#downloadsBody');
const subscriptionsBody = document.querySelector('#subscriptionsBody');
const checkAllSubsBtn = document.querySelector('#checkAllSubsBtn');
const notificationsBody = document.querySelector('#notificationsBody');
const markAllReadBtn = document.querySelector('#markAllReadBtn');
const subscriptionModal = document.querySelector('#subscriptionModal');
const subscriptionForm = document.querySelector('#subscriptionForm');
const subscriptionModalTitle = document.querySelector('#subscriptionModalTitle');
const subscriptionModalHint = document.querySelector('#subscriptionModalHint');
const saveSubscriptionBtn = document.querySelector('#saveSubscriptionBtn');
const subEditId = document.querySelector('#subEditId');
const subEditTitle = document.querySelector('#subEditTitle');
const subEditSeason = document.querySelector('#subEditSeason');
const subEditTotal = document.querySelector('#subEditTotal');
const subEditMediaType = document.querySelector('#subEditMediaType');
const subEditEnabled = document.querySelector('#subEditEnabled');
const subEditCompleted = document.querySelector('#subEditCompleted');
const subEditAutoSave = document.querySelector('#subEditAutoSave');
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
const manualSubscribeBtn = document.querySelector('#manualSubscribeBtn');
const manualSubscribeModal = document.querySelector('#manualSubscribeModal');
const manualUrl = document.querySelector('#manualUrl');
const manualPassword = document.querySelector('#manualPassword');
const manualProbeBtn = document.querySelector('#manualProbeBtn');
const manualCreateSubBtn = document.querySelector('#manualCreateSubBtn');
const manualProbeResult = document.querySelector('#manualProbeResult');
const manualProbeContent = document.querySelector('#manualProbeContent');
const pageTitle = document.querySelector('#pageTitle');
const resultCount = document.querySelector('#resultCount');
const resultCloudTabs = document.querySelector('#resultCloudTabs');
const driveBody = document.querySelector('#driveBody');
const drivePath = document.querySelector('#drivePath');
const driveBackBtn = document.querySelector('#driveBackBtn');
const driveRefreshBtn = document.querySelector('#driveRefreshBtn');
const driveNewFolderBtn = document.querySelector('#driveNewFolderBtn');

const chatId = `webui-${Math.random().toString(36).slice(2)}`;
let appSettings = null;
const downloadLogs = [];
let currentResultCloud = 'all';
let lastSearchResults = [];
let driveStack = [{ fid: '0', name: '根目录' }];
let currentDriveItems = [];
let driveSelectMode = false;
let driveSelectedFids = new Set();

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

function showPage(pageId, pushState = true) {
  document.querySelectorAll('.page').forEach(p => p.classList.toggle('active', p.id === pageId));
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.page === pageId));
  const activeTab = document.querySelector(`.tab[data-page=\"${pageId}\"]`);
  if (pageTitle && activeTab) pageTitle.textContent = activeTab.textContent.trim();
  
  // 添加到浏览器历史
  if (pushState) {
    const url = new URL(window.location);
    url.searchParams.set('page', pageId);
    window.history.pushState({ page: pageId }, '', url);
  }
}

document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => showPage(tab.dataset.page));
});

// 处理浏览器后退/前进按钮
window.addEventListener('popstate', (event) => {
  if (event.state && event.state.page) {
    showPage(event.state.page, false);
  } else {
    // 如果没有 state，从 URL 读取
    const urlParams = new URLSearchParams(window.location.search);
    const pageId = urlParams.get('page') || 'searchPage';
    showPage(pageId, false);
  }
});

// 页面加载时从 URL 恢复状态
(function() {
  const urlParams = new URLSearchParams(window.location.search);
  const pageId = urlParams.get('page');
  if (pageId) {
    showPage(pageId, false);
  } else {
    // 初始页面也添加到历史
    const initialPage = document.querySelector('.page.active')?.id || 'searchPage';
    window.history.replaceState({ page: initialPage }, '', `?page=${initialPage}`);
  }
})();

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
  const text = await res.text().catch(() => '');
  let data = {};
  try {
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed === 'object') data = parsed;
  } catch {
    data = {};
  }
  if (!res.ok) {
    throw new Error(data.detail || data.message || text.trim() || `请求失败：${res.status}`);
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

function markSecretInput(input, configured, label = '已保存，留空不修改') {
  if (!input) return;
  input.value = '';
  input.placeholder = configured ? label : '留空则不修改';
  input.classList.toggle('secret-configured', !!configured);
}

function applySettingsToUi(settings) {
  appSettings = settings;
  setUsername.value = settings.app_username || '';
  markSecretInput(setPassword, settings.app_password_configured);
  setAria2Rpc.value = settings.aria2_rpc_url || '';
  markSecretInput(setAria2Secret, settings.aria2_secret_configured);
  setAria2Dir.value = settings.aria2_dir || '';
  checkLinksInput.checked = settings.check_links !== false;
  probeFilesInput.checked = settings.probe_quark_files !== false;
  filterBadLinksInput.checked = settings.filter_bad_links !== false;
  if (setAutoDownloadNewItems) setAutoDownloadNewItems.checked = !!settings.auto_download_new_subscription_items;
  if (setSubscriptionScheduler) setSubscriptionScheduler.checked = !!settings.subscription_scheduler_enabled;
  if (setSubscriptionInterval) setSubscriptionInterval.value = settings.subscription_check_interval_minutes || 60;
  if (setQuarkSaveEnabled) setQuarkSaveEnabled.checked = !!settings.quark_save_enabled;
  markSecretInput(setQuarkCookie, settings.quark_cookie_configured, 'Cookie 已保存，留空不修改');
  if (setQuarkSaveRoot) setQuarkSaveRoot.value = settings.quark_save_root || '';
  if (setQuarkSaveMovieDir) setQuarkSaveMovieDir.value = settings.quark_save_movie_dir || '/电影';
  if (setQuarkSaveSeriesDir) setQuarkSaveSeriesDir.value = settings.quark_save_series_dir || '/连续剧';
  if (setQuarkSaveAnimeDir) setQuarkSaveAnimeDir.value = settings.quark_save_anime_dir || '/动画';
  loadCustomCategories(settings.custom_categories || []);
  if (setNasSyncEnabled) setNasSyncEnabled.checked = !!settings.nas_sync_enabled;
  if (setNasSyncSource) setNasSyncSource.value = settings.nas_sync_source || '';
  if (setNasSyncTarget) setNasSyncTarget.value = settings.nas_sync_target || '';
  if (setPushOnUpdate) setPushOnUpdate.checked = settings.push_on_update !== false;
  if (setPushOnFailed) setPushOnFailed.checked = settings.push_on_failed !== false;
  if (setPushOnCompleted) setPushOnCompleted.checked = settings.push_on_completed !== false;
  if (setPushOnSave) setPushOnSave.checked = settings.push_on_save !== false;
  if (setPushSilent) setPushSilent.checked = !!settings.push_silent;
  if (setWecomUrl) setWecomUrl.value = settings.wecom_bot_url || '';
  if (setWxpusherToken) { setWxpusherToken.value = ''; setWxpusherToken.placeholder = settings.wxpusher_app_token ? '已保存，留空不修改' : 'AT_xxxxxxxx'; }
  if (setWxpusherUids) setWxpusherUids.value = settings.wxpusher_uids || '';
  if (setTelegramToken) { setTelegramToken.value = ''; setTelegramToken.placeholder = settings.telegram_bot_token ? '已保存，留空不修改' : '123456:ABC-DEF...'; }
  if (setTelegramChatId) setTelegramChatId.value = settings.telegram_chat_id || '';
  if (setBarkUrl) setBarkUrl.value = settings.bark_url || '';
  if (setGotifyUrl) setGotifyUrl.value = settings.gotify_url || '';
  if (setGotifyToken) { setGotifyToken.value = ''; setGotifyToken.placeholder = settings.gotify_token ? '已保存，留空不修改' : 'Axxxxxx'; }
  if (setPushplusToken) { setPushplusToken.value = ''; setPushplusToken.placeholder = settings.pushplus_token ? '已保存，留空不修改' : '你的 token'; }
  if (setServerchanKey) { setServerchanKey.value = ''; setServerchanKey.placeholder = settings.serverchan_key ? '已保存，留空不修改' : 'SCTxxxxxx'; }
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
  setStatus('正在搜索内置资源源...');
  resultsBox.className = 'results empty-card';
  resultsBox.innerHTML = '<p class="empty">正在搜索，请稍候...</p>';
  if (resultCount) resultCount.textContent = '搜索中';
  if (resultCloudTabs) { resultCloudTabs.classList.add('hidden'); resultCloudTabs.innerHTML = ''; }

  try {
    const data = await postJson('/api/search', {
      chat_id: chatId,
      keyword,
      limit: 50,
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
    setStatus('选择成功。可发送到 Aria2；夸克转存和 NAS 同步可在设置中启用。', 'ok');
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
  setStatus(`正在创建第 ${index} 条订阅...`);
  try {
    const shouldAutoSave = !!appSettings?.quark_save_enabled;
    const data = await postJson('/api/subscriptions', { chat_id: chatId, index, media_type: mediaType, notify_only: !shouldAutoSave });
    await loadSubscriptions();
    openSubscriptionModal(data.subscription || {}, { mode: 'create' });
    setStatus('订阅已创建，请先设置匹配、转存和重命名规则。', 'ok');
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
  openSubscriptionModal(sub, { mode: 'edit' });
}

function openSubscriptionModal(sub, options = {}) {
  const mode = options.mode || 'edit';
  const rules = sub.rules || {};
  if (subscriptionModalTitle) subscriptionModalTitle.textContent = mode === 'create' ? '设置新订阅' : '编辑订阅规则';
  if (subscriptionModalHint) {
    subscriptionModalHint.textContent = mode === 'create'
      ? '订阅已创建。确认匹配规则和自动转存开关；保存后会立即检查一次，并按设置转存到夸克网盘。'
      : '这里用于后续修改已有订阅的匹配、转存和重命名规则；保存后会立即检查一次。';
  }
  if (saveSubscriptionBtn) saveSubscriptionBtn.textContent = mode === 'create' ? '保存并启用订阅' : '保存规则';
  subEditId.value = sub.id || '';
  subEditTitle.value = sub.title || '';
  subEditSeason.value = sub.season || 1;
  subEditTotal.value = sub.total_episode_number || '';
  subEditMediaType.value = sub.media_type || 'series';
  subEditEnabled.checked = sub.enabled !== false;
  subEditCompleted.checked = !!sub.completed;
  if (subEditAutoSave) subEditAutoSave.checked = sub.notify_only === false;
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
  const originalText = saveSubscriptionBtn?.textContent;
  if (saveSubscriptionBtn) {
    saveSubscriptionBtn.disabled = true;
    saveSubscriptionBtn.textContent = '保存并检查中...';
  }
  if (previewSubPlanBtn) previewSubPlanBtn.disabled = true;
  setStatus('正在保存订阅规则并立即检查更新...');
  try {
    await postJson('/api/subscriptions/update', {
      subscription_id: id,
      title: subEditTitle.value.trim(),
      media_type: subEditMediaType.value,
      season: Number(subEditSeason.value || 1),
      total_episode_number: subEditTotal.value ? Number(subEditTotal.value) : null,
      enabled: subEditEnabled.checked,
      completed: subEditCompleted.checked,
      notify_only: !subEditAutoSave?.checked,
      rules: collectSubscriptionRulesFromModal()
    });
    closeSubscriptionModal();
    await checkSubscription(id, { showStartStatus: false });
  } catch (err) {
    setStatus(`订阅保存或检查失败：${err.message}`, 'error');
  } finally {
    if (saveSubscriptionBtn) {
      saveSubscriptionBtn.disabled = false;
      saveSubscriptionBtn.textContent = originalText || '保存规则';
    }
    if (previewSubPlanBtn) previewSubPlanBtn.disabled = false;
  }
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

function summarizeNasSyncs(nasSyncs = []) {
  const successes = nasSyncs.filter(item => item?.status === 'success').length;
  const failures = nasSyncs.filter(item => item?.status === 'failed').length;
  const skipped = nasSyncs.filter(item => item && !['success', 'failed'].includes(item.status)).length;
  const parts = [];
  if (successes) parts.push(`NAS 同步 ${successes} 个`);
  if (failures) parts.push(`NAS 失败 ${failures} 项`);
  if (skipped) parts.push(`NAS 跳过 ${skipped} 项`);
  return parts;
}

function buildSubscriptionCheckStatus(data) {
  const count = (data.new_files || []).length;
  const downloadCount = (data.downloads || []).filter(item => item && item.status !== 'skipped').length;
  const quarkCount = (data.quark_saves || []).length;
  const parts = [];
  if (count) parts.push(`发现 ${count} 个新文件`);
  if (downloadCount) parts.push(`Aria2 提交 ${downloadCount} 个`);
  if (quarkCount) parts.push(`夸克转存 ${quarkCount} 个`);
  parts.push(...summarizeNasSyncs(data.nas_syncs || []));
  return {
    message: parts.length ? parts.join('，') + '。' : '没有发现新文件。',
    hasNewFiles: !!count,
  };
}

async function checkSubscription(id, options = {}) {
  const { showStartStatus = true } = options;
  if (showStartStatus) setStatus('正在检查订阅更新...');
  try {
    const data = await postJson('/api/subscriptions/check', { subscription_id: id });
    await loadSubscriptions();
    await loadNotifications();
    addDownloadLogsFromSubscription(data.downloads || []);
    const status = buildSubscriptionCheckStatus(data);
    setStatus(status.message, status.hasNewFiles ? 'ok' : '');
    return data;
  } catch (err) {
    setStatus(`检查失败：${err.message}`, 'error');
    throw err;
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
    const parts = [];
    if (count) parts.push(`发现 ${count} 个新文件`);
    if (downloadCount) parts.push(`Aria2 提交 ${downloadCount} 个`);
    if (quarkCount) parts.push(`夸克转存 ${quarkCount} 个`);
    for (const result of data.results || []) parts.push(...summarizeNasSyncs(result.nas_syncs || []));
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

function formatBytes(bytes) {
  const value = Number(bytes || 0);
  if (!value) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function renderDrive(items = []) {
  currentDriveItems = items;
  
  // 应用排序和筛选
  const displayItems = sortAndFilterItems(items);
  
  if (drivePath) drivePath.textContent = driveStack.map(item => item.name).join(' / ');
  if (driveBackBtn) driveBackBtn.disabled = driveStack.length <= 1;
  
  // 更新选择模式 UI
  const selectAllBox = document.querySelector('#driveSelectAllBox');
  const selectAll = document.querySelector('#driveSelectAll');
  if (selectAllBox) selectAllBox.style.display = driveSelectMode ? 'block' : 'none';
  if (selectAll) {
    selectAll.checked = driveSelectedFids.size > 0 && driveSelectedFids.size === items.length;
    selectAll.indeterminate = driveSelectedFids.size > 0 && driveSelectedFids.size < items.length;
  }
  
  if (!driveBody) return;
  if (!displayItems.length) {
    driveBody.innerHTML = '<p class="empty">当前目录为空或没有符合筛选条件的文件。</p>';
    return;
  }
  
  driveBody.innerHTML = displayItems.map(item => {
    const size = formatFileSize(item.size || 0);
    const time = item.updated_at ? new Date(item.updated_at * 1000).toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit'
    }) : '-';
    const isSelected = driveSelectedFids.has(item.fid);
    
    return `
      <article class="drive-card ${item.is_dir ? 'is-folder' : 'is-file'} ${isSelected ? 'selected' : ''}" data-fid="${escapeHtml(item.fid)}" style="display: grid; grid-template-columns: ${driveSelectMode ? 'auto ' : ''}1fr 120px 150px 250px; align-items: center; padding: 12px; border-bottom: 1px solid var(--border); cursor: pointer;">
        ${driveSelectMode ? `<input type="checkbox" class="drive-checkbox" data-fid="${escapeHtml(item.fid)}" ${isSelected ? 'checked' : ''} onclick="event.stopPropagation();" />` : ''}
        <div class="drive-name ${item.is_dir ? 'dir' : 'file'}" style="display: flex; align-items: center; gap: 8px;">
          <span class="drive-icon">${item.is_dir ? '📁' : '📄'}</span>
          <span class="drive-label"><strong>${escapeHtml(item.name || '-')}</strong></span>
        </div>
        <div style="font-size: 13px; color: var(--muted);">${item.is_dir ? '-' : size}</div>
        <div style="font-size: 12px; color: var(--muted);">${time}</div>
        <div class="card-actions drive-actions" style="justify-content: flex-end; gap: 4px; display: flex; flex-wrap: wrap;">
          ${!item.is_dir ? `<button class="secondary small" data-drive-download="${escapeHtml(item.fid)}" data-drive-name="${escapeHtml(item.name)}" onclick="event.stopPropagation();">⬇️</button>` : ''}
          ${!item.is_dir ? `<button class="secondary small" data-drive-aria2="${escapeHtml(item.fid)}" data-drive-name="${escapeHtml(item.name)}" onclick="event.stopPropagation();">Aria2</button>` : ''}
          <button class="secondary small" data-drive-move="${escapeHtml(item.fid)}" onclick="event.stopPropagation();">移动</button>
          <button class="secondary small" data-drive-copy="${escapeHtml(item.fid)}" onclick="event.stopPropagation();">复制</button>
          <button class="secondary small" data-drive-rename="${escapeHtml(item.fid)}" onclick="event.stopPropagation();">重命名</button>
          <button class="secondary small" data-drive-delete="${escapeHtml(item.fid)}" onclick="event.stopPropagation();">删除</button>
        </div>
      </article>
    `;
  }).join('');
  
  // 添加事件监听
  driveBody.querySelectorAll('[data-fid]').forEach(el => {
    el.addEventListener('click', event => {
      if (event.target.closest('button') || event.target.closest('input[type="checkbox"]')) return;
      const fid = el.dataset.fid;
      const item = currentDriveItems.find(entry => entry.fid === fid);
      if (item?.is_dir) openDriveFolder(item);
    });
  });
  
  driveBody.querySelectorAll('.drive-checkbox').forEach(cb => {
    cb.addEventListener('change', (e) => {
      const fid = cb.dataset.fid;
      if (cb.checked) {
        driveSelectedFids.add(fid);
      } else {
        driveSelectedFids.delete(fid);
      }
      updateBatchButtons();
      renderDrive(currentDriveItems);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-download]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      downloadDriveItem(btn.dataset.driveDownload, btn.dataset.driveName);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-aria2]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      downloadDriveItem(btn.dataset.driveAria2, btn.dataset.driveName);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-move]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      moveDriveItem(btn.dataset.driveMove);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-copy]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      copyDriveItem(btn.dataset.driveCopy);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-rename]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      renameDriveItem(btn.dataset.driveRename);
    });
  });
  
  driveBody.querySelectorAll('[data-drive-delete]').forEach(btn => {
    btn.addEventListener('click', event => {
      event.stopPropagation();
      deleteDriveItem(btn.dataset.driveDelete);
    });
  });
}

function formatFileSize(bytes) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
}

async function loadDrive() {
  if (!driveBody) return;
  const current = driveStack[driveStack.length - 1];
  driveBody.innerHTML = '<p class="empty">正在读取夸克网盘...</p>';
  try {
    const data = await postJson('/api/quark-drive/list', { parent_fid: current.fid });
    if (!data.ok) {
      driveBody.innerHTML = `<p class="empty">${escapeHtml(data.message || '读取失败')}</p>`;
      setStatus(data.message || '读取夸克网盘失败', 'error');
      return;
    }
    renderDrive(data.items || []);
    setStatus('夸克网盘目录已刷新。', 'ok');
  } catch (err) {
    driveBody.innerHTML = `<p class="empty">读取失败：${escapeHtml(err.message)}</p>`;
    setStatus(`读取网盘失败：${err.message}`, 'error');
  }
}

function openDriveFolder(item) {
  driveStack.push({ fid: item.fid, name: item.name || '未命名目录' });
  loadDrive();
}

function backDriveFolder() {
  if (driveStack.length <= 1) return;
  driveStack.pop();
  loadDrive();
}

async function createDriveFolder() {
  const name = prompt('新文件夹名称');
  if (!name) return;
  const current = driveStack[driveStack.length - 1];
  const data = await postJson('/api/quark-drive/folder', { parent_fid: current.fid, name });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  if (data.ok) await loadDrive();
}

async function renameDriveItem(fid) {
  const item = currentDriveItems.find(entry => entry.fid === fid);
  if (!item) return;
  const name = prompt('新名称', item.name || '');
  if (!name || name === item.name) return;
  const data = await postJson('/api/quark-drive/rename', { fid, name });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  if (data.ok) await loadDrive();
}

async function deleteDriveItem(fid) {
  const item = currentDriveItems.find(entry => entry.fid === fid);
  if (!item) return;
  if (!confirm(`确定删除「${item.name}」吗？此操作会移到夸克回收站/删除区。`)) return;
  const data = await postJson('/api/quark-drive/delete', { fids: [fid] });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  if (data.ok) await loadDrive();
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

function collectSettingsPayload({ includeSecrets = true } = {}) {
  const payload = {
    app_username: setUsername.value.trim(),
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
    quark_save_movie_dir: setQuarkSaveMovieDir?.value.trim() || '/电影',
    quark_save_series_dir: setQuarkSaveSeriesDir?.value.trim() || '/连续剧',
    quark_save_anime_dir: setQuarkSaveAnimeDir?.value.trim() || '/动画',
    custom_categories: collectCustomCategories(),
    nas_sync_enabled: !!setNasSyncEnabled?.checked,
    nas_sync_source: setNasSyncSource?.value.trim() || '',
    nas_sync_target: setNasSyncTarget?.value.trim() || '',
    push_on_update: !!setPushOnUpdate?.checked,
    push_on_failed: !!setPushOnFailed?.checked,
    push_on_completed: !!setPushOnCompleted?.checked,
    push_on_save: !!setPushOnSave?.checked,
    push_silent: !!setPushSilent?.checked,
    wecom_bot_url: setWecomUrl?.value.trim() || '',
    wxpusher_app_token: includeSecrets && setWxpusherToken?.value ? setWxpusherToken.value : undefined,
    wxpusher_uids: setWxpusherUids?.value.trim() || '',
    telegram_bot_token: includeSecrets && setTelegramToken?.value ? setTelegramToken.value : undefined,
    telegram_chat_id: setTelegramChatId?.value.trim() || '',
    bark_url: setBarkUrl?.value.trim() || '',
    gotify_url: setGotifyUrl?.value.trim() || '',
    gotify_token: includeSecrets && setGotifyToken?.value ? setGotifyToken.value : undefined,
    pushplus_token: includeSecrets && setPushplusToken?.value ? setPushplusToken.value : undefined,
    serverchan_key: includeSecrets && setServerchanKey?.value ? setServerchanKey.value : undefined,
  };
  if (includeSecrets) {
    if (setPassword.value) payload.app_password = setPassword.value;
    if (setAria2Secret.value) payload.aria2_secret = setAria2Secret.value;
    if (setQuarkCookie?.value) payload.quark_cookie = setQuarkCookie.value;
  }
  return payload;
}

async function saveSettings() {
  saveSettingsBtn.disabled = true;
  const payload = collectSettingsPayload();
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

async function testSettingsEndpoint(button, url, label) {
  if (!button) return;
  button.disabled = true;
  setStatus(`正在测试 ${label}...`);
  try {
    const data = await postJson(url, collectSettingsPayload());
    setStatus(data.message || `${label} 测试完成`, data.ok ? 'ok' : 'error');
    if (url === '/api/settings/test/quark' && data.ok) await loadSettings();
  } catch (err) {
    setStatus(`${label} 测试失败：${err.message}`, 'error');
  } finally {
    button.disabled = false;
  }
}

async function testPushChannels() {
  if (!testPushBtn) return;
  testPushBtn.disabled = true;
  setStatus('正在测试推送渠道...');
  try {
    const data = await postJson('/api/push/test', {});
    if (data.ok) {
      const results = data.results || {};
      const details = Object.entries(results).map(([ch, r]) => `${ch}: ${r.ok ? '✅' : '❌'}`).join(', ');
      setStatus(`${data.message} (${details})`, 'ok');
    } else {
      setStatus(data.message || '推送测试失败', 'error');
    }
  } catch (err) {
    setStatus(`推送测试异常：${err.message}`, 'error');
  } finally {
    testPushBtn.disabled = false;
  }
}

searchBtn.addEventListener('click', search);
keywordInput.addEventListener('keydown', event => {
  if (event.key === 'Enter') search();
});
saveSettingsBtn.addEventListener('click', saveSettings);
testAria2Btn.addEventListener('click', testAria2);
testQuarkBtn?.addEventListener('click', () => testSettingsEndpoint(testQuarkBtn, '/api/settings/test/quark', '夸克 Cookie'));
testNasSyncBtn?.addEventListener('click', () => testSettingsEndpoint(testNasSyncBtn, '/api/settings/test/mount-paths', '挂载路径'));
testPushBtn?.addEventListener('click', testPushChannels);
driveBackBtn?.addEventListener('click', backDriveFolder);
driveRefreshBtn?.addEventListener('click', loadDrive);
driveNewFolderBtn?.addEventListener('click', createDriveFolder);

checkAllSubsBtn.addEventListener('click', checkAllSubscriptions);
markAllReadBtn.addEventListener('click', markAllNotificationsRead);
subscriptionForm?.addEventListener('submit', saveSubscriptionModal);
previewSubPlanBtn?.addEventListener('click', previewSubscriptionPlan);
document.querySelectorAll('[data-close-sub-modal]').forEach(el => el.addEventListener('click', closeSubscriptionModal));
document.addEventListener('keydown', event => { if (event.key === 'Escape') closeSubscriptionModal(); });

loadSettings()
  .then(loadSubscriptions)
  .then(loadNotifications)
  .then(loadDrive)
  .catch(err => setStatus(`加载设置失败：${err.message}`, 'error'));

// 手动订阅功能
let manualProbeData = null;

manualSubscribeBtn?.addEventListener('click', () => {
  manualSubscribeModal?.classList.remove('hidden');
  manualUrl.value = '';
  manualPassword.value = '';
  manualProbeResult?.classList.add('hidden');
  manualCreateSubBtn?.classList.add('hidden');
  manualProbeData = null;
});

document.querySelectorAll('[data-close-manual-modal]').forEach(el => {
  el.addEventListener('click', () => manualSubscribeModal?.classList.add('hidden'));
});

manualProbeBtn?.addEventListener('click', async () => {
  const url = manualUrl.value.trim();
  if (!url) {
    setStatus('请输入网盘链接', 'error');
    return;
  }
  manualProbeBtn.disabled = true;
  manualProbeBtn.textContent = '嗅探中...';
  try {
    const res = await fetch('/api/quark/probe', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url, password: manualPassword.value.trim() })
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.detail || '嗅探失败');
    
    manualProbeData = data;
    manualProbeContent.innerHTML = `
      <p><strong>状态：</strong>${data.ok ? '✅ 成功' : '❌ 失败'}</p>
      <p><strong>消息：</strong>${data.message || '-'}</p>
      <p><strong>文件数：</strong>${data.file_count || 0}</p>
      <p><strong>疑似集数：</strong>${data.episode_count || '未识别'}</p>
      ${data.files && data.files.length ? `<details><summary>文件列表</summary><ul>${data.files.slice(0, 20).map(f => `<li>${f}</li>`).join('')}${data.files.length > 20 ? '<li>...</li>' : ''}</ul></details>` : ''}
    `;
    manualProbeResult?.classList.remove('hidden');
    manualCreateSubBtn?.classList.remove('hidden');
    setStatus('嗅探完成', 'ok');
  } catch (err) {
    setStatus(`嗅探失败：${err.message}`, 'error');
  } finally {
    manualProbeBtn.disabled = false;
    manualProbeBtn.textContent = '嗅探文件';
  }
});

manualCreateSubBtn?.addEventListener('click', async () => {
  if (!manualProbeData) {
    setStatus('请先嗅探文件', 'error');
    return;
  }
  const title = prompt('请输入订阅标题：');
  if (!title) return;
  
  manualCreateSubBtn.disabled = true;
  try {
    const res = await fetch('/api/subscriptions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title,
        url: manualUrl.value.trim(),
        password: manualPassword.value.trim(),
        media_type: 'series',
        season: 1,
        enabled: true
      })
    });
    if (!res.ok) throw new Error('创建订阅失败');
    
    setStatus('订阅创建成功', 'ok');
    manualSubscribeModal?.classList.add('hidden');
    await loadSubscriptions();
  } catch (err) {
    setStatus(`创建失败：${err.message}`, 'error');
  } finally {
    manualCreateSubBtn.disabled = false;
  }
});

// 自定义分类管理
function loadCustomCategories(categories) {
  console.log('loadCustomCategories called with:', categories);
  if (!customCategoriesBox) {
    console.error('customCategoriesBox not found!');
    return;
  }
  customCategoriesBox.innerHTML = categories.map((cat, idx) => `
    <div class="form-grid" data-category-idx="${idx}">
      <label>分类名称<input class="custom-cat-name" value="${cat.name || ''}" placeholder="例如：综艺" /></label>
      <label>保存目录<input class="custom-cat-dir" value="${cat.dir || ''}" placeholder="例如：/综艺" /></label>
      <button type="button" class="secondary small remove-cat-btn" data-idx="${idx}">删除</button>
    </div>
  `).join('');
  updateMediaTypeOptions();
  document.querySelectorAll('.remove-cat-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const idx = parseInt(e.target.dataset.idx);
      const current = collectCustomCategories();
      current.splice(idx, 1);
      loadCustomCategories(current);
    });
  });
}

function collectCustomCategories() {
  const rows = customCategoriesBox.querySelectorAll('[data-category-idx]');
  const result = Array.from(rows).map(row => ({
    name: row.querySelector('.custom-cat-name')?.value.trim() || '',
    dir: row.querySelector('.custom-cat-dir')?.value.trim() || ''
  })).filter(c => c.name && c.dir);
  console.log('collectCustomCategories:', result);
  return result;
}

function updateMediaTypeOptions() {
  const categories = collectCustomCategories();
  const options = [
    { value: 'movie', label: '电影' },
    { value: 'series', label: '连续剧' },
    { value: 'anime', label: '动画' },
    ...categories.map(c => ({ value: `custom_${c.name}`, label: c.name }))
  ];
  const current = subEditMediaType?.value;
  subEditMediaType.innerHTML = options.map(o => 
    `<option value="${o.value}" ${o.value === current ? 'selected' : ''}>${o.label}</option>`
  ).join('');
}

addCustomCategoryBtn?.addEventListener('click', () => {
  const current = collectCustomCategories();
  current.push({ name: '', dir: '' });
  loadCustomCategories(current);
});


// ============ 推送历史 ============
const refreshPushHistoryBtn = document.querySelector('#refreshPushHistoryBtn');
const pushHistoryList = document.querySelector('#pushHistoryList');
const pushStatsBox = document.querySelector('#pushStatsBox');

async function loadPushHistory() {
  try {
    const [historyRes, statsRes] = await Promise.all([
      fetch('/api/push/history?limit=20', { headers }),
      fetch('/api/push/stats', { headers })
    ]);
    
    if (historyRes.ok) {
      const historyData = await historyRes.json();
      renderPushHistory(historyData.history || []);
    }
    
    if (statsRes.ok) {
      const statsData = await statsRes.json();
      renderPushStats(statsData);
    }
  } catch (err) {
    console.error('加载推送历史失败:', err);
    showStatus('加载推送历史失败', 'error');
  }
}

function renderPushStats(stats) {
  if (!pushStatsBox) return;
  
  const total = stats.total || 0;
  const successful = stats.successful || 0;
  const failed = stats.failed || 0;
  const successRate = total > 0 ? ((successful / total) * 100).toFixed(1) : 0;
  
  pushStatsBox.innerHTML = `
    <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; text-align: center;">
      <div>
        <div style="font-size: 24px; font-weight: 600; color: var(--accent);">${total}</div>
        <div style="font-size: 12px; color: var(--muted); margin-top: 4px;">总推送</div>
      </div>
      <div>
        <div style="font-size: 24px; font-weight: 600; color: var(--success);">${successful}</div>
        <div style="font-size: 12px; color: var(--muted); margin-top: 4px;">成功</div>
      </div>
      <div>
        <div style="font-size: 24px; font-weight: 600; color: var(--error);">${failed}</div>
        <div style="font-size: 12px; color: var(--muted); margin-top: 4px;">失败</div>
      </div>
      <div>
        <div style="font-size: 24px; font-weight: 600; color: var(--text);">${successRate}%</div>
        <div style="font-size: 12px; color: var(--muted); margin-top: 4px;">成功率</div>
      </div>
    </div>
  `;
}

function renderPushHistory(history) {
  if (!pushHistoryList) return;
  
  if (!history || history.length === 0) {
    pushHistoryList.innerHTML = '<div class="empty-card" style="padding: 32px;">暂无推送记录</div>';
    return;
  }
  
  pushHistoryList.innerHTML = history.map(record => {
    const time = new Date(record.timestamp * 1000).toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit'
    });
    
    const scenarioLabels = {
      'subscription_update': '订阅更新',
      'subscription_failed': '订阅失败',
      'subscription_completed': '订阅完结',
      'save_completed': '转存完成',
      'save_failed': '转存失败',
      'download_completed': '下载完成',
      'daily_summary': '每日摘要',
      'manual': '手动推送'
    };
    const scenarioLabel = scenarioLabels[record.scenario] || record.scenario;
    
    const channels = record.channels || [];
    const results = record.results || {};
    const successCount = channels.filter(ch => results[ch]).length;
    const allSuccess = successCount === channels.length;
    const allFailed = successCount === 0;
    
    let resultBadge = '';
    if (allSuccess) {
      resultBadge = '<span style="color: var(--success);">✓ 全部</span>';
    } else if (allFailed) {
      resultBadge = '<span style="color: var(--error);">✗ 全部</span>';
    } else {
      resultBadge = `<span style="color: var(--warning);">${successCount}/${channels.length}</span>`;
    }
    
    return `
      <div style="display: grid; grid-template-columns: 120px 1fr 100px 80px; padding: 12px; border-bottom: 1px solid var(--border); align-items: center;">
        <div style="font-size: 12px; color: var(--muted);">${time}</div>
        <div>
          <div style="font-weight: 500;">${record.title || '无标题'}</div>
          <div style="font-size: 12px; color: var(--muted); margin-top: 2px;">${(record.message || '').substring(0, 60)}${record.message && record.message.length > 60 ? '...' : ''}</div>
        </div>
        <div style="font-size: 13px;">${scenarioLabel}</div>
        <div style="text-align: center;">${resultBadge}</div>
      </div>
    `;
  }).join('');
}

refreshPushHistoryBtn?.addEventListener('click', loadPushHistory);

// 初始化时加载推送历史（如果在设置页面）
document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => {
    if (tab.dataset.page === 'settingsPage') {
      setTimeout(loadPushHistory, 100);
    }
  });
});

// ============ 网盘批量操作 ============
const driveSelectModeBtn = document.querySelector('#driveSelectModeBtn');
const driveBatchDeleteBtn = document.querySelector('#driveBatchDeleteBtn');
const driveSelectAll = document.querySelector('#driveSelectAll');

function toggleDriveSelectMode() {
  driveSelectMode = !driveSelectMode;
  if (!driveSelectMode) {
    driveSelectedFids.clear();
  }
  if (driveSelectModeBtn) {
    driveSelectModeBtn.textContent = driveSelectMode ? '取消选择' : '选择';
    driveSelectModeBtn.className = driveSelectMode ? 'secondary small' : 'secondary small';
  }
  updateBatchDeleteButton();
  renderDrive(currentDriveItems);
}

async function batchDeleteDrive() {
  if (driveSelectedFids.size === 0) return;
  if (!confirm(`确定删除选中的 ${driveSelectedFids.size} 项吗？此操作会移到夸克回收站/删除区。`)) return;
  
  const fids = Array.from(driveSelectedFids);
  const data = await postJson('/api/quark-drive/delete', { fids });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  
  if (data.ok) {
    driveSelectedFids.clear();
    updateBatchDeleteButton();
    await loadDrive();
  }
}

driveSelectModeBtn?.addEventListener('click', toggleDriveSelectMode);
driveBatchDeleteBtn?.addEventListener('click', batchDeleteDrive);

driveSelectAll?.addEventListener('change', (e) => {
  if (e.target.checked) {
    currentDriveItems.forEach(item => driveSelectedFids.add(item.fid));
  } else {
    driveSelectedFids.clear();
  }
  updateBatchDeleteButton();
  renderDrive(currentDriveItems);
});

// ============ 网盘完整功能 ============
const driveSelectModeBtn = document.querySelector('#driveSelectModeBtn');
const driveBatchDeleteBtn = document.querySelector('#driveBatchDeleteBtn');
const driveBatchMoveBtn = document.querySelector('#driveBatchMoveBtn');
const driveBatchCopyBtn = document.querySelector('#driveBatchCopyBtn');
const driveSelectAll = document.querySelector('#driveSelectAll');
const driveSortBy = document.querySelector('#driveSortBy');
const driveFilterType = document.querySelector('#driveFilterType');

let driveSortOrder = 'name';
let driveFilterFileType = 'all';

function updateBatchButtons() {
  const count = driveSelectedFids.size;
  if (driveBatchDeleteBtn) {
    driveBatchDeleteBtn.style.display = count > 0 ? 'inline-block' : 'none';
    driveBatchDeleteBtn.textContent = `批量删除 (${count})`;
  }
  if (driveBatchMoveBtn) {
    driveBatchMoveBtn.style.display = count > 0 ? 'inline-block' : 'none';
    driveBatchMoveBtn.textContent = `批量移动 (${count})`;
  }
  if (driveBatchCopyBtn) {
    driveBatchCopyBtn.style.display = count > 0 ? 'inline-block' : 'none';
    driveBatchCopyBtn.textContent = `批量复制 (${count})`;
  }
}

function toggleDriveSelectMode() {
  driveSelectMode = !driveSelectMode;
  if (!driveSelectMode) {
    driveSelectedFids.clear();
  }
  if (driveSelectModeBtn) {
    driveSelectModeBtn.textContent = driveSelectMode ? '取消选择' : '选择';
  }
  updateBatchButtons();
  renderDrive(currentDriveItems);
}

async function batchDeleteDrive() {
  if (driveSelectedFids.size === 0) return;
  if (!confirm(`确定删除选中的 ${driveSelectedFids.size} 项吗？此操作会移到夸克回收站/删除区。`)) return;
  
  const fids = Array.from(driveSelectedFids);
  const data = await postJson('/api/quark-drive/delete', { fids });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  
  if (data.ok) {
    driveSelectedFids.clear();
    updateBatchButtons();
    await loadDrive();
  }
}

async function batchMoveDrive() {
  if (driveSelectedFids.size === 0) return;
  const targetFid = await selectTargetFolder('选择移动目标文件夹');
  if (!targetFid) return;
  
  const fids = Array.from(driveSelectedFids);
  const data = await postJson('/api/quark-drive/move', { fids, target_fid: targetFid });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  
  if (data.ok) {
    driveSelectedFids.clear();
    updateBatchButtons();
    await loadDrive();
  }
}

async function batchCopyDrive() {
  if (driveSelectedFids.size === 0) return;
  const targetFid = await selectTargetFolder('选择复制目标文件夹');
  if (!targetFid) return;
  
  const fids = Array.from(driveSelectedFids);
  const data = await postJson('/api/quark-drive/copy', { fids, target_fid: targetFid });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  
  if (data.ok) {
    driveSelectedFids.clear();
    updateBatchButtons();
    await loadDrive();
  }
}

async function moveDriveItem(fid) {
  const targetFid = await selectTargetFolder('选择移动目标文件夹');
  if (!targetFid) return;
  
  const data = await postJson('/api/quark-drive/move', { fids: [fid], target_fid: targetFid });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  if (data.ok) await loadDrive();
}

async function copyDriveItem(fid) {
  const targetFid = await selectTargetFolder('选择复制目标文件夹');
  if (!targetFid) return;
  
  const data = await postJson('/api/quark-drive/copy', { fids: [fid], target_fid: targetFid });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
  if (data.ok) await loadDrive();
}

async function downloadDriveItem(fid, fileName) {
  const data = await postJson('/api/quark-drive/download', { fid, file_name: fileName });
  setStatus(data.message || '操作完成', data.ok ? 'ok' : 'error');
}

async function selectTargetFolder(title = '选择目标文件夹') {
  const current = driveStack[driveStack.length - 1];
  const options = [
    { fid: '0', name: '根目录' },
    ...driveStack.slice(1)
  ];
  
  // 简化版：直接用 prompt 输入路径或选择当前文件夹
  const choice = prompt(`${title}\n\n当前目录：${current.name}\n\n输入选项：\n0 - 根目录\n1 - 当前目录\n2 - 返回上级`, '1');
  if (!choice) return null;
  
  if (choice === '0') return '0';
  if (choice === '1') return current.fid;
  if (choice === '2' && driveStack.length > 1) return driveStack[driveStack.length - 2].fid;
  
  return null;
}

function getFileType(name, isDir) {
  if (isDir) return 'folder';
  const ext = name.split('.').pop().toLowerCase();
  const videoExts = ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'ts'];
  const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'svg'];
  const docExts = ['pdf', 'doc', 'docx', 'txt', 'xls', 'xlsx', 'ppt', 'pptx'];
  
  if (videoExts.includes(ext)) return 'video';
  if (imageExts.includes(ext)) return 'image';
  if (docExts.includes(ext)) return 'document';
  return 'other';
}

function sortAndFilterItems(items) {
  let filtered = items;
  
  // 筛选
  if (driveFilterFileType !== 'all') {
    filtered = items.filter(item => {
      const type = getFileType(item.name, item.is_dir);
      return type === driveFilterFileType;
    });
  }
  
  // 排序
  filtered.sort((a, b) => {
    // 文件夹优先
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    
    if (driveSortOrder === 'name') {
      return a.name.localeCompare(b.name, 'zh-CN');
    } else if (driveSortOrder === 'size') {
      return (b.size || 0) - (a.size || 0);
    } else if (driveSortOrder === 'time') {
      return (b.updated_at || 0) - (a.updated_at || 0);
    }
    return 0;
  });
  
  return filtered;
}

driveSelectModeBtn?.addEventListener('click', toggleDriveSelectMode);
driveBatchDeleteBtn?.addEventListener('click', batchDeleteDrive);
driveBatchMoveBtn?.addEventListener('click', batchMoveDrive);
driveBatchCopyBtn?.addEventListener('click', batchCopyDrive);

driveSelectAll?.addEventListener('change', (e) => {
  if (e.target.checked) {
    currentDriveItems.forEach(item => driveSelectedFids.add(item.fid));
  } else {
    driveSelectedFids.clear();
  }
  updateBatchButtons();
  renderDrive(currentDriveItems);
});

driveSortBy?.addEventListener('change', (e) => {
  driveSortOrder = e.target.value;
  renderDrive(currentDriveItems);
});

driveFilterType?.addEventListener('change', (e) => {
  driveFilterFileType = e.target.value;
  renderDrive(currentDriveItems);
});
