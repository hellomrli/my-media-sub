// 这是要添加到 app.js 的浏览器历史支持代码

// 修改 showPage 函数，添加 pushState 参数
function showPage(pageId, pushState = true) {
  document.querySelectorAll('.page').forEach(p => p.classList.toggle('active', p.id === pageId));
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.page === pageId));
  const activeTab = document.querySelector(`.tab[data-page="${pageId}"]`);
  if (pageTitle && activeTab) pageTitle.textContent = activeTab.textContent.trim();
  
  // 添加到浏览器历史
  if (pushState) {
    const url = new URL(window.location);
    url.searchParams.set('page', pageId);
    window.history.pushState({ page: pageId }, '', url);
  }
}

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
window.addEventListener('DOMContentLoaded', () => {
  const urlParams = new URLSearchParams(window.location.search);
  const pageId = urlParams.get('page');
  if (pageId) {
    showPage(pageId, false);
  } else {
    // 初始页面也添加到历史
    const initialPage = document.querySelector('.page.active')?.id || 'searchPage';
    window.history.replaceState({ page: initialPage }, '', `?page=${initialPage}`);
  }
});
