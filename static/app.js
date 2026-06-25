function app() {
  return {
    currentTab: 'search',
    currentSettingsTab: 'basic',

    tabs: [
      {id: 'search', name: '资源搜索', description: '搜索影视资源并添加订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>'},
      {id: 'drive', name: '我的网盘', description: '管理夸克网盘文件', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/></svg>'},
      {id: 'downloads', name: '下载任务', description: '查看 Aria2 实时进度', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1M8 12l4 4m0 0l4-4m-4 4V4"/></svg>'},
      {id: 'subscriptions', name: '订阅管理', description: '管理媒体订阅', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"/></svg>'},
      {id: 'transferHistory', name: '后台日志', description: '查看后台任务和执行记录', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v6h6M20 20v-6h-6M5 19A9 9 0 0019 5M19 5h-5M5 19h5"/></svg>'},
      {id: 'notifications', name: '通知中心', description: '查看用户通知和推送记录', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/></svg>'},
      {id: 'settings', name: '系统设置', description: '配置系统参数', icon: '<svg fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>'}
    ],

    settingsTabs: [
      {id: 'basic', name: '基础设置', icon: '⚙'},
      {id: 'quark', name: '夸克网盘', icon: '☁'},
      {id: 'push', name: '消息推送', icon: '↗'},
      {id: 'automation', name: '自动化', icon: '⏱'},
      {id: 'rules', name: '规则中心', icon: '✦'},
      {id: 'advanced', name: '高级', icon: '⌘'},
      {id: 'update', name: '在线更新', icon: '⇧'}
    ],

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

    // 搜索
    searchQuery: '',
    searching: false,
    searchResults: [],
    searchHistory: [],
    cloudTypes: ['夸克'],
    searchOptions: {probeFiles: true, filterBad: true},
    searchProgress: {value: 0, label: '', detail: ''},
    searchProgressTimer: null,

    // 订阅
    subscriptions: [],
    lastCheckResult: null,
    checkingAllSubscriptions: false,
    showSubscriptionDialog: false,
    subscriptionStatusTab: 'active',
    subscriptionStatusTabs: [
      {id: 'active', name: '追更中'},
      {id: 'invalid', name: '已失效'},
      {id: 'completed', name: '已完结'}
    ],
    subscriptionMode: 'once',  // 'once' 或 'continuous'
    subscriptionDialogTab: 'content',
    subscriptionEditingId: null,
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
      include_keywords_text: '',
      exclude_keywords_text: '预告, 花絮, 解说, 彩蛋, trailer, preview',
      match_regex: '',
      ignore_extensions: false,
      rename_regex: '',
      rename_replacement: '',
      only_latest: false,
      skip_existing_transferred: true,
      duplicate_episode_strategy: 'highest_quality',
      auto_create_target_dir: true,
      start_episode_number: '',
      keep_progress_on_source_change: true,
      continue_from_current_episode: true,
      finish_after_episode: '',
      preview_samples: ''
    },

    // 通知
    notifications: [],
    notificationFilter: 'all',
    notificationFilters: [
      {id: 'all', name: '全部'},
      {id: 'unread', name: '未读'},
      {id: 'push', name: '推送记录'},
      {id: 'system', name: '系统通知'}
    ],
    jobs: [],
    jobEvents: null,
    backgroundJobFilterKind: 'all',
    backgroundJobFilterStatus: 'all',
    backgroundJobQuery: '',
    selectedJob: null,
    showJobDetailDialog: false,

    // 网盘
    driveItems: [],
    driveCurrentPath: '/',
    driveCurrentFid: '0',  // 当前目录的 fid
    driveFidStack: [{fid: '0', name: '根目录'}],  // 导航栈
    driveLoading: false,
    driveRefreshing: false,
    driveError: '',
    driveLastLoadedAt: null,
    driveSelectMode: false,
    driveSelectedItems: [],
    driveSortBy: 'name',
    driveSortDirection: 'asc',
    driveFilterType: 'all',
    driveSearchQuery: '',
    driveViewMode: 'list',
    driveVisibleLimit: 200,
    driveActionLoading: '',
    showNewFolderModal: false,
    newFolderName: '',

    // 下载任务
    downloads: {active: [], waiting: [], stopped: []},
    downloadsLoading: false,
    downloadsRefreshing: false,
    downloadsError: '',
    downloadsUpdatedAt: null,
    downloadsAutoRefresh: true,
    downloadsPoller: null,
    downloadsBulkAction: '',
    downloadTaskActions: {},

    // 在线更新
    updateInfo: null,
    updateLoading: false,
    updateApplying: false,
    updateError: '',
    updateReleases: [],
    updateReleasesLoading: false,
    selectedUpdateTag: '',
    updateProgress: null,
    updateProgressTimer: null,
    showUpdateProgressDialog: false,
    updateRestarting: false,
    updateRestartError: '',

    // 转存相关
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
    quarkSigninLoading: false,
    quarkHealthLoading: false,
    quarkHealth: {status: 'unknown', message: '尚未检测', checkedAt: null, nickname: '', signinMessage: '', signinResult: null},

    // 规则中心
    ruleCenter: {
      media_type: 'series',
      season: 1,
      title: '示例剧集',
      rename_template: '{title}.S{season}E{episode}.{ext}',
      include_keywords_text: '',
      exclude_keywords_text: '',
      match_regex: '',
      rename_regex: '',
      rename_replacement: '',
      ignore_extensions: false,
      only_latest: false,
      skip_existing_transferred: true,
      duplicate_episode_strategy: 'highest_quality',
      finish_after_episode: '',
      samples: '178重置版.mp4\n第179话.mp4\nShow.S01E180.1080p.mkv'
    },
    ruleCenterPreview: null,
    ruleCenterPreviewLoading: false,
    ruleCenterPreviewError: '',

    // 设置
    settings: {
      app_username: '', app_password: '', app_password_configured: false, quark_cookie: '', quark_cookie_configured: false, quark_save_enabled: false, quark_save_root: '',
      quark_signin_cookie: '', quark_signin_cookie_configured: false,
      quark_signin_enabled: false, quark_signin_hour: 8,
      quark_save_movie_dir: '', quark_save_series_dir: '', quark_save_anime_dir: '',
      custom_categories: [],
      aria2_rpc_url: '', aria2_secret: '', aria2_secret_configured: false,
      aria2_movie_dir: '', aria2_series_dir: '', aria2_anime_dir: '',
      strm_enabled: false, strm_output_dir: '', strm_public_base_url: '', strm_access_token: '', strm_access_token_configured: false,
      cloud_types: ['quark'], push_on_update: true, push_on_failed: true, push_on_completed: true, push_on_save: true, push_on_download_completed: true, push_on_quark_signin: true,
      metadata_provider: 'tmdb', tmdb_api_key: '', tmdb_api_key_configured: false, tmdb_language: 'zh-CN',
      wecom_bot_url: '', wecom_bot_url_configured: false, telegram_bot_token: '', telegram_bot_token_configured: false, telegram_chat_id: '', bark_url: '', bark_url_configured: false, serverchan_key: '', serverchan_key_configured: false,
      wxpusher_app_token: '', wxpusher_app_token_configured: false, wxpusher_uids: '', gotify_url: '', gotify_token: '', gotify_token_configured: false, pushplus_token: '', pushplus_token_configured: false,
      subscription_check_interval_minutes: 60, subscription_scheduler_enabled: false, pansou_api_url: '', pansou_api_url_configured: false, check_links: true,
      probe_quark_files: true, filter_bad_links: true, push_silent: false,
      auto_download_new_subscription_items: false, default_rename_template: ''
    },

    get unreadNotifications() {
      return this.notificationCenterNotifications.filter(n => !n.read).length;
    },

    get pushNotifications() {
      return this.notificationCenterNotifications.filter(n => n.event === 'push_sent');
    },

    get backgroundNotificationEvents() {
      return [
        'subscription_transferred',
        'subscription_transfer_failed',
        'manual_transfer_succeeded',
        'manual_transfer_failed',
        'metadata_scrape_completed'
      ];
    },

    get notificationCenterNotifications() {
      return this.notifications.filter(n => !this.backgroundNotificationEvents.includes(n.event));
    },

    get systemNotifications() {
      return this.notificationCenterNotifications.filter(n => n.event !== 'push_sent');
    },

    get filteredNotifications() {
      if (this.notificationFilter === 'unread') return this.notificationCenterNotifications.filter(n => !n.read);
      if (this.notificationFilter === 'push') return this.pushNotifications;
      if (this.notificationFilter === 'system') return this.systemNotifications;
      return this.notificationCenterNotifications;
    },

    get backgroundLogStats() {
      const jobs = this.backgroundJobs;
      const success = jobs.filter(job => job.status === 'succeeded').length;
      const failed = jobs.filter(job => job.status === 'failed').length;
      const canceled = jobs.filter(job => job.status === 'canceled').length;
      const active = jobs.filter(job => ['queued', 'running'].includes(job.status)).length;
      const saved = jobs.reduce((sum, job) => sum + Number((job.result || {}).saved_count || (job.result || {}).transferred_count || 0), 0);
      return {total: jobs.length, active, success, failed, canceled, saved};
    },

    get backgroundJobs() {
      return this.jobs.filter(job => ['manual_transfer', 'subscription_transfer', 'metadata_scrape', 'push_dispatch'].includes(job.kind));
    },

    get recentBackgroundJobs() {
      return this.filteredBackgroundJobs.slice(0, 80);
    },

    get filteredBackgroundJobs() {
      const query = this.backgroundJobQuery.trim().toLowerCase();
      return this.backgroundJobs.filter(job => {
        if (this.backgroundJobFilterKind !== 'all' && job.kind !== this.backgroundJobFilterKind) return false;
        if (this.backgroundJobFilterStatus !== 'all' && job.status !== this.backgroundJobFilterStatus) return false;
        if (!query) return true;
        return [
          job.title,
          job.message,
          job.error,
          job.kind,
          job.status,
          JSON.stringify(job.payload || {}),
          JSON.stringify(job.result || {})
        ].some(value => String(value || '').toLowerCase().includes(query));
      });
    },

    get backgroundJobKinds() {
      return [
        {id: 'all', name: '全部类型'},
        {id: 'manual_transfer', name: '手动转存'},
        {id: 'subscription_transfer', name: '自动订阅'},
        {id: 'metadata_scrape', name: '元数据刮削'},
        {id: 'push_dispatch', name: '推送派发'}
      ];
    },

    get backgroundJobStatuses() {
      return [
        {id: 'all', name: '全部状态'},
        {id: 'queued', name: '排队中'},
        {id: 'running', name: '执行中'},
        {id: 'succeeded', name: '成功'},
        {id: 'failed', name: '失败'},
        {id: 'canceled', name: '已取消'}
      ];
    },

    get subscriptionWizardSteps() {
      const steps = [{id: 'content', name: '内容'}];
      if (this.subscriptionMode === 'continuous' || this.subscriptionEditingId) {
        steps.push({id: 'download', name: '下载'});
      }
      return steps;
    },

    get rulePresets() {
      return [
        {
          id: 'standard_tv',
          name: '标准剧集',
          description: 'S01E01 风格，适合电视剧和动画',
          template: '{title}.S{season}E{episode}.{ext}',
          exclude: this.defaultExcludeKeywords(),
          duplicate: 'highest_quality'
        },
        {
          id: 'episode_only',
          name: '仅集数',
          description: '生成 01.mp4 / 02.mkv，适合短目录',
          template: '{episode}.{ext}',
          exclude: this.defaultExcludeKeywords(),
          duplicate: 'highest_quality'
        },
        {
          id: 'original_keep',
          name: '保留原名',
          description: '尽量不改文件名，只做过滤和去重',
          template: '{original}.{ext}',
          exclude: this.defaultExcludeKeywords(),
          duplicate: 'latest_upload'
        },
        {
          id: 'movie_title',
          name: '电影标题',
          description: '电影直接使用标题和扩展名',
          template: '{title}.{ext}',
          exclude: '预告, 花絮, 解说, 彩蛋, trailer, preview, sample',
          duplicate: 'largest_size'
        }
      ];
    },

    get filteredDriveItems() {
      let items = [...this.driveItems];
      const query = this.driveSearchQuery.trim().toLowerCase();
      if (query) {
        items = items.filter(item => (item.file_name || '').toLowerCase().includes(query));
      }
      if (this.driveFilterType !== 'all') {
        items = items.filter(item => {
          if (this.driveFilterType === 'folder') return !item.file;
          if (this.driveFilterType === 'video') return this.isDriveVideo(item);
          if (this.driveFilterType === 'other') return item.file && !this.isDriveVideo(item);
          return item.file;
        });
      }
      const direction = this.driveSortDirection === 'desc' ? -1 : 1;
      items.sort((a, b) => {
        if (!a.file && b.file) return -1;
        if (a.file && !b.file) return 1;
        let value = 0;
        if (this.driveSortBy === 'size') {
          value = Number(a.size || 0) - Number(b.size || 0);
        } else if (this.driveSortBy === 'time') {
          value = this.driveTimestamp(a.updated_at) - this.driveTimestamp(b.updated_at);
        } else {
          value = (a.file_name || '').localeCompare(b.file_name || '', 'zh-CN', {numeric: true, sensitivity: 'base'});
        }
        return value * direction;
      });
      return items;
    },

    get driveBreadcrumbs() {
      return this.driveFidStack.map((item, index) => ({
        ...item,
        index,
        label: index === 0 ? '根目录' : item.name
      }));
    },

    get visibleDriveItems() {
      return this.filteredDriveItems.slice(0, this.driveVisibleLimit);
    },

    get hasMoreDriveItems() {
      return this.filteredDriveItems.length > this.visibleDriveItems.length;
    },

    get driveStats() {
      const folders = this.driveItems.filter(item => !item.file).length;
      const files = this.driveItems.length - folders;
      const videos = this.driveItems.filter(item => this.isDriveVideo(item)).length;
      const totalSize = this.driveItems
        .filter(item => item.file)
        .reduce((sum, item) => sum + Number(item.size || 0), 0);
      return {folders, files, videos, totalSize};
    },

    get selectedDriveItems() {
      const selected = new Set(this.driveSelectedItems);
      return this.driveItems.filter(item => selected.has(item.fid));
    },

    get selectedDriveFileCount() {
      return this.selectedDriveItems.filter(item => item.file).length;
    },

    get allDownloadTasks() {
      return [
        ...(this.downloads.active || []),
        ...(this.downloads.waiting || []),
        ...(this.downloads.stopped || [])
      ];
    },

    get filteredSubscriptions() {
      return this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === this.subscriptionStatusTab);
    },

    get downloadStats() {
      const active = this.downloads.active || [];
      return {
        speed: active.reduce((sum, item) => sum + Number(item.download_speed || 0), 0),
        completed: active.reduce((sum, item) => sum + Number(item.completed_length || 0), 0),
        total: active.reduce((sum, item) => sum + Number(item.total_length || 0), 0)
      };
    },

    async init() {
      this.initNavigation();
      await this.loadSubscriptions();
      await this.loadNotifications();
      await this.loadJobs();
      await this.loadSettings();
      await this.loadSettingsSchema();
      this.setupJobEvents();
      this.loadSearchHistory();
      this.runCurrentTabEffects();
    },

    initNavigation() {
      this.applyRouteFromUrl({runEffects: false});
      this.replaceRouteState();
      window.addEventListener('popstate', event => {
        if (event.state && event.state.appRoute) {
          this.applyRouteState(event.state, {runEffects: true});
        } else {
          this.applyRouteFromUrl({runEffects: true});
        }
      });
    },

    isValidTab(tabId) {
      return this.tabs.some(tab => tab.id === tabId);
    },

    isValidSettingsTab(tabId) {
      return this.settingsTabs.some(tab => tab.id === tabId);
    },

    routeUrl(tabId = this.currentTab, settingsTab = this.currentSettingsTab) {
      const url = new URL(window.location.href);
      url.searchParams.set('tab', this.isValidTab(tabId) ? tabId : 'search');
      if (tabId === 'settings') {
        url.searchParams.set('settings', this.isValidSettingsTab(settingsTab) ? settingsTab : 'basic');
      } else {
        url.searchParams.delete('settings');
      }
      return `${url.pathname}${url.search}${url.hash}`;
    },

    routeState(tabId = this.currentTab, settingsTab = this.currentSettingsTab) {
      return {
        appRoute: true,
        tab: this.isValidTab(tabId) ? tabId : 'search',
        settingsTab: this.isValidSettingsTab(settingsTab) ? settingsTab : 'basic'
      };
    },

    pushRouteState() {
      history.pushState(this.routeState(), '', this.routeUrl());
    },

    replaceRouteState() {
      history.replaceState(this.routeState(), '', this.routeUrl());
    },

    applyRouteFromUrl(options = {}) {
      const params = new URLSearchParams(window.location.search);
      const tabId = this.isValidTab(params.get('tab')) ? params.get('tab') : 'search';
      const settingsTab = this.isValidSettingsTab(params.get('settings')) ? params.get('settings') : 'basic';
      this.applyRouteState({tab: tabId, settingsTab}, options);
    },

    applyRouteState(state, options = {}) {
      this.currentTab = this.isValidTab(state.tab) ? state.tab : 'search';
      this.currentSettingsTab = this.isValidSettingsTab(state.settingsTab) ? state.settingsTab : 'basic';
      if (options.runEffects !== false) {
        this.runCurrentTabEffects();
      }
    },

    runCurrentTabEffects() {
      if (this.currentTab === 'downloads') {
        this.loadDownloads();
        this.startDownloadsPolling();
      } else {
        this.stopDownloadsPolling();
      }

      if (this.currentTab === 'settings' && this.currentSettingsTab === 'update') {
        if (!this.updateInfo && !this.updateLoading) this.checkUpdate(true);
        if (!this.updateReleases.length && !this.updateReleasesLoading) this.loadUpdateReleases(true);
        this.loadUpdateProgress().then(progress => {
          if (progress && progress.running && !this.updateProgressTimer) {
            this.startUpdateProgressPolling();
          }
          if (progress && progress.stage === 'restart_required') {
            this.showUpdateProgressDialog = true;
          }
        });
      }
    },

    selectTab(tabId, pushHistory = true) {
      if (!this.isValidTab(tabId)) return;
      const changed = this.currentTab !== tabId;
      this.currentTab = tabId;
      this.runCurrentTabEffects();
      if (pushHistory && changed) {
        this.pushRouteState();
      }
    },

    selectSettingsTab(tabId, pushHistory = true) {
      if (!this.isValidSettingsTab(tabId)) return;
      const changed = this.currentSettingsTab !== tabId;
      this.currentSettingsTab = tabId;
      this.runCurrentTabEffects();
      if (pushHistory && changed) {
        this.pushRouteState();
      }
    },

    openRuleCenter() {
      this.selectTab('settings', false);
      this.selectSettingsTab('rules');
    },

    async refresh() {
      if (this.currentTab === 'subscriptions') await this.loadSubscriptions();
      else if (this.currentTab === 'transferHistory') {
        await this.loadJobs();
      }
      else if (this.currentTab === 'notifications') await this.loadNotifications();
      else if (this.currentTab === 'settings') {
        if (this.currentSettingsTab === 'update') await this.checkUpdate(true);
        else await this.loadSettings();
      }
      else if (this.currentTab === 'drive') await this.loadDrive();
      else if (this.currentTab === 'downloads') await this.loadDownloads();
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
      this.searchProgressTimer = setInterval(() => {
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
      if (this.searchProgressTimer) {
        clearInterval(this.searchProgressTimer);
        this.searchProgressTimer = null;
      }
    },

    // ===== 搜索 =====
    async search() {
      if (!this.searchQuery.trim()) return;
      this.searching = true;
      this.searchResults = [];
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

        const response = await fetch('/api/search', {
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
        this.searchResults = data.data || [];

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
        this.showNotification('error', '搜索失败');
      } finally {
        this.searching = false;
        this.stopSearchProgressTimer();
        setTimeout(() => {
          if (!this.searching) this.resetSearchProgress();
        }, 2400);
      }
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
        include_keywords_text: '',
        exclude_keywords_text: this.defaultExcludeKeywords(),
        match_regex: '',
        ignore_extensions: false,
        rename_regex: '',
        rename_replacement: '',
        only_latest: false,
        skip_existing_transferred: true,
        duplicate_episode_strategy: 'highest_quality',
        auto_create_target_dir: true,
        start_episode_number: '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: true,
        finish_after_episode: '',
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
        include_keywords_text: '',
        exclude_keywords_text: this.defaultExcludeKeywords(),
        match_regex: '',
        ignore_extensions: false,
        rename_regex: '',
        rename_replacement: '',
        only_latest: false,
        skip_existing_transferred: true,
        duplicate_episode_strategy: 'highest_quality',
        auto_create_target_dir: true,
        start_episode_number: '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: true,
        finish_after_episode: '',
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
        include_keywords_text: (rules.include_keywords || []).join(', '),
        exclude_keywords_text: (rules.exclude_keywords || []).join(', ') || this.defaultExcludeKeywords(),
        match_regex: rules.match_regex || '',
        ignore_extensions: !!rules.ignore_extensions,
        rename_regex: rules.rename_regex || '',
        rename_replacement: rules.rename_replacement || '',
        only_latest: !!rules.only_latest,
        skip_existing_transferred: rules.skip_existing_transferred !== false,
        duplicate_episode_strategy: rules.duplicate_episode_strategy || 'highest_quality',
        auto_create_target_dir: rules.auto_create_target_dir !== false,
        start_episode_number: sub.start_episode_number || '',
        keep_progress_on_source_change: true,
        continue_from_current_episode: sub.media_type !== 'movie' && Number(sub.current_episode_number || 0) > 0,
        finish_after_episode: rules.finish_after_episode || '',
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

    // 格式化文件大小
    formatSize(bytes) {
      if (!bytes || bytes === 0) return '';
      const units = ['B', 'KB', 'MB', 'GB', 'TB'];
      const i = Math.floor(Math.log(bytes) / Math.log(1024));
      return (bytes / Math.pow(1024, i)).toFixed(1) + ' ' + units[i];
    },

    formatTime(timestamp) {
      if (!timestamp) return '-';
      return new Date(Number(timestamp) * 1000).toLocaleString();
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
      if (index < current) return 'bg-green-600/20 text-green-200 border-green-500/30';
      if (index === current) return 'bg-blue-600/20 text-blue-200 border-blue-500/40';
      return 'bg-dark-bg text-gray-500 border-dark-border';
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

    rulePresetById(id) {
      return this.rulePresets.find(item => item.id === id) || null;
    },

    applyRulePresetToSubscription(id) {
      const preset = this.rulePresetById(id);
      if (!preset) return;
      this.newSubscription.custom_rename = true;
      this.newSubscription.rename_template = preset.template;
      this.newSubscription.exclude_keywords_text = preset.exclude;
      this.newSubscription.duplicate_episode_strategy = preset.duplicate;
      if (id === 'movie_title') {
        this.newSubscription.media_type = 'movie';
      }
      this.updateSubscriptionDefaults();
      this.previewSubscriptionRename(true);
      this.showNotification('success', `已应用规则：${preset.name}`);
    },

    applyRulePresetToRuleCenter(id) {
      const preset = this.rulePresetById(id);
      if (!preset) return;
      this.ruleCenter.rename_template = preset.template;
      this.ruleCenter.exclude_keywords_text = preset.exclude;
      this.ruleCenter.duplicate_episode_strategy = preset.duplicate;
      if (id === 'movie_title') {
        this.ruleCenter.media_type = 'movie';
        this.ruleCenter.title = '示例电影';
      } else if (this.ruleCenter.media_type === 'movie') {
        this.ruleCenter.media_type = 'series';
        this.ruleCenter.title = '示例剧集';
      }
      this.previewRuleCenter(true);
      this.showNotification('success', `已载入规则：${preset.name}`);
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
        include_keywords: this.splitKeywords(this.ruleCenter.include_keywords_text),
        exclude_keywords: this.splitKeywords(this.ruleCenter.exclude_keywords_text),
        match_regex: this.ruleCenter.match_regex.trim(),
        ignore_extensions: !!this.ruleCenter.ignore_extensions,
        rename_regex: this.ruleCenter.rename_regex.trim(),
        rename_replacement: this.ruleCenter.rename_replacement,
        skip_existing_transferred: !!this.ruleCenter.skip_existing_transferred,
        auto_create_target_dir: true,
        rename_template: String(this.ruleCenter.rename_template || '').trim(),
        only_latest: !!this.ruleCenter.only_latest,
        duplicate_episode_strategy: this.ruleCenter.duplicate_episode_strategy || 'highest_quality',
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
        const response = await fetch('/api/subscriptions/rename-preview', {
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
        this.ruleCenterPreviewError = '预览失败: ' + error.message;
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
      if (action === 'new') return 'text-green-300';
      if (action === 'known') return 'text-blue-300';
      if (action === 'skip') return 'text-amber-300';
      return 'text-gray-300';
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
      if (status === 'active') return 'text-green-300';
      if (status === 'completed') return 'text-yellow-300';
      if (status === 'invalid') return 'text-gray-400';
      return 'text-gray-300';
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
        const response = await fetch(`/api/metadata/search?${params.toString()}`);
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
        if (!silent) this.showNotification('error', '元数据匹配失败');
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
        ignore_extensions: !!this.newSubscription.ignore_extensions,
        rename_regex: this.newSubscription.rename_regex.trim(),
        rename_replacement: this.newSubscription.rename_replacement,
        rename_template: this.resolveSubscriptionRenameTemplate(),
        only_latest: !!this.newSubscription.only_latest,
        duplicate_episode_strategy: this.newSubscription.duplicate_episode_strategy || 'highest_quality',
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
        const response = await fetch('/api/subscriptions/rename-preview', {
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
        this.renamePreviewError = '预览失败: ' + error.message;
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
        const response = await fetch(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);
        const data = await response.json();

        if (data.found && data.fid) {
          this.newSubscription.target_fid = data.fid;
          this.newSubscription.target_path = selected.path;
          this.showNotification('success', `已选择 ${selected.name}`);
        }
      } catch (error) {
        console.error('查找目录失败:', error);
        this.showNotification('error', '查找目录失败');
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
        const response = await fetch(`/api/drive/aria2/browse?${params.toString()}`);
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.success) {
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
        this.aria2DirError = '读取 Aria2 下载目录失败: ' + error.message;
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
        const response = await fetch('/api/transfer', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            url: this.newSubscription.url,
            passcode: this.newSubscription.password || '',
            target_fid: this.newSubscription.target_fid || '0'
          })
        });

        const data = await response.json();

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
        this.showNotification('error', '转存失败');
      }
    },

    // 创建持续订阅
    async createContinuousSubscription() {
      try {
        const rules = this.buildSubscriptionRules();

        const response = await fetch('/api/subscriptions', {
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
        this.showNotification('error', '创建订阅失败');
      }
    },

    async updateSubscription() {
      try {
        const rules = this.buildSubscriptionRules();
        const response = await fetch(`/api/subscriptions/${this.subscriptionEditingId}`, {
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
        this.showNotification('error', '保存订阅失败');
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
        const response = await fetch(`/api/drive?fid=${fid}`);
        const data = await response.json();
        this.transferBrowseItems = data.list || [];
      } catch (error) {
        console.error('加载目录失败:', error);
        this.showNotification('error', '加载目录失败');
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

        const response = await fetch('/api/transfer', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            url: this.transferTargetResult.url,
            passcode: '',
            target_fid: this.transferTargetFid
          })
        });

        const data = await response.json();

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
        this.showNotification('error', '转存失败: ' + error.message);
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
        const response = await fetch(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);
        const data = await response.json();

        if (data.found && data.fid) {
          this.transferTargetFid = data.fid;
          this.transferTargetPath = selected.path;
          this.transferBrowseFidStack = [{fid: '0', name: '根目录'}, {fid: data.fid, name: selected.name}];
          await this.loadTransferBrowse(data.fid);
          this.showNotification('success', `已切换到 ${selected.name}`);
        } else {
          this.showNotification('warning', `目录 ${selected.path} 不存在，已创建`);
          // 目录已被 ensure_dir_path 创建，重新查找
          const retryResponse = await fetch(`/api/drive/find-path?path=${encodeURIComponent(selected.path)}`);
          const retryData = await retryResponse.json();
          if (retryData.found && retryData.fid) {
            this.transferTargetFid = retryData.fid;
            this.transferTargetPath = selected.path;
            this.transferBrowseFidStack = [{fid: '0', name: '根目录'}, {fid: retryData.fid, name: selected.name}];
            await this.loadTransferBrowse(retryData.fid);
          }
        }
      } catch (error) {
        console.error('查找目录失败:', error);
        this.showNotification('error', '查找目录失败');
      }
    },

    // ===== 订阅 =====
    async loadSubscriptions() {
      try {
        const response = await fetch('/api/subscriptions');
        const data = await response.json();
        this.subscriptions = data.data || [];
      } catch (error) {
        console.error('加载订阅失败:', error);
      }
    },

    async checkSubscription(id, options = {}) {
      try {
        this.showNotification('info', '正在检查订阅...');
        const response = await fetch(`/api/subscriptions/${id}/check`, {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({force_transfer: !!options.forceTransfer})
        });
        const data = await response.json();

        if (response.ok && data.data) {
          const result = data.data;
          this.lastCheckResult = result;
          if (result.new_files.length > 0) {
            this.showNotification('success', `发现 ${result.new_files.length} 个新文件`);
          } else {
            this.showNotification('info', '无更新');
          }
          await this.loadSubscriptions();
          await this.loadJobs();
          await this.loadNotifications();
        } else {
          this.showNotification('error', data.message || '检查失败');
        }
      } catch (error) {
        console.error('检查订阅失败:', error);
        this.showNotification('error', '检查订阅失败');
      }
    },

    async checkAllSubscriptions() {
      this.checkingAllSubscriptions = true;
      try {
        this.showNotification('info', '正在批量检查订阅...');
        const response = await fetch('/api/subscriptions/check', {method: 'POST'});
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
        this.showNotification('error', '批量检查失败: ' + error.message);
      } finally {
        this.checkingAllSubscriptions = false;
      }
    },

    async renameExistingFiles(id) {
      try {
        this.showNotification('info', '正在按订阅模板修复命名...');
        const response = await fetch(`/api/subscriptions/${id}/rename-existing`, {
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
        this.showNotification('error', '修复命名失败');
      }
    },

    async generateSubscriptionStrm(id) {
      try {
        this.showNotification('info', '正在生成 STRM 文件...');
        const response = await fetch(`/api/subscriptions/${id}/strm`, {
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
        this.showNotification('error', '生成 STRM 失败');
      }
    },

    async scrapeSubscriptionMetadata(id) {
      try {
        this.showNotification('info', '已提交元数据刮削任务');
        const response = await fetch(`/api/subscriptions/${id}/metadata/scrape`, {
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
        this.showNotification('error', '提交刮削任务失败');
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
        const response = await fetch(`/api/metadata/search?${params.toString()}`);
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
        this.showNotification('error', '元数据搜索失败');
      } finally {
        this.manualMetadataSearching = false;
      }
    },

    async applyManualMetadata(item) {
      if (!this.manualMetadataSubscriptionId || !item) return;
      this.manualMetadataApplying = true;
      try {
        const response = await fetch(`/api/subscriptions/${this.manualMetadataSubscriptionId}`, {
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
        this.showNotification('error', '应用元数据失败');
      } finally {
        this.manualMetadataApplying = false;
      }
    },

    async scrapeAllSubscriptionMetadata() {
      try {
        this.showNotification('info', '已提交批量元数据刮削任务');
        const response = await fetch('/api/subscriptions/metadata/scrape', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({overwrite: false})
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
        this.showNotification('error', '提交批量刮削任务失败');
      }
    },

    async deleteSubscription(id) {
      if (!confirm('确定删除？')) return;
      try {
        const response = await fetch(`/api/subscriptions/${id}`, {method: 'DELETE'});
        if (response.ok) {
          this.showNotification('success', '已删除');
          await this.loadSubscriptions();
        }
      } catch (error) {
        console.error('删除失败:', error);
      }
    },

    editSubscription(sub) {
      this.openEditSubscriptionDialog(sub);
    },

    // ===== 通知 =====
    async loadNotifications() {
      try {
        const response = await fetch('/api/notifications');
        const data = await response.json();
        this.notifications = data.data || [];
      } catch (error) {
        console.error('加载通知失败:', error);
      }
    },

    notificationFilterCount(filterId) {
      if (filterId === 'unread') return this.unreadNotifications;
      if (filterId === 'push') return this.pushNotifications.length;
      if (filterId === 'system') return this.systemNotifications.length;
      return this.notificationCenterNotifications.length;
    },

    notificationLevelLabel(level) {
      const labels = {info: '信息', success: '成功', warning: '警告', error: '错误'};
      return labels[level] || level || '信息';
    },

    notificationLevelClass(level) {
      const classes = {
        info: 'bg-blue-600/20 text-blue-300',
        success: 'bg-green-600/20 text-green-300',
        warning: 'bg-yellow-600/20 text-yellow-300',
        error: 'bg-red-600/20 text-red-300'
      };
      return classes[level] || classes.info;
    },

    notificationEventLabel(event) {
      const labels = {
        push_sent: '推送记录',
        subscription_updated: '订阅更新',
        subscription_invalid: '订阅失效',
        subscription_completed: '订阅完结',
        subscription_transferred: '自动转存',
        download_completed: '下载完成',
        quark_signin: '夸克签到',
        subscription_transfer_failed: '转存失败',
        manual_transfer_succeeded: '手动转存',
        manual_transfer_failed: '转存失败',
        metadata_scrape_completed: '元数据刮削'
      };
      return labels[event] || '系统通知';
    },

    notificationPushChannels(notif) {
      const statuses = this.notificationPushChannelStatuses(notif);
      if (statuses.length === 0) return '-';
      return statuses.map(item => item.name).join('、');
    },

    notificationPushChannelStatuses(notif) {
      const meta = notif && notif.meta ? notif.meta : {};
      const results = meta.results || {};
      const attempts = meta.attempts || {};
      const channels = Array.isArray(meta.channels) ? meta.channels : Object.keys(results);
      return channels.map(channel => ({
        channel,
        name: this.pushChannelName(channel),
        success: results[channel] === true,
        attempts: Number(attempts[channel] || 0)
      }));
    },

    notificationPushErrors(notif) {
      const errors = (notif && notif.meta && notif.meta.errors) || {};
      return Object.entries(errors)
        .filter(([_, error]) => !!error)
        .map(([channel, error]) => ({channel, error}));
    },

    async loadJobs() {
      try {
        const response = await fetch('/api/jobs');
        const data = await response.json();
        this.jobs = data.data || [];
      } catch (error) {
        console.error('加载任务失败:', error);
      }
    },

    jobStatusLabel(status) {
      const labels = {
        queued: '排队中',
        running: '执行中',
        succeeded: '成功',
        failed: '失败',
        canceled: '已取消'
      };
      return labels[status] || status;
    },

    jobKindLabel(kind) {
      const labels = {
        manual_transfer: '手动转存',
        subscription_transfer: '自动订阅',
        metadata_scrape: '元数据刮削',
        push_dispatch: '推送派发'
      };
      return labels[kind] || kind || '后台任务';
    },

    jobStatusClass(status) {
      const classes = {
        queued: 'bg-yellow-600/20 text-yellow-300',
        running: 'bg-blue-600/20 text-blue-300',
        succeeded: 'bg-green-600/20 text-green-300',
        failed: 'bg-red-600/20 text-red-300',
        canceled: 'bg-gray-600/30 text-gray-300'
      };
      return classes[status] || 'bg-gray-600/30 text-gray-300';
    },

    resetBackgroundJobFilters() {
      this.backgroundJobFilterKind = 'all';
      this.backgroundJobFilterStatus = 'all';
      this.backgroundJobQuery = '';
    },

    openJobDetail(job) {
      this.selectedJob = job || null;
      this.showJobDetailDialog = !!job;
    },

    jobDurationLabel(job) {
      if (!job) return '-';
      const start = Number(job.started_at || job.created_at || 0);
      const end = Number(job.finished_at || job.updated_at || 0);
      if (!start || !end || end < start) return '-';
      const seconds = end - start;
      if (seconds < 60) return `${seconds}秒`;
      const minutes = Math.floor(seconds / 60);
      const rest = seconds % 60;
      if (minutes < 60) return rest ? `${minutes}分${rest}秒` : `${minutes}分钟`;
      const hours = Math.floor(minutes / 60);
      return `${hours}小时${minutes % 60}分钟`;
    },

    jobSummaryText(job) {
      if (!job) return '';
      const lines = [
        `任务：${job.title || '-'}`,
        `类型：${this.jobKindLabel(job.kind)}`,
        `状态：${this.jobStatusLabel(job.status)}`,
        `进度：${job.progress || 0}%`,
        `创建：${this.formatTime(job.created_at)}`,
        `开始：${this.formatTime(job.started_at)}`,
        `结束：${this.formatTime(job.finished_at)}`,
        `耗时：${this.jobDurationLabel(job)}`,
        `消息：${job.message || '-'}`,
      ];
      if (job.error) lines.push(`错误：${job.error}`);
      lines.push('', 'Payload:', JSON.stringify(job.payload || {}, null, 2));
      lines.push('', 'Result:', JSON.stringify(job.result || {}, null, 2));
      return lines.join('\n');
    },

    async copySelectedJobDetail() {
      if (!this.selectedJob) return;
      await this.copyText(this.jobSummaryText(this.selectedJob));
    },

    jobPayloadPretty(job) {
      return JSON.stringify((job && job.payload) || {}, null, 2);
    },

    jobResultPretty(job) {
      return JSON.stringify((job && job.result) || {}, null, 2);
    },

    canCancelJob(job) {
      return job && ['queued', 'running'].includes(job.status);
    },

    canRetryJob(job) {
      return job && ['failed', 'canceled'].includes(job.status);
    },

    async cancelJob(job) {
      if (!job || !this.canCancelJob(job)) return;
      try {
        const response = await fetch(`/api/jobs/${job.id}/cancel`, {method: 'POST'});
        const data = await response.json();
        if (response.ok) {
          this.upsertJob(data.data);
          this.showNotification('success', '任务已取消');
        } else {
          this.showNotification('error', data.message || '取消任务失败');
        }
      } catch (error) {
        this.showNotification('error', '取消任务失败');
      }
    },

    async retryJob(job) {
      if (!job || !this.canRetryJob(job)) return;
      try {
        const response = await fetch(`/api/jobs/${job.id}/retry`, {method: 'POST'});
        const data = await response.json();
        if (response.ok) {
          this.upsertJob(data.data);
          this.showNotification('success', '重试任务已创建');
        } else {
          this.showNotification('error', data.message || '重试任务失败');
        }
      } catch (error) {
        this.showNotification('error', '重试任务失败');
      }
    },

    upsertJob(job) {
      if (!job) return;
      const index = this.jobs.findIndex(item => item.id === job.id);
      if (index >= 0) {
        this.jobs.splice(index, 1, job);
      } else {
        this.jobs.unshift(job);
      }
    },

    setupJobEvents() {
      if (this.jobEvents || typeof EventSource === 'undefined') return;

      const source = new EventSource('/api/jobs/events');
      source.addEventListener('snapshot', (event) => {
        try {
          this.jobs = JSON.parse(event.data || '[]');
        } catch (error) {
          console.error('解析任务快照失败:', error);
        }
      });
      source.addEventListener('job', async (event) => {
        try {
          const job = JSON.parse(event.data);
          this.upsertJob(job);
          if (['succeeded', 'failed', 'canceled'].includes(job.status)) {
            await this.loadNotifications();
          }
          if (job.kind === 'metadata_scrape' && job.status === 'succeeded') {
            await this.loadSubscriptions();
          }
        } catch (error) {
          console.error('解析任务事件失败:', error);
        }
      });
      source.onerror = () => {
        console.warn('任务事件连接异常，浏览器会自动重连');
      };
      this.jobEvents = source;
    },

    async markRead(id) {
      try {
        await fetch(`/api/notifications/${id}/read`, {method: 'POST'});
        await this.loadNotifications();
      } catch (error) {
        console.error('标记失败:', error);
      }
    },

    async markAllRead() {
      try {
        await fetch('/api/notifications/read-all', {method: 'POST'});
        this.showNotification('success', '全部已读');
        await this.loadNotifications();
      } catch (error) {
        console.error('操作失败:', error);
      }
    },

    async clearNotifications() {
      if (!confirm('确定清空所有通知？')) return;
      try {
        await fetch('/api/notifications/clear', {method: 'POST'});
        this.showNotification('success', '已清空');
        await this.loadNotifications();
      } catch (error) {
        console.error('清空失败:', error);
      }
    },

    // ===== 网盘 =====
    async loadDrive(forceRefresh = false) {
      if (this.driveLoading || this.driveRefreshing) return;
      const hadItems = this.driveItems.length > 0;
      this.driveLoading = !hadItems;
      this.driveRefreshing = hadItems;
      this.driveError = '';
      try {
        const params = new URLSearchParams({fid: this.driveCurrentFid});
        if (forceRefresh) params.set('refresh', 'true');
        const response = await fetch(`/api/drive?${params.toString()}`);
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.driveItems = data.list || [];
          this.driveLastLoadedAt = Date.now();
          this.driveVisibleLimit = 200;
          const visibleFids = new Set(this.driveItems.map(item => item.fid));
          this.driveSelectedItems = this.driveSelectedItems.filter(fid => visibleFids.has(fid));
        } else {
          this.driveError = data.message || data.error || '加载网盘失败';
          this.driveItems = [];
        }
      } catch (error) {
        console.error('加载网盘失败:', error);
        this.driveError = '加载网盘失败: ' + error.message;
        this.driveItems = [];
      } finally {
        this.driveLoading = false;
        this.driveRefreshing = false;
      }
    },

    updateDriveCurrentPath() {
      if (this.driveFidStack.length <= 1) {
        this.driveCurrentPath = '/';
        return;
      }
      this.driveCurrentPath = this.driveFidStack.slice(1).map(d => d.name).join(' / ');
    },

    async driveGoBack() {
      if (this.driveFidStack.length > 1) {
        this.driveFidStack.pop();
        const parent = this.driveFidStack[this.driveFidStack.length - 1];
        this.driveCurrentFid = parent.fid;
        this.updateDriveCurrentPath();
        this.driveSelectedItems = [];
        await this.loadDrive();
      }
    },

    async driveOpenBreadcrumb(index) {
      if (index < 0 || index >= this.driveFidStack.length) return;
      this.driveFidStack = this.driveFidStack.slice(0, index + 1);
      const current = this.driveFidStack[this.driveFidStack.length - 1];
      this.driveCurrentFid = current.fid;
      this.updateDriveCurrentPath();
      this.driveSelectedItems = [];
      await this.loadDrive();
    },

    async driveItemClick(item) {
      if (this.driveSelectMode) {
        this.toggleDriveItemSelection(item);
        return;
      }
      if (!item.file) {
        this.driveFidStack.push({fid: item.fid, name: item.file_name});
        this.driveCurrentFid = item.fid;
        this.updateDriveCurrentPath();
        this.driveSelectedItems = [];
        await this.loadDrive();
      }
    },

    toggleSelectMode() {
      this.driveSelectMode = !this.driveSelectMode;
      if (!this.driveSelectMode) this.driveSelectedItems = [];
    },

    toggleDriveItemSelection(item) {
      if (!item || !item.fid) return;
      if (this.driveSelectedItems.includes(item.fid)) {
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => fid !== item.fid);
      } else {
        this.driveSelectedItems = [...this.driveSelectedItems, item.fid];
      }
    },

    isDriveItemSelected(item) {
      return item && this.driveSelectedItems.includes(item.fid);
    },

    driveAllVisibleSelected() {
      const visible = this.visibleDriveItems;
      return visible.length > 0 && visible.every(item => this.driveSelectedItems.includes(item.fid));
    },

    toggleVisibleDriveSelection() {
      const visibleFids = this.visibleDriveItems.map(item => item.fid);
      if (visibleFids.length === 0) return;
      if (this.driveAllVisibleSelected()) {
        const visible = new Set(visibleFids);
        this.driveSelectedItems = this.driveSelectedItems.filter(fid => !visible.has(fid));
      } else {
        this.driveSelectedItems = [...new Set([...this.driveSelectedItems, ...visibleFids])];
      }
      this.driveSelectMode = this.driveSelectedItems.length > 0;
    },

    setDriveSort(sortBy) {
      if (this.driveSortBy === sortBy) {
        this.driveSortDirection = this.driveSortDirection === 'asc' ? 'desc' : 'asc';
      } else {
        this.driveSortBy = sortBy;
        this.driveSortDirection = sortBy === 'name' ? 'asc' : 'desc';
      }
      this.driveVisibleLimit = 200;
    },

    showMoreDriveItems() {
      this.driveVisibleLimit += 200;
    },

    driveTimestamp(value) {
      if (!value) return 0;
      const parsed = Date.parse(value);
      return Number.isNaN(parsed) ? 0 : parsed;
    },

    driveUpdatedLabel(item) {
      const timestamp = this.driveTimestamp(item && item.updated_at);
      if (!timestamp) return '-';
      return new Date(timestamp).toLocaleString();
    },

    driveFileExtension(item) {
      const name = (item && item.file_name) || '';
      const match = name.match(/\.([^.]+)$/);
      return match ? match[1].toUpperCase() : 'FILE';
    },

    isDriveVideo(item) {
      return !!(item && item.file && /\.(mp4|mkv|avi|mov|ts|m4v|wmv|flv|rmvb|webm)$/i.test(item.file_name || ''));
    },

    driveItemTypeLabel(item) {
      if (!item) return '-';
      if (!item.file) return '文件夹';
      if (this.isDriveVideo(item)) return '视频';
      return this.driveFileExtension(item);
    },

    driveItemIconClass(item) {
      if (!item || !item.file) return 'bg-amber-500/15 text-amber-300 border-amber-500/20';
      if (this.isDriveVideo(item)) return 'bg-green-500/15 text-green-300 border-green-500/20';
      return 'bg-blue-500/15 text-blue-300 border-blue-500/20';
    },

    async createFolder() {
      if (!this.newFolderName.trim()) return;
      this.driveActionLoading = 'mkdir';
      try {
        const response = await fetch('/api/drive/mkdir', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({parent_fid: this.driveCurrentFid, name: this.newFolderName})
        });
        if (response.ok) {
          this.showNotification('success', '创建成功');
          this.showNewFolderModal = false;
          this.newFolderName = '';
          await this.loadDrive(true);
        } else {
          const data = await response.json().catch(() => ({}));
          this.showNotification('error', data.message || data.error || '创建失败');
        }
      } catch (error) {
        console.error('创建失败:', error);
        this.showNotification('error', '创建失败: ' + error.message);
      } finally {
        this.driveActionLoading = '';
      }
    },

    async deleteDriveItem(item) {
      if (!confirm(`确定删除 ${item.file_name}？`)) return;
      this.driveActionLoading = `delete:${item.fid}`;
      try {
        const response = await fetch('/api/drive/delete', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids: [item.fid]})
        });
        if (response.ok) {
          this.showNotification('success', '已删除');
          this.driveSelectedItems = this.driveSelectedItems.filter(fid => fid !== item.fid);
          await this.loadDrive(true);
        } else {
          const data = await response.json().catch(() => ({}));
          this.showNotification('error', data.message || data.error || '删除失败');
        }
      } catch (error) {
        console.error('删除失败:', error);
        this.showNotification('error', '删除失败: ' + error.message);
      } finally {
        this.driveActionLoading = '';
      }
    },

    async renameDriveItem(item) {
      const newName = prompt('新名称:', item.file_name);
      if (!newName || newName === item.file_name) return;
      this.driveActionLoading = `rename:${item.fid}`;
      try {
        const response = await fetch('/api/drive/rename', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            fid: item.fid,
            name: newName,
            parent_fid: this.driveCurrentFid
          })
        });
        if (response.ok) {
          const data = await response.json();
          this.showNotification('success', data.message || '重命名成功');
          await this.loadDrive(true);
        } else {
          const data = await response.json().catch(() => ({}));
          this.showNotification('error', data.message || data.error || '重命名失败');
        }
      } catch (error) {
        console.error('重命名失败:', error);
        this.showNotification('error', '重命名失败: ' + error.message);
      } finally {
        this.driveActionLoading = '';
      }
    },

    async sendDriveItemToAria2(item) {
      if (!item || !item.file) return;
      await this.submitDriveFidsToAria2([item.fid]);
    },

    async sendSelectedDriveItemsToAria2() {
      const selectedFiles = this.selectedDriveItems
        .filter(item => item.file)
        .map(item => item.fid);
      if (selectedFiles.length === 0) {
        this.showNotification('warning', '请选择要发送到 Aria2 的文件');
        return;
      }
      await this.submitDriveFidsToAria2(selectedFiles);
    },

    async submitDriveFidsToAria2(fids) {
      this.driveActionLoading = 'aria2';
      try {
        const response = await fetch('/api/drive/aria2', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids})
        });
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.showNotification('success', data.message || `已提交 ${fids.length} 个 Aria2 任务`);
          this.selectTab('downloads');
        } else {
          this.showNotification('error', data.message || data.error || '提交 Aria2 失败');
        }
      } catch (error) {
        console.error('提交 Aria2 失败:', error);
        this.showNotification('error', '提交 Aria2 失败: ' + error.message);
      } finally {
        this.driveActionLoading = '';
      }
    },

    // ===== 下载任务 =====
    async loadDownloads(silent = false) {
      if (this.downloadsLoading || this.downloadsRefreshing) return;
      this.downloadsLoading = !silent;
      this.downloadsRefreshing = silent;
      try {
        const response = await fetch('/api/drive/aria2/tasks?stopped_limit=10');
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.downloads = {
            active: data.active || [],
            waiting: data.waiting || [],
            stopped: data.stopped || []
          };
          this.downloadsError = '';
          this.downloadsUpdatedAt = Date.now();
        } else {
          this.downloadsError = data.message || data.error || '加载 Aria2 任务失败';
        }
      } catch (error) {
        console.error('加载 Aria2 任务失败:', error);
        this.downloadsError = '加载 Aria2 任务失败: ' + error.message;
      } finally {
        this.downloadsLoading = false;
        this.downloadsRefreshing = false;
      }
    },

    async controlAllDownloads(action) {
      const labels = {
        pause: '暂停全部下载任务',
        stop: '停止全部下载任务'
      };
      if (action === 'stop' && !confirm('确定停止全部活动和排队中的 Aria2 下载任务？')) return;
      this.downloadsBulkAction = action;
      try {
        const response = await fetch(`/api/drive/aria2/tasks/${action}-all`, {method: 'POST'});
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
          await this.loadDownloads(true);
        } else {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
        }
      } catch (error) {
        this.showNotification('error', `${labels[action] || '操作'}失败: ${error.message}`);
      } finally {
        this.downloadsBulkAction = '';
      }
    },

    async controlDownloadTask(task, action) {
      if (!task || !task.gid) return;
      const labels = {
        pause: '暂停下载任务',
        resume: '继续下载任务',
        stop: '停止下载任务',
        delete: '删除下载任务记录'
      };
      if (action === 'stop' && !confirm(`确定停止下载任务 ${task.file_name || task.gid}？`)) return;
      if (action === 'delete' && !confirm(`确定删除下载任务记录 ${task.file_name || task.gid}？`)) return;

      this.downloadTaskActions = {...this.downloadTaskActions, [task.gid]: action};
      try {
        const response = await fetch(`/api/drive/aria2/tasks/${encodeURIComponent(task.gid)}/${action}`, {method: 'POST'});
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.showNotification('success', data.message || `${labels[action] || '操作'}成功`);
          await this.loadDownloads(true);
        } else {
          this.showNotification('error', data.message || data.error || `${labels[action] || '操作'}失败`);
        }
      } catch (error) {
        this.showNotification('error', `${labels[action] || '操作'}失败: ${error.message}`);
      } finally {
        const next = {...this.downloadTaskActions};
        delete next[task.gid];
        this.downloadTaskActions = next;
      }
    },

    hasRunningDownloadTasks() {
      return [...(this.downloads.active || []), ...(this.downloads.waiting || [])]
        .some(task => ['active', 'waiting', 'paused'].includes(task.status));
    },

    downloadTaskActionLoading(task) {
      return task && task.gid ? this.downloadTaskActions[task.gid] || '' : '';
    },

    canPauseDownloadTask(task) {
      return task && ['active', 'waiting'].includes(task.status);
    },

    canResumeDownloadTask(task) {
      return task && task.status === 'paused';
    },

    canStopDownloadTask(task) {
      return task && ['active', 'waiting', 'paused'].includes(task.status);
    },

    startDownloadsPolling() {
      this.stopDownloadsPolling();
      if (!this.downloadsAutoRefresh || this.currentTab !== 'downloads') return;
      this.downloadsPoller = setInterval(() => this.loadDownloads(true), 2000);
    },

    stopDownloadsPolling() {
      if (this.downloadsPoller) {
        clearInterval(this.downloadsPoller);
        this.downloadsPoller = null;
      }
    },

    downloadStatusLabel(status) {
      const labels = {
        active: '下载中',
        waiting: '排队中',
        paused: '已暂停',
        complete: '已完成',
        error: '失败',
        removed: '已移除'
      };
      return labels[status] || status || '-';
    },

    downloadStatusClass(status) {
      if (status === 'active') return 'bg-blue-600/20 text-blue-300';
      if (status === 'waiting') return 'bg-amber-600/20 text-amber-300';
      if (status === 'complete') return 'bg-green-600/20 text-green-300';
      if (status === 'error') return 'bg-red-600/20 text-red-300';
      return 'bg-gray-600/20 text-gray-300';
    },

    downloadProgressStyle(task) {
      const value = Math.max(0, Math.min(100, Number(task && task.progress ? task.progress : 0)));
      return `width: ${value}%`;
    },

    formatPercent(value) {
      return `${Math.max(0, Math.min(100, Number(value || 0))).toFixed(1)}%`;
    },

    formatDownloadSize(bytes) {
      return Number(bytes || 0) > 0 ? this.formatSize(Number(bytes)) : '-';
    },

    formatSpeed(bytes) {
      return Number(bytes || 0) > 0 ? `${this.formatSize(Number(bytes))}/s` : '0 B/s';
    },

    formatDuration(seconds) {
      const value = Number(seconds || 0);
      if (!value) return '-';
      const hours = Math.floor(value / 3600);
      const minutes = Math.floor((value % 3600) / 60);
      const secs = value % 60;
      if (hours > 0) return `${hours}h ${minutes}m`;
      if (minutes > 0) return `${minutes}m ${secs}s`;
      return `${secs}s`;
    },

    sortDriveItems() {
      // 列表排序由 filteredDriveItems 统一派生。
    },

    filterDriveItems() {
      // 由 computed 属性处理
    },

    async batchDeleteDrive() {
      if (this.driveSelectedItems.length === 0) return;
      if (!confirm(`确定删除选中的 ${this.driveSelectedItems.length} 个项目？`)) return;

      this.driveActionLoading = 'batch-delete';
      try {
        const fids = [...this.driveSelectedItems];
        const response = await fetch('/api/drive/delete', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({fids})
        });
        const data = await response.json().catch(() => ({}));
        if (response.ok) {
          this.showNotification('success', data.message || `已删除 ${fids.length} 项`);
          this.driveSelectedItems = [];
          this.driveSelectMode = false;
          await this.loadDrive(true);
        } else {
          this.showNotification('error', data.message || data.error || '批量删除失败');
        }
      } catch (error) {
        console.error('批量删除失败:', error);
        this.showNotification('error', '批量删除失败: ' + error.message);
      } finally {
        this.driveActionLoading = '';
      }
    },

    formatSize(bytes) {
      if (!bytes) return '-';
      const units = ['B', 'KB', 'MB', 'GB', 'TB'];
      let size = bytes;
      let unitIndex = 0;
      while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024;
        unitIndex++;
      }
      return `${size.toFixed(2)} ${units[unitIndex]}`;
    },

    // ===== 设置 =====
    async checkUpdate(silent = false) {
      this.updateLoading = true;
      this.updateError = '';
      try {
        const [checkResponse] = await Promise.all([
          fetch('/api/update/check'),
          this.loadUpdateReleases(true)
        ]);
        const response = checkResponse;
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateInfo = result.data;
          if (!silent) {
            this.showNotification(
              result.data.update_available ? 'success' : 'info',
              result.data.update_available ? `发现新版本 ${result.data.latest_tag}` : '当前已是最新版本'
            );
          }
        } else {
          this.updateError = result.message || result.error || '检查更新失败';
          if (!silent) this.showNotification('error', this.updateError);
        }
      } catch (error) {
        this.updateError = '检查更新失败: ' + error.message;
        if (!silent) this.showNotification('error', this.updateError);
      } finally {
        this.updateLoading = false;
      }
    },

    async loadUpdateReleases(silent = false) {
      this.updateReleasesLoading = true;
      try {
        const response = await fetch('/api/update/releases', {cache: 'no-store'});
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateReleases = result.data || [];
          if (!this.selectedUpdateTag) {
            const latest = this.updateReleases.find(item => !item.is_current && item.asset) || this.updateReleases[0];
            this.selectedUpdateTag = latest ? latest.tag : '';
          }
        } else if (!silent) {
          this.updateError = result.message || result.error || '读取版本列表失败';
          this.showNotification('error', this.updateError);
        }
      } catch (error) {
        if (!silent) {
          this.updateError = '读取版本列表失败: ' + error.message;
          this.showNotification('error', this.updateError);
        }
      } finally {
        this.updateReleasesLoading = false;
      }
    },

    selectedUpdateRelease() {
      return this.updateReleases.find(item => item.tag === this.selectedUpdateTag) || null;
    },

    selectedUpdateActionLabel() {
      const item = this.selectedUpdateRelease();
      if (!item) return '选择版本';
      if (item.is_current) return '当前版本';
      return item.is_newer ? '升级到所选版本' : '回退到所选版本';
    },

    selectedUpdateDescription() {
      const item = this.selectedUpdateRelease();
      if (!item) return '请选择一个 Release 版本。';
      if (!item.asset) return '该版本没有 Linux x86_64 二进制包，不能在线切换。';
      if (item.is_current) return '当前服务已经运行该版本。';
      const direction = item.is_newer ? '升级' : '回退';
      return `将${direction}到 ${item.tag}，替换二进制和 WebUI 静态资源，完成后需要重启。`;
    },

    canApplySelectedUpdate() {
      const item = this.selectedUpdateRelease();
      return Boolean(item && item.asset && !item.is_current && !this.updateApplying);
    },

    async applySelectedUpdate() {
      const item = this.selectedUpdateRelease();
      if (!item) {
        this.showNotification('info', '请先选择版本');
        return;
      }
      if (item.is_current) {
        this.showNotification('info', '当前已经是所选版本');
        return;
      }
      await this.applyUpdate(item.tag);
    },

    async applyUpdate(targetTag = null) {
      if (!targetTag && (!this.updateInfo || !this.updateInfo.update_available)) {
        this.showNotification('info', '当前已是最新版本');
        return;
      }
      this.updateApplying = true;
      this.updateError = '';
      this.updateRestartError = '';
      this.updateRestarting = false;
      this.showUpdateProgressDialog = true;
      this.setLocalUpdateProgress(1, '正在准备升级');
      this.startUpdateProgressPolling();
      try {
        const body = targetTag ? JSON.stringify({tag: targetTag}) : '{}';
        const response = await fetch('/api/update/apply', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body
        });
        const result = await response.json().catch(() => ({}));
        await this.loadUpdateProgress();
        if (response.ok && result.data) {
          const restartRequired = Boolean(result.data.restart_required);
          this.setLocalUpdateProgress(
            100,
            result.data.message || (restartRequired ? '升级完成，请重启服务后生效' : '升级完成'),
            restartRequired ? 'restart_required' : 'completed',
            false
          );
          this.stopUpdateProgressPolling();
          this.showNotification('success', result.data.message || '升级完成');
          if (this.updateInfo) {
            this.updateInfo.current_version = result.data.new_version || this.updateInfo.current_version;
            this.updateInfo.update_available = false;
          }
          await this.loadUpdateReleases(true);
        } else {
          this.updateError = result.message || result.error || '升级失败';
          this.markUpdateProgressFailed(this.updateError);
          this.stopUpdateProgressPolling();
          this.showNotification('error', this.updateError);
        }
      } catch (error) {
        await this.loadUpdateProgress();
        if (this.updateRestartRequired()) {
          this.updateError = '';
          this.showNotification('success', '升级完成，请重启服务后生效');
        } else {
          this.updateError = '升级失败: ' + error.message;
          this.markUpdateProgressFailed(this.updateError);
          this.showNotification('error', this.updateError);
        }
        this.stopUpdateProgressPolling();
      } finally {
        this.updateApplying = false;
        if (!this.updateProgress || !this.updateProgress.running) {
          this.stopUpdateProgressPolling();
        }
      }
    },

    async restartAfterUpdate() {
      if (this.updateRestarting) return;
      this.updateRestarting = true;
      this.updateRestartError = '';
      this.updateError = '';
      this.showUpdateProgressDialog = true;
      this.setLocalUpdateProgress(100, '正在请求服务重启', 'restarting', false);
      try {
        const response = await fetch('/api/update/restart', {method: 'POST'});
        const result = await response.json().catch(() => ({}));
        if (!response.ok) {
          throw new Error(result.message || result.error || '请求重启失败');
        }
        this.setLocalUpdateProgress(100, result.data?.message || '服务正在重启，请稍后刷新页面', 'restarting', false);
        await this.waitForServiceRestart();
        window.location.reload();
      } catch (error) {
        if (error instanceof TypeError) {
          try {
            await this.waitForServiceRestart();
            window.location.reload();
            return;
          } catch (pollError) {
            this.updateRestartError = pollError.message;
          }
        } else {
          this.updateRestartError = '重启失败: ' + error.message;
        }
        this.setLocalUpdateProgress(100, this.updateRestartError, 'restart_required', false, this.updateRestartError);
        this.showNotification('error', this.updateRestartError);
      } finally {
        this.updateRestarting = false;
      }
    },

    async waitForServiceRestart(timeoutMs = 60000) {
      await this.sleep(2000);
      const startedAt = Date.now();
      while (Date.now() - startedAt < timeoutMs) {
        try {
          const response = await fetch(`/health?restart=${Date.now()}`, {cache: 'no-store'});
          if (response.ok) return true;
        } catch (error) {
          // The connection is expected to fail while the process is restarting.
        }
        await this.sleep(1000);
      }
      throw new Error('服务重启超时，请稍后手动刷新页面');
    },

    sleep(ms) {
      return new Promise(resolve => setTimeout(resolve, ms));
    },

    async loadUpdateProgress(silent = true) {
      try {
        const response = await fetch('/api/update/progress', {cache: 'no-store'});
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.updateProgress = this.normalizeUpdateProgress(result.data);
          if (this.updateProgress && this.updateProgress.stage === 'restart_required') {
            this.showUpdateProgressDialog = true;
          }
          if (this.updateProgress && !this.updateProgress.running) {
            this.stopUpdateProgressPolling();
          }
          return this.updateProgress;
        }
        if (!silent) {
          this.updateError = result.message || result.error || '读取升级进度失败';
        }
      } catch (error) {
        if (!silent) {
          this.updateError = '读取升级进度失败: ' + error.message;
        }
      }
      return this.updateProgress;
    },

    normalizeUpdateProgress(progress) {
      if (!progress) return null;
      const percent = Math.max(0, Math.min(100, Number(progress.percent) || 0));
      if (!progress.running && !progress.error && progress.stage === 'idle' && percent === 0) {
        return null;
      }
      return {
        ...progress,
        percent,
        downloaded_bytes: Number(progress.downloaded_bytes) || 0,
        total_bytes: progress.total_bytes ? Number(progress.total_bytes) : null
      };
    },

    startUpdateProgressPolling() {
      this.stopUpdateProgressPolling();
      this.loadUpdateProgress();
      this.updateProgressTimer = setInterval(() => this.loadUpdateProgress(), 800);
    },

    stopUpdateProgressPolling() {
      if (this.updateProgressTimer) {
        clearInterval(this.updateProgressTimer);
        this.updateProgressTimer = null;
      }
    },

    setLocalUpdateProgress(percent, message, stage = 'starting', running = true, error = null) {
      const current = this.updateProgress || {};
      this.updateProgress = {
        running,
        percent: Math.max(0, Math.min(100, Number(percent) || 0)),
        stage,
        message,
        downloaded_bytes: Number(current.downloaded_bytes) || 0,
        total_bytes: current.total_bytes || null,
        error,
        updated_at: new Date().toISOString()
      };
    },

    markUpdateProgressFailed(message) {
      const currentPercent = this.updateProgress ? this.updateProgress.percent : 1;
      this.setLocalUpdateProgress(currentPercent, message, 'failed', false, message);
    },

    updateRestartRequired() {
      return Boolean(this.updateProgress && this.updateProgress.stage === 'restart_required' && !this.updateProgress.error);
    },

    updateDialogBusy() {
      return Boolean(this.updateApplying || this.updateRestarting || (this.updateProgress && this.updateProgress.running));
    },

    updateDialogCanClose() {
      return !this.updateDialogBusy() && !this.updateRestartRequired();
    },

    closeUpdateProgressDialog() {
      if (this.updateDialogCanClose()) {
        this.showUpdateProgressDialog = false;
      }
    },

    updateDialogTitle() {
      if (this.updateRestarting || (this.updateProgress && this.updateProgress.stage === 'restarting')) return '服务重启中';
      if (this.updateProgress && this.updateProgress.error) return '升级失败';
      if (this.updateRestartRequired()) return '升级完成';
      return '在线升级';
    },

    updateProgressPercent() {
      return this.updateProgress ? this.updateProgress.percent : 0;
    },

    updateProgressMessage() {
      return this.updateProgress && this.updateProgress.message ? this.updateProgress.message : '正在准备升级';
    },

    updateProgressBarClass() {
      if (this.updateProgress && this.updateProgress.error) return 'bg-red-500';
      if (this.updateProgress && this.updateProgress.percent >= 100) return 'bg-green-500';
      return 'bg-blue-500';
    },

    updateStatusLabel() {
      if (!this.updateInfo) return '未检查';
      return this.updateInfo.update_available ? '可更新' : '已最新';
    },

    updateStatusClass() {
      if (!this.updateInfo) return 'text-gray-400';
      return this.updateInfo.update_available ? 'text-amber-300' : 'text-green-300';
    },

    updateRuntimeLabel() {
      if (!this.updateInfo) return '-';
      return this.updateInfo.runtime === 'docker' ? 'Docker' : '二进制';
    },

    assetSizeLabel(asset) {
      return asset && asset.size ? this.formatSize(asset.size) : '-';
    },

    formatUpdateTime(value) {
      return value ? new Date(value).toLocaleString() : '-';
    },

    async copyText(value) {
      if (!value) return;
      try {
        await navigator.clipboard.writeText(value);
        this.showNotification('success', '已复制');
      } catch (error) {
        this.showNotification('error', '复制失败');
      }
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
        const response = await fetch(`/api/settings/secret/${encodeURIComponent(key)}`);
        const result = await response.json().catch(() => ({}));
        if (response.ok && result.data) {
          this.settings[key] = result.data.value || '';
          this.revealedSecrets[key] = true;
        } else {
          this.showNotification('error', result.message || result.error || '读取明文失败');
        }
      } catch (error) {
        console.error('读取明文失败:', error);
        this.showNotification('error', '读取明文失败');
      } finally {
        this.secretLoading[key] = false;
      }
    },

    async loadSettings() {
      try {
        const response = await fetch('/api/settings');
        const data = await response.json();
        // 直接使用服务器返回的 data 对象
        if (data.data) {
          this.settings = {...this.settings, ...data.data};
        } else {
          this.settings = {...this.settings, ...data};
        }
        this.normalizeCustomCategories();
        this.resetSecretVisibility();
      } catch (error) {
        console.error('加载设置失败:', error);
      }
    },

    async loadSettingsSchema() {
      try {
        const response = await fetch('/api/settings/schema');
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
        this.normalizeCustomCategories();
        const response = await fetch('/api/settings', {
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
        this.showNotification('error', '保存失败');
      }
    },

    async runQuarkSignin() {
      if (!this.settings.quark_cookie && !this.settings.quark_cookie_configured) {
        this.showNotification('error', '请先在设置中配置夸克 Cookie');
        return;
      }
      this.quarkSigninLoading = true;
      try {
        const response = await fetch('/api/quark/signin', {method: 'POST'});
        const data = await response.json().catch(() => ({}));
        if (response.ok && data.success) {
          this.quarkHealth.signinMessage = data.message || '夸克签到成功';
          this.quarkHealth.signinResult = data.result || null;
          this.showNotification('success', data.message || '夸克签到成功');
          await this.loadNotifications();
        } else {
          this.quarkHealth.signinMessage = data.message || data.error || '夸克签到失败';
          this.showNotification('error', data.message || data.error || '夸克签到失败');
        }
      } catch (error) {
        console.error('夸克签到失败:', error);
        this.quarkHealth.signinMessage = '夸克签到失败: ' + error.message;
        this.showNotification('error', '夸克签到失败: ' + error.message);
      } finally {
        this.quarkSigninLoading = false;
      }
    },

    quarkHealthStatusLabel() {
      const labels = {ok: '正常', failed: '异常', unknown: '未检测'};
      return labels[this.quarkHealth.status] || '未检测';
    },

    quarkHealthStatusClass() {
      if (this.quarkHealth.status === 'ok') return 'text-green-300';
      if (this.quarkHealth.status === 'failed') return 'text-red-300';
      return 'text-gray-300';
    },

    quarkSigninProgressLabel() {
      const result = this.quarkHealth.signinResult || {};
      const progress = Number(result.sign_progress || 0);
      const target = Number(result.sign_target || 0);
      if (progress > 0 && target > 0) return `${progress}/${target}`;
      return '-';
    },

    quarkCapacityLabel() {
      const result = this.quarkHealth.signinResult || {};
      const bytes = Number(result.total_capacity_bytes || 0);
      return bytes > 0 ? this.formatSize(bytes) : '-';
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
        const response = await fetch('/api/quark/test', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({cookie})
        });
        const data = await response.json();
        if (response.ok && data.success) {
          this.quarkHealth = {
            ...this.quarkHealth,
            status: 'ok',
            message: '夸克 Cookie 可用',
            nickname: data.nickname || '夸克用户',
            checkedAt: Date.now() / 1000
          };
          if (!silent) this.showNotification('success', `测试成功！用户: ${data.nickname || '未知'}`);
        } else {
          this.quarkHealth = {
            ...this.quarkHealth,
            status: 'failed',
            message: data.error || '测试失败，请检查 Cookie',
            nickname: '',
            checkedAt: Date.now() / 1000
          };
          if (!silent) this.showNotification('error', data.error || '测试失败，请检查 Cookie');
        }
      } catch (error) {
        console.error('测试失败:', error);
        this.quarkHealth = {
          ...this.quarkHealth,
          status: 'failed',
          message: '连接失败，请检查配置',
          nickname: '',
          checkedAt: Date.now() / 1000
        };
        if (!silent) this.showNotification('error', '连接失败，请检查配置');
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
        const response = await fetch('/api/drive/aria2/test');
        const data = await response.json().catch(() => ({}));
        if (response.ok && data.success) {
          this.showNotification('success', data.message || 'Aria2 连接成功');
        } else {
          this.showNotification('error', data.message || data.error || 'Aria2 测试失败');
        }
      } catch (error) {
        console.error('Aria2 测试失败:', error);
        this.showNotification('error', 'Aria2 测试失败: ' + error.message);
      }
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
        const response = await fetch('/api/push/test', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify(channel ? {channels: [channel]} : {})
        });
        const data = await response.json();
        if (response.ok) {
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
        } else {
          this.showNotification('error', data.error || '推送失败');
        }
      } catch (error) {
        console.error('推送失败:', error);
        this.showNotification('error', '推送失败，请检查配置');
      }
    },

    showNotification(type, message) {
      const container = document.getElementById('toastContainer');
      if (!container) {
        console[type === 'error' ? 'error' : 'info'](`[${type}] ${message}`);
        return;
      }

      const toast = document.createElement('div');
      toast.className = `toast toast-${type}`;

      const icon = {
        success: '✓',
        error: '✕',
        warning: '⚠',
        info: 'ℹ'
      }[type] || 'ℹ';

      const iconEl = document.createElement('span');
      iconEl.className = 'toast-icon';
      iconEl.textContent = icon;

      const messageEl = document.createElement('span');
      messageEl.className = 'toast-message';
      messageEl.textContent = String(message || '');

      toast.appendChild(iconEl);
      toast.appendChild(messageEl);

      container.appendChild(toast);

      setTimeout(() => {
        toast.style.transition = 'all 0.3s ease-out';
        toast.style.opacity = '0';
        toast.style.transform = 'translateX(400px)';
        setTimeout(() => toast.remove(), 300);
      }, 3000);
    }
  }
}
