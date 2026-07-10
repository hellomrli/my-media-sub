(function (root, factory) {
  const api = factory(root);

  if (typeof module === 'object' && module.exports) {
    module.exports = api;
  }

  root.MediaSubApi = api;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const STATUS_MESSAGES = {
    400: '请求参数不正确',
    401: '认证已失效，请重新登录',
    403: '请求被拒绝',
    404: '请求的资源不存在',
    408: '请求超时，请稍后重试',
    409: '当前状态不允许此操作',
    413: '请求内容过大',
    429: '请求过于频繁，请稍后重试',
    500: '服务内部错误，请稍后重试',
    502: '上游服务请求失败',
    503: '服务暂时不可用，请稍后重试',
    504: '上游服务响应超时'
  };

  class ApiError extends Error {
    constructor(message, options = {}) {
      super(message || '请求失败');
      this.name = 'ApiError';
      this.status = Number(options.status || 0);
      this.code = options.code || '';
      this.payload = options.payload ?? null;
      this.response = options.response || null;
      this.url = options.url || '';
      this.isNetworkError = !!options.isNetworkError;
      if (options.cause !== undefined) this.cause = options.cause;
    }
  }

  function requestUrl(input) {
    if (typeof input === 'string') return input;
    if (input && typeof input.url === 'string') return input.url;
    return String(input || '');
  }

  function isPlainObject(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value);
  }

  function hasOwn(value, key) {
    return Object.prototype.hasOwnProperty.call(value, key);
  }

  function firstMessage(...values) {
    return values.find(value => typeof value === 'string' && value.trim())?.trim() || '';
  }

  function payloadMessage(payload) {
    if (typeof payload === 'string') {
      const value = payload.trim();
      if (/^(unauthorized|forbidden|not found|bad request|internal server error)$/i.test(value)) {
        return '';
      }
      return value;
    }
    if (!isPlainObject(payload)) return '';

    const nestedError = isPlainObject(payload.error) ? payload.error : null;
    return firstMessage(
      payload.message,
      payload.detail,
      nestedError && nestedError.message,
      typeof payload.error === 'string' && !/^[a-z0-9_]+$/i.test(payload.error)
        ? payload.error
        : ''
    );
  }

  function payloadCode(payload) {
    if (!isPlainObject(payload)) return '';
    if (typeof payload.code === 'string') return payload.code;
    if (typeof payload.error === 'string' && /^[a-z0-9_]+$/i.test(payload.error)) {
      return payload.error;
    }
    return '';
  }

  function statusMessage(status, statusText = '') {
    if (STATUS_MESSAGES[status]) return STATUS_MESSAGES[status];
    if (status >= 500) return '服务暂时不可用，请稍后重试';
    if (status >= 400) return `请求失败（HTTP ${status}）`;
    return statusText ? `请求失败：${statusText}` : '请求失败';
  }

  function unwrapApiData(payload, options = {}) {
    if (!isPlainObject(payload)) return payload;

    if (payload.ok === false) {
      throw new ApiError(
        options.errorMessage || payloadMessage(payload) || '请求失败',
        {
          code: payloadCode(payload),
          payload,
          url: options.url || ''
        }
      );
    }

    const hasData = hasOwn(payload, 'data');
    const legacyEnvelope = hasData && Object.keys(payload).every(key => key === 'data' || key === 'message');
    if ((payload.ok === true && hasData) || legacyEnvelope) {
      return payload.data ?? null;
    }

    return payload;
  }

  async function readErrorPayload(response) {
    if (!response || typeof response.clone !== 'function') return null;

    try {
      const text = await response.clone().text();
      if (!text.trim()) return null;

      const contentType = response.headers && typeof response.headers.get === 'function'
        ? response.headers.get('content-type') || ''
        : '';
      if (contentType.includes('json') || /^[\[{]/.test(text.trim())) {
        try {
          return JSON.parse(text);
        } catch (_) {
          return text.trim();
        }
      }
      return text.trim();
    } catch (_) {
      return null;
    }
  }

  function normalizeHeaders(headers, json) {
    const HeadersCtor = root.Headers;
    if (typeof HeadersCtor !== 'function') {
      const normalized = {...(headers || {})};
      if (!Object.keys(normalized).some(key => key.toLowerCase() === 'accept')) {
        normalized.Accept = 'application/json';
      }
      if (json !== undefined && !Object.keys(normalized).some(key => key.toLowerCase() === 'content-type')) {
        normalized['Content-Type'] = 'application/json';
      }
      return normalized;
    }

    const normalized = new HeadersCtor(headers || undefined);
    if (!normalized.has('Accept')) normalized.set('Accept', 'application/json');
    if (json !== undefined && !normalized.has('Content-Type')) {
      normalized.set('Content-Type', 'application/json');
    }
    return normalized;
  }

  function createApiClient(fetchImpl) {
    if (typeof fetchImpl !== 'function') {
      throw new TypeError('createApiClient requires a fetch implementation');
    }

    async function apiFetch(input, options = {}) {
      const {
        json,
        errorMessage,
        ...requestOptions
      } = options || {};
      const url = requestUrl(input);
      const headers = normalizeHeaders(requestOptions.headers, json);
      const init = {...requestOptions, headers};

      if (json !== undefined) {
        init.body = JSON.stringify(json);
      }

      let response;
      try {
        response = await fetchImpl(input, init);
      } catch (error) {
        if (error instanceof ApiError) throw error;
        const aborted = error && error.name === 'AbortError';
        throw new ApiError(
          errorMessage || (aborted ? '请求已取消' : '无法连接服务，请检查网络后重试'),
          {
            url,
            isNetworkError: !aborted,
            cause: error
          }
        );
      }

      if (!response.ok) {
        const payload = await readErrorPayload(response);
        throw new ApiError(
          errorMessage || payloadMessage(payload) || statusMessage(response.status, response.statusText),
          {
            status: response.status,
            code: payloadCode(payload),
            payload,
            response,
            url
          }
        );
      }

      return response;
    }

    async function apiJson(input, options = {}) {
      const response = await apiFetch(input, options);
      if (response.status === 204) return null;

      try {
        return await response.json();
      } catch (error) {
        throw new ApiError('服务返回了无法解析的数据', {
          status: response.status,
          response,
          url: requestUrl(input),
          cause: error
        });
      }
    }

    async function apiData(input, options = {}) {
      const payload = await apiJson(input, options);
      return unwrapApiData(payload, {
        errorMessage: options && options.errorMessage,
        url: requestUrl(input)
      });
    }

    return {apiData, apiFetch, apiJson};
  }

  const client = createApiClient((...args) => {
    if (typeof root.fetch !== 'function') {
      return Promise.reject(new TypeError('Fetch API is unavailable'));
    }
    return root.fetch(...args);
  });

  function getApiErrorMessage(error, fallback = '请求失败') {
    if (error instanceof ApiError && error.message) return error.message;
    if (error && error.name === 'AbortError') return '请求已取消';
    return fallback;
  }

  return Object.freeze({
    ApiError,
    apiData: client.apiData,
    apiFetch: client.apiFetch,
    apiJson: client.apiJson,
    createApiClient,
    getApiErrorMessage,
    unwrapApiData
  });
});
