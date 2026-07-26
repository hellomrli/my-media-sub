(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubDashboard = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  /// 工作台卡片目录。zone 决定卡片落在哪一区：compact 区按内容高度排布，
  /// panel 区平分剩余高度并各自内部滚动——这样无论用户怎么排都还是一屏。
  /// 工作台卡片目录。zone 决定卡片落在哪一区：compact 区按内容高度排布，
  /// panel 区平分剩余高度并各自内部滚动——这样无论用户怎么排都还是一屏。
  const CARD_CATALOG = Object.freeze([
    {id: 'command', name: '概览与操作', hint: '状态摘要与主操作', zone: 'compact', span: 12},
    {id: 'kpis', name: '运行指标', hint: '订阅、任务与下载计数', zone: 'compact', span: 12},
    {id: 'calendar', name: '更新日历', hint: '播出排期与缺集状态', zone: 'panel', span: 12}
  ]);

  /// 旧布局映射。更新日历顶替了订阅看板；快捷入口、自动化执行、最近活动的内容
  /// 已被指标格与活动中心覆盖；夸克网盘状态自 v2.2.20 起常驻左侧导航栏。
  const LEGACY_CARD_IDS = Object.freeze({
    hero: ['command'],
    library: ['calendar'],
    operations: [],
    cloud: [],
    quick_actions: [],
    automation: [],
    activity: []
  });

  const CARD_IDS = CARD_CATALOG.map(card => card.id);

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
    return result;
  }

  function createStore() {
    return {
    dashboardEditing: false,
    dashboardLayoutDraft: [],

    dashboardWidgetEnabled(id) {
      return this.dashboardLayout.includes(id);
    },

    /// 当前生效的卡片顺序；编辑态下看草稿，所见即所得。
    get dashboardLayout() {
      const source = this.dashboardEditing
        ? this.dashboardLayoutDraft
        : (this.settings && this.settings.dashboard_widgets);
      return normalizeCardIds(source);
    },

    dashboardCardCatalog() {
      return CARD_CATALOG;
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

    toggleDashboardCard(id) {
      if (!CARD_IDS.includes(id)) return;
      const draft = this.dashboardLayoutDraft.filter(item => item !== id);
      if (draft.length === this.dashboardLayoutDraft.length) draft.push(id);
      this.dashboardLayoutDraft = draft;
    },

    moveDashboardCard(id, delta) {
      const draft = [...this.dashboardLayoutDraft];
      const index = draft.indexOf(id);
      const target = index + delta;
      if (index < 0 || target < 0 || target >= draft.length) return;
      [draft[index], draft[target]] = [draft[target], draft[index]];
      this.dashboardLayoutDraft = draft;
    },

    resetDashboardLayout() {
      this.dashboardLayoutDraft = [...CARD_IDS];
    },

    async saveDashboardLayout() {
      this.settings.dashboard_widgets = [...this.dashboardLayoutDraft];
      this.dashboardEditing = false;
      this.dashboardLayoutDraft = [];
      await this.saveSettings();
    },

    get dashboardStats() {
      const activeSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'active').length;
      const invalidSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'invalid').length;
      const completedSubs = this.subscriptions.filter(sub => this.subscriptionStatusKey(sub) === 'completed').length;
      const runningJobs = this.jobs.filter(job => ['queued', 'running'].includes(job.status)).length;
      const failedJobs = this.jobs.filter(job => job.status === 'failed').length;
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

    /// 仪表盘指标。异常项在计数不为 0 时才转为警示色，否则一屏都是红黄反而看不出重点。
    dashboardTiles() {
      const stats = this.dashboardStats;
      return [
        {id: 'active', label: '追更中', value: stats.activeSubs, hint: '持续追踪更新', tone: 'primary'},
        {id: 'completed', label: '已完结', value: stats.completedSubs, hint: '已收齐全集', tone: 'success'},
        {id: 'invalid', label: '失效订阅', value: stats.invalidSubs, hint: '需要换源', tone: stats.invalidSubs ? 'danger' : 'idle'},
        {id: 'jobs', label: '后台任务', value: stats.runningJobs, hint: '排队与运行中', tone: stats.runningJobs ? 'cyan' : 'idle'},
        {id: 'failed', label: '失败任务', value: stats.failedJobs, hint: '查看错误与重试', tone: stats.failedJobs ? 'danger' : 'idle'},
        {id: 'unread', label: '未读通知', value: stats.unreadNotifications, hint: '运行消息', tone: stats.unreadNotifications ? 'warning' : 'idle'},
        {id: 'download', label: '下载速度', value: this.formatSpeed(stats.downloadSpeed), hint: 'Aria2 实时速度', tone: stats.downloadSpeed ? 'violet' : 'idle'}
      ];
    },

    openDashboardTile(id) {
      if (id === 'active' || id === 'completed') {
        this.setSubscriptionStatusTab(id);
        this.selectTab('subscriptions');
      } else if (id === 'invalid') {
        this.openDashboardAttention('subscriptions');
      } else if (id === 'failed') {
        this.openDashboardAttention('jobs');
      } else if (id === 'unread') {
        this.openDashboardAttention('notifications');
      } else if (id === 'jobs') {
        this.selectTab('notifications');
      } else if (id === 'download') {
        this.selectTab('downloads');
      }
    },

    /// 订阅看板只看追更中的：已完结的不需要再盯，失效的由「失效订阅」指标标红并跳转。
    get dashboardWatchlist() {
      const checkedAt = sub => Number(sub.last_checked_at || sub.updated_at || 0);
      return this.subscriptions
        .filter(sub => this.subscriptionStatusKey(sub) === 'active')
        .sort((left, right) => checkedAt(right) - checkedAt(left))
        .slice(0, 14);
    },

    dashboardStatusSummary() {
      if (this.subscriptions.length === 0) {
        return '当前还没有订阅。可以先搜索资源并创建订阅。';
      }
      if (this.dashboardAttentionCount === 0) {
        return `共 ${this.subscriptions.length} 个订阅；当前没有失效订阅、失败任务或未读通知。`;
      }
      return `共 ${this.subscriptions.length} 个订阅，${this.dashboardAttentionCount} 项状态需要处理。`;
    },

    openDashboardAttention(kind) {
      if (kind === 'subscriptions') {
        this.setSubscriptionStatusTab('invalid');
        this.selectTab('subscriptions');
      } else if (kind === 'jobs') {
        this.backgroundJobFilterStatus = 'failed';
        this.activityFilter = 'failed';
        this.selectTab('notifications');
      } else if (kind === 'notifications') {
        this.notificationFilter = 'unread';
        this.activityFilter = 'unread';
        this.selectTab('notifications');
      }
    },

    dashboardDateLabel() {
      return new Intl.DateTimeFormat('zh-CN', {
        month: 'long',
        day: 'numeric',
        weekday: 'long'
      }).format(new Date());
    },

    get dashboardRecentSubscriptions() {
      return [...this.subscriptions]
        .sort((a, b) => Number(b.last_checked_at || b.updated_at || 0) - Number(a.last_checked_at || a.updated_at || 0))
        .slice(0, 9);
    },

    get dashboardRecentActivity() {
      return (this.activityItems || []).slice(0, 6);
    },

    openDashboardActivity(item) {
      if (!item) return;
      this.selectTab('notifications');
      if (item.source === 'job' && typeof this.openJobDetail === 'function') {
        this.openJobDetail(item.raw);
      }
    },

    };
  }

  return {createStore};
});
