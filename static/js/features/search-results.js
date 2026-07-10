(function (root, factory) {
  const sharedFormatters = root.MediaSubFormatters
    || (typeof module === 'object' && module.exports ? require('../core/formatters.js') : null);
  const tools = factory(sharedFormatters);

  if (typeof module === 'object' && module.exports) {
    module.exports = tools;
  }

  root.MediaSubSearchResults = tools;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (formatters) {
  'use strict';

  const VIDEO_EXTENSIONS = new Set([
    'mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'ts', 'm2ts', 'rmvb', 'iso'
  ]);

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  function asArray(value) {
    return Array.isArray(value) ? value : [];
  }

  function finiteNumber(value, fallback = 0) {
    const number = Number(value);
    return Number.isFinite(number) ? number : fallback;
  }

  function parseTimestamp(value) {
    if (formatters && typeof formatters.parseTimestamp === 'function') {
      return formatters.parseTimestamp(value);
    }
    if (value === null || value === undefined || value === '') return 0;
    if (typeof value === 'number' || /^\d+(?:\.\d+)?$/.test(String(value).trim())) {
      const number = finiteNumber(value);
      if (!number) return 0;
      return number < 1e12 ? number * 1000 : number;
    }
    const parsed = Date.parse(String(value));
    return Number.isFinite(parsed) ? parsed : 0;
  }

  function fileExtension(name) {
    const match = String(name || '').toLowerCase().match(/\.([a-z0-9]{2,5})$/);
    return match ? match[1] : '';
  }

  function isVideoFile(file) {
    if (!file || file.is_dir) return false;
    if (typeof file.category === 'string' && /video/i.test(file.category)) return true;
    return VIDEO_EXTENSIONS.has(fileExtension(file.name));
  }

  function resultFiles(result) {
    return asArray(result && result.probe_info && result.probe_info.files);
  }

  function resultValidity(result) {
    if (result && typeof result.is_valid === 'boolean') return result.is_valid;
    if (result && result.probe_info && typeof result.probe_info.ok === 'boolean') {
      return result.probe_info.ok;
    }
    return null;
  }

  function resultTitle(result) {
    return String(
      (result && (result.note || result.title || result.name || result.file_name || result.url)) || ''
    ).replace(/\s+/g, ' ').trim();
  }

  function resultPoster(result) {
    return asArray(result && result.images).find(value => typeof value === 'string' && value.trim()) || '';
  }

  function resultHost(result) {
    try {
      return new URL(result && result.url ? result.url : '').host;
    } catch (_) {
      return '';
    }
  }

  function inferEpisodeCount(files) {
    const episodes = new Set();
    for (const file of files) {
      if (!isVideoFile(file)) continue;
      const name = String(file.name || '');
      const patterns = [
        /S\d{1,2}E(\d{1,4})/ig,
        /(?:^|[\s._\-[【(])EP?\s*(\d{1,4})(?=$|[\s._\-\]】)])/ig,
        /第\s*(\d{1,4})\s*[集话]/g
      ];
      let matched = false;
      for (const pattern of patterns) {
        let match;
        while ((match = pattern.exec(name)) !== null) {
          episodes.add(Number(match[1]));
          matched = true;
        }
        if (matched) break;
      }
    }
    return episodes.size;
  }

  function resolutionInfo(text) {
    if (/\b(?:8k|4320p)\b/i.test(text)) return {label: '8K', score: 32};
    if (/\b(?:4k|uhd|2160p)\b/i.test(text)) return {label: '4K', score: 28};
    if (/\b(?:1440p|2k)\b/i.test(text)) return {label: '2K', score: 23};
    if (/\b(?:1080p?|fhd)\b/i.test(text)) return {label: '1080P', score: 19};
    if (/\b(?:720p?|hd)\b/i.test(text)) return {label: '720P', score: 11};
    if (/\b(?:480p?|sd)\b/i.test(text)) return {label: 'SD', score: 4};
    return {label: '', score: 0};
  }

  function unique(values) {
    return [...new Set(values.filter(Boolean))];
  }

  function analyzeSearchResult(result, options = {}) {
    const now = finiteNumber(options.now, Date.now());
    const backendQuality = result && result.quality;
    if (backendQuality && Number.isFinite(Number(backendQuality.score))) {
      const validity = resultValidity(result);
      const files = resultFiles(result);
      const updatedTimestamp = parseTimestamp(backendQuality.updated_at);
      return {
        title: resultTitle(result),
        poster: resultPoster(result),
        host: resultHost(result),
        source: String((result && result.source) || '').trim(),
        validity,
        validityLabel: validity === true ? '有效' : (validity === false ? '失效' : '未检测'),
        score: clamp(Math.round(finiteNumber(backendQuality.score)), 0, 100),
        grade: String(backendQuality.grade || '谨慎'),
        tone: String(backendQuality.tone || 'danger'),
        tags: unique(asArray(backendQuality.tags)).slice(0, 6),
        risks: unique(asArray(backendQuality.risks)),
        resolution: String(backendQuality.resolution || '未知清晰度'),
        fileCount: Math.max(0, finiteNumber(backendQuality.file_count)),
        videoCount: Math.max(0, finiteNumber(backendQuality.video_count)),
        episodeCount: Math.max(0, finiteNumber(backendQuality.episode_count)),
        episodeStart: backendQuality.episode_start ?? null,
        episodeEnd: backendQuality.episode_end ?? null,
        totalSize: Math.max(0, finiteNumber(backendQuality.total_size)),
        updatedTimestamp,
        updatedAt: String(backendQuality.updated_at || ''),
        recommendationReasons: unique(asArray(backendQuality.recommendation_reasons)),
        backendAuthoritative: true,
        files
      };
    }
    const files = resultFiles(result);
    const regularFiles = files.filter(file => !file.is_dir);
    const videoFiles = regularFiles.filter(isVideoFile);
    const probe = (result && result.probe_info) || null;
    const title = resultTitle(result);
    const searchableText = [title, ...videoFiles.slice(0, 30).map(file => file.name || '')].join(' ');
    const validity = resultValidity(result);
    const resolution = resolutionInfo(searchableText);
    const fileCount = Math.max(finiteNumber(probe && probe.file_count), files.length);
    const videoCount = videoFiles.length;
    const episodeCount = Math.max(
      finiteNumber(probe && probe.episode_count),
      inferEpisodeCount(videoFiles)
    );
    const totalSize = regularFiles.reduce((sum, file) => sum + Math.max(0, finiteNumber(file.size)), 0);
    const updatedTimestamp = Math.max(
      parseTimestamp(result && result.datetime),
      ...files.map(file => parseTimestamp(file.updated_at))
    );
    const ageDays = updatedTimestamp > 0 ? Math.max(0, (now - updatedTimestamp) / 86400000) : Infinity;

    const tags = [];
    if (resolution.label) tags.push(resolution.label);
    if (/\b(?:dolby[ ._-]?vision|dovi|dv)\b/i.test(searchableText)) tags.push('杜比视界');
    else if (/\b(?:hdr10\+?|hdr)\b/i.test(searchableText)) tags.push('HDR');
    if (/\b(?:av1)\b/i.test(searchableText)) tags.push('AV1');
    else if (/\b(?:x265|h\.?265|hevc)\b/i.test(searchableText)) tags.push('H.265');
    else if (/\b(?:x264|h\.?264|avc)\b/i.test(searchableText)) tags.push('H.264');
    if (/\b(?:atmos|truehd|dts[ ._-]?hd)\b/i.test(searchableText)) tags.push('高规格音轨');
    if (/\b(?:web[ ._-]?dl|webrip)\b/i.test(searchableText)) tags.push('WEB');
    else if (/\b(?:blu[ ._-]?ray|bdrip|remux)\b/i.test(searchableText)) tags.push('蓝光');
    if (episodeCount > 0) tags.push(`${episodeCount} 集`);

    const risks = [];
    if (validity === false) risks.push('链接已失效');
    if (/广告|推广|公众号|加群|解压密码|防失联/i.test(searchableText)) risks.push('疑似广告内容');
    if (/前\s*[一二三四五六七八九十\d]+\s*季|全\s*[一二三四五六七八九十\d]+\s*季|多季|合集|大合集/i.test(searchableText)) {
      risks.push('合集或跨季风险');
    }
    if (probe && probe.ok && fileCount > 0 && videoCount === 0) risks.push('未发现视频文件');

    let score = 24 + resolution.score;
    if (validity === true) score += 18;
    else if (validity === null) score += 5;
    else score -= 32;

    if (tags.includes('杜比视界') || tags.includes('HDR')) score += 7;
    if (tags.includes('AV1') || tags.includes('H.265')) score += 6;
    else if (tags.includes('H.264')) score += 3;
    if (tags.includes('高规格音轨')) score += 5;
    if (tags.includes('蓝光')) score += 5;
    else if (tags.includes('WEB')) score += 3;
    if (videoCount > 0) score += 7;
    if (episodeCount > 0) score += Math.min(11, 4 + Math.ceil(episodeCount / 6));
    if (fileCount > 0) score += 3;
    if (ageDays <= 7) score += 8;
    else if (ageDays <= 30) score += 5;
    else if (ageDays <= 180) score += 2;
    score -= risks.filter(risk => risk !== '链接已失效').length * 9;
    score = clamp(Math.round(score), 0, 100);
    if (validity === false) score = Math.min(score, 24);

    let grade = '谨慎';
    let tone = 'danger';
    if (score >= 85) {
      grade = '旗舰';
      tone = 'excellent';
    } else if (score >= 70) {
      grade = '优质';
      tone = 'good';
    } else if (score >= 55) {
      grade = '清晰';
      tone = 'fair';
    } else if (score >= 35) {
      grade = '普通';
      tone = 'muted';
    }

    return {
      title,
      poster: resultPoster(result),
      host: resultHost(result),
      source: String((result && result.source) || '').trim(),
      validity,
      validityLabel: validity === true ? '有效' : (validity === false ? '失效' : '未检测'),
      score,
      grade,
      tone,
      tags: unique(tags).slice(0, 6),
      risks: unique(risks),
      resolution: resolution.label || '未知清晰度',
      fileCount,
      videoCount,
      episodeCount,
      totalSize,
      updatedTimestamp,
      updatedAt: updatedTimestamp ? new Date(updatedTimestamp).toISOString() : '',
      recommendationReasons: [],
      backendAuthoritative: false,
      files
    };
  }

  function formatSearchResultDate(value, options = {}) {
    const timestamp = parseTimestamp(value);
    if (!timestamp) return '时间未知';
    const now = finiteNumber(options.now, Date.now());
    const days = Math.max(0, Math.floor((now - timestamp) / 86400000));
    if (days === 0) return '今天更新';
    if (days === 1) return '昨天更新';
    if (days < 7) return `${days} 天前`;

    const date = new Date(timestamp);
    const sameYear = date.getFullYear() === new Date(now).getFullYear();
    return new Intl.DateTimeFormat('zh-CN', sameYear
      ? {month: 'short', day: 'numeric'}
      : {year: 'numeric', month: 'short', day: 'numeric'}
    ).format(date);
  }

  function compareSearchResults(left, right, sort = 'quality') {
    const a = left && left._insights ? left._insights : analyzeSearchResult(left);
    const b = right && right._insights ? right._insights : analyzeSearchResult(right);
    if (sort === 'updated') return b.updatedTimestamp - a.updatedTimestamp || b.score - a.score;
    if (sort === 'files') return b.fileCount - a.fileCount || b.episodeCount - a.episodeCount || b.score - a.score;
    if (sort === 'title') return a.title.localeCompare(b.title, 'zh-CN', {numeric: true, sensitivity: 'base'});
    return b.score - a.score || b.updatedTimestamp - a.updatedTimestamp || b.fileCount - a.fileCount;
  }

  return Object.freeze({
    analyzeSearchResult,
    compareSearchResults,
    formatSearchResultDate,
    isVideoFile,
    parseTimestamp,
    resultFiles,
    resultTitle,
    resultValidity
  });
});
