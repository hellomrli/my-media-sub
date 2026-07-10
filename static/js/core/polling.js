(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubPolling = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  function createPollingRegistry(scheduler = root) {
    const resources = new Map();

    function stop(name) {
      const cleanup = resources.get(name);
      if (!cleanup) return false;
      resources.delete(name);
      cleanup();
      return true;
    }

    function track(name, cleanup) {
      stop(name);
      resources.set(name, cleanup);
      return name;
    }

    function startInterval(name, callback, intervalMs) {
      const timer = scheduler.setInterval(callback, intervalMs);
      track(name, () => scheduler.clearInterval(timer));
      return timer;
    }

    function listen(name, target, eventName, handler, options) {
      if (!target || typeof target.addEventListener !== 'function') return null;
      target.addEventListener(eventName, handler, options);
      track(name, () => target.removeEventListener(eventName, handler, options));
      return handler;
    }

    function own(name, resource, close = value => value && value.close()) {
      if (!resource) return null;
      track(name, () => close(resource));
      return resource;
    }

    function stopAll() {
      [...resources.keys()].forEach(stop);
    }

    return {
      startInterval,
      listen,
      own,
      stop,
      stopAll,
      has: name => resources.has(name),
      names: () => [...resources.keys()],
      get size() { return resources.size; }
    };
  }

  function createStore() {
    return {
      pollingRegistry: null,
      lifecycleDestroyed: false,

      ensurePollingRegistry() {
        if (!this.pollingRegistry) this.pollingRegistry = createPollingRegistry(root);
        return this.pollingRegistry;
      },

      startPolling(name, callback, intervalMs) {
        return this.ensurePollingRegistry().startInterval(name, callback, intervalMs);
      },

      stopPolling(name) {
        if (!this.pollingRegistry) return false;
        return this.pollingRegistry.stop(name);
      },

      listenLifecycle(name, target, eventName, handler, options) {
        return this.ensurePollingRegistry().listen(name, target, eventName, handler, options);
      },

      ownLifecycle(name, resource, close) {
        return this.ensurePollingRegistry().own(name, resource, close);
      },

      setupLifecycleCleanup() {
        if (!root || typeof root.addEventListener !== 'function') return;
        this.listenLifecycle('app-pagehide', root, 'pagehide', () => this.destroy());
      },

      destroy() {
        if (this.lifecycleDestroyed) return;
        this.lifecycleDestroyed = true;
        if (this.pollingRegistry) this.pollingRegistry.stopAll();
        this.searchProgressTimer = null;
        this.notificationsPoller = null;
        this.downloadsPoller = null;
        this.updateProgressTimer = null;
        this.jobEvents = null;
      }
    };
  }

  return {createPollingRegistry, createStore};
});
