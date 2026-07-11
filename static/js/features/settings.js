(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubSettings = moduleApi;
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
    checkIntervalPresets: [15, 30, 60, 120, 360, 720],
    pushChannels: [
      {id: 'wecom', name: '企业微信'},
      {id: 'telegram', name: 'Telegram'},
      {id: 'wxpusher', name: 'WxPusher'},
      {id: 'bark', name: 'Bark'},
      {id: 'gotify', name: 'Gotify'},
      {id: 'pushplus', name: 'PushPlus'},
      {id: 'serverchan', name: 'Server 酱'}
    ],
    sensitiveSettingKeys: [
      'app_password',
      'aria2_secret',
      'quark_cookie',
      'quark_signin_cookie',
      'strm_access_token',
      'pansou_api_url',
      'tmdb_api_key',
      'wecom_bot_url',
      'bark_url',
      'wxpusher_app_token',
      'telegram_bot_token',
      'gotify_token',
      'pushplus_token',
      'serverchan_key'
    ],
    revealedSecrets: {},
    secretLoading: {},
    settingsSchema: null,
    settingsLoaded: false,

    // 更新日历
    quarkSigninLoading: false,
    quarkHealthLoading: false,
    quarkHealth: {status: 'unknown', message: '尚未检测', checkedAt: null, nickname: '', signinMessage: '', signinResult: null, issues: [], directories: {}, saveEnabled: false, signinEnabled: false, rootConfigured: false, strmReady: false, capacityBytes: 0, usedCapacityBytes: null, memberType: '', signProgress: 0, signTarget: 0},

    // 规则中心
    settings: {
      app_username: '', app_password: '', app_password_configured: false, quark_cookie: '', quark_cookie_configured: false, quark_save_enabled: false, quark_save_root: '',
      quark_signin_cookie: '', quark_signin_cookie_configured: false,
      quark_signin_enabled: false, quark_signin_hour: 8,
      quark_save_movie_dir: '', quark_save_series_dir: '', quark_save_anime_dir: '',
      custom_categories: [],
      aria2_rpc_url: '', aria2_secret: '', aria2_secret_configured: false,
      aria2_movie_dir: '', aria2_series_dir: '', aria2_anime_dir: '',
      strm_enabled: false, strm_output_dir: '', strm_public_base_url: '', strm_access_token: '', strm_access_token_configured: false, strm_token_in_url: false,
      cloud_types: ['quark'], dashboard_widgets: ['quick_actions','hero','kpis','library','operations'], push_on_update: true, push_on_failed: true, push_on_completed: true, push_on_save: true, push_on_download_completed: true, push_on_quark_signin: true,
      metadata_provider: 'tmdb', tmdb_api_key: '', tmdb_api_key_configured: false, tmdb_language: 'zh-CN',
      wecom_bot_url: '', wecom_bot_url_configured: false, telegram_bot_token: '', telegram_bot_token_configured: false, telegram_chat_id: '', bark_url: '', bark_url_configured: false, serverchan_key: '', serverchan_key_configured: false,
      wxpusher_app_token: '', wxpusher_app_token_configured: false, wxpusher_uids: '', gotify_url: '', gotify_token: '', gotify_token_configured: false, pushplus_token: '', pushplus_token_configured: false,
      subscription_check_interval_minutes: 60, subscription_check_max_concurrency: 4, external_api_max_concurrency: 8,
      job_max_concurrency: 4, job_transfer_max_concurrency: 2, job_metadata_max_concurrency: 2, job_push_max_concurrency: 4,
      job_maintenance_mode: false,
      aria2_batch_submit_limit: 20, subscription_scheduler_enabled: false, pansou_api_url: '', pansou_api_url_configured: false, check_links: true,
      probe_quark_files: true, filter_bad_links: true, push_silent: false, webhook_enabled: false, webhook_urls: [], webhook_secret: '',
      push_event_routes: {}, push_min_level: 'info', push_quiet_hours_enabled: false, push_quiet_start_hour: 23, push_quiet_end_hour: 8,
      push_quiet_allow_error: true, push_dedup_window_seconds: 300, push_digest_enabled: false, push_digest_window_minutes: 15,
      push_title_template: '{{title}}', push_message_template: '{{message}}',
      auto_download_new_subscription_items: false,
      auto_source_switch_enabled: false, auto_source_switch_mode: 'search_only', source_switch_min_score: 70,
      source_switch_min_score_delta: 10, source_switch_failure_threshold: 2, source_switch_cooldown_hours: 24,
      default_rename_template: '', rule_presets: []
    },

    get configuredPushChannelCount() {
      return this.pushChannels.filter(channel => this.pushChannelConfigured(channel.id)).length;
    },

    get settingsCompletionItems() {
      const s = this.settings || {};
      const saveDirectoryConfigured = Boolean(
        String(s.quark_save_movie_dir || '').trim()
        || String(s.quark_save_series_dir || '').trim()
        || String(s.quark_save_anime_dir || '').trim()
        || (s.custom_categories || []).some(category => String(category.quark_dir || '').trim())
      );
      return [
        {id: 'auth', label: '管理账号', description: '已设置登录密码', configured: !!(s.app_password_configured || (s.app_password && !this.isMaskedSecret(s.app_password))), tab: 'connections'},
        {id: 'quark', label: '夸克连接', description: this.quarkHealth.message || 'Cookie 尚未测试', configured: !!(s.quark_cookie_configured || s.quark_cookie), tab: 'connections'},
        {id: 'storage', label: '媒体目录', description: saveDirectoryConfigured ? '至少一个保存目录可用' : '尚未配置保存目录', configured: saveDirectoryConfigured, tab: 'connections'},
        {id: 'aria2', label: 'Aria2', description: s.aria2_rpc_url ? 'RPC 地址已配置' : '可选：同步下载', configured: !!s.aria2_rpc_url, optional: true, tab: 'connections'},
        {id: 'metadata', label: 'TMDB 元数据', description: s.tmdb_api_key_configured || s.tmdb_api_key ? 'API Key 已配置' : '可选：海报与剧集信息', configured: !!(s.tmdb_api_key_configured || s.tmdb_api_key), optional: true, tab: 'connections'},
        {id: 'automation', label: '订阅调度', description: s.subscription_scheduler_enabled ? '自动检查已启用' : '自动检查未启用', configured: !!s.subscription_scheduler_enabled, tab: 'automation'},
        {id: 'notification', label: '消息通知', description: this.configuredPushChannelCount ? `${this.configuredPushChannelCount} 个渠道可用` : '可选：尚未配置渠道', configured: this.configuredPushChannelCount > 0, optional: true, tab: 'notifications'},
        {id: 'strm', label: 'STRM', description: s.strm_enabled ? '已启用媒体串流' : '可选：未启用', configured: !!s.strm_enabled, optional: true, tab: 'connections'}
      ];
    },

    get settingsCompletionPercent() {
      const items = this.settingsCompletionItems;
      return items.length ? Math.round(items.filter(item => item.configured).length / items.length * 100) : 0;
    },

    isMaskedSecret(value) {
      return typeof value === 'string' && value.length > 0 && /^\*+$/.test(value);
    },

    secretVisible(key) {
      return !!this.revealedSecrets[key];
    },

    secretConfigured(key) {
      return !!this.settings[`${key}_configured`];
    },

    secretToggleDisabled(key) {
      return !!this.secretLoading[key] || (!this.secretVisible(key) && !this.secretConfigured(key));
    },

    secretButtonLabel(key) {
      if (this.secretLoading[key]) return '读取中';
      return this.secretVisible(key) ? '隐藏' : '显示';
    },

    resetSecretVisibility() {
      const next = {};
      for (const key of this.sensitiveSettingKeys) {
        next[key] = false;
      }
      this.revealedSecrets = next;
    },

    prepareSecretInput(key) {
      if (!this.secretVisible(key) && this.isMaskedSecret(this.settings[key])) {
        this.settings[key] = '';
      }
    },

    async toggleSettingSecret(key) {
      if (this.secretVisible(key)) {
        const value = this.settings[key] || '';
        this.settings[key] = value ? '*'.repeat([...value].length) : '';
        this.revealedSecrets[key] = false;
        return;
      }

      if (!this.secretConfigured(key)) return;
      this.secretLoading[key] = true;
      try {
        const response = await apiFetch(`/api/settings/secret/${encodeURIComponent(key)}`);
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.settings[key] = result.data.value || '';
          this.revealedSecrets[key] = true;
        } else {
          this.showNotification('error', result.message || result.error || '读取明文失败');
        }
      } catch (error) {
        console.error('读取明文失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '读取明文失败'));
      } finally {
        this.secretLoading[key] = false;
      }
    },

    async loadSettings() {
      try {
        const response = await apiFetch('/api/settings');
        const data = await response.json();
        // 直接使用服务器返回的 data 对象
        if (data.data) {
          this.settings = {...this.settings, ...data.data};
        } else {
          this.settings = {...this.settings, ...data};
        }
        this.normalizeCustomCategories();
        this.settingsLoaded = true;
        this.settings.rule_presets = this.rulePresets;
        this.resetSecretVisibility();
      } catch (error) {
        console.error('加载设置失败:', error);
      }
    },

    async loadSettingsSchema() {
      try {
        const response = await apiFetch('/api/settings/schema');
        const data = await response.json();
        this.settingsSchema = data.data || data || null;
      } catch (error) {
        console.error('加载设置字段定义失败:', error);
      }
    },

    async saveSettings() {
      try {
        this.sanitizeCheckInterval();
        this.sanitizeQuarkSigninHour();
        this.sanitizeSourceSwitchPolicy();
        this.normalizeCustomCategories();
        const response = await apiFetch('/api/settings', {
          method: 'POST',  // 改为 POST
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify(this.settings)
        });
        if (response.ok) {
          const data = await response.json();
          if (data.data) {
            this.settings = {...this.settings, ...data.data};
            this.normalizeCustomCategories();
          }
          this.resetSecretVisibility();
          this.showNotification('success', '设置已保存');
        }
      } catch (error) {
        console.error('保存失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '保存失败'));
      }
    },

    async runQuarkSignin() {
      const hasQuarkCookie = !!this.settings.quark_cookie || !!this.settings.quark_cookie_configured;
      const hasSigninCookie = !!this.settings.quark_signin_cookie || !!this.settings.quark_signin_cookie_configured;
      if (!hasQuarkCookie && !hasSigninCookie) {
        this.showNotification('error', '请先在设置中配置夸克 Cookie 或签到 Cookie');
        return;
      }
      this.quarkSigninLoading = true;
      try {
        const data = await apiData('/api/quark/signin', {method: 'POST'});
        if (data.success) {
          this.quarkHealth.signinMessage = data.message || '夸克签到成功';
          this.quarkHealth.signinResult = data.result || null;
          this.quarkHealth.capacityBytes = Number((data.result || {}).total_capacity_bytes || this.quarkHealth.capacityBytes || 0);
          this.quarkHealth.usedCapacityBytes = (data.result || {}).used_capacity_bytes ?? this.quarkHealth.usedCapacityBytes ?? null;
          this.quarkHealth.memberType = (data.result || {}).member_type || this.quarkHealth.memberType || '';
          this.quarkHealth.signProgress = Number((data.result || {}).sign_progress || this.quarkHealth.signProgress || 0);
          this.quarkHealth.signTarget = Number((data.result || {}).sign_target || this.quarkHealth.signTarget || 0);
          this.showNotification('success', data.message || '夸克签到成功');
          await this.loadNotifications();
        } else {
          this.quarkHealth.signinMessage = data.message || data.error || '夸克签到失败';
          this.showNotification('error', data.message || data.error || '夸克签到失败');
          await this.loadNotifications();
        }
      } catch (error) {
        console.error('夸克签到失败:', error);
        this.quarkHealth.signinMessage = this.apiErrorMessage(error, '夸克签到失败');
        this.showNotification('error', this.quarkHealth.signinMessage);
      } finally {
        this.quarkSigninLoading = false;
      }
    },

    // 上海时区（+8）的日索引，与后端 shanghai_day_index 保持一致
    shanghaiDayIndex(unixSeconds) {
      return Math.floor((Number(unixSeconds) + 8 * 3600) / 86400);
    },

    // 从已加载的通知里找出「今天」的夸克签到记录（成功优先）
    todaysSigninNotification() {
      const today = this.shanghaiDayIndex(Date.now() / 1000);
      const items = (this.notifications || []).filter(n =>
        n && n.event === 'quark_signin' && this.shanghaiDayIndex(n.created_at) === today
      );
      if (!items.length) return null;
      const success = items.find(n => n.level === 'success');
      if (success) return success;
      return items.sort((a, b) => Number(b.created_at || 0) - Number(a.created_at || 0))[0];
    },

    // 仪表盘签到卡片：今日签到状态标签
    quarkSigninTodayLabel() {
      const notif = this.todaysSigninNotification();
      if (notif) return notif.level === 'success' ? '今日已签到' : '今日签到失败';
      if (this.settings.quark_signin_enabled) return '今日待签到';
      return '未开启自动';
    },

    // 今日签到状态对应的文字颜色
    quarkSigninTodayClass() {
      const notif = this.todaysSigninNotification();
      if (notif) return notif.level === 'success' ? 'text-success' : 'text-danger';
      return this.settings.quark_signin_enabled ? 'text-warning' : 'text-muted';
    },

    quarkHealthStatusLabel() {
      const labels = {ok: '正常', failed: '异常', unknown: '未检测'};
      return labels[this.quarkHealth.status] || '未检测';
    },

    quarkHealthStatusClass() {
      if (this.quarkHealth.status === 'ok') return 'text-success';
      if (this.quarkHealth.status === 'failed') return 'text-danger';
      return 'text-text/80';
    },

    quarkSigninProgressLabel() {
      const result = this.quarkHealth.signinResult || {};
      const progress = Number(result.sign_progress || this.quarkHealth.signProgress || 0);
      const target = Number(result.sign_target || this.quarkHealth.signTarget || 0);
      if (progress > 0 && target > 0) return `${progress}/${target}`;
      return '-';
    },

    quarkCapacityLabel() {
      const result = this.quarkHealth.signinResult || {};
      const total = Number(this.quarkHealth.capacityBytes || result.total_capacity_bytes || 0);
      const rawUsed = this.quarkHealth.usedCapacityBytes ?? result.used_capacity_bytes;
      const used = rawUsed === null || rawUsed === undefined ? null : Number(rawUsed);
      if (total > 0 && used !== null && Number.isFinite(used)) {
        const percent = Math.min(999.9, Math.max(0, (used / total) * 100));
        return `${this.formatSize(used)} / ${this.formatSize(total)} (${percent.toFixed(1)}%)`;
      }
      return total > 0 ? this.formatSize(total) : '-';
    },

    quarkHealthIssueText() {
      const issues = this.quarkHealth.issues || [];
      return issues.length ? issues.join('；') : '关键配置完整';
    },

    async refreshQuarkHealth() {
      await this.testQuark({silent: true});
    },

    async testQuark(options = {}) {
      const silent = !!options.silent;
      if (!silent) this.showNotification('info', '测试夸克连接中...');
      this.quarkHealthLoading = true;
      try {
        const cookie = this.isMaskedSecret(this.settings.quark_cookie)
          ? ''
          : (this.settings.quark_cookie || '');
        const data = await apiData('/api/quark/test', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({cookie})
        });
        if (data.success) {
          this.quarkHealth = {
            ...this.quarkHealth,
            status: 'ok',
            message: '夸克 Cookie 可用',
            nickname: data.nickname || '夸克用户',
            checkedAt: Date.now() / 1000,
            issues: data.issues || [],
            directories: data.directories || {},
            saveEnabled: !!data.save_enabled,
            signinEnabled: !!data.signin_enabled,
            rootConfigured: !!data.root_configured,
            strmReady: !!data.strm_ready,
            capacityBytes: Number(data.total_capacity_bytes || 0),
            usedCapacityBytes: data.used_capacity_bytes ?? null,
            memberType: data.member_type || '',
            signProgress: Number(data.sign_progress || 0),
            signTarget: Number(data.sign_target || 0)
          };
          if (!silent) this.showNotification('success', `测试成功！用户: ${data.nickname || '未知'}`);
        } else {
          this.quarkHealth = {
            ...this.quarkHealth,
            status: 'failed',
            message: data.error || '测试失败，请检查 Cookie',
            nickname: '',
            checkedAt: Date.now() / 1000,
            issues: data.issues || [],
            directories: data.directories || {},
            saveEnabled: !!data.save_enabled,
            signinEnabled: !!data.signin_enabled,
            rootConfigured: !!data.root_configured,
            strmReady: !!data.strm_ready,
            capacityBytes: Number(data.total_capacity_bytes || 0),
            usedCapacityBytes: data.used_capacity_bytes ?? null,
            memberType: data.member_type || '',
            signProgress: Number(data.sign_progress || 0),
            signTarget: Number(data.sign_target || 0)
          };
          if (!silent) this.showNotification('error', data.error || '测试失败，请检查 Cookie');
        }
      } catch (error) {
        console.error('测试失败:', error);
        const message = this.apiErrorMessage(error, '连接失败，请检查配置');
        this.quarkHealth = {
          ...this.quarkHealth,
          status: 'failed',
          message,
          nickname: '',
          checkedAt: Date.now() / 1000
        };
        if (!silent) this.showNotification('error', message);
      } finally {
        this.quarkHealthLoading = false;
      }
    },

    async testAria2() {
      if (!this.settings.aria2_rpc_url.trim()) {
        this.showNotification('warning', '请先填写 Aria2 RPC URL');
        return;
      }

      this.showNotification('info', '测试 Aria2 连接中...');
      try {
        const data = await apiData('/api/drive/aria2/test');
        if (data.success) {
          this.showNotification('success', data.message || 'Aria2 连接成功');
        } else {
          this.showNotification('error', data.message || data.error || 'Aria2 测试失败');
        }
      } catch (error) {
        console.error('Aria2 测试失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, 'Aria2 测试失败'));
      }
    },

    settingsCompletionItemClass(item) {
      if (item && item.configured) return 'is-ready';
      if (item && item.optional) return 'is-optional';
      return 'is-required';
    },

    pushChannelConfigured(channel) {
      const s = this.settings || {};
      if (channel === 'wecom') return !!s.wecom_bot_url;
      if (channel === 'telegram') return !!s.telegram_bot_token && !!s.telegram_chat_id;
      if (channel === 'wxpusher') return !!s.wxpusher_app_token;
      if (channel === 'bark') return !!s.bark_url;
      if (channel === 'gotify') return !!s.gotify_url && !!s.gotify_token;
      if (channel === 'pushplus') return !!s.pushplus_token;
      if (channel === 'serverchan') return !!s.serverchan_key;
      return false;
    },

    pushChannelName(channel) {
      const item = this.pushChannels.find(item => item.id === channel);
      return item ? item.name : channel;
    },

    async testPush(channel = null) {
      this.showNotification('info', channel ? `发送 ${this.pushChannelName(channel)} 测试推送中...` : '发送测试推送中...');
      try {
        const data = await apiData('/api/push/test', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify(channel ? {channels: [channel]} : {})
        });
        const successCount = data.success_count || 0;
        const failedCount = data.failed_count || 0;
        const enabledChannels = data.enabled_channels || [];

        if (successCount > 0) {
          const channelList = Object.entries(data.results || {})
            .filter(([_, success]) => success)
            .map(([channel, _]) => channel)
            .join(', ');
          this.showNotification('success', `推送成功 ${successCount}/${successCount + failedCount} 个渠道：${channelList}`);
        } else if (enabledChannels.length === 0) {
          this.showNotification('warning', '未配置任何推送渠道');
        } else if (channel) {
          const error = (data.errors || {})[channel];
          this.showNotification('error', error ? `${this.pushChannelName(channel)} 推送测试失败：${error}` : `${this.pushChannelName(channel)} 推送测试失败`);
        } else {
          this.showNotification('error', `推送失败，${failedCount} 个渠道失败`);
        }
      } catch (error) {
        console.error('推送失败:', error);
        this.showNotification('error', this.apiErrorMessage(error, '推送失败，请检查配置'));
      }
    },

    };
  }

  return {createStore};
});
