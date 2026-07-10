(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubSearchPage = moduleApi;
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

  function createStore() {
    return {
    searchQuery: '',
    searching: false,
    searchResults: [],
    searchHistory: [],
    cloudTypes: ['夸克'],
    searchOptions: {probeFiles: true, filterBad: true},
    searchProgress: {value: 0, label: '', detail: ''},
    searchProgressTimer: null,
    searchViewMode: 'poster',
    searchSort: 'quality',
    searchResultFilter: 'all',
    searchHasRun: false,
    selectedSearchResult: null,
    showSearchResultPreview: false,

    // 订阅
    get visibleSearchResults() {
      const filtered = this.searchResults.filter(result => {
        const insight = this.searchResultInsights(result);
        if (this.searchResultFilter === 'valid') return insight.validity === true;
        if (this.searchResultFilter === 'quality') return insight.validity !== false && insight.score >= 70;
        if (this.searchResultFilter === 'risky') return insight.validity === false || insight.risks.length > 0;
        return true;
      });
      return filtered.sort((left, right) => searchResultTools.compareSearchResults(left, right, this.searchSort));
    },

    get searchResultStats() {
      const insights = this.searchResults.map(result => this.searchResultInsights(result));
      return {
        total: insights.length,
        checked: insights.filter(item => item.validity !== null).length,
        valid: insights.filter(item => item.validity === true).length,
        quality: insights.filter(item => item.validity !== false && item.score >= 70).length,
        risky: insights.filter(item => item.validity === false || item.risks.length > 0).length,
        files: insights.reduce((sum, item) => sum + item.fileCount, 0)
      };
    },

    get selectedSearchResultInsight() {
      return this.selectedSearchResult ? this.searchResultInsights(this.selectedSearchResult) : null;
    },

    get selectedSearchResultFiles() {
      return this.selectedSearchResultInsight ? this.selectedSearchResultInsight.files : [];
    },

    resetSearchProgress() {
      this.stopSearchProgressTimer();
      this.searchProgress = {value: 0, label: '', detail: ''};
    },

    setSearchProgress(value, label, detail = '') {
      this.searchProgress = {
        value: Math.max(0, Math.min(100, Math.round(value))),
        label,
        detail
      };
    },

    searchProgressStyle() {
      return `width: ${this.searchProgress.value || 0}%`;
    },

    startSearchProgressTimer() {
      this.stopSearchProgressTimer();
      this.searchProgressTimer = this.startPolling('search-progress', () => {
        if (!this.searching) return;
        const limit = this.searchOptions.probeFiles ? 88 : (this.searchOptions.filterBad ? 82 : 72);
        if ((this.searchProgress.value || 0) < limit) {
          this.setSearchProgress(
            (this.searchProgress.value || 0) + 3,
            this.searchProgress.label || '正在搜索资源',
            this.searchProgress.detail
          );
        }
      }, 700);
    },

    stopSearchProgressTimer() {
      this.stopPolling('search-progress');
      this.searchProgressTimer = null;
    },

    // ===== 搜索 =====
    async search() {
      if (!this.searchQuery.trim()) return;
      this.searching = true;
      this.searchResults = [];
      this.searchHasRun = false;
      this.closeSearchResultPreview();
      this.setSearchProgress(8, '提交搜索请求', '正在连接资源搜索服务');
      this.startSearchProgressTimer();
      try {
        // 保存搜索历史
        this.addSearchHistory(this.searchQuery.trim());

        // 根据用户选择显示不同提示
        let statusMsg = '搜索中';
        if (this.searchOptions.probeFiles) {
          statusMsg = '搜索中，正在嗅探文件列表...';
          this.setSearchProgress(18, '搜索资源并嗅探文件', '会检测链接并读取可用文件列表');
        } else if (this.searchOptions.filterBad) {
          statusMsg = '搜索中，正在检测链接有效性...';
          this.setSearchProgress(18, '搜索资源并检测链接', '会过滤失效链接');
        } else {
          this.setSearchProgress(18, '搜索资源', '正在等待 PanSou 返回结果');
        }
        this.showNotification('info', statusMsg);

        const response = await apiFetch('/api/search', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            keyword: this.searchQuery,
            limit: 50,
            check_links: this.searchOptions.filterBad,
            probe_files: this.searchOptions.probeFiles,
            filter_bad: this.searchOptions.filterBad,
            max_files: 50
          })
        });
        this.setSearchProgress(this.searchOptions.probeFiles ? 84 : 76, '整理搜索结果', '正在生成可操作的资源列表');
        const data = await response.json();
        this.searchResults = (data.data || []).map(result => this.prepareSearchResult(result));
        this.searchHasRun = true;
        this.searchResultFilter = 'all';

        if (this.searchResults.length > 0) {
          this.setSearchProgress(100, '搜索完成', `找到 ${this.searchResults.length} 个结果`);
          let suffix = '个结果';
          if (this.searchOptions.filterBad) {
            suffix = '个有效结果';
          } else if (this.searchOptions.probeFiles) {
            suffix = '个结果（已嗅探）';
          }
          this.showNotification('success', `找到 ${this.searchResults.length} ${suffix}`);
        } else {
          this.setSearchProgress(100, '搜索完成', '没有匹配结果');
          const msg = this.searchOptions.filterBad ? '未找到有效资源，请尝试其他关键词' : '未找到结果';
          this.showNotification('warning', msg);
        }
      } catch (error) {
        console.error('搜索失败:', error);
        this.setSearchProgress(100, '搜索失败', '请检查网络、PanSou 地址或稍后重试');
        this.showNotification('error', this.apiErrorMessage(error, '搜索失败'));
      } finally {
        this.searching = false;
        this.stopSearchProgressTimer();
        setTimeout(() => {
          if (!this.searching) this.resetSearchProgress();
        }, 2400);
      }
    },

    loadSearchPreferences() {
      try {
        const viewMode = localStorage.getItem('searchViewMode');
        const sort = localStorage.getItem('searchSort');
        if (['poster', 'list'].includes(viewMode)) this.searchViewMode = viewMode;
        if (['quality', 'updated', 'files', 'title'].includes(sort)) this.searchSort = sort;
      } catch (error) {
        console.warn('读取搜索视图偏好失败:', error);
      }
    },

    setSearchViewMode(mode) {
      if (!['poster', 'list'].includes(mode)) return;
      this.searchViewMode = mode;
      try {
        localStorage.setItem('searchViewMode', mode);
      } catch (_) {
        // Storage may be disabled; the current view still works for this session.
      }
    },

    setSearchSort(sort) {
      if (!['quality', 'updated', 'files', 'title'].includes(sort)) return;
      this.searchSort = sort;
      try {
        localStorage.setItem('searchSort', sort);
      } catch (_) {
        // Storage may be disabled; the current sort still works for this session.
      }
    },

    setSearchResultFilter(filter) {
      if (['all', 'valid', 'quality', 'risky'].includes(filter)) this.searchResultFilter = filter;
    },

    searchResultFilterCount(filter) {
      if (filter === 'valid') return this.searchResultStats.valid;
      if (filter === 'quality') return this.searchResultStats.quality;
      if (filter === 'risky') return this.searchResultStats.risky;
      return this.searchResultStats.total;
    },

    prepareSearchResult(result) {
      return {...result, _insights: searchResultTools.analyzeSearchResult(result)};
    },

    searchResultInsights(result) {
      if (!result) return searchResultTools.analyzeSearchResult({});
      if (!result._insights) result._insights = searchResultTools.analyzeSearchResult(result);
      return result._insights;
    },

    searchResultQualityClass(result) {
      return `search-quality-pill is-${this.searchResultInsights(result).tone}`;
    },

    searchResultUpdateLabel(result) {
      return searchResultTools.formatSearchResultDate(this.searchResultInsights(result).updatedTimestamp);
    },

    searchResultFileSummary(result) {
      const insight = this.searchResultInsights(result);
      if (!insight.fileCount) return '未嗅探文件';
      if (insight.episodeCount) return `${insight.fileCount} 个文件 · ${insight.episodeCount} 集`;
      return `${insight.fileCount} 个文件`;
    },

    openSearchResultPreview(result) {
      if (!result) return;
      this.selectedSearchResult = result;
      this.showSearchResultPreview = true;
    },

    closeSearchResultPreview() {
      this.showSearchResultPreview = false;
      this.selectedSearchResult = null;
    },

    startSearchResultAction(mode) {
      const result = this.selectedSearchResult;
      if (!result) return;
      this.closeSearchResultPreview();
      this.openSubscriptionDialog(result, mode);
    },

    // 加载搜索历史
    loadSearchHistory() {
      try {
        const history = localStorage.getItem('searchHistory');
        if (history) {
          this.searchHistory = JSON.parse(history);
        }
      } catch (error) {
        console.error('加载搜索历史失败:', error);
      }
    },

    // 添加搜索历史
    addSearchHistory(query) {
      // 移除重复项
      this.searchHistory = this.searchHistory.filter(h => h !== query);
      // 添加到开头
      this.searchHistory.unshift(query);
      // 只保留最近 20 条
      this.searchHistory = this.searchHistory.slice(0, 20);
      // 保存到 localStorage
      try {
        localStorage.setItem('searchHistory', JSON.stringify(this.searchHistory));
      } catch (error) {
        console.error('保存搜索历史失败:', error);
      }
    },

    // 清空搜索历史
    clearSearchHistory() {
      this.searchHistory = [];
      try {
        localStorage.removeItem('searchHistory');
        this.showNotification('success', '搜索历史已清空');
      } catch (error) {
        console.error('清空搜索历史失败:', error);
      }
    },

    };
  }

  return {createStore};
});
