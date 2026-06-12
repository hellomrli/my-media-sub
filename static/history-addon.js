// 在 app.js 末尾添加的浏览器历史支持代码
// 不修改原有 showPage 函数，而是包装它

(function() {
  // 保存原始的 showPage 函数
  const originalShowPage = window.showPage;
  
  // 包装 showPage 函数，添加历史支持
  window.showPage = function(pageId, skipHistory) {
    // 调用原始函数
    originalShowPage(pageId);
    
    // 如果不是从历史导航触发，则添加到历史
    if (!skipHistory) {
      const url = new URL(window.location);
      url.searchParams.set('page', pageId);
      window.history.pushState({ page: pageId }, '', url);
    }
  };
  
  // 处理浏览器后退/前进
  window.addEventListener('popstate', (event) => {
    if (event.state && event.state.page) {
      showPage(event.state.page, true);
    } else {
      const urlParams = new URLSearchParams(window.location.search);
      const pageId = urlParams.get('page') || 'searchPage';
      showPage(pageId, true);
    }
  });
  
  // 初始化：设置当前页面的历史状态
  const urlParams = new URLSearchParams(window.location.search);
  const pageId = urlParams.get('page');
  if (pageId) {
    showPage(pageId, true);
  } else {
    const initialPage = document.querySelector('.page.active')?.id || 'searchPage';
    window.history.replaceState({ page: initialPage }, '', `?page=${initialPage}`);
  }
})();
