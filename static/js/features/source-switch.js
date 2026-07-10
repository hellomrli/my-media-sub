(function (root, factory) {
  const tools = factory();
  if (typeof module === 'object' && module.exports) module.exports = tools;
  root.MediaSubSourceSwitch = tools;
})(typeof globalThis !== 'undefined' ? globalThis : window, function () {
  'use strict';

  function quality(candidate) {
    return (candidate && candidate.quality) || {
      score: 0,
      grade: '未评分',
      tone: 'danger',
      tags: [],
      risks: [],
      recommendation_reasons: []
    };
  }

  function sortCandidates(candidates) {
    return [...(Array.isArray(candidates) ? candidates : [])].sort((left, right) => {
      const score = Number(quality(right).score || 0) - Number(quality(left).score || 0);
      if (score) return score;
      return String((left && left.id) || '').localeCompare(String((right && right.id) || ''));
    });
  }

  function episodeRange(candidate) {
    const item = quality(candidate);
    if (item.episode_start && item.episode_end) {
      return item.episode_start === item.episode_end
        ? `E${item.episode_start}`
        : `E${item.episode_start}–E${item.episode_end}`;
    }
    return item.episode_count ? `${item.episode_count} 集` : '集数未知';
  }

  function historyLabel(item) {
    if (!item) return '-';
    if (item.status === 'rolled_back') return '已回滚';
    if (item.status === 'failed') return '失败';
    return item.automatic ? '自动换源' : '手动换源';
  }

  function canApplyPreview(preview, candidateId) {
    return !!(preview && preview.can_apply && preview.candidate && preview.candidate.id === candidateId);
  }

  return Object.freeze({canApplyPreview, episodeRange, historyLabel, quality, sortCandidates});
});
