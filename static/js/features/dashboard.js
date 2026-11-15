(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDashboard = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  // 活动中心不展示推送派发任务；工作台的失败计数必须用同一口径，
  // 否则点进「失败任务」会找不到被计入的那一条。
  const notificationsApi = root.MediaSubNotifications || {};
  const isHiddenActivityJob = typeof notificationsApi.isNoisyActivityJob === 'function'
    ? notificationsApi.isNoisyActivityJob
    : () => false;

  /// 工作台卡片目录。zone 决定卡片落在哪一区：compact 区按内容高度排布，
  /// panel 区平分剩余高度并各自内部滚动——这样无论用户怎么排都还是一屏。
  const CARD_CATALOG = Object.freeze([
    {id: 'command', name: '概览与操作', hint: '状态摘要与主操作', zone: 'compact', span: 12},
    {id: 'calendar', name: '更新日历', hint: '播出排期与缺集状态', zone: 'panel', span: 12}
  ]);

  /// 旧布局映射。更新日历顶替了订阅看板；快捷入口、自动化执行、最近活动的内容
  /// 已被指标格与活动中心覆盖；夸克网盘状态自 v2.2.20 起常驻左侧导航栏。
  /// 指标行的计数与「检查全部」重复：追更/完结在订阅页一目了然，失败任务与
  /// 未读通知去了活动中心，下载数据自 v2.2.21 起常驻侧边栏「自动化节点」。
  const LEGACY_CARD_IDS = Object.freeze({
    hero: ['command'],
    library: ['calendar'],
    kpis: [],
    operations: [],
    cloud: [],
    quick_actions: [],
    automation: [],
    activity: []
  });

  const CARD_IDS = CARD_CATALOG.map(card => card.id);

  /// 空数组与缺省值都按「全部显示」处理：旧版本把「从未自定义」存成 []，
  /// 因此「全部隐藏」没有可持久化的表示，编辑时强制至少保留一张卡片。
  function normalizeCardIds(list) {
    if (!Array.isArray(list) || list.length === 0) return [...CARD_IDS];
    const seen = new Set();
    const result = [];
    for (const raw of list) {
      const ids = LEGACY_CARD_IDS[raw] || [raw];
      for (const id of ids) {
        if (!CARD_IDS.includes(id) || seen.has(id)) continue;
        seen.add(id);
        result.push(id);
      }
    }
    // 只含已下线卡片的旧布局同样回到默认，而不是渲染一个空工作台。
    return result.length ? result : [...CARD_IDS];
  }

  /// 工作台待处理事项：每项对应一个可以直接筛出问题的页面。
  const ATTENTION_ENTRIES = Object.freeze([
    {id: 'invalid', label: '失效订阅', stat: 'invalidSubs', tone: 'danger'},
    {id: 'failed', label: '失败任务', stat: 'failedJobs', tone: 'warning'},
    {id: 'unread', label: '未读通知', stat: 'unreadNotifications', tone: 'primary'}
  ]);

  function createStore() {
    return {
    dashboardEditing: false,
    dashboardLayoutDraft: [],

    /// 当前生效的卡片顺序；编辑态下看草稿，所见即所得。
    get dashboardLayout() {
      const source = this.dashboardEditing
        ? this.dashboardLayoutDraft
        : (this.settings && this.settings.dashboard_widgets);
      return normalizeCardIds(source);
    },

    dashboardCards(zone) {
      return this.dashboardLayout
        .map(id => CARD_CATALOG.find(card => card.id === id))
        .filter(card => card && card.zone === zone);
    },

    dashboardHiddenCards() {
      const visible = this.dashboardLayout;
      return CARD_CATALOG.filter(card => !visible.includes(card.id));
    },

    startDashboardEdit() {
      this.dashboardLayoutDraft = normalizeCardIds(
        this.settings && this.settings.dashboard_widgets
      );
      this.dashboardEditing = true;
    },

    cancelDashboardEdit() {
      this.dashboardEditing = false;
      this.dashboardLayoutDraft = [];
    },

    /// 最后一张可见卡片不能隐藏（见 normalizeCardIds 的说明）。
    canHideDashboardCard(id) {
      return this.dashboardLayoutDraft.includes(id) && this.dashboardLayoutDraft.length > 1;
    },

    /// 卡片的先后由所在区域决定（概览在上、日历在下），因此这里只切换显隐，
    /// 不再提供跨区域无法生效的上移/下移。
    toggleDashboardCard(id) {
      if (!CARD_IDS.includes(id)) return;
      if (this.dashboardLayoutDraft.includes(id)) {
        if (!this.canHideDashboardCard(id)) return;
        this.dashboardLayoutDraft = this.dashboardLayoutDraft.filter(item => item !== id);
        return;
      }
      // 恢复时按目录顺序插回，保存结果不依赖点击先后。
      const draft = [...this.dashboardLayoutDraft, id];
      this.dashboardLayoutDraft = CARD_IDS.filter(item => draft.includes(item));
    },

    resetDashboardLayout() {
      this.dashboardLayoutDraft = [...CARD_IDS];
    },

    async saveDashboardLayout() {
      this.settings.dashboard_widgets = normalizeCardIds(this.dashboardLayoutDraft);
      this.dashboardEditing = false;
      this.dashboardLayoutDraft = [];
      await this.saveSettings();
    },

    get dashboardStats() {
      const activeSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'active').length;
      const invalidSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'invalid').length;
      const completedSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'completed').length;
      const runningJobs = this.jobs.filter(job => ['queued', 'running'].includes(job.status)).length;
      const failedJobs = this.jobs.filter(job => job.status === 'failed' && !isHiddenActivityJob(job)).length;
      return {
        activeSubs,
        invalidSubs,
        completedSubs,
        runningJobs,
        failedJobs,
        unreadNotifications: this.unreadNotifications,
        downloadSpeed: this.downloadStats.speed
      };
    },

    get dashboardAttentionCount() {
      return this.dashboardStats.invalidSubs
        + this.dashboardStats.failedJobs
        + this.dashboardStats.unreadNotifications;
    },

    /// 需要处理的事项，按类别分开计数；数量为 0 的类别不出现。
    get dashboardAttentionItems() {
      const stats = this.dashboardStats;
      return ATTENTION_ENTRIES
        .map(entry => ({id: entry.id, label: entry.label, tone: entry.tone, count: Number(stats[entry.stat]) || 0}))
        .filter(item => item.count > 0);
    },

    /// 从工作台直接进入对应页面并带上筛选，不用再逐页寻找问题所在。
    openDashboardAttention(id) {
      if (id === 'invalid') {
        this.setSubscriptionStatusTab('invalid');
        this.selectTab('subscriptions');
        return;
      }
      if (id !== 'failed' && id !== 'unread') return;
      this.activityQuery = '';
      this.activityFilter = id;
      this.activityVisibleLimit = 100;
      this.selectTab('notifications');
    },

    dashboardStatusSummary() {
      if (this.subscriptions.length === 0) {
        return '当前还没有订阅。可以先搜索资源并创建订阅。';
      }
      if (this.dashboardAttentionCount === 0) {
        return `共 ${this.subscriptions.length} 个订阅；当前没有失效订阅、失败任务或未读通知。`;
      }
      return `共 ${this.subscriptions.length} 个订阅，${this.dashboardAttentionCount} 项状态需要处理：`;
    },

    dashboardDateLabel() {
      return new Intl.DateTimeFormat('zh-CN', {
        month: 'long',
        day: 'numeric',
        weekday: 'long'
      }).format(new Date());
    },

    };
  }

  return {createStore};
});
