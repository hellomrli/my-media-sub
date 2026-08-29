// 原生 JS 前端的静态安全网，无 npm 依赖（CI 用 npx eslint@pinned 运行）。
// no-undef 拦截的正是 v2.2.14 浏览器 Push 失效那类 bug：模块作用域裸标识符
// 在 'use strict' 下直到用户触发才抛 ReferenceError，测试与 node --check 都不报。
// globals 手工枚举以免引入 npm 依赖；新用到的浏览器 API 在这里显式登记。

const browserGlobals = Object.fromEntries([
  'window', 'document', 'navigator', 'location', 'history',
  'localStorage', 'sessionStorage',
  'fetch', 'Headers', 'Request', 'Response', 'AbortController', 'AbortSignal',
  'URL', 'URLSearchParams', 'EventSource',
  'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval',
  'requestAnimationFrame', 'cancelAnimationFrame', 'queueMicrotask',
  'console', 'alert', 'confirm', 'prompt',
  'atob', 'btoa', 'crypto', 'performance',
  'CustomEvent', 'Event', 'Blob', 'File', 'FileReader', 'FormData',
  'DOMParser', 'MutationObserver', 'IntersectionObserver', 'ResizeObserver',
  'matchMedia', 'getComputedStyle',
  'Notification', 'PushManager',
  'structuredClone', 'TextDecoder', 'TextEncoder',
  // UMD 双环境模块（浏览器 + Node 测试）：module/require 由 typeof module 守卫，
  // self 是窗口与 Service Worker 共有的 global。
  'module', 'require', 'self'
].map(name => [name, 'readonly']));

const serviceWorkerGlobals = Object.fromEntries([
  'self', 'caches', 'clients', 'importScripts', 'registration',
  'fetch', 'Request', 'Response', 'Headers', 'URL', 'URLSearchParams',
  'location', 'setTimeout', 'clearTimeout', 'console', 'MediaSubPwaPolicy'
].map(name => [name, 'readonly']));

export default [
  {
    ignores: ['static/vendor/**', 'target/**', 'node_modules/**']
  },
  {
    files: ['static/**/*.js'],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: 'script',
      globals: browserGlobals
    },
    rules: {
      'no-undef': 'error',
      'no-unused-vars': ['error', {vars: 'all', args: 'none', caughtErrors: 'none'}]
    }
  },
  {
    files: ['static/service-worker.js'],
    languageOptions: {
      globals: serviceWorkerGlobals
    }
  }
];
