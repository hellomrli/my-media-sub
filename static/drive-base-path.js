// 网盘基础目录支持
// 在页面切换到网盘时，根据设置的基础目录初始化

(function() {
  // 保存原始的 showPage
  const _originalShowPage = window.showPage;
  
  // 标记是否已初始化网盘
  let driveInitialized = false;
  
  // 包装 showPage，添加网盘页面初始化
  window.showPage = function(pageId, pushState) {
    _originalShowPage(pageId, pushState);
    
    // 当切换到网盘页面时，初始化基础目录
    if (pageId === 'drivePage' && !driveInitialized && appSettings) {
      driveInitialized = true;
      initDriveBasePath();
    }
  };
  
  // 初始化网盘基础目录
  async function initDriveBasePath() {
    const saveRoot = (appSettings.quark_save_root || '').trim();
    
    // 如果没有配置基础目录，使用根目录
    if (!saveRoot) {
      if (typeof loadDrive === 'function') {
        loadDrive();
      }
      return;
    }
    
    try {
      // 调用后端 API 解析路径到 fid
      const result = await postJson('/api/quark-drive/resolve-path', { path: saveRoot });
      
      if (result.ok && result.fid) {
        // 更新 driveStack 为基础目录
        driveStack = [{ fid: result.fid, name: result.name || saveRoot }];
        
        // 重新加载网盘
        if (typeof loadDrive === 'function') {
          loadDrive();
        }
      } else {
        // 解析失败，使用根目录
        console.warn('基础目录解析失败:', result.message);
        if (typeof setStatus === 'function') {
          setStatus(`基础目录 ${saveRoot} 不存在，已切换到根目录`, 'error');
        }
        driveStack = [{ fid: '0', name: '根目录' }];
        if (typeof loadDrive === 'function') {
          loadDrive();
        }
      }
    } catch (err) {
      console.error('初始化基础目录失败:', err);
      driveStack = [{ fid: '0', name: '根目录' }];
      if (typeof loadDrive === 'function') {
        loadDrive();
      }
    }
  }
  
  // 当设置更新后，重置初始化标记
  window.addEventListener('settings-updated', function() {
    driveInitialized = false;
  });
})();
