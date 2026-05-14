const keywordInput = document.querySelector('#keyword');
const searchBtn = document.querySelector('#searchBtn');
const statusBox = document.querySelector('#status');
const resultsBox = document.querySelector('#results');
const selectedPanel = document.querySelector('#selected');
const selectedBody = document.querySelector('#selectedBody');

const chatId = `webui-${Math.random().toString(36).slice(2)}`;

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

async function postJson(url, payload) {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(data.detail || data.message || `请求失败：${res.status}`);
  }
  return data;
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
        <div class="url">${escapeHtml(item.url)}</div>
      </div>
      <button data-select="${item.index}">选择</button>
    </article>
  `).join('');

  resultsBox.querySelectorAll('[data-select]').forEach(btn => {
    btn.addEventListener('click', () => selectResult(Number(btn.dataset.select)));
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
    const data = await postJson('/api/search', { chat_id: chatId, keyword, limit: 12 });
    renderResults(data.results || []);
    setStatus(`找到 ${(data.results || []).length} 条结果。`, 'ok');
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
    `;
    selectedPanel.classList.remove('hidden');
    setStatus('选择成功。夸克转存和 OpenList/NAS 下载将在下一阶段接入。', 'ok');
  } catch (err) {
    setStatus(err.message, 'error');
  }
}

searchBtn.addEventListener('click', search);
keywordInput.addEventListener('keydown', event => {
  if (event.key === 'Enter') search();
});
