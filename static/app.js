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
const settingsPanel = document.querySelector('#settingsPanel');
const settingsBtn = document.querySelector('#settingsBtn');
const closeSettingsBtn = document.querySelector('#closeSettingsBtn');
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

const chatId = `webui-${Math.random().toString(36).slice(2)}`;
let appSettings = null;

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
    <label><input type="checkbox" data-cloud value="${escapeHtml(type)}" ${selected.includes(type) ? 'checked' : ''} /> ${escapeHtml(type)}</label>
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
  if (item.cloud_type) bits.push(`网盘：${escapeHtml(item.cloud_type)}`);
  if (check.state) bits.push(`有效性：${escapeHtml(check.state)}${check.summary ? `（${escapeHtml(check.summary)}）` : ''}`);
  if (probe.file_count !== undefined) bits.push(`文件：${escapeHtml(probe.file_count)}`);
  if (probe.episode_count) bits.push(`疑似剧集：${escapeHtml(probe.episode_count)}集`);
  if (probe.message) bits.push(`嗅探：${escapeHtml(probe.message)}`);
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
        <button class="secondary" data-aria2="${item.index}">Aria2</button>
      </div>
    </article>
  `).join('');

  resultsBox.querySelectorAll('[data-select]').forEach(btn => {
    btn.addEventListener('click', () => selectResult(Number(btn.dataset.select)));
  });
  resultsBox.querySelectorAll('[data-aria2]').forEach(btn => {
    btn.addEventListener('click', () => sendToAria2(Number(btn.dataset.aria2)));
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

async function sendToAria2(index) {
  setStatus(`正在把第 ${index} 条发送到 Aria2...`);
  try {
    const data = await postJson('/api/download/aria2', { chat_id: chatId, index });
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
settingsBtn.addEventListener('click', () => settingsPanel.classList.remove('hidden'));
closeSettingsBtn.addEventListener('click', () => settingsPanel.classList.add('hidden'));
saveSettingsBtn.addEventListener('click', saveSettings);
testAria2Btn.addEventListener('click', testAria2);

loadSettings().catch(err => setStatus(`加载设置失败：${err.message}`, 'error'));
