(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubSubscriptions = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiData, apiFetch, getApiErrorMessage} = api;
  const mediaFormatters = root.MediaSubFormatters || {};
  const searchResultTools = root.MediaSubSearchResults || {};
  const subscriptionDetailTools = root.MediaSubSubscriptionDetail || {};
  const calendarTools = root.MediaSubCalendar || {};
  const sourceSwitchTools = root.MediaSubSourceSwitch || {};
  const automationEventTools = root.MediaSubAutomationEvents || {};
  const ux = root.MediaSubUx || {};

  function createStore() {
    return {
    subscriptions: [],
    lastCheckResult: null,
    checkingAllSubscriptions: false,
    scrapingAllMetadata: false,
    showSubscriptionDialog: false,
    subscriptionStatusTab: 'active',
    subscriptionViewMode: 'table',
    subscriptionVisibleLimit: 100,
    selectedSubscriptionIds: [],
    subscriptionBatchLoading: false,
    subscriptionActionMenuId: '',
    selectedSubscriptionId: '',
    subscriptionDetail: null,
    subscriptionDetailLoading: false,
    subscriptionDetailRequestId: 0,
    subscriptionDetailError: '',
    subscriptionAutomationPipeline: null,
    subscriptionAutomationLoading: false,
    subscriptionAutomationError: '',
    automationSummary: {total: 0, by_status: {}, by_stage: {}, recent_failed: [], stuck: [], retry_hotspots: {}, success_rate: 0},
    automationSummaryLoading: false,
    automationRetryingId: '',
    subscriptionEpisodeFilter: 'all',
    subscriptionActivityLimit: 20,
    subscriptionStatusTabs: [
      {id: 'active', name: '追更中'},
      {id: 'invalid', name: '已失效'},
      {id: 'completed', name: '已完结'}
    ],
    subscriptionMode: 'once',  // 'once' 或 'continuous'
    subscriptionDialogTab: 'content',
    subscriptionEditingId: null,
    showSourceSwitchDialog: false,
    sourceSwitchSubscriptionId: '',
    sourceSwitchSubscriptionTitle: '',
    sourceSwitchCandidates: [],
    sourceSwitchLoading: false,
    sourceSwitchSearching: false,
    sourceSwitchApplyingId: '',
    sourceSwitchPreviewingId: '',
    sourceSwitchPreview: null,
    sourceSwitchHistory: [],
    sourceSwitchHistoryLoading: false,
    sourceSwitchRollbackLoading: false,
    sourceSwitchError: '',
    currentSearchResult: null,  // 当前操作的搜索结果
    metadataSearching: false,
    metadataResults: [],
    showManualMetadataDialog: false,
    manualMetadataSubscriptionId: '',
    manualMetadataSubscriptionTitle: '',
    manualMetadataQuery: '',
    manualMetadataMediaType: 'series',
    manualMetadataSearching: false,
    manualMetadataApplying: false,
    manualMetadataResults: [],
    renamePreviewLoading: false,
    renamePreview: null,
    renamePreviewError: '',
    newSubscription: {
      title: '',
      url: '',
      password: '',
      original_url: '',
      original_password: '',
      original_current_episode: 0,
      original_known_episode_count: 0,
      original_transferred_count: 0,
      original_start_episode_number: '',
      media_type: 'series',
      season: 1,
      target_path: '',
      target_fid: '0',
      target_dir_name: '',
      rename_template: '',
      custom_dir: false,
      custom_rename: false,
      notify_only: false,
      sync_download_enabled: false,
      sync_download_dir: '',
      strm_enabled: false,
      metadata: null,
      manual_schedule_enabled: false,
      manual_schedule_start_date: '',
      manual_schedule_weekdays: [],
      manual_schedule_air_time: '',
      manual_schedule_interval_weeks: 1,
      manual_schedule_first_episode: 1,
      manual_schedule_total_episodes: '',
      include_keywords_text: '',
      exclude_keywords_text: '预告, 花絮, 解说, 彩蛋, trailer, preview',
      match_regex: '',
      episode_regex: '',
      source_search_keywords_text: '',
      source_exclude_keywords_text: '',
      source_prefer_keywords_text: '',
      ignore_extensions: false,
      rename_regex: '',
      rename_replacement: '',
      only_latest: false,
      skip_existing_transferred: true,
      duplicate_episode_strategy: 'highest_quality',
      conflict_strategy: 'skip',
      auto_create_target_dir: true,
      start_episode_number: '',
      keep_progress_on_source_change: true,
      continue_from_current_episode: true,
      finish_after_episode: '',
      rule_preset_id: '',
      preview_samples: ''
    },

    // 通知
    showTransferModal: false,
    transferTargetResult: null,
    transferTargetFid: '0',
    transferTargetPath: '根目录',
    transferBrowseItems: [],
    transferBrowseFidStack: [{fid: '0', name: '根目录'}],
    showAria2DirDialog: false,
    aria2DirItems: [],
    aria2DirRoot: '',
    aria2DirCurrent: '',
    aria2DirParent: '',
    aria2DirLoading: false,
    aria2DirError: '',
    ruleCenter: {
      preset_id: '',
      preset_name: '',
      preset_description: '',
      media_type: 'series',
      season: 1,
      title: '示例剧集',
      rename_template: '{title}.S{season}E{episode}.{ext}',
      include_keywords_text: '',
      exclude_keywords_text: '',
      match_regex: '',
      episode_regex: '',
      source_search_keywords_text: '',
      source_exclude_keywords_text: '',
      source_prefer_keywords_text: '',
      rename_regex: '',
      rename_replacement: '',
      ignore_extensions: false,
      only_latest: false,
      skip_existing_transferred: true,
      duplicate_episode_strategy: 'highest_quality',
      conflict_strategy: 'skip',
      finish_after_episode: '',
      samples: '178重置版.mp4\n第179话.mp4\nShow.S01E180.1080p.mkv'
    },
    ruleCenterPreview: null,
    ruleCenterPreviewLoading: false,
    ruleCenterPreviewError: '',

    // 设置
    get subscriptionWizardSteps() {
      const steps = [{id: 'content', name: '内容'}];
      if (this.subscriptionMode === 'continuous' || this.subscriptionEditingId) {
        steps.push({id: 'schedule', name: '排期'});
        steps.push({id: 'download', name: '下载'});
      }
      return steps;
    },

    get rulePresets() {
      const presets = Array.isArray(this.settings.rule_presets) && (this.settings.rule_presets.length || this.settingsLoaded)
        ? this.settings.rule_presets
        : this.defaultRulePresets();
      return presets.map(preset => this.normalizeRulePreset(preset));
    },

    createSubscriptionFromSearch(result) {
      this.openSubscriptionDialog(result, 'continuous');
    },

    defaultExcludeKeywords() {
      return '预告, 花絮, 解说, 彩蛋, trailer, preview';
    },

    setSubscriptionMode(mode) {
      this.subscriptionMode = mode;
      if (mode === 'once') {
        this.subscriptionDialogTab = 'content';
      }
    },

    resetSubscriptionForm() {
      this.subscriptionEditingId = null;
      this.subscriptionMode = 'continuous';
      this.subscriptionDialogTab = 'content';
      this.currentSearchResult = null;
      this.renamePreview = null;
      this.renamePreviewError = '';
      this.newSubscription = {
        title: '',
        url: '',
        password: '',
        original_url: '',
        original_password: '',
        original_current_episode: 0,
        original_known_episode_count: 0,
        original_transferred_count: 0,
        original_start_episode_number: '',
        media_type: 'series',
        season: 1,
        target_path: '',
        target_fid: '0',
        target_dir_name: '',
        rename_template: '',
        custom_dir: false,
        custom_rename: false,
        notify_only: false,
        sync_download_enabled: false,
        sync_download_dir: '',
        strm_enabled: false,
        metadata: null,
        manual_schedule_enabled: false,
        manual_schedule_start_date: '',
        manual_schedule_weekdays: [],
        manual_schedule_air_time: '',
        manual_schedule_interval_weeks: 1,
        manual_schedule_first_episode: 1,
        manual_schedule_total_episodes: '',
        include_keywords_text: '',
        exclude_keywords_text: this.defaultExcludeKeywords(),
        match_regex: '',
      episode_regex: '',
      source_search_keywords_text: '',
      source_exclude_keywords_text: '',
      source_prefer_keywords_text: '',
        ignore_extensions: false,
        rename_regex: '',
        rename_replacement: '',
        only_latest: false,
        skip_existing_transferred: true,
        duplicate_episode_strategy: 'highest_quality',
      conflict_strategy: 'skip',
        auto_create_target_dir: true,
        start_episode_number: '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: true,
        finish_after_episode: '',
        rule_preset_id: '',
        preview_samples: ''
      };
    },

    openBlankSubscriptionDialog() {
      this.resetSubscriptionForm();
      this.showSubscriptionDialog = true;
    },

    sampleFilesFromSearchResult(result) {
      const files = (result && result.probe_info && result.probe_info.files) || [];
      return files.filter(file => !file.is_dir).map(file => file.name).join('\n');
    },

    searchResultTitle(result) {
      return String(
        (result && (result.note || result.title || result.name || result.file_name || result.url)) || ''
      ).trim();
    },

    searchResultValidityBadgeClass(result) {
      const validity = this.searchResultInsights(result).validity;
      if (validity === true) return 'badge badge-success';
      if (validity === false) return 'badge badge-danger';
      return 'badge badge-muted';
    },

    searchResultValidityLabel(result) {
      return this.searchResultInsights(result).validityLabel;
    },

    stripResourceTags(value) {
      let title = String(value || '').replace(/\s+/g, ' ').trim();
      const tagPattern = /\s*(?:\[[^\]]*\]|【[^】]*】|（[^）]*）|\([^)]*\))\s*$/;
      const metadataPattern = /^(?:\d{4}|20\d{2}|19\d{2}|.*(?:\d{3,4}p|4k|8k|hdr|dv|web-?dl|bluray|bdrip|hdtv|x26[45]|hevc|aac|flac|内封|内嵌|简繁|简中|繁中|中字|字幕|双语|多语|全\s*\d+\s*集|全集|完结|更新|第\s*\d+\s*集).*)$/i;

      let changed = true;
      while (changed) {
        changed = false;
        title = title.replace(tagPattern, (match) => {
          const content = match.replace(/^[\s\[【（(]+|[\]】）)\s]+$/g, '').trim();
          if (!content || metadataPattern.test(content)) {
            changed = true;
            return '';
          }
          return match;
        }).trim();
      }

      return title;
    },

    trimBilingualResourceTitle(value) {
      let title = String(value || '').trim();
      if (!title) return title;

      const kanaIndex = title.search(/[\u3040-\u30ff]/);
      if (kanaIndex > 0 && /[\u4e00-\u9fff]/.test(title.slice(0, kanaIndex))) {
        title = title.slice(0, kanaIndex).replace(/[\s·・,，/|:：\-–—_]+$/g, '').trim();
      }

      const separatedParts = title
        .split(/\s+[|/／]\s+|\s+[|/／]\s*|\s*[|/／]\s+/)
        .map(part => part.trim())
        .filter(Boolean);
      if (separatedParts.length > 1 && /[\u4e00-\u9fff]/.test(separatedParts[0])) {
        title = separatedParts[0];
      }

      return title;
    },

    trimResourceSuffixes(value) {
      return String(value || '')
        .replace(/\s+(?:S\d{1,2}|Season\s*\d+|第[一二三四五六七八九十\d]+季)$/i, '')
        .replace(/\s+(?:\d{3,4}p|4k|8k|web-?dl|bluray|bdrip|hdtv|x26[45]|hevc|aac)$/i, '')
        .replace(/[\s._-]+$/g, '')
        .trim();
    },

    inferSubscriptionTitle(rawTitle) {
      const original = String(rawTitle || '').trim();
      if (!original || /^https?:\/\//i.test(original)) return original;

      let title = original
        .replace(/\s+/g, ' ')
        .trim();

      title = this.stripResourceTags(title);
      title = this.trimBilingualResourceTitle(title);
      title = this.trimResourceSuffixes(title);

      return title || original;
    },

    // 打开订阅对话框（支持立即转存或连续订阅）
    openSubscriptionDialog(result, mode = 'once') {
      if (!this.settings.quark_cookie && !this.settings.quark_cookie_configured) {
        this.showNotification('error', '请先在设置中配置夸克 Cookie');
        return;
      }

      this.currentSearchResult = result;
      this.subscriptionEditingId = null;
      this.subscriptionMode = mode;
      this.subscriptionDialogTab = 'content';
      this.renamePreview = null;
      this.renamePreviewError = '';
      const sourceTitle = this.searchResultTitle(result);
      this.newSubscription = {
        title: this.inferSubscriptionTitle(sourceTitle),
        url: result.url,
        password: result.password || '',
        original_url: result.url,
        original_password: result.password || '',
        original_current_episode: 0,
        original_known_episode_count: 0,
        original_transferred_count: 0,
        original_start_episode_number: '',
        media_type: 'series',
        season: 1,
        target_path: '',
        target_fid: '0',
        target_dir_name: '',
        rename_template: '',
        custom_dir: false,
        custom_rename: false,
        notify_only: false,
        sync_download_enabled: false,
        sync_download_dir: '',
        strm_enabled: false,
        metadata: null,
        manual_schedule_enabled: false,
        manual_schedule_start_date: '',
        manual_schedule_weekdays: [],
        manual_schedule_air_time: '',
        manual_schedule_interval_weeks: 1,
        manual_schedule_first_episode: 1,
        manual_schedule_total_episodes: '',
        include_keywords_text: '',
        exclude_keywords_text: this.defaultExcludeKeywords(),
        match_regex: '',
      episode_regex: '',
      source_search_keywords_text: '',
      source_exclude_keywords_text: '',
      source_prefer_keywords_text: '',
        ignore_extensions: false,
        rename_regex: '',
        rename_replacement: '',
        only_latest: false,
        skip_existing_transferred: true,
        duplicate_episode_strategy: 'highest_quality',
      conflict_strategy: 'skip',
        auto_create_target_dir: true,
        start_episode_number: '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: true,
        finish_after_episode: '',
        rule_preset_id: '',
        preview_samples: this.sampleFilesFromSearchResult(result)
      };
      this.showSubscriptionDialog = true;
      this.metadataResults = [];
      this.searchMetadataForSubscription(true);
      if (this.newSubscription.preview_samples) this.previewSubscriptionRename(true);
    },

    openEditSubscriptionDialog(sub) {
      const rules = sub.rules || {};
      this.subscriptionEditingId = sub.id;
      this.subscriptionMode = 'continuous';
      this.subscriptionDialogTab = 'content';
      this.currentSearchResult = null;
      this.renamePreview = null;
      this.renamePreviewError = '';
      this.newSubscription = {
        title: sub.title || '',
        url: sub.url || '',
        password: sub.password || '',
        original_url: sub.url || '',
        original_password: sub.password || '',
        original_current_episode: Number(sub.current_episode_number || 0),
        original_known_episode_count: Array.isArray(sub.known_episodes) ? sub.known_episodes.length : 0,
        original_transferred_count: Array.isArray(sub.transferred_file_keys) ? sub.transferred_file_keys.length : (Array.isArray(sub.transferred_files) ? sub.transferred_files.length : 0),
        original_start_episode_number: sub.start_episode_number || '',
        media_type: sub.media_type || 'series',
        season: this.normalizeSeason(sub.season),
        target_path: '',
        target_fid: '0',
        target_dir_name: rules.target_dir || '',
        rename_template: rules.rename_template || '',
        custom_dir: !!rules.target_dir,
        custom_rename: !!rules.rename_template,
        notify_only: !!sub.notify_only,
        sync_download_enabled: !!sub.sync_download_enabled,
        sync_download_dir: sub.sync_download_dir || '',
        strm_enabled: !!sub.strm_enabled,
        metadata: sub.metadata || null,
        manual_schedule_enabled: !!sub.manual_schedule,
        manual_schedule_start_date: (sub.manual_schedule && sub.manual_schedule.start_date) || '',
        manual_schedule_weekdays: (sub.manual_schedule && sub.manual_schedule.weekdays) || [],
        manual_schedule_air_time: (sub.manual_schedule && sub.manual_schedule.air_time) || '',
        manual_schedule_interval_weeks: Number((sub.manual_schedule && sub.manual_schedule.interval_weeks) || 1),
        manual_schedule_first_episode: Number((sub.manual_schedule && sub.manual_schedule.first_episode_number) || 1),
        manual_schedule_total_episodes: (sub.manual_schedule && sub.manual_schedule.total_episodes) || '',
        include_keywords_text: (rules.include_keywords || []).join(', '),
        exclude_keywords_text: (rules.exclude_keywords || []).join(', ') || this.defaultExcludeKeywords(),
        match_regex: rules.match_regex || '',
        episode_regex: rules.episode_regex || '',
        source_search_keywords_text: (rules.source_search_keywords || []).join(', '),
        source_exclude_keywords_text: (rules.source_exclude_keywords || []).join(', '),
        source_prefer_keywords_text: (rules.source_prefer_keywords || []).join(', '),
        ignore_extensions: !!rules.ignore_extensions,
        rename_regex: rules.rename_regex || '',
        rename_replacement: rules.rename_replacement || '',
        only_latest: !!rules.only_latest,
        skip_existing_transferred: rules.skip_existing_transferred !== false,
        duplicate_episode_strategy: rules.duplicate_episode_strategy || 'highest_quality',
        conflict_strategy: rules.conflict_strategy || 'skip',
        auto_create_target_dir: rules.auto_create_target_dir !== false,
        start_episode_number: sub.start_episode_number || '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: sub.media_type !== 'movie' && Number(sub.current_episode_number || 0) > 0,
        finish_after_episode: rules.finish_after_episode || '',
        rule_preset_id: sub.rule_preset_id || '',
        preview_samples: ((sub.last_probe && sub.last_probe.files) || []).map(file => file.name).join('\n')
      };
      this.showSubscriptionDialog = true;
      this.metadataResults = [];
      this.previewSubscriptionRename(true);
    },

    // 显示订阅对话框（兼容旧版）
    showSubscriptionDialogLegacy(result) {
      this.openSubscriptionDialog(result, 'continuous');
    },

    formatTime(timestamp) {
      return mediaFormatters.formatDateTime(timestamp);
    },

    normalizeSeason(value) {
      const season = Number(value);
      return Number.isFinite(season) && season > 0 ? Math.floor(season) : 1;
    },

    normalizeStartEpisode(value) {
      const episode = Number(value);
      return Number.isFinite(episode) && episode > 0 ? Math.floor(episode) : 0;
    },

    subscriptionWizardStepIndex(tabId = this.subscriptionDialogTab) {
      const index = this.subscriptionWizardSteps.findIndex(step => step.id === tabId);
      return index >= 0 ? index : 0;
    },

    subscriptionWizardStepClass(step, index) {
      const current = this.subscriptionWizardStepIndex();
      if (index < current) return 'bg-success/20 text-success/85 border-success/30';
      if (index === current) return 'bg-primary/20 text-primary/85 border-primary/40';
      return 'bg-app text-muted/75 border-border';
    },

    canGoPreviousSubscriptionStep() {
      return this.subscriptionWizardStepIndex() > 0;
    },

    canGoNextSubscriptionStep() {
      return this.subscriptionWizardStepIndex() < this.subscriptionWizardSteps.length - 1;
    },

    previousSubscriptionStep() {
      const index = this.subscriptionWizardStepIndex();
      if (index <= 0) return;
      this.subscriptionDialogTab = this.subscriptionWizardSteps[index - 1].id;
    },

    nextSubscriptionStep() {
      const index = this.subscriptionWizardStepIndex();
      if (index >= this.subscriptionWizardSteps.length - 1) return;
      this.subscriptionDialogTab = this.subscriptionWizardSteps[index + 1].id;
      if (this.subscriptionDialogTab === 'rename' && !this.renamePreview && this.newSubscription.preview_samples) {
        this.previewSubscriptionRename(true);
      }
    },

    subscriptionStartEpisodePayload() {
      if (this.newSubscription.media_type === 'movie') return 0;
      return this.normalizeStartEpisode(this.newSubscription.start_episode_number);
    },

    toggleManualScheduleWeekday(day) {
      const value = Number(day);
      const weekdays = new Set((this.newSubscription.manual_schedule_weekdays || []).map(Number));
      if (weekdays.has(value)) weekdays.delete(value);
      else weekdays.add(value);
      this.newSubscription.manual_schedule_weekdays = Array.from(weekdays).sort((left, right) => left - right);
    },

    manualSchedulePayload() {
      if (!this.newSubscription.manual_schedule_enabled) return null;
      const total = this.normalizeStartEpisode(this.newSubscription.manual_schedule_total_episodes);
      return {
        start_date: String(this.newSubscription.manual_schedule_start_date || '').trim(),
        weekdays: (this.newSubscription.manual_schedule_weekdays || []).map(Number).filter(day => day >= 1 && day <= 7),
        air_time: String(this.newSubscription.manual_schedule_air_time || '').trim(),
        interval_weeks: Math.max(1, Number(this.newSubscription.manual_schedule_interval_weeks || 1)),
        first_episode_number: Math.max(1, this.normalizeStartEpisode(this.newSubscription.manual_schedule_first_episode) || 1),
        total_episodes: total > 0 ? total : null
      };
    },

    subscriptionSourceChanged() {
      if (!this.subscriptionEditingId) return false;
      return String(this.newSubscription.url || '') !== String(this.newSubscription.original_url || '')
        || String(this.newSubscription.password || '') !== String(this.newSubscription.original_password || '');
    },

    sourceChangeNextStartEpisode() {
      const current = Math.max(0, Number(this.newSubscription.original_current_episode || 0));
      if (!this.newSubscription.keep_progress_on_source_change) return 0;
      if (this.newSubscription.media_type !== 'movie' && this.newSubscription.continue_from_current_episode && current > 0) {
        return current + 1;
      }
      return this.normalizeStartEpisode(this.newSubscription.start_episode_number || this.newSubscription.original_start_episode_number);
    },

    sourceChangeStats() {
      return [
        {label: '当前进度', value: this.newSubscription.original_current_episode ? `第 ${this.newSubscription.original_current_episode} 集` : '-'},
        {label: '保存后起始', value: this.sourceChangeNextStartEpisode() ? `第 ${this.sourceChangeNextStartEpisode()} 集` : '不限制'},
        {label: '已知集数', value: String(this.newSubscription.original_known_episode_count || 0)},
        {label: '已转存', value: String(this.newSubscription.original_transferred_count || 0)}
      ];
    },

    sourceChangeSummary() {
      const current = Math.max(0, Number(this.newSubscription.original_current_episode || 0));
      if (!this.newSubscription.keep_progress_on_source_change) return '清空追更记录后按新资源重新识别。';
      if (this.newSubscription.media_type !== 'movie' && this.newSubscription.continue_from_current_episode && current > 0) {
        return `保留记录，并从第 ${current + 1} 集继续追更。`;
      }
      return '保留已知文件、已知集数和已转存记录。';
    },

    mediaTypeLabel(type) {
      const labels = {movie: '电影', series: '连续剧', anime: '动画'};
      const custom = this.customCategoryByType(type);
      if (custom) return custom.name || '自定义类别';
      if (String(type || '').startsWith('custom_')) return '自定义类别';
      return labels[type] || type || '-';
    },

    customCategoryMediaType(category) {
      return category && category.id ? `custom_${category.id}` : '';
    },

    customCategoryByType(type) {
      if (!type || !String(type).startsWith('custom_')) return null;
      const id = String(type).slice('custom_'.length);
      return (this.settings.custom_categories || []).find(category => category.id === id) || null;
    },

    generateCustomCategoryId() {
      if (window.crypto && window.crypto.randomUUID) {
        return window.crypto.randomUUID().replace(/-/g, '').slice(0, 12);
      }
      return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
    },

    normalizeCustomCategories() {
      const usedIds = new Set();
      this.settings.custom_categories = (Array.isArray(this.settings.custom_categories) ? this.settings.custom_categories : [])
        .map(category => {
          const item = category && typeof category === 'object' ? category : {};
          let id = String(item.id || '').trim();
          if (!id || usedIds.has(id)) id = this.generateCustomCategoryId();
          usedIds.add(id);
          return {
            id,
            name: String(item.name || ''),
            dir: String(item.dir || ''),
            aria2_dir: String(item.aria2_dir || '')
          };
        });
    },

    addCustomCategory() {
      if (!Array.isArray(this.settings.custom_categories)) {
        this.settings.custom_categories = [];
      }
      this.settings.custom_categories.push({
        id: this.generateCustomCategoryId(),
        name: '',
        dir: '',
        aria2_dir: ''
      });
    },

    removeCustomCategory(index) {
      if (!Array.isArray(this.settings.custom_categories)) return;
      const removed = this.settings.custom_categories[index];
      this.settings.custom_categories.splice(index, 1);
      if (removed && this.newSubscription.media_type === this.customCategoryMediaType(removed)) {
        this.setSubscriptionMediaType('series');
      }
    },

    setSubscriptionMediaType(type) {
      this.newSubscription.media_type = type || 'series';
      this.updateSubscriptionDefaults();
    },

    defaultRulePresets() {
      return [
        {id: 'standard_tv', name: '标准剧集', description: 'S01E01 风格，适合电视剧和动画', media_type: 'series', rules: {...this.emptyTransferRules(), rename_template: '{title}.S{season}E{episode}.{ext}'}},
        {id: 'episode_only', name: '仅集数', description: '生成 01.mp4 / 02.mkv，适合短目录', media_type: 'series', rules: {...this.emptyTransferRules(), rename_template: '{episode}.{ext}'}},
        {id: 'original_keep', name: '保留原名', description: '尽量不改文件名，只做过滤和去重', media_type: 'series', rules: {...this.emptyTransferRules(), rename_template: '{original}.{ext}', duplicate_episode_strategy: 'latest_upload'}},
        {id: 'movie_title', name: '电影标题', description: '电影直接使用标题和扩展名', media_type: 'movie', rules: {...this.emptyTransferRules(), rename_template: '{title}.{ext}', duplicate_episode_strategy: 'largest_size', exclude_keywords: [...this.splitRuleWords(this.defaultExcludeKeywords()), 'sample']}}
      ];
    },

    emptyTransferRules() {
      return {
        target_dir: '',
        auto_create_target_dir: true,
        skip_existing_transferred: true,
        duplicate_episode_strategy: 'highest_quality',
      conflict_strategy: 'skip',
        include_keywords: [],
        exclude_keywords: this.splitRuleWords(this.defaultExcludeKeywords()),
        match_regex: '',
      episode_regex: '',
      source_search_keywords_text: '',
      source_exclude_keywords_text: '',
      source_prefer_keywords_text: '',
        ignore_extensions: false,
        rename_regex: '',
        rename_replacement: '',
        rename_template: '',
        only_latest: false,
        notify_on_update: true,
        notify_on_invalid: true,
        check_interval_minutes: Number(this.settings.subscription_check_interval_minutes || 60),
        check_weekdays: [],
        finish_after_episode: null
      };
    },

    normalizeRulePreset(preset) {
      const raw = preset && typeof preset === 'object' ? preset : {};
      const legacyRules = {
        ...this.emptyTransferRules(),
        rename_template: String(raw.template || ''),
        exclude_keywords: this.splitRuleWords(raw.exclude || this.defaultExcludeKeywords()),
        duplicate_episode_strategy: raw.duplicate || 'highest_quality'
      };
      const rules = raw.rules && typeof raw.rules === 'object'
        ? {...this.emptyTransferRules(), ...raw.rules}
        : legacyRules;
      return {
        id: String(raw.id || this.generateCustomCategoryId()),
        name: String(raw.name || '未命名规则'),
        description: String(raw.description || ''),
        media_type: String(raw.media_type || ''),
        rules
      };
    },

    ruleWordsText(words) {
      return Array.isArray(words) ? words.join(', ') : '';
    },

    rulePresetById(id) {
      return this.rulePresets.find(item => item.id === id) || null;
    },

    applyRulePresetToSubscription(id) {
      const preset = this.rulePresetById(id);
      if (!preset) return;
      this.newSubscription.rule_preset_id = preset.id;
      this.applyRulesToSubscription(preset.rules);
      if (preset.media_type) {
        this.newSubscription.media_type = preset.media_type;
      }
      this.updateSubscriptionDefaults();
      this.previewSubscriptionRename(true);
      this.showNotification('success', `已应用规则：${preset.name}`);
    },

    applyRulesToSubscription(rules) {
      const next = {...this.emptyTransferRules(), ...(rules || {})};
      this.newSubscription.custom_rename = true;
      this.newSubscription.rename_template = next.rename_template || '';
      this.newSubscription.include_keywords_text = this.ruleWordsText(next.include_keywords);
      this.newSubscription.exclude_keywords_text = this.ruleWordsText(next.exclude_keywords);
      this.newSubscription.match_regex = next.match_regex || '';
      this.newSubscription.episode_regex = next.episode_regex || '';
      this.newSubscription.source_search_keywords_text = (next.source_search_keywords || []).join(', ');
      this.newSubscription.source_exclude_keywords_text = (next.source_exclude_keywords || []).join(', ');
      this.newSubscription.source_prefer_keywords_text = (next.source_prefer_keywords || []).join(', ');
      this.newSubscription.ignore_extensions = !!next.ignore_extensions;
      this.newSubscription.rename_regex = next.rename_regex || '';
      this.newSubscription.rename_replacement = next.rename_replacement || '';
      this.newSubscription.only_latest = !!next.only_latest;
      this.newSubscription.skip_existing_transferred = next.skip_existing_transferred !== false;
      this.newSubscription.duplicate_episode_strategy = next.duplicate_episode_strategy || 'highest_quality';
      this.newSubscription.conflict_strategy = next.conflict_strategy || 'skip';
      this.newSubscription.auto_create_target_dir = next.auto_create_target_dir !== false;
      this.newSubscription.finish_after_episode = next.finish_after_episode || '';
    },

    applyRulePresetToRuleCenter(id) {
      const preset = this.rulePresetById(id);
      if (!preset) return;
      const rules = {...this.emptyTransferRules(), ...preset.rules};
      this.ruleCenter.preset_id = preset.id;
      this.ruleCenter.preset_name = preset.name;
      this.ruleCenter.preset_description = preset.description;
      this.ruleCenter.rename_template = rules.rename_template || '';
      this.ruleCenter.include_keywords_text = this.ruleWordsText(rules.include_keywords);
      this.ruleCenter.exclude_keywords_text = this.ruleWordsText(rules.exclude_keywords);
      this.ruleCenter.match_regex = rules.match_regex || '';
      this.ruleCenter.episode_regex = rules.episode_regex || '';
      this.ruleCenter.source_search_keywords_text = (rules.source_search_keywords || []).join(', ');
      this.ruleCenter.source_exclude_keywords_text = (rules.source_exclude_keywords || []).join(', ');
      this.ruleCenter.source_prefer_keywords_text = (rules.source_prefer_keywords || []).join(', ');
      this.ruleCenter.rename_regex = rules.rename_regex || '';
      this.ruleCenter.rename_replacement = rules.rename_replacement || '';
      this.ruleCenter.ignore_extensions = !!rules.ignore_extensions;
      this.ruleCenter.only_latest = !!rules.only_latest;
      this.ruleCenter.skip_existing_transferred = rules.skip_existing_transferred !== false;
      this.ruleCenter.duplicate_episode_strategy = rules.duplicate_episode_strategy || 'highest_quality';
      this.ruleCenter.conflict_strategy = rules.conflict_strategy || 'skip';
      this.ruleCenter.finish_after_episode = rules.finish_after_episode || '';
      if (preset.media_type) {
        this.ruleCenter.media_type = preset.media_type;
      }
      if (this.ruleCenter.media_type === 'movie') {
        this.ruleCenter.title = '示例电影';
      } else {
        this.ruleCenter.title = '示例剧集';
      }
      this.previewRuleCenter(true);
      this.showNotification('success', `已载入规则：${preset.name}`);
    },

    async saveRuleCenterPreset() {
      const name = String(this.ruleCenter.preset_name || '').trim();
      if (!name) {
        this.showNotification('warning', '请填写预设名称');
        return;
      }
      const id = String(this.ruleCenter.preset_id || '').trim() || this.generateCustomCategoryId();
      const preset = {
        id,
        name,
        description: String(this.ruleCenter.preset_description || '').trim(),
        media_type: this.ruleCenter.media_type || 'series',
        rules: this.ruleCenterRules()
      };
      const presets = this.rulePresets.filter(item => item.id !== id);
      presets.unshift(preset);
      this.settings.rule_presets = presets;
      this.ruleCenter.preset_id = id;
      await this.saveSettings();
    },

    async deleteRulePreset(id) {
      if (!id || !await this.requestDangerConfirmation({title:'删除规则预设', message:'该预设将被永久删除。'})) return;
      this.settings.rule_presets = this.rulePresets.filter(item => item.id !== id);
      if (this.ruleCenter.preset_id === id) {
        this.ruleCenter.preset_id = '';
        this.ruleCenter.preset_name = '';
        this.ruleCenter.preset_description = '';
      }
      await this.saveSettings();
    },

    useRuleCenterAsDefaultTemplate() {
      const template = String(this.ruleCenter.rename_template || '').trim();
      if (!template) {
        this.showNotification('warning', '请先填写重命名模板');
        return;
      }
      this.settings.default_rename_template = template;
      this.showNotification('success', '已写入默认重命名模板，保存设置后生效');
    },

    defaultRenameTemplateLabel() {
      const template = String(this.settings.default_rename_template || '').trim();
      return template || '内置默认：{title}.S{season}E{episode}.{ext}';
    },

    ruleCenterRules() {
      const finish = this.normalizeStartEpisode(this.ruleCenter.finish_after_episode);
      return {
        target_dir: '',
        include_keywords: this.splitRuleWords(this.ruleCenter.include_keywords_text),
        exclude_keywords: this.splitRuleWords(this.ruleCenter.exclude_keywords_text),
        match_regex: this.ruleCenter.match_regex.trim(),
        episode_regex: this.ruleCenter.episode_regex.trim(),
        source_search_keywords: this.splitRuleWords(this.ruleCenter.source_search_keywords_text),
        source_exclude_keywords: this.splitRuleWords(this.ruleCenter.source_exclude_keywords_text),
        source_prefer_keywords: this.splitRuleWords(this.ruleCenter.source_prefer_keywords_text),
        ignore_extensions: !!this.ruleCenter.ignore_extensions,
        rename_regex: this.ruleCenter.rename_regex.trim(),
        rename_replacement: this.ruleCenter.rename_replacement,
        skip_existing_transferred: !!this.ruleCenter.skip_existing_transferred,
        auto_create_target_dir: true,
        rename_template: String(this.ruleCenter.rename_template || '').trim(),
        only_latest: !!this.ruleCenter.only_latest,
        duplicate_episode_strategy: this.ruleCenter.duplicate_episode_strategy || 'highest_quality',
        conflict_strategy: this.ruleCenter.conflict_strategy || 'skip',
        notify_on_update: true,
        notify_on_invalid: true,
        check_interval_minutes: Number(this.settings.subscription_check_interval_minutes || 60),
        finish_after_episode: finish > 0 ? finish : null
      };
    },

    ruleCenterSampleFiles() {
      return String(this.ruleCenter.samples || '')
        .split('\n')
        .map(name => name.trim())
        .filter(Boolean)
        .map(name => ({name, is_dir: false}));
    },

    async previewRuleCenter(silent = false) {
      this.ruleCenterPreviewLoading = true;
      this.ruleCenterPreviewError = '';
      try {
        const response = await apiFetch('/api/subscriptions/rename-preview', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            title: this.ruleCenter.title || '示例',
            url: 'https://pan.quark.cn/s/preview',
            password: '',
            media_type: this.ruleCenter.media_type,
            season: this.normalizeSeason(this.ruleCenter.season),
            start_episode_number: 0,
            rules: this.ruleCenterRules(),
            sample_files: this.ruleCenterSampleFiles()
          })
        });
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.ruleCenterPreview = result.data;
        } else {
          this.ruleCenterPreview = null;
          this.ruleCenterPreviewError = result.message || '预览失败';
          if (!silent) this.showNotification('error', this.ruleCenterPreviewError);
        }
      } catch (error) {
        this.ruleCenterPreview = null;
        this.ruleCenterPreviewError = this.apiErrorMessage(error, '预览失败');
        if (!silent) this.showNotification('error', this.ruleCenterPreviewError);
      } finally {
        this.ruleCenterPreviewLoading = false;
      }
    },

    cloudTypeLabel(type) {
      return type === 'quark' || type === '夸克' ? '夸克网盘' : (type || '-');
    },

    subscriptionPoster(sub) {
      return (sub && sub.metadata && sub.metadata.poster_url) || '';
    },

    subscriptionDisplayTitle(sub) {
      return (sub && sub.metadata && sub.metadata.title) || (sub && sub.title) || '未命名';
    },

    subscriptionSourceLabel(sub) {
      if (!sub) return '';
      const displayTitle = this.subscriptionDisplayTitle(sub);
      const sourceTitle = sub.source_title || sub.title || '';
      if (!sourceTitle || sourceTitle === displayTitle) return '';
      return sourceTitle;
    },

    subscriptionRatingLabel(sub) {
      const rating = sub && sub.metadata ? sub.metadata.vote_average : null;
      if (rating === 0 || rating) return `TMDB ${Number(rating).toFixed(1)}`;
      return 'TMDB -';
    },

    subscriptionSeasonLabel(sub) {
      const season = this.normalizeSeason(sub && sub.season);
      return `第 ${season} 季`;
    },

    subscriptionProgressLabel(sub) {
      const current = Math.max(0, Number((sub && sub.current_episode_number) || 0));
      const total = Number((sub && sub.total_episode_number) || (sub && sub.rules && sub.rules.finish_after_episode) || 0);
      return total > 0 ? `${current}/${total} 集` : `${current}/- 集`;
    },

    subscriptionProgressPercent(sub) {
      const current = Math.max(0, Number((sub && sub.current_episode_number) || 0));
      const total = Number((sub && sub.total_episode_number) || (sub && sub.rules && sub.rules.finish_after_episode) || 0);
      if (total <= 0) return 0;
      return Math.max(0, Math.min(100, (current / total) * 100));
    },

    subscriptionStartEpisodeLabel(sub) {
      if (!sub || sub.media_type === 'movie') return '';
      const episode = this.normalizeStartEpisode(sub.start_episode_number || 0);
      return episode > 1 ? `从第 ${episode} 集开始` : '';
    },

    lastCheckMetric(key) {
      return (this.lastCheckResult && this.lastCheckResult.details && this.lastCheckResult.details[key]) || 0;
    },

    checkDetailItems() {
      const items = (this.lastCheckResult && this.lastCheckResult.details && this.lastCheckResult.details.items) || [];
      return items.slice(0, 80);
    },

    checkActionLabel(action) {
      return {new: '新增', known: '已知', skip: '跳过'}[action] || action || '-';
    },

    checkActionClass(action) {
      if (action === 'new') return 'text-success';
      if (action === 'known') return 'text-primary';
      if (action === 'skip') return 'text-warning';
      return 'text-text/80';
    },

    subscriptionStatusKey(subOrStatus) {
      if (subOrStatus && typeof subOrStatus === 'object') {
        if (subOrStatus.status === 'invalid' || subOrStatus.invalid_since) return 'invalid';
        const current = Number(subOrStatus.current_episode_number || 0);
        const total = Number((subOrStatus.total_episode_number) || (subOrStatus.rules && subOrStatus.rules.finish_after_episode) || 0);
        if ((subOrStatus.status === 'completed' || subOrStatus.completed) && total > 0 && current < total) return 'active';
        if (subOrStatus.status === 'completed' || subOrStatus.completed) return 'completed';
        return 'active';
      }
      if (subOrStatus === 'invalid' || subOrStatus === 'completed') return subOrStatus;
      return 'active';
    },

    subscriptionStatusLabel(subOrStatus) {
      const labels = {active: '追更中', completed: '已完结', invalid: '已失效'};
      return labels[this.subscriptionStatusKey(subOrStatus)] || '-';
    },

    subscriptionStatusClass(subOrStatus) {
      const status = this.subscriptionStatusKey(subOrStatus);
      if (status === 'active') return 'text-success';
      if (status === 'completed') return 'text-warning';
      if (status === 'invalid') return 'text-muted';
      return 'text-text/80';
    },

    subscriptionStatusBadgeClass(subOrStatus) {
      const status = this.subscriptionStatusKey(subOrStatus);
      if (status === 'active') return 'badge badge-primary';
      if (status === 'completed') return 'badge badge-success';
      if (status === 'invalid') return 'badge badge-danger';
      return 'badge badge-muted';
    },

    subscriptionStatusCount(status) {
      return this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === status).length;
    },

    setCheckInterval(minutes) {
      this.settings.subscription_check_interval_minutes = Number(minutes);
    },

    sanitizeCheckInterval() {
      const minutes = Number(this.settings.subscription_check_interval_minutes);
      this.settings.subscription_check_interval_minutes = Math.max(5, Math.floor(Number.isFinite(minutes) ? minutes : 60));
    },

    sanitizeQuarkSigninHour() {
      const hour = Number(this.settings.quark_signin_hour);
      this.settings.quark_signin_hour = Math.min(23, Math.max(0, Math.floor(Number.isFinite(hour) ? hour : 8)));
    },

    sanitizeSourceSwitchPolicy() {
      const clampNumber = (value, fallback, min, max) => {
        const number = Number(value);
        return Math.min(max, Math.max(min, Math.floor(Number.isFinite(number) ? number : fallback)));
      };
      this.settings.auto_source_switch_mode = this.settings.auto_source_switch_mode === 'apply' ? 'apply' : 'search_only';
      this.settings.source_switch_min_score = clampNumber(this.settings.source_switch_min_score, 70, 0, 100);
      this.settings.source_switch_min_score_delta = clampNumber(this.settings.source_switch_min_score_delta, 10, 0, 100);
      this.settings.source_switch_failure_threshold = clampNumber(this.settings.source_switch_failure_threshold, 2, 1, 20);
      this.settings.source_switch_cooldown_hours = clampNumber(this.settings.source_switch_cooldown_hours, 24, 1, 720);
    },

    checkIntervalLabel(minutes) {
      const raw = Number(minutes);
      const value = Math.max(5, Number.isFinite(raw) ? raw : 60);
      if (value < 60) return `${value}分钟`;
      const hours = Math.floor(value / 60);
      const rest = value % 60;
      return rest > 0 ? `${hours}小时${rest}分钟` : `${hours}小时`;
    },

    // 更新订阅默认值
    updateSubscriptionDefaults() {
      if (this.newSubscription.media_type === 'movie') {
        this.newSubscription.start_episode_number = '';
      }
      if (!this.newSubscription.custom_dir) {
        this.newSubscription.target_dir_name = this.mediaFolderName();
      }
      if (!this.newSubscription.custom_rename) {
        this.newSubscription.rename_template = '';
      }
    },

    metadataSearchAvailable() {
      return this.settings.metadata_provider === 'tmdb' && (this.settings.tmdb_api_key || this.settings.tmdb_api_key_configured);
    },

    async searchMetadataForSubscription(silent = false) {
      if (!this.metadataSearchAvailable() || !this.newSubscription.title.trim()) return;
      this.metadataSearching = true;
      try {
        const params = new URLSearchParams({
          query: this.newSubscription.title.trim(),
          media_type: this.newSubscription.media_type || 'series'
        });
        const response = await apiFetch(`/api/metadata/search?${params.toString()}`);
        const data = await response.json();
        if (response.ok) {
          this.metadataResults = data.data || [];
          if (this.metadataResults.length > 0 && !this.newSubscription.metadata) {
            this.applyMetadata(this.metadataResults[0], true);
          } else if (!silent && this.metadataResults.length === 0) {
            this.showNotification('warning', '未匹配到媒体元数据');
          }
        } else if (!silent) {
          this.showNotification('error', data.message || '元数据匹配失败');
        }
      } catch (error) {
        if (!silent) this.showNotification('error', this.apiErrorMessage(error, '元数据匹配失败'));
      } finally {
        this.metadataSearching = false;
      }
    },

    applyMetadata(item, silent = false) {
      if (!item) return;
      this.newSubscription.metadata = item;
      this.newSubscription.title = item.title || this.newSubscription.title;
      if (['movie', 'series', 'anime'].includes(item.media_type)) {
        this.newSubscription.media_type = item.media_type;
      }
      this.updateSubscriptionDefaults();
      if (!silent) this.showNotification('success', '已应用媒体元数据');
    },

    isSelectedMetadata(item) {
      const selected = this.newSubscription.metadata;
      return selected && item && selected.provider === item.provider && selected.provider_id === item.provider_id;
    },

    selectedMetadataLabel() {
      const item = this.newSubscription.metadata;
      if (!item) return '可从 TMDB 匹配标题、类型、年份和海报';
      return `${item.provider.toUpperCase()} #${item.provider_id}`;
    },

    metadataSubtitle(item) {
      const parts = [];
      if (item.media_type) parts.push(this.mediaTypeLabel(item.media_type));
      if (item.release_date) parts.push(item.release_date.slice(0, 4));
      if (item.vote_average === 0 || item.vote_average) parts.push(`评分 ${Number(item.vote_average).toFixed(1)}`);
      if (item.number_of_episodes) parts.push(`${item.number_of_episodes} 集`);
      return parts.join(' · ') || item.provider.toUpperCase();
    },

    targetBaseDir(type = this.newSubscription.media_type) {
      const custom = this.customCategoryByType(type);
      if (custom) return custom.dir || '';
      return {
        'movie': this.settings.quark_save_movie_dir || '/NAS/电影',
        'series': this.settings.quark_save_series_dir || '/NAS/连续剧',
        'anime': this.settings.quark_save_anime_dir || '/NAS/动画'
      }[type] || '';
    },

    configuredDirectoryItems(includeRoot = false) {
      const items = includeRoot ? [{type: 'root', path: '/', name: '根目录'}] : [];
      items.push(
        {type: 'movie', path: this.settings.quark_save_movie_dir || '/NAS/电影', name: '电影'},
        {type: 'series', path: this.settings.quark_save_series_dir || '/NAS/连续剧', name: '连续剧'},
        {type: 'anime', path: this.settings.quark_save_anime_dir || '/NAS/动画', name: '动画'}
      );
      for (const category of this.settings.custom_categories || []) {
        items.push({
          type: this.customCategoryMediaType(category),
          path: category.dir || '',
          name: category.name || '自定义类别'
        });
      }
      return items;
    },

    configuredDirectoryByType(type, includeRoot = false) {
      return this.configuredDirectoryItems(includeRoot).find(item => item.type === type) || null;
    },

    aria2DirForMediaType(type = this.newSubscription.media_type) {
      const custom = this.customCategoryByType(type);
      if (custom) return custom.aria2_dir || '';
      return {
        'movie': this.settings.aria2_movie_dir || '',
        'series': this.settings.aria2_series_dir || '',
        'anime': this.settings.aria2_anime_dir || ''
      }[type] || '';
    },

    subscriptionAria2Dir(sub) {
      if (!sub) return '';
      return sub.sync_download_dir || this.aria2DirForMediaType(sub.media_type);
    },

    metadataYear() {
      const date = this.newSubscription.metadata && this.newSubscription.metadata.release_date;
      if (!date || !/^\d{4}/.test(date)) return '';
      return date.slice(0, 4);
    },

    safePathSegment(value) {
      const segment = String(value || '')
        .replace(/[\\/]/g, ' ')
        .replace(/\s+/g, ' ')
        .trim();
      return segment || '未命名';
    },

    mediaFolderName() {
      const title = this.safePathSegment((this.newSubscription.metadata && this.newSubscription.metadata.title) || this.newSubscription.title || '未命名');
      const year = this.metadataYear();
      return year ? `${title}（${year}）` : title;
    },

    seasonFolderName() {
      return `Season ${this.normalizeSeason(this.newSubscription.season)}`;
    },

    appendPath(base, segment) {
      const cleanBase = String(base || '').trim().replace(/\/+$/, '');
      const cleanSegment = String(segment || '').trim().replace(/^\/+|\/+$/g, '');
      if (!cleanSegment) return cleanBase;
      if (!cleanBase || cleanBase === '/') return `/${cleanSegment}`;
      return `${cleanBase}/${cleanSegment}`;
    },

    hasSeasonSuffix(path) {
      const last = String(path || '').trim().replace(/\/+$/, '').split('/').pop() || '';
      return /^Season\s+\d+$/i.test(last.trim());
    },

    withSeasonFolder(path) {
      if (this.newSubscription.media_type === 'movie') return path;
      if (this.hasSeasonSuffix(path)) return path;
      return this.appendPath(path, this.seasonFolderName());
    },

    // 获取默认目标目录
    getDefaultTargetDir() {
      const mediaDir = this.appendPath(this.targetBaseDir(), this.mediaFolderName());
      return this.withSeasonFolder(mediaDir);
    },

    // 获取默认重命名模板
    getDefaultRenameTemplate() {
      const title = this.newSubscription.title || '未命名';
      if (this.newSubscription.media_type === 'movie') return title;
      const configured = String(this.settings.default_rename_template || '').trim();
      if (configured) return configured;
      const season = this.normalizeSeason(this.newSubscription.season);
      return `${title}.S${String(season).padStart(2, '0')}E{}`;
    },

    splitRuleWords(value) {
      return String(value || '')
        .split(/[,\n]/)
        .map(item => item.trim())
        .filter(Boolean);
    },

    resolveSubscriptionTargetDir() {
      if (this.newSubscription.custom_dir) {
        const dirName = this.newSubscription.target_dir_name.trim() || this.mediaFolderName();
        const target = dirName.startsWith('/')
          ? dirName
          : this.appendPath(this.targetBaseDir(), dirName);
        return this.withSeasonFolder(target);
      }
      return this.getDefaultTargetDir();
    },

    resolveSubscriptionRenameTemplate() {
      return this.newSubscription.custom_rename
        ? this.newSubscription.rename_template.trim()
        : '';
    },

    buildSubscriptionRules() {
      const finish = Number(this.newSubscription.finish_after_episode || 0);
      return {
        target_dir: this.resolveSubscriptionTargetDir(),
        auto_create_target_dir: !!this.newSubscription.auto_create_target_dir,
        skip_existing_transferred: !!this.newSubscription.skip_existing_transferred,
        include_keywords: this.splitRuleWords(this.newSubscription.include_keywords_text),
        exclude_keywords: this.splitRuleWords(this.newSubscription.exclude_keywords_text),
        match_regex: this.newSubscription.match_regex.trim(),
        episode_regex: this.newSubscription.episode_regex.trim(),
        source_search_keywords: this.splitRuleWords(this.newSubscription.source_search_keywords_text),
        source_exclude_keywords: this.splitRuleWords(this.newSubscription.source_exclude_keywords_text),
        source_prefer_keywords: this.splitRuleWords(this.newSubscription.source_prefer_keywords_text),
        ignore_extensions: !!this.newSubscription.ignore_extensions,
        rename_regex: this.newSubscription.rename_regex.trim(),
        rename_replacement: this.newSubscription.rename_replacement,
        rename_template: this.resolveSubscriptionRenameTemplate(),
        only_latest: !!this.newSubscription.only_latest,
        duplicate_episode_strategy: this.newSubscription.duplicate_episode_strategy || 'highest_quality',
        conflict_strategy: this.newSubscription.conflict_strategy || 'skip',
        notify_on_update: true,
        notify_on_invalid: true,
        check_interval_minutes: Number(this.settings.subscription_check_interval_minutes || 60),
        finish_after_episode: finish > 0 ? finish : null
      };
    },

    ruleSummary(rules) {
      if (!rules) return '默认规则';
      const parts = [];
      if (rules.target_dir) parts.push(`目录 ${rules.target_dir}`);
      if (rules.match_regex) parts.push(`正则 ${rules.match_regex}`);
      if (rules.include_keywords && rules.include_keywords.length) parts.push(`包含 ${rules.include_keywords.join('/')}`);
      if (rules.exclude_keywords && rules.exclude_keywords.length) parts.push(`排除 ${rules.exclude_keywords.slice(0, 4).join('/')}`);
      if (rules.rename_template) parts.push(`模板 ${rules.rename_template}`);
      if (rules.rename_regex) parts.push(`替换 ${rules.rename_regex}→${rules.rename_replacement || ''}`);
      if (rules.only_latest) parts.push('仅最新');
      if (rules.skip_existing_transferred !== false) parts.push('跳过已转存');
      const duplicateStrategy = rules.duplicate_episode_strategy || 'highest_quality';
      if (duplicateStrategy === 'latest_upload') parts.push('同集保留最新上传');
      else if (duplicateStrategy === 'largest_size') parts.push('同集保留最大文件');
      else if (duplicateStrategy === 'first') parts.push('同集保留最先出现');
      return parts.length ? parts.join('；') : '默认规则';
    },

    previewSampleFiles() {
      return String(this.newSubscription.preview_samples || '')
        .split('\n')
        .map(name => name.trim())
        .filter(Boolean)
        .map(name => ({name, is_dir: false}));
    },

    async previewSubscriptionRename(silent = false) {
      this.renamePreviewLoading = true;
      this.renamePreviewError = '';
      try {
        const response = await apiFetch('/api/subscriptions/rename-preview', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            subscription_id: this.subscriptionEditingId,
            title: this.newSubscription.title,
            url: this.newSubscription.url,
            password: this.newSubscription.password,
            media_type: this.newSubscription.media_type,
            season: this.normalizeSeason(this.newSubscription.season),
            start_episode_number: this.subscriptionStartEpisodePayload(),
            rules: this.buildSubscriptionRules(),
            sample_files: this.previewSampleFiles()
          })
        });
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.renamePreview = result.data;
        } else {
          this.renamePreview = null;
          this.renamePreviewError = result.message || '预览失败';
          if (!silent) this.showNotification('error', this.renamePreviewError);
        }
      } catch (error) {
        this.renamePreview = null;
        this.renamePreviewError = this.apiErrorMessage(error, '预览失败');
        if (!silent) this.showNotification('error', this.renamePreviewError);
      } finally {
        this.renamePreviewLoading = false;
      }
    },

    // 为订阅选择快速目录
    async selectQuickDirForSub(dirType) {
      const selected = this.configuredDirectoryByType(dirType);
      if (!selected) return;

      try {
        const data = await apiData(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);

        if (data.found && data.fid) {
          this.newSubscription.target_fid = data.fid;
          this.newSubscription.target_path = selected.path;
          this.showNotification('success', `已选择 ${selected.name}`);
        }
      } catch (error) {
        console.error('查找目录失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '查找目录失败'));
      }
    },

    // 浏览目标目录（复用转存目录浏览）
    async browseTargetDir() {
      this.transferTargetFid = this.newSubscription.target_fid || '0';
      this.transferTargetPath = this.newSubscription.target_path || '根目录';
      this.transferBrowseFidStack = [{fid: '0', name: '根目录'}];
      this.showTransferModal = true;
      this.transferingForSubscription = true;
      await this.loadTransferBrowse(this.transferTargetFid);
    },

    async openAria2DirBrowser() {
      const root = String(this.aria2DirForMediaType() || '').trim().replace(/\/+$/, '');
      if (!root) {
        this.showNotification('warning', '请先在系统设置中配置当前媒体类型的 Aria2 下载目录');
        return;
      }

      this.showAria2DirDialog = true;
      this.aria2DirError = '';
      const current = String(this.newSubscription.sync_download_dir || this.aria2DirForMediaType() || '').trim();
      const startPath = current === root || current.startsWith(`${root}/`) ? current : '';
      await this.loadAria2Dir(startPath);
    },

    async loadAria2Dir(path = '') {
      this.aria2DirLoading = true;
      this.aria2DirError = '';
      try {
        const params = new URLSearchParams();
        params.set('media_type', this.newSubscription.media_type || 'series');
        if (path) params.set('path', path);
        const result = await apiData(`/api/drive/aria2/browse?${params.toString()}`);
        if (result.success) {
          this.aria2DirItems = result.items || [];
          this.aria2DirRoot = result.root || '';
          this.aria2DirCurrent = result.current || '';
          this.aria2DirParent = result.parent || '';
        } else {
          this.aria2DirItems = [];
          this.aria2DirError = result.message || result.error || '读取 Aria2 下载目录失败';
        }
      } catch (error) {
        console.error('读取 Aria2 下载目录失败:', error);
        this.aria2DirError = this.apiErrorMessage(error, '读取 Aria2 下载目录失败');
      } finally {
        this.aria2DirLoading = false;
      }
    },

    async aria2DirEnter(item) {
      if (!item || !item.path) return;
      await this.loadAria2Dir(item.path);
    },

    async aria2DirBack() {
      if (!this.aria2DirParent) return;
      await this.loadAria2Dir(this.aria2DirParent);
    },

    selectAria2Dir(path = this.aria2DirCurrent) {
      if (!path) return;
      this.newSubscription.sync_download_dir = path;
      this.showAria2DirDialog = false;
    },

    // 确认订阅
    async confirmSubscription() {
      if (!this.newSubscription.title.trim()) {
        this.showNotification('error', '请输入订阅名称');
        return;
      }

      if (!this.newSubscription.url) {
        this.showNotification('error', '缺少分享链接');
        return;
      }

      if (this.newSubscription.manual_schedule_enabled && !this.newSubscription.manual_schedule_start_date) {
        this.showNotification('error', '启用手动排期后必须填写开播日期');
        this.subscriptionDialogTab = 'schedule';
        return;
      }

      if (this.subscriptionEditingId) {
        await this.updateSubscription();
        return;
      }

      if (this.subscriptionMode === 'once') {
        // 仅转存一次
        await this.doOneTimeTransfer();
      } else {
        // 创建持续订阅
        await this.createContinuousSubscription();
      }
    },

    // 一次性转存
    async doOneTimeTransfer() {
      try {
        const data = await apiData('/api/transfer', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            url: this.newSubscription.url,
            passcode: this.newSubscription.password || '',
            target_fid: this.newSubscription.target_fid || '0'
          })
        });

        if (data.success) {
          this.showNotification('success', data.message || '转存任务已创建');
          this.showSubscriptionDialog = false;
          await this.loadJobs();
          await this.loadNotifications();
        } else {
          this.showNotification('error', data.message || '转存失败');
          await this.loadNotifications();
        }
      } catch (error) {
        console.error('转存失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '转存失败'));
      }
    },

    // 创建持续订阅
    async createContinuousSubscription() {
      try {
        const rules = this.buildSubscriptionRules();

        const response = await apiFetch('/api/subscriptions', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            title: this.newSubscription.title,
            url: this.newSubscription.url,
            password: this.newSubscription.password,
            media_type: this.newSubscription.media_type,
            season: this.normalizeSeason(this.newSubscription.season),
            start_episode_number: this.subscriptionStartEpisodePayload(),
            target_dir: rules.target_dir,
            target_fid: '0',
            rename_template: rules.rename_template,
            notify_only: this.newSubscription.notify_only,
            sync_download_enabled: !!this.newSubscription.sync_download_enabled,
            sync_download_dir: this.newSubscription.sync_download_dir,
            strm_enabled: !!this.newSubscription.strm_enabled,
            metadata: this.newSubscription.metadata,
            manual_schedule: this.manualSchedulePayload(),
            rule_preset_id: this.newSubscription.rule_preset_id || '',
            rules
          })
        });

        const result = await response.json();

        if (response.ok && result.data) {
          // 订阅创建成功后立即检查；是否转存由自动化设置决定。
          this.showNotification('success', '订阅创建成功，正在检查更新...');
          this.showSubscriptionDialog = false;

          const subId = result.data.id;
          // 立即触发检查
          await this.checkSubscription(subId, {forceTransfer: true});
          await this.loadSubscriptions();
        } else {
          this.showNotification('error', result.message || '创建订阅失败');
        }
      } catch (error) {
        console.error('创建订阅失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '创建订阅失败'));
      }
    },

    async updateSubscription() {
      try {
        const rules = this.buildSubscriptionRules();
        const response = await apiFetch(`/api/subscriptions/${this.subscriptionEditingId}`, {
          method: 'PUT',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            title: this.newSubscription.title,
            url: this.newSubscription.url,
            password: this.newSubscription.password,
            media_type: this.newSubscription.media_type,
            season: this.normalizeSeason(this.newSubscription.season),
            start_episode_number: this.subscriptionStartEpisodePayload(),
            notify_only: this.newSubscription.notify_only,
            sync_download_enabled: !!this.newSubscription.sync_download_enabled,
            sync_download_dir: this.newSubscription.sync_download_dir,
            strm_enabled: !!this.newSubscription.strm_enabled,
            keep_progress_on_source_change: !!this.newSubscription.keep_progress_on_source_change,
            continue_from_current_episode: !!this.newSubscription.continue_from_current_episode,
            metadata: this.newSubscription.metadata,
            manual_schedule: this.manualSchedulePayload(),
            rule_preset_id: this.newSubscription.rule_preset_id || '',
            rules,
            target_dir: rules.target_dir,
            rename_template: rules.rename_template
          })
        });
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.showNotification('success', '订阅已保存');
          this.showSubscriptionDialog = false;
          this.subscriptionEditingId = null;
          await this.loadSubscriptions();
        } else {
          this.showNotification('error', result.message || '保存订阅失败');
        }
      } catch (error) {
        console.error('保存订阅失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '保存订阅失败'));
      }
    },

    async transferToQuark(result) {
      if (!this.settings.quark_cookie && !this.settings.quark_cookie_configured) {
        this.showNotification('error', '请先在设置中配置夸克 Cookie');
        return;
      }

      if (!result.url) {
        this.showNotification('error', '缺少分享链接');
        return;
      }

      // 打开目录选择对话框
      this.transferTargetResult = result;
      this.transferTargetFid = '0';
      this.transferTargetPath = '根目录';
      this.transferBrowseFidStack = [{fid: '0', name: '根目录'}];
      this.showTransferModal = true;
      await this.loadTransferBrowse('0');
    },

    async loadTransferBrowse(fid) {
      try {
        const data = await apiData(`/api/drive?fid=${fid}`);
        this.transferBrowseItems = data.list || [];
      } catch (error) {
        console.error('加载目录失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '加载目录失败'));
      }
    },

    async transferBrowseEnter(item) {
      if (item.file) return; // 只能进入文件夹
      this.transferBrowseFidStack.push({fid: item.fid, name: item.file_name});
      this.transferTargetFid = item.fid;
      this.transferTargetPath = this.transferBrowseFidStack.map(f => f.name).join(' / ');
      await this.loadTransferBrowse(item.fid);
    },

    async transferBrowseBack() {
      if (this.transferBrowseFidStack.length <= 1) return;
      this.transferBrowseFidStack.pop();
      const current = this.transferBrowseFidStack[this.transferBrowseFidStack.length - 1];
      this.transferTargetFid = current.fid;
      this.transferTargetPath = this.transferBrowseFidStack.map(f => f.name).join(' / ');
      await this.loadTransferBrowse(current.fid);
    },

    async confirmTransfer() {
      if (!this.transferTargetResult) return;

      this.showTransferModal = false;

      try {
        this.showNotification('info', '正在转存到夸克网盘...');

        const data = await apiData('/api/transfer', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            url: this.transferTargetResult.url,
            passcode: '',
            target_fid: this.transferTargetFid
          })
        });

        if (data.success) {
          const msg = data.message || `转存任务已创建`;
          this.showNotification('success', msg);
        } else {
          const msg = data.message || '转存失败';
          this.showNotification('error', msg);
        }
        await this.loadJobs();
        await this.loadNotifications();
      } catch (error) {
        console.error('转存失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '转存失败'));
      } finally {
        this.transferTargetResult = null;
      }
    },

    async selectQuickDir(dirType) {
      const selected = this.configuredDirectoryByType(dirType, true);
      if (!selected) return;

      // 如果是根目录，直接设置
      if (dirType === 'root') {
        this.transferTargetFid = '0';
        this.transferTargetPath = '根目录';
        this.transferBrowseFidStack = [{fid: '0', name: '根目录'}];
        await this.loadTransferBrowse('0');
        return;
      }

      try {
        // 调用 API 查找路径对应的 fid
        const data = await apiData(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);

        if (data.found && data.fid) {
          this.transferTargetFid = data.fid;
          this.transferTargetPath = selected.path;
          this.transferBrowseFidStack = [{fid: '0', name: '根目录'}, {fid: data.fid, name: selected.name}];
          await this.loadTransferBrowse(data.fid);
          this.showNotification('success', `已切换到 ${selected.name}`);
        } else {
          this.showNotification('warning', `目录 ${selected.path} 不存在，已创建`);
          // 目录已被 ensure_dir_path 创建，重新查找
          const retryData = await apiData(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);
          if (retryData.found && retryData.fid) {
            this.transferTargetFid = retryData.fid;
            this.transferTargetPath = selected.path;
            this.transferBrowseFidStack = [{fid: '0', name: '根目录'}, {fid: retryData.fid, name: selected.name}];
            await this.loadTransferBrowse(retryData.fid);
          }
        }
      } catch (error) {
        console.error('查找目录失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '查找目录失败'));
      }
    },

    // ===== 订阅 =====
    async openSubscriptionDetail(subscriptionOrId, pushHistory = true) {
      const id = typeof subscriptionOrId === 'string' ? subscriptionOrId : (subscriptionOrId && subscriptionOrId.id);
      if (!id) return;
      const changed = this.selectedSubscriptionId !== id || this.currentTab !== 'subscriptions';
      this.currentTab = 'subscriptions';
      this.selectedSubscriptionId = id;
      this.subscriptionDetailError = '';
      this.subscriptionEpisodeFilter = 'all';
      this.subscriptionActivityLimit = 20;
      if (changed) this.subscriptionDetail = null;
      if (pushHistory && changed) this.pushRouteState();
      await this.loadSubscriptionDetail(id);
    },

    closeSubscriptionDetail(pushHistory = true) {
      if (!this.selectedSubscriptionId) return;
      this.selectedSubscriptionId = '';
      this.subscriptionDetailRequestId += 1;
      this.subscriptionDetailLoading = false;
      this.subscriptionDetail = null;
      this.subscriptionDetailError = '';
      this.subscriptionAutomationPipeline = null;
      this.subscriptionAutomationError = '';
      this.subscriptionEpisodeFilter = 'all';
      if (pushHistory) this.pushRouteState();
    },

    async loadSubscriptionDetail(id = this.selectedSubscriptionId) {
      if (!id) return;
      const requestId = ++this.subscriptionDetailRequestId;
      this.subscriptionDetailLoading = true;
      this.subscriptionDetailError = '';
      try {
        const response = await apiFetch(`/api/subscriptions/${encodeURIComponent(id)}/status`, {cache: 'no-store'});
        const result = await response.json();
        if (requestId !== this.subscriptionDetailRequestId || this.selectedSubscriptionId !== id) return;
        this.subscriptionDetail = result.data || null;
        await this.loadSubscriptionAutomation(id, requestId);
        const updated = this.subscriptionDetail && this.subscriptionDetail.subscription;
        if (updated) {
          const index = this.subscriptions.findIndex(sub => sub.id === updated.id);
          if (index >= 0) this.subscriptions.splice(index, 1, updated);
        }
      } catch (error) {
        console.error('加载订阅详情失败:', error);
        if (requestId === this.subscriptionDetailRequestId && this.selectedSubscriptionId === id) {
          this.subscriptionDetailError = this.apiErrorMessage(error, '加载订阅详情失败');
        }
      } finally {
        if (requestId === this.subscriptionDetailRequestId) this.subscriptionDetailLoading = false;
      }
    },

    setSubscriptionEpisodeFilter(filter) {
      if (['all', 'missing', 'pending', 'ready', 'recent'].includes(filter)) {
        this.subscriptionEpisodeFilter = filter;
      }
    },

    async loadAutomationSummary() {
      this.automationSummaryLoading = true;
      try {
        this.automationSummary = await apiData('/api/automation/summary', {cache: 'no-store'});
      } catch (error) {
        console.error('加载自动化摘要失败:', error);
      } finally {
        this.automationSummaryLoading = false;
      }
    },

    async loadSubscriptionAutomation(id = this.selectedSubscriptionId, requestId = this.subscriptionDetailRequestId) {
      if (!id) return;
      this.subscriptionAutomationLoading = true;
      this.subscriptionAutomationError = '';
      try {
        const data = await apiData(`/api/subscriptions/${encodeURIComponent(id)}/pipeline`, {cache: 'no-store'});
        if (requestId === this.subscriptionDetailRequestId && this.selectedSubscriptionId === id) {
          this.subscriptionAutomationPipeline = data;
        }
      } catch (error) {
        if (requestId === this.subscriptionDetailRequestId) this.subscriptionAutomationError = this.apiErrorMessage(error, '加载结构化流水线失败');
      } finally {
        if (requestId === this.subscriptionDetailRequestId) this.subscriptionAutomationLoading = false;
      }
    },

    automationStageLabel(stage) { return automationEventTools.stageLabel(stage); },
    automationStatusLabel(status) { return automationEventTools.statusLabel(status); },
    automationStatusTone(status) { return automationEventTools.statusTone(status); },
    automationEventDuration(event) { return automationEventTools.duration(event); },
    automationCanRetry(event) { return automationEventTools.canRetry(event); },
    get subscriptionAutomationTimeline() { return automationEventTools.timeline ? automationEventTools.timeline((this.subscriptionAutomationPipeline && this.subscriptionAutomationPipeline.events) || [], 100) : []; },
    async copySubscriptionAutomationTimeline() {
      const value = ux.safeJson ? ux.safeJson(this.subscriptionAutomationTimeline) : JSON.stringify(this.subscriptionAutomationTimeline, null, 2);
      await this.copyText(value);
    },

    async retryAutomationEvent(event) {
      if (!event || !event.id || !this.automationCanRetry(event)) return;
      this.automationRetryingId = event.id;
      try {
        const result = await apiData(`/api/automation/events/${encodeURIComponent(event.id)}/retry`, {method: 'POST'});
        this.showNotification('success', result.message || '已创建重试');
        await Promise.all([this.loadSubscriptionAutomation(), this.loadAutomationSummary(), this.loadJobs()]);
      } catch (error) {
        this.showNotification('error', this.apiErrorMessage(error, '阶段重试失败'));
      } finally {
        this.automationRetryingId = '';
      }
    },

    subscriptionEpisodeFilterCount(filter) {
      return subscriptionDetailTools.episodeFilterCount(
        (this.subscriptionDetail && this.subscriptionDetail.episodes) || [],
        filter
      );
    },

    subscriptionEpisodeClass(episode) {
      return `subscription-episode-cell is-${subscriptionDetailTools.episodeStage(episode)}`;
    },

    subscriptionEpisodeLabel(episode) {
      return subscriptionDetailTools.episodeStageLabel(episode);
    },

    subscriptionEpisodeTitle(episode) {
      const files = (episode && episode.files) || [];
      const parts = [`第 ${episode.episode} 集`, this.subscriptionEpisodeLabel(episode)];
      if (episode.download_status && episode.download_status !== 'disabled') parts.push(`下载: ${episode.download_status}`);
      if (episode.strm_status && episode.strm_status !== 'disabled') parts.push(`STRM: ${episode.strm_status}`);
      if (files.length) parts.push(files.slice(0, 3).join(' / '));
      return parts.join(' · ');
    },

    subscriptionPipelineClass(step) {
      return `subscription-pipeline-step is-${(step && step.status) || 'idle'}`;
    },

    subscriptionPipelineStatusLabel(step) {
      return subscriptionDetailTools.pipelineStatusLabel(step && step.status);
    },

    subscriptionActivityTone(item) {
      return subscriptionDetailTools.activityTone(item);
    },

    subscriptionActivityKindLabel(item) {
      return {job: '任务', notification: '通知', check: '检查'}[(item && item.kind) || ''] || '记录';
    },

    subscriptionDetailProgressStyle() {
      const value = Math.max(0, Math.min(100, Number(this.subscriptionDetailSummary.completion_percent || 0)));
      return `width: ${value.toFixed(1)}%`;
    },

    async repairSubscriptionMissingEpisodes() {
      if (!this.selectedSubscriptionId) return;
      await this.checkSubscription(this.selectedSubscriptionId, {forceTransfer: true});
      await this.loadSubscriptionDetail(this.selectedSubscriptionId);
    },

    async loadSubscriptions() {
      try {
        const response = await apiFetch('/api/subscriptions');
        const data = await response.json();
        this.subscriptions = data.data || [];
        const currentIds = new Set(this.subscriptions.map(sub => sub.id));
        this.selectedSubscriptionIds = this.selectedSubscriptionIds.filter(id => currentIds.has(id));
        if (this.selectedSubscriptionId && !this.subscriptions.some(sub => sub.id === this.selectedSubscriptionId)) {
          this.closeSubscriptionDetail(false);
          this.replaceRouteState();
        }
      } catch (error) {
        console.error('加载订阅失败:', error);
      }
    },

    async checkSubscription(id, options = {}) {
      try {
        if (!options.silent) this.showNotification('info', '正在检查订阅...');
        const response = await apiFetch(`/api/subscriptions/${id}/check`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({force_transfer: !!options.forceTransfer})
        });
        const data = await response.json();

        if (response.ok && data.data) {
          const result = data.data;
          this.lastCheckResult = result;
          if (result.new_files.length > 0) {
            if (!options.silent) this.showNotification('success', `发现 ${result.new_files.length} 个新文件`);
          } else {
            if (!options.silent) this.showNotification('info', '无更新');
          }
          if (options.deferReload) return true;
          await this.loadSubscriptions();
          await this.loadJobs();
          await this.loadNotifications();
          if (this.selectedSubscriptionId === id) await this.loadSubscriptionDetail(id);
        } else {
          if (!options.silent) this.showNotification('error', data.message || '检查失败');
        }
      } catch (error) {
        console.error('检查订阅失败:', error);
        if (!options.silent) this.showNotification('error', this.apiErrorMessage(error, '检查订阅失败'));
        return false;
      }
    },

    async checkAllSubscriptions() {
      this.checkingAllSubscriptions = true;
      try {
        this.showNotification('info', '正在批量检查订阅...');
        const response = await apiFetch('/api/subscriptions/check', {method: 'POST'});
        const result = await response.json().catch(() => ({}));

        if (response.ok && result.data) {
          const results = result.data || [];
          const newFileCount = results.reduce((sum, item) => sum + ((item.new_files || []).length), 0);
          const invalidCount = results.filter(item => item.became_invalid).length;
          const completedCount = results.filter(item => item.became_completed).length;
          if (newFileCount > 0) {
            this.showNotification('success', `批量检查完成，发现 ${newFileCount} 个新文件`);
          } else {
            this.showNotification('info', '批量检查完成，暂无更新');
          }
          if (invalidCount > 0 || completedCount > 0) {
            this.showNotification('info', `状态变化：${invalidCount} 个失效，${completedCount} 个完结`);
          }
          await this.loadSubscriptions();
          await this.loadJobs();
          await this.loadNotifications();
        } else {
          this.showNotification('error', result.message || result.error || '批量检查失败');
        }
      } catch (error) {
        console.error('批量检查订阅失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '批量检查失败'));
      } finally {
        this.checkingAllSubscriptions = false;
      }
    },

    async renameExistingFiles(id) {
      try {
        this.showNotification('info', '正在按订阅模板修复命名...');
        const response = await apiFetch(`/api/subscriptions/${id}/rename-existing`, {
          method: 'POST'
        });
        const result = await response.json();

        if (response.ok && result.data) {
          const count = result.data.renamed_count || 0;
          this.showNotification('success', `已重命名 ${count} 个文件`);
        } else {
          this.showNotification('error', result.message || '修复命名失败');
        }
      } catch (error) {
        console.error('修复命名失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '修复命名失败'));
      }
    },

    async generateSubscriptionStrm(id) {
      try {
        this.showNotification('info', '正在生成 STRM 文件...');
        const response = await apiFetch(`/api/subscriptions/${id}/strm`, {
          method: 'POST'
        });
        const result = await response.json().catch(() => ({}));

        if (response.ok && result.data) {
          const count = result.data.generated_count || 0;
          const dir = result.data.output_dir || '';
          this.showNotification('success', dir ? `已生成 ${count} 个 STRM 文件到 ${dir}` : `已生成 ${count} 个 STRM 文件`);
        } else {
          this.showNotification('error', result.message || result.error || '生成 STRM 失败');
        }
      } catch (error) {
        console.error('生成 STRM 失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '生成 STRM 失败'));
      }
    },

    async scrapeSubscriptionMetadata(id) {
      try {
        this.showNotification('info', '已提交元数据刮削任务');
        const response = await apiFetch(`/api/subscriptions/${id}/metadata/scrape`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({overwrite: true})
        });
        const result = await response.json();

        if (response.ok && result.data) {
          this.upsertJob(result.data);
          await this.loadJobs();
        } else {
          this.showNotification('error', result.message || '提交刮削任务失败');
        }
      } catch (error) {
        console.error('提交刮削任务失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '提交刮削任务失败'));
      }
    },

    openManualMetadataScrape(sub) {
      if (!this.metadataSearchAvailable()) {
        this.showNotification('error', '请先在系统设置中配置 TMDB');
        return;
      }
      this.manualMetadataSubscriptionId = sub.id;
      this.manualMetadataSubscriptionTitle = this.subscriptionDisplayTitle(sub);
      this.manualMetadataQuery = (sub.metadata && sub.metadata.title) || sub.title || '';
      this.manualMetadataMediaType = sub.media_type || 'series';
      this.manualMetadataResults = [];
      this.showManualMetadataDialog = true;
      this.searchManualMetadata();
    },

    closeManualMetadataDialog() {
      this.showManualMetadataDialog = false;
      this.manualMetadataSubscriptionId = '';
      this.manualMetadataSubscriptionTitle = '';
      this.manualMetadataQuery = '';
      this.manualMetadataResults = [];
    },

    async searchManualMetadata() {
      const query = this.manualMetadataQuery.trim();
      if (!query) {
        this.showNotification('warning', '请输入元数据搜索关键词');
        return;
      }
      this.manualMetadataSearching = true;
      try {
        const params = new URLSearchParams({
          query,
          media_type: this.manualMetadataMediaType || 'series'
        });
        const response = await apiFetch(`/api/metadata/search?${params.toString()}`);
        const data = await response.json();
        if (response.ok) {
          this.manualMetadataResults = data.data || [];
          if (this.manualMetadataResults.length === 0) {
            this.showNotification('warning', '未匹配到媒体元数据');
          }
        } else {
          this.showNotification('error', data.message || '元数据搜索失败');
        }
      } catch (error) {
        console.error('元数据搜索失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '元数据搜索失败'));
      } finally {
        this.manualMetadataSearching = false;
      }
    },

    async applyManualMetadata(item) {
      if (!this.manualMetadataSubscriptionId || !item) return;
      this.manualMetadataApplying = true;
      try {
        const response = await apiFetch(`/api/subscriptions/${this.manualMetadataSubscriptionId}`, {
          method: 'PUT',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({metadata: item})
        });
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.showNotification('success', '已应用媒体元数据');
          this.closeManualMetadataDialog();
          await this.loadSubscriptions();
        } else {
          this.showNotification('error', result.message || '应用元数据失败');
        }
      } catch (error) {
        console.error('应用元数据失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '应用元数据失败'));
      } finally {
        this.manualMetadataApplying = false;
      }
    },

    async scrapeAllSubscriptionMetadata(options = {}) {
      this.scrapingAllMetadata = true;
      try {
        const overwrite = options.overwrite !== false;
        this.showNotification('info', overwrite ? '已提交批量元数据刷新任务' : '已提交批量元数据补全任务');
        const response = await apiFetch('/api/subscriptions/metadata/scrape', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({overwrite})
        });
        const result = await response.json();

        if (response.ok && result.data) {
          this.upsertJob(result.data);
          await this.loadJobs();
        } else {
          this.showNotification('error', result.message || '提交批量刮削任务失败');
        }
      } catch (error) {
        console.error('提交批量刮削任务失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '提交批量刮削任务失败'));
      } finally {
        this.scrapingAllMetadata = false;
      }
    },

    selectedSourceSwitchSubscription() {
      return this.subscriptions.find(sub => sub.id === this.sourceSwitchSubscriptionId) || null;
    },

    async openSourceSwitchDialog(sub) {
      this.sourceSwitchSubscriptionId = sub.id;
      this.sourceSwitchSubscriptionTitle = this.subscriptionDisplayTitle(sub);
      this.sourceSwitchCandidates = Array.isArray(sub.source_candidates) ? [...sub.source_candidates] : [];
      this.sourceSwitchError = '';
      this.sourceSwitchApplyingId = '';
      this.sourceSwitchPreviewingId = '';
      this.sourceSwitchPreview = null;
      this.sourceSwitchHistory = [];
      this.showSourceSwitchDialog = true;
      await Promise.all([this.loadSourceCandidates(), this.loadSourceSwitchHistory()]);
    },

    closeSourceSwitchDialog() {
      this.showSourceSwitchDialog = false;
      this.sourceSwitchSubscriptionId = '';
      this.sourceSwitchSubscriptionTitle = '';
      this.sourceSwitchCandidates = [];
      this.sourceSwitchPreview = null;
      this.sourceSwitchHistory = [];
      this.sourceSwitchError = '';
      this.sourceSwitchApplyingId = '';
      this.sourceSwitchPreviewingId = '';
    },

    normalizeSourceCandidatesPayload(payload) {
      const candidates = Array.isArray(payload) ? payload : (payload && Array.isArray(payload.data) ? payload.data : []);
      return sourceSwitchTools.sortCandidates(candidates);
    },

    async loadSourceCandidates() {
      if (!this.sourceSwitchSubscriptionId) return;
      this.sourceSwitchLoading = true;
      this.sourceSwitchError = '';
      try {
        const result = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-candidates`, {cache: 'no-store'});
        this.sourceSwitchCandidates = this.normalizeSourceCandidatesPayload(result);
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '读取换源候选失败');
      } finally {
        this.sourceSwitchLoading = false;
      }
    },

    async searchSourceCandidates() {
      if (!this.sourceSwitchSubscriptionId) return;
      this.sourceSwitchSearching = true;
      this.sourceSwitchError = '';
      try {
        const result = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-candidates/search`, {
          method: 'POST'
        });
        this.sourceSwitchCandidates = this.normalizeSourceCandidatesPayload(result);
        if (this.sourceSwitchCandidates.length > 0) {
          this.showNotification('success', `找到 ${this.sourceSwitchCandidates.length} 个换源候选`);
        } else {
          this.showNotification('warning', '未找到换源候选');
        }
        await this.loadSubscriptions();
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '搜索换源候选失败');
        this.showNotification('error', this.sourceSwitchError);
      } finally {
        this.sourceSwitchSearching = false;
      }
    },

    async loadSourceSwitchHistory() {
      if (!this.sourceSwitchSubscriptionId) return;
      this.sourceSwitchHistoryLoading = true;
      try {
        const result = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-history`, {cache: 'no-store'});
        this.sourceSwitchHistory = Array.isArray(result) ? result : [];
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '读取换源历史失败');
      } finally {
        this.sourceSwitchHistoryLoading = false;
      }
    },

    async previewSourceCandidate(candidate) {
      if (!this.sourceSwitchSubscriptionId || !candidate || !candidate.id) return null;
      this.sourceSwitchPreviewingId = candidate.id;
      this.sourceSwitchError = '';
      try {
        const preview = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-candidates/preview`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({candidate_id: candidate.id})
        });
        this.sourceSwitchPreview = preview;
        const index = this.sourceSwitchCandidates.findIndex(item => item.id === candidate.id);
        if (index >= 0 && preview.candidate) this.sourceSwitchCandidates.splice(index, 1, preview.candidate);
        return preview;
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '候选预览失败');
        this.showNotification('error', this.sourceSwitchError);
        return null;
      } finally {
        this.sourceSwitchPreviewingId = '';
      }
    },

    async rollbackSourceSwitch() {
      if (!this.sourceSwitchSubscriptionId || this.sourceSwitchRollbackLoading) return;
      if (!await this.requestDangerConfirmation({title:'回滚订阅来源', message:'回滚会保留当前追更和转存记录，并立即检查旧来源。'})) return;
      this.sourceSwitchRollbackLoading = true;
      this.sourceSwitchError = '';
      try {
        const result = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-history/rollback`, {method: 'POST'});
        this.showNotification('success', result.message || '来源已回滚');
        await Promise.all([this.loadSubscriptions(), this.loadSourceCandidates(), this.loadSourceSwitchHistory()]);
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '来源回滚失败');
        this.showNotification('error', this.sourceSwitchError);
      } finally {
        this.sourceSwitchRollbackLoading = false;
      }
    },

    async applySourceCandidate(candidate) {
      if (!this.sourceSwitchSubscriptionId || !candidate || !candidate.id) return;
      let preview = this.sourceSwitchPreview && this.sourceSwitchPreview.candidate && this.sourceSwitchPreview.candidate.id === candidate.id
        ? this.sourceSwitchPreview
        : await this.previewSourceCandidate(candidate);
      if (!preview || !preview.can_apply) {
        this.sourceSwitchError = preview && preview.warnings && preview.warnings.length
          ? `候选未通过安全检查：${preview.warnings.join('；')}`
          : '请先完成候选预览和安全检查';
        return;
      }
      this.sourceSwitchApplyingId = candidate.id;
      this.sourceSwitchError = '';
      try {
        const result = await apiData(`/api/subscriptions/${this.sourceSwitchSubscriptionId}/source-candidates/apply`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({candidate_id: candidate.id})
        });
        if (result.success === false) {
          this.sourceSwitchError = result.message || result.error || '应用换源失败';
          this.showNotification('error', this.sourceSwitchError);
          return;
        }
        this.showNotification('success', result.message || '换源成功');
        if (result.check_summary) {
          this.showNotification('info', result.check_summary);
        }
        if (result.check_error) {
          this.showNotification('warning', result.check_error);
        }
        this.closeSourceSwitchDialog();
        this.subscriptionActionMenuId = '';
        await this.loadSubscriptions();
        await this.loadNotifications();
      } catch (error) {
        this.sourceSwitchError = this.apiErrorMessage(error, '应用换源失败');
        this.showNotification('error', this.sourceSwitchError);
      } finally {
        this.sourceSwitchApplyingId = '';
      }
    },

    sourceCandidateQuality(candidate) {
      return sourceSwitchTools.quality(candidate);
    },

    sourceCandidateEpisodeRange(candidate) {
      return sourceSwitchTools.episodeRange(candidate);
    },

    sourceSwitchHistoryLabel(item) {
      return sourceSwitchTools.historyLabel(item);
    },

    sourceCandidateHost(candidate) {
      try {
        return new URL(candidate.url).host;
      } catch (_) {
        return candidate && candidate.url ? candidate.url : '-';
      }
    },

    sourceCandidateNote(candidate) {
      const note = (candidate && candidate.note || '').trim();
      return note || this.sourceCandidateHost(candidate);
    },

    sourceCandidateMeta(candidate) {
      const source = (candidate && candidate.source) || '未知来源';
      const time = candidate && candidate.discovered_at ? this.formatTime(candidate.discovered_at) : '-';
      return `${source} · ${time}`;
    },

    toggleSubscriptionActionMenu(id) {
      this.subscriptionActionMenuId = this.subscriptionActionMenuId === id ? '' : id;
    },

    async deleteSubscription(id) {
      if (this.requestDangerConfirmation && !await this.requestDangerConfirmation({title:'删除订阅', message:'订阅配置将被永久删除，媒体文件不会删除。', phrase:'DELETE'})) return;
      try {
        const response = await apiFetch(`/api/subscriptions/${id}?confirm=${encodeURIComponent(id)}`, {method: 'DELETE'});
        if (response.ok) {
          this.showNotification('success', '已删除');
          if (this.selectedSubscriptionId === id) this.closeSubscriptionDetail();
          await this.loadSubscriptions();
        }
      } catch (error) {
        console.error('删除失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '删除订阅失败'));
      }
    },

    editSubscription(sub) {
      this.openEditSubscriptionDialog(sub);
    },

    // ===== 通知 =====
    get filteredSubscriptions() {
      return this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === this.subscriptionStatusTab);
    },

    get visibleFilteredSubscriptions() {
      return ux.visibleWindow ? ux.visibleWindow(this.filteredSubscriptions, this.subscriptionVisibleLimit, 1000) : this.filteredSubscriptions.slice(0, this.subscriptionVisibleLimit);
    },

    get hasMoreSubscriptions() { return this.visibleFilteredSubscriptions.length < this.filteredSubscriptions.length; },
    get allVisibleSubscriptionsSelected() {
      return this.visibleFilteredSubscriptions.length > 0 && this.visibleFilteredSubscriptions.every(sub => this.selectedSubscriptionIds.includes(sub.id));
    },

    setSubscriptionStatusTab(value) {
      if (!['active','invalid','completed'].includes(value)) return;
      this.subscriptionStatusTab = value; this.subscriptionVisibleLimit = 100;
      if (ux.writePreference) ux.writePreference('subscriptions.status', value);
    },
    setSubscriptionViewMode(value) {
      if (!['table','poster'].includes(value)) return;
      this.subscriptionViewMode = value;
      if (ux.writePreference) ux.writePreference('subscriptions.view', value);
    },
    loadSubscriptionPreferences() {
      if (!ux.readPreference) return;
      this.subscriptionStatusTab = ux.readPreference('subscriptions.status', 'active', ['active','invalid','completed']);
      this.subscriptionViewMode = ux.readPreference('subscriptions.view', 'table', ['table','poster']);
    },
    toggleSubscriptionSelection(id) {
      this.selectedSubscriptionIds = this.selectedSubscriptionIds.includes(id) ? this.selectedSubscriptionIds.filter(value => value !== id) : [...this.selectedSubscriptionIds, id];
    },
    toggleAllVisibleSubscriptions() {
      const visible = this.visibleFilteredSubscriptions.map(sub => sub.id);
      this.selectedSubscriptionIds = this.allVisibleSubscriptionsSelected ? this.selectedSubscriptionIds.filter(id => !visible.includes(id)) : [...new Set([...this.selectedSubscriptionIds, ...visible])];
    },
    async batchCheckSelectedSubscriptions() {
      const ids = [...this.selectedSubscriptionIds]; if (!ids.length || this.subscriptionBatchLoading) return;
      this.subscriptionBatchLoading = true;
      try {
        const run = id => this.checkSubscription(id, {silent:true, deferReload:true});
        await (ux.runPool ? ux.runPool(ids, 3, run) : Promise.all(ids.map(run)));
        await Promise.all([this.loadSubscriptions(), this.loadJobs(), this.loadNotifications()]);
        this.showNotification('success', `已检查 ${ids.length} 个订阅`);
      } finally { this.subscriptionBatchLoading = false; }
    },
    async batchDeleteSelectedSubscriptions() {
      const ids = [...this.selectedSubscriptionIds]; if (!ids.length || this.subscriptionBatchLoading) return;
      const approved = await this.requestDangerConfirmation({title:'批量删除订阅', message:`将永久删除 ${ids.length} 个订阅。`, phrase:'DELETE'});
      if (!approved) return;
      this.subscriptionBatchLoading = true;
      try {
        const remove = id => apiFetch(`/api/subscriptions/${encodeURIComponent(id)}?confirm=${encodeURIComponent(id)}`, {method:'DELETE'});
        await (ux.runPool ? ux.runPool(ids, 3, remove) : Promise.all(ids.map(remove)));
        this.selectedSubscriptionIds = []; await this.loadSubscriptions(); this.showNotification('success', `已删除 ${ids.length} 个订阅`);
      } catch (error) { this.showNotification('error', this.apiErrorMessage(error, '批量删除失败')); }
      finally { this.subscriptionBatchLoading = false; }
    },

    get selectedSubscription() {
      return (this.subscriptionDetail && this.subscriptionDetail.subscription)
        || this.subscriptions.find(sub => sub.id === this.selectedSubscriptionId)
        || null;
    },

    get subscriptionDetailSummary() {
      return (this.subscriptionDetail && this.subscriptionDetail.summary) || {
        expected_count: 0, discovered_count: 0, transferred_count: 0, downloaded_count: 0,
        strm_count: 0, missing_count: 0, pending_transfer_count: 0, pending_download_count: 0,
        completion_percent: 0, target_episode: null, data_inferred: false, grid_truncated: false
      };
    },

    get visibleSubscriptionEpisodes() {
      return subscriptionDetailTools.filterEpisodes(
        (this.subscriptionDetail && this.subscriptionDetail.episodes) || [],
        this.subscriptionEpisodeFilter
      );
    },

    get subscriptionDetailActivity() {
      return subscriptionDetailTools.buildSubscriptionActivity(this.subscriptionDetail)
        .slice(0, this.subscriptionActivityLimit);
    },

    };
  }

  return {createStore};
});
