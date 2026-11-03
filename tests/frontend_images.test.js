const test = require('node:test');
const assert = require('node:assert/strict');
const shell = require('../static/js/core/shell.js');
require('../static/js/features/search-results.js');
const searchPage = require('../static/js/features/search-page.js');

const BASE = 'https://media.example.com/?tab=search';

function harness(t) {
  const previousWindow = global.window;
  global.window = {location: {href: BASE}};
  t.after(() => { global.window = previousWindow; });
  const store = Object.defineProperties(shell.createStore(), Object.getOwnPropertyDescriptors(searchPage.createStore()));
  const timers = [];
  t.mock.method(global, 'setTimeout', callback => { timers.push(callback); return timers.length; });
  t.mock.method(global, 'clearTimeout', () => {});
  return {store, timers};
}

function image(source) {
  const classes = new Set();
  return {
    getAttribute(name) { return name === 'src' ? source : null; },
    get src() { return new URL(source || '', BASE).href; },
    set src(value) { source = value; },
    get currentSrc() { return source ? this.src : ''; },
    dataset: {}, hidden: false, isConnected: true,
    classList: {
      add(...values) { values.forEach(value => classes.add(value)); },
      remove(...values) { values.forEach(value => classes.delete(value)); },
      contains(value) { return classes.has(value); }
    }
  };
}

test('remote HTTP artwork is upgraded without loosening CSP or creating an arbitrary proxy', t => {
  const {store} = harness(t);
  assert.equal(store.remoteImageUrl('http://cdn.example.com/poster.jpg?token=abc'), 'https://cdn.example.com/poster.jpg?token=abc');
  assert.equal(store.remoteImageUrl('http://image.tmdb.org/t/p/w500/Poster.jpg'), '/api/images/tmdb/w500/Poster.jpg');
  assert.equal(store.remoteImageUrl('//cdn.example.com/poster.jpg'), 'https://cdn.example.com/poster.jpg');
  assert.equal(store.remoteImageUrl('/icons/icon-192.png'), '/icons/icon-192.png');
  for (const source of ['', ' ', 'javascript:alert(1)', 'file:///etc/passwd', 'https://user:password@cdn.example.com/a.jpg']) {
    assert.equal(store.remoteImageUrl(source), '', source);
  }
});

test('empty src attributes never schedule a retry of the current HTML document', t => {
  const {store, timers} = harness(t);
  assert.equal(store.imageRetrySource(''), '');
  for (const source of ['', null, ' ']) store.handleRemoteImageError({currentTarget: image(source)});
  assert.equal(timers.length, 0);
  assert.equal(store.recoverRemoteImages({querySelectorAll: () => [image('')]}), 0);
});

test('a retry cannot replace a newer image URL on a reused node', t => {
  const {store, timers} = harness(t);
  const element = image('/old.jpg');
  store.handleRemoteImageError({currentTarget: element});
  element.src = '/new.jpg';
  timers[0]();
  assert.equal(element.getAttribute('src'), '/new.jpg');
});

test('search images advance through candidates and remove artwork after exhaustion', t => {
  const {store, timers} = harness(t);
  const result = store.prepareSearchResult({images: [' /bad.jpg ', '/good.jpg', '/good.jpg', '', null]});
  assert.equal(store.searchResultPoster(result), '/bad.jpg');
  const element = image(store.searchResultPoster(result));
  function failCandidate() {
    for (let attempt = 0; attempt < 3; attempt++) {
      store.handleSearchResultImageError({currentTarget: element}, result);
      if (attempt < 2) timers.at(-1)();
    }
  }
  failCandidate();
  assert.equal(store.searchResultPoster(result), '/good.jpg');
  // A second view's late error must not skip the new candidate.
  store.handleSearchResultImageError({currentTarget: element}, result);
  assert.equal(store.searchResultPoster(result), '/good.jpg');
  element.src = store.searchResultPoster(result);
  failCandidate();
  assert.equal(store.searchResultPoster(result), '');
  assert.equal(store.searchResultPoster(store.prepareSearchResult({images: []})), '');
});

test('successful fallback clears failure state and a new search resets candidates', t => {
  const {store} = harness(t);
  const element = image('/good.jpg');
  element.classList.add('remote-image-failed');
  store.handleRemoteImageLoad({currentTarget: element});
  assert.equal(element.classList.contains('remote-image-failed'), false);
  assert.equal(element.dataset.imageRetryCount, '0');
  const result = store.prepareSearchResult({images: ['http://cdn.example.com/a.jpg', 'https://cdn.example.com/a.jpg']});
  assert.equal(store.searchResultPoster(result), 'https://cdn.example.com/a.jpg');
  assert.equal(result._posterCandidates.length, 1);
});
