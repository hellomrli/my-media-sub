(function (root) {
  'use strict';

  const modules = [
    root.MediaSubPolling,
    root.MediaSubRouter,
    root.MediaSubNotifications,
    root.MediaSubDownloads,
    root.MediaSubDrive,
    root.MediaSubJobs,
    root.MediaSubSubscriptions,
    root.MediaSubUpdates,
    root.MediaSubSettings,
    root.MediaSubDiagnostics,
    root.MediaSubPwa,
    root.MediaSubDashboard,
    root.MediaSubSearchPage,
    root.MediaSubCalendarPage,
    root.MediaSubShell
  ];

  function composeStores(stores) {
    const appStore = {};
    for (const store of stores) {
      Object.defineProperties(appStore, Object.getOwnPropertyDescriptors(store));
    }
    return appStore;
  }

  function app() {
    const missing = modules.find(moduleApi => !moduleApi || typeof moduleApi.createStore !== 'function');
    if (missing) throw new Error('前端模块未完整加载');
    return composeStores(modules.map(moduleApi => moduleApi.createStore()));
  }

  root.MediaSubApp = {app, composeStores};
  root.app = app;

  if (typeof module === 'object' && module.exports) {
    module.exports = root.MediaSubApp;
  }
})(typeof globalThis !== 'undefined' ? globalThis : window);
