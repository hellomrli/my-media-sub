const test = require('node:test');
const assert = require('node:assert/strict');

let request;
global.MediaSubApi = {
  apiFetch: async (url, init) => {
    request = {url, init};
    return new Response(JSON.stringify({
      ok: true,
      data: {id: 'job-1', status: 'queued', priority: 'high'}
    }), {status: 200, headers: {'Content-Type': 'application/json'}});
  }
};

const jobsModule = require('../static/js/stores/jobs.js');

test('jobs store labels legacy priority as normal and includes it in copied detail', () => {
  const store = jobsModule.createStore();
  store.formatTime = value => String(value || '-');

  assert.equal(store.jobPriorityLabel(undefined), '普通');
  assert.equal(store.jobPriorityLabel('high'), '高');
  assert.match(store.jobSummaryText({
    id: 'job-1', title: '测试', kind: 'metadata_scrape', status: 'queued',
    progress: 0, created_at: 1, updated_at: 1, payload: {}, result: {}
  }), /优先级：普通/);
  assert.equal(store.jobErrorClassLabel('timed_out'), '执行超时');
});

test('jobs store updates queued priority through the stable API contract', async () => {
  const store = jobsModule.createStore();
  store.jobs = [{id: 'job-1', status: 'queued', priority: 'normal'}];
  store.showNotification = () => {};
  store.apiErrorMessage = (_error, fallback) => fallback;

  await store.setJobPriority(store.jobs[0], 'high');

  assert.equal(request.url, '/api/jobs/job-1/priority');
  assert.equal(request.init.method, 'POST');
  assert.deepEqual(JSON.parse(request.init.body), {priority: 'high'});
  assert.equal(store.jobs[0].priority, 'high');
});
