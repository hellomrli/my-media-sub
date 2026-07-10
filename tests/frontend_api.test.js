const test = require('node:test');
const assert = require('node:assert/strict');

const {
  ApiError,
  createApiClient,
  getApiErrorMessage,
  unwrapApiData
} = require('../static/js/core/api.js');

test('apiFetch preserves the native Response on success and adds JSON defaults', async () => {
  let receivedInit;
  const client = createApiClient(async (_input, init) => {
    receivedInit = init;
    return new Response(JSON.stringify({data: [1, 2, 3]}), {
      status: 200,
      headers: {'Content-Type': 'application/json'}
    });
  });

  const response = await client.apiFetch('/api/example', {
    method: 'POST',
    json: {name: 'demo'}
  });

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), {data: [1, 2, 3]});
  assert.equal(receivedInit.headers.get('Accept'), 'application/json');
  assert.equal(receivedInit.headers.get('Content-Type'), 'application/json');
  assert.equal(receivedInit.body, JSON.stringify({name: 'demo'}));
});

test('apiFetch exposes the backend message and error code for non-2xx responses', async () => {
  const client = createApiClient(async () => new Response(JSON.stringify({
    error: 'validation_error',
    message: '订阅名称不能为空'
  }), {
    status: 400,
    headers: {'Content-Type': 'application/json'}
  }));

  await assert.rejects(
    client.apiFetch('/api/subscriptions'),
    error => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.status, 400);
      assert.equal(error.code, 'validation_error');
      assert.equal(error.message, '订阅名称不能为空');
      return true;
    }
  );
});

test('apiFetch normalizes plain-text auth failures', async () => {
  const client = createApiClient(async () => new Response('Unauthorized', {status: 401}));

  await assert.rejects(
    client.apiFetch('/api/settings'),
    error => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.status, 401);
      assert.equal(error.message, '认证已失效，请重新登录');
      return true;
    }
  );
});

test('apiFetch normalizes network failures without leaking browser-specific text', async () => {
  const client = createApiClient(async () => {
    throw new TypeError('Failed to fetch');
  });

  await assert.rejects(
    client.apiFetch('/api/jobs'),
    error => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.status, 0);
      assert.equal(error.isNetworkError, true);
      assert.equal(error.message, '无法连接服务，请检查网络后重试');
      return true;
    }
  );
});

test('apiJson reports malformed successful responses consistently', async () => {
  const client = createApiClient(async () => new Response('<html>oops</html>', {
    status: 200,
    headers: {'Content-Type': 'text/html'}
  }));

  await assert.rejects(
    client.apiJson('/api/jobs'),
    error => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.message, '服务返回了无法解析的数据');
      return true;
    }
  );
});

test('apiData unwraps current and legacy envelopes while preserving raw payloads', async () => {
  assert.deepEqual(unwrapApiData({ok: true, data: {list: [1, 2]}}), {list: [1, 2]});
  assert.deepEqual(unwrapApiData({data: [1, 2], message: null}), [1, 2]);
  assert.deepEqual(unwrapApiData({success: true, message: 'legacy raw response'}), {
    success: true,
    message: 'legacy raw response'
  });

  const client = createApiClient(async () => new Response(JSON.stringify({
    ok: true,
    data: {active: [], waiting: [], stopped: []}
  }), {
    status: 200,
    headers: {'Content-Type': 'application/json'}
  }));
  assert.deepEqual(await client.apiData('/api/drive/aria2/tasks'), {
    active: [],
    waiting: [],
    stopped: []
  });
});

test('apiData rejects a logical error envelope returned with HTTP 200', async () => {
  const client = createApiClient(async () => new Response(JSON.stringify({
    ok: false,
    error: 'operation_failed',
    message: '操作未完成'
  }), {
    status: 200,
    headers: {'Content-Type': 'application/json'}
  }));

  await assert.rejects(
    client.apiData('/api/example'),
    error => {
      assert.ok(error instanceof ApiError);
      assert.equal(error.code, 'operation_failed');
      assert.equal(error.message, '操作未完成');
      return true;
    }
  );
});

test('getApiErrorMessage keeps safe fallbacks for unknown errors', () => {
  assert.equal(getApiErrorMessage(new ApiError('后端错误'), '操作失败'), '后端错误');
  assert.equal(getApiErrorMessage(new Error('implementation detail'), '操作失败'), '操作失败');
});
