(function (root, factory) {
  const moduleApi = factory(root);
  if (typeof module === 'object' && module.exports) module.exports = moduleApi;
  root.MediaSubCalendarPage = moduleApi;
})(typeof globalThis !== 'undefined' ? globalThis : window, function (root) {
  'use strict';

  const api = root.MediaSubApi || {};
  const {apiData} = api;
  const calendarTools = root.MediaSubCalendar || {};

  /// 周视图是 7 列 × 145px 的网格，手机上只能横向滚动，默认改用列表视图。
  function defaultCalendarView(scope) {
    const width = scope && Number(scope.innerWidth);
    return width > 0 && width < 768 ? 'list' : 'week';
  }

  function createStore() {
    return {
    calendar: null,
    calendarLoading: false,
    calendarError: '',
    calendarView: defaultCalendarView(root),
    calendarCursor: '',
    calendarStatusFilter: 'all',
    calendarMediaFilter: 'all',
    /// 请求序号：快速切换视图/翻页时丢弃乱序返回的过期响应。
    calendarRequestId: 0,
    calendarStatusOptions: [
      {id: 'all', name: '全部状态'},
      {id: 'today', name: '今日更新'},
      {id: 'this_week', name: '本周待更新'},
      {id: 'aired_undiscovered', name: '已播未发现'},
      {id: 'discovered_pending_transfer', name: '已发现待转存'},
      {id: 'transferred_pending_download', name: '已转存待下载'},
      {id: 'completed_missing', name: '完结缺集'},
      {id: 'ready', name: '已就绪'},
      {id: 'unknown_schedule', name: '排期未知'}
    ],

    // 搜索
    get calendarItems() {
      return (this.calendar && this.calendar.items) || [];
    },

    get calendarSummary() {
      return (this.calendar && this.calendar.summary) || {total: 0, subscriptions: 0, source_alerts: 0, by_status: {}, by_media_type: {}};
    },

    get calendarSourceAlerts() {
      return (this.calendar && Array.isArray(this.calendar.source_alerts)) ? this.calendar.source_alerts : [];
    },

    get calendarRange() {
      return calendarTools.viewRange(this.calendarView, this.calendarCursor || this.calendarTodayKey());
    },

    get calendarMonthCells() {
      return calendarTools.monthCells(
        this.calendarCursor || this.calendarTodayKey(),
        this.calendarItems,
        (this.calendar && this.calendar.today) || this.calendarTodayKey()
      );
    },

    get calendarWeekDays() {
      return calendarTools.weekDays(
        this.calendarCursor || this.calendarTodayKey(),
        this.calendarItems,
        (this.calendar && this.calendar.today) || this.calendarTodayKey()
      );
    },

    get calendarListGroups() {
      return calendarTools.listGroups(this.calendarItems);
    },

    calendarTodayKey() {
      const parts = new Intl.DateTimeFormat('en', {
        timeZone: 'Asia/Shanghai', year: 'numeric', month: '2-digit', day: '2-digit'
      }).formatToParts(new Date()).reduce((result, part) => {
        if (part.type !== 'literal') result[part.type] = part.value;
        return result;
      }, {});
      return `${parts.year}-${parts.month}-${parts.day}`;
    },

    calendarRangeLabel() {
      const range = this.calendarRange;
      return calendarTools.rangeLabel(this.calendarView, this.calendarCursor, range.from, range.to);
    },

    calendarDateLabel(key, compact = false) {
      if (!key || key === 'unknown') return '排期未知';
      const date = calendarTools.parseDate(key);
      if (!date) return key;
      return new Intl.DateTimeFormat('zh-CN', compact
        ? {timeZone: 'UTC', month: 'numeric', day: 'numeric', weekday: 'short'}
        : {timeZone: 'UTC', year: 'numeric', month: 'long', day: 'numeric', weekday: 'short'}
      ).format(date);
    },

    calendarStatusLabel(status) {
      return calendarTools.statusLabel(status);
    },

    calendarSourceLabel(source) {
      return calendarTools.sourceLabel(source);
    },

    calendarConfidenceLabel(confidence) {
      return calendarTools.confidenceLabel(confidence);
    },

    calendarTransferProgressLabel(item, compact = false) {
      return calendarTools.transferProgressLabel(item, compact);
    },

    calendarCardDayLabel(item) {
      return calendarTools.cardDayLabel(item);
    },

    calendarCardSavedLabel(item) {
      return calendarTools.cardSavedLabel(item);
    },

    calendarSourceAlertLabel(alert) {
      return calendarTools.sourceAlertLabel(alert);
    },

    calendarItemClass(item) {
      const sourceAlert = item && item.source_change_recommended ? ' has-source-alert' : '';
      return `calendar-item is-${(item && item.primary_status) || 'scheduled'}${sourceAlert}`;
    },

    async openCalendarSourceSwitch(alert) {
      if (!alert || !alert.subscription_id) return;
      const subscription = (this.subscriptions || []).find(item => item.id === alert.subscription_id) || {
        id: alert.subscription_id,
        title: alert.subscription_title,
        media_type: alert.media_type,
        source_candidates: []
      };
      await this.openSourceSwitchDialog(subscription);
    },

    setCalendarView(view) {
      if (!['week', 'month', 'list'].includes(view) || this.calendarView === view) return;
      this.calendarView = view;
      this.loadCalendar();
    },

    shiftCalendar(direction) {
      this.calendarCursor = calendarTools.shiftCursor(this.calendarCursor, this.calendarView, direction);
      this.loadCalendar();
    },

    resetCalendarToday() {
      this.calendarCursor = this.calendarTodayKey();
      this.loadCalendar();
    },

    async loadCalendar() {
      const requestId = ++this.calendarRequestId;
      this.calendarLoading = true;
      this.calendarError = '';
      const range = this.calendarRange;
      const params = new URLSearchParams({from: range.from, to: range.to});
      if (this.calendarStatusFilter !== 'all') params.set('status', this.calendarStatusFilter);
      if (this.calendarMediaFilter !== 'all') params.set('media_type', this.calendarMediaFilter);
      try {
        const data = await apiData(`/api/calendar?${params.toString()}`, {cache: 'no-store'});
        if (requestId !== this.calendarRequestId) return;
        this.calendar = data;
      } catch (error) {
        if (requestId !== this.calendarRequestId) return;
        console.error('加载更新日历失败:', error);
        this.calendarError = this.apiErrorMessage(error, '更新日历加载失败');
      } finally {
        if (requestId === this.calendarRequestId) this.calendarLoading = false;
      }
    },

    async runCalendarAction(item, repair = false) {
      if (!item || !item.subscription_id) return;
      await this.checkSubscription(item.subscription_id, {forceTransfer: repair});
      await this.loadCalendar();
    },

    };
  }

  return {createStore};
});
