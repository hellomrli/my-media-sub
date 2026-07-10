const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const {
  analyzeSearchResult,
  compareSearchResults,
  formatSearchResultDate,
  resultValidity
} = require('../static/js/features/search-results.js');

const NOW = Date.parse('2026-07-10T12:00:00+08:00');

test('probe validity is used when is_valid is absent', () => {
  assert.equal(resultValidity({probe_info: {ok: true}}), true);
  assert.equal(resultValidity({probe_info: {ok: false}}), false);
  assert.equal(resultValidity({}), null);
});

test('4K HDR H.265 episodic resources receive useful quality insights', () => {
  const result = {
    note: '示例剧 S01 2160P WEB-DL HDR H265',
    url: 'https://pan.quark.cn/s/example',
    source: '测试源',
    datetime: '2026-07-09T12:00:00+08:00',
    images: ['https://example.com/poster.jpg'],
    probe_info: {
      ok: true,
      file_count: 3,
      episode_count: 2,
      files: [
        {name: 'Show.S01E01.2160p.HDR.HEVC.mkv', size: 4_000_000_000, is_dir: false},
        {name: 'Show.S01E02.2160p.HDR.HEVC.mkv', size: 4_100_000_000, is_dir: false},
        {name: 'Season 1', size: 0, is_dir: true}
      ]
    }
  };

  const insight = analyzeSearchResult(result, {now: NOW});
  assert.equal(insight.validity, true);
  assert.equal(insight.resolution, '4K');
  assert.equal(insight.videoCount, 2);
  assert.equal(insight.episodeCount, 2);
  assert.equal(insight.totalSize, 8_100_000_000);
  assert.ok(insight.tags.includes('HDR'));
  assert.ok(insight.tags.includes('H.265'));
  assert.ok(insight.score >= 85);
  assert.equal(insight.tone, 'excellent');
});

test('invalid advertising collections are capped as risky results', () => {
  const insight = analyzeSearchResult({
    note: '前五季大合集 公众号推广',
    url: 'https://pan.quark.cn/s/invalid',
    is_valid: false,
    datetime: '2026-07-10T08:00:00+08:00'
  }, {now: NOW});

  assert.ok(insight.score <= 24);
  assert.equal(insight.grade, '谨慎');
  assert.ok(insight.risks.includes('链接已失效'));
  assert.ok(insight.risks.includes('疑似广告内容'));
  assert.ok(insight.risks.includes('合集或跨季风险'));
});

test('quality sorting prefers better resources and keeps deterministic fallbacks', () => {
  const high = {...analyzeSearchResult({note: '电影 4K HDR', is_valid: true}, {now: NOW})};
  const low = {...analyzeSearchResult({note: '电影 480P', is_valid: true}, {now: NOW})};
  const results = [
    {note: '低', _insights: low},
    {note: '高', _insights: high}
  ];

  results.sort((a, b) => compareSearchResults(a, b, 'quality'));
  assert.equal(results[0].note, '高');
});

test('relative update labels are concise', () => {
  assert.equal(formatSearchResultDate('2026-07-10T01:00:00Z', {now: NOW}), '今天更新');
  assert.equal(formatSearchResultDate('2026-07-09T01:00:00Z', {now: NOW}), '昨天更新');
  assert.equal(formatSearchResultDate('', {now: NOW}), '时间未知');
});

test('shared source quality fixtures preserve historical frontend scoring', () => {
  const fixtures = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/source_quality.json'), 'utf8'));
  for (const fixture of fixtures) {
    const input = fixture.input;
    const result = {
      note: input.title,
      datetime: input.datetime,
      is_valid: input.validity,
      probe_info: input.probe_ok === null && input.probe_file_count === 0 && input.files.length === 0
        ? undefined
        : {
            ok: input.probe_ok,
            file_count: input.probe_file_count,
            episode_count: input.probe_episode_count,
            files: input.files
          }
    };
    const insight = analyzeSearchResult(result, {now: fixture.now_ms});
    assert.equal(insight.score, fixture.expected.score, fixture.name);
    assert.equal(insight.grade, fixture.expected.grade, fixture.name);
    assert.equal(insight.resolution, fixture.expected.resolution, fixture.name);
    assert.equal(insight.videoCount, fixture.expected.video_count, fixture.name);
    assert.equal(insight.episodeCount, fixture.expected.episode_count, fixture.name);
    assert.deepEqual(insight.risks, fixture.expected.risks, fixture.name);
    assert.equal(insight.backendAuthoritative, false, fixture.name);
  }
});

test('backend quality is authoritative when present', () => {
  const insight = analyzeSearchResult({
    note: '历史标题看起来只有 480P',
    is_valid: true,
    quality: {
      score: 91,
      grade: '旗舰',
      tone: 'excellent',
      tags: ['4K', 'HDR'],
      risks: [],
      resolution: '4K',
      file_count: 12,
      video_count: 10,
      episode_count: 10,
      episode_start: 1,
      episode_end: 10,
      total_size: 1234,
      updated_at: '2026-07-10T01:00:00Z',
      recommendation_reasons: ['后端推荐']
    }
  }, {now: NOW});
  assert.equal(insight.score, 91);
  assert.equal(insight.resolution, '4K');
  assert.equal(insight.backendAuthoritative, true);
  assert.deepEqual(insight.recommendationReasons, ['后端推荐']);
});
