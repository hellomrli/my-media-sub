//! 进程内定时器：固定间隔，以及「每天某个整点」两种触发方式。
//!
//! 取代 `tokio-cron-scheduler`。换掉它有两个原因：
//!
//! 1. **重复任务在进程被冻结后永久停摆。** 该 crate 用墙上时钟记录 `next_tick`，
//!    重复任务触发后把它推进一个间隔（`next_tick += 间隔`），并要求
//!    `last_tick <= next_tick` 才允许下一次触发。进程被冻结超过一个间隔之后
//!    （主机休眠、`docker pause`、虚拟机挂起，或没有 RTC 的设备开机后 NTP 把时钟
//!    向前拨），补跑那一次写入的 `last_tick` 已经晚于新的 `next_tick`，条件从此
//!    不再成立：订阅自动检查只补跑一次就再也不触发，诊断页却仍显示调度器在运行，
//!    只有重启或重新保存设置才能恢复。用 1 秒间隔冻结 3.5 秒即可复现。
//! 2. 它是 `chrono` / `chrono-tz` 进入依赖树的主要路径，两者已被 RustSec 标记为
//!    停止维护（RUSTSEC-2026-0543 / RUSTSEC-2026-0555）。
//!
//! 固定间隔定时器同时参考单调时钟与墙上时钟：
//!
//! - 单调时钟保证墙上时钟被向后拨时不会漏跑，也不会重复触发；
//! - 墙上时钟保证休眠醒来或时钟前跳之后能补跑。Linux 的 `CLOCK_MONOTONIC` 在
//!   系统休眠期间不走，只看单调时钟会把休眠的时长整段漏掉。
//!
//! 每日定时器遵守墙上时钟的当地整点；回拨后重新计算下一次执行时间，同一天
//! 已触发过则跳过重复触发。跨过多个执行时刻时只补跑一次。
//!
//! 为了及时发现墙上时钟的变化，单次睡眠不超过 [`MAX_SLEEP`]。
//!
//! 每次触发都把任务放进独立的 tokio 任务：一次执行很慢不会推迟后面的触发，
//! 停止定时器也不会打断已经开始的那一次（与被取代的实现一致）。是否允许重叠
//! 执行由调用方决定，例如订阅检查用 `try_lock` 跳过仍在进行的上一轮。

use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{error, info};

/// 单次睡眠的上限，也是休眠醒来或时钟跳变后最晚多久能被发现。
const MAX_SLEEP: Duration = Duration::from_secs(30);

const SECONDS_PER_DAY: i64 = 86_400;

/// 墙上时钟的来源。生产环境是 `SystemTime::now`，测试注入可以跳变的时钟。
type WallClock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

fn system_clock() -> WallClock {
    Arc::new(SystemTime::now)
}

/// 定时器句柄。`stop()` 或 drop 之后不再触发；已经开始的那次执行不受影响。
pub struct Timer {
    task: Option<JoinHandle<()>>,
}

impl Timer {
    /// 每隔 `interval` 触发一次，第一次在一个完整间隔之后。
    ///
    /// 进程被冻结或墙上时钟前跳超过一个间隔时补跑一次，从补跑时重新计算间隔。
    /// `interval` 必须大于零。
    pub fn every<F, Fut>(label: &'static str, interval: Duration, run: F) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self::every_with_clock(label, interval, system_clock(), run)
    }

    /// 每天在 `utc_offset_seconds` 时区的 `hour:00:00` 触发一次。
    ///
    /// 时区用固定偏移表示，不处理夏令时；目前唯一的调用方是北京时间的夸克签到。
    /// 到点时进程处于冻结状态，则在恢复后补跑。
    pub fn daily_at<F, Fut>(label: &'static str, hour: u8, utc_offset_seconds: i32, run: F) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self::daily_at_with_clock(label, hour, utc_offset_seconds, system_clock(), run)
    }

    fn every_with_clock<F, Fut>(
        label: &'static str,
        interval: Duration,
        clock: WallClock,
        mut run: F,
    ) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        assert!(
            !interval.is_zero(),
            "timer interval must be greater than zero"
        );
        // 在创建句柄时计时；执行器稍后才运行该任务也不能丢掉已过去的时间。
        let mut last_monotonic = Instant::now();
        let mut last_wall = clock();
        let task = tokio::spawn(async move {
            loop {
                let monotonic_elapsed = last_monotonic.elapsed();
                // 墙上时钟被向后拨时 duration_since 出错，按 0 处理，由单调时钟决定。
                let wall_elapsed = clock().duration_since(last_wall).unwrap_or_default();
                let elapsed = monotonic_elapsed.max(wall_elapsed);
                if elapsed < interval {
                    tokio::time::sleep((interval - elapsed).min(MAX_SLEEP)).await;
                    continue;
                }
                if monotonic_elapsed < interval {
                    info!(
                        "{}：系统时间比预期提前到点（休眠、暂停或时钟调整），立即补跑一次",
                        label
                    );
                }
                last_monotonic = Instant::now();
                last_wall = clock();
                spawn_run(label, &mut run);
            }
        });
        Self { task: Some(task) }
    }

    fn daily_at_with_clock<F, Fut>(
        label: &'static str,
        hour: u8,
        utc_offset_seconds: i32,
        clock: WallClock,
        mut run: F,
    ) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut target = next_daily_occurrence(unix_seconds(clock()), hour, utc_offset_seconds);
        let task = tokio::spawn(async move {
            let mut last_run_day = None;
            loop {
                let now = since_epoch(clock());
                let now_seconds = now.as_secs() as i64;
                if now_seconds >= target {
                    target = next_daily_occurrence(now_seconds, hour, utc_offset_seconds);
                    let day =
                        (now_seconds + i64::from(utc_offset_seconds)).div_euclid(SECONDS_PER_DAY);
                    if last_run_day != Some(day) {
                        last_run_day = Some(day);
                        spawn_run(label, &mut run);
                    }
                    continue;
                }
                // 下一次触发最多在 24 小时之后。差得更远说明时钟被向后拨过，
                // 按新的时间重新计算，否则会多等一天。
                if target - now_seconds > SECONDS_PER_DAY {
                    info!("{}：系统时间被向后调整，重新计算下一次触发时间", label);
                    target = next_daily_occurrence(now_seconds, hour, utc_offset_seconds);
                    continue;
                }
                let remaining = Duration::from_secs(target as u64).saturating_sub(now);
                tokio::time::sleep(remaining.min(MAX_SLEEP)).await;
            }
        });
        Self { task: Some(task) }
    }

    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|task| !task.is_finished())
    }

    /// 停止并等待调度循环退出，保证返回后不会再创建执行任务。
    pub async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// 创建 future 的同步 panic 和执行 future 的异步 panic 都不能杀死调度循环。
/// 执行和观察任务独立于定时器句柄，停止调度时已经开始的一轮仍会结束。
fn spawn_run<F, Fut>(label: &'static str, run: &mut F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let future = match catch_unwind(AssertUnwindSafe(run)) {
        Ok(future) => future,
        Err(_) => {
            error!("{}：创建执行任务时发生 panic，下次到点继续尝试", label);
            return;
        }
    };
    let task = tokio::spawn(future);
    tokio::spawn(async move {
        if let Err(err) = task.await {
            error!("{}：执行任务异常退出，下次到点继续尝试: {}", label, err);
        }
    });
}

fn since_epoch(time: SystemTime) -> Duration {
    time.duration_since(UNIX_EPOCH).unwrap_or_default()
}

fn unix_seconds(time: SystemTime) -> i64 {
    since_epoch(time).as_secs() as i64
}

/// `now`（Unix 秒）之后，下一个在给定固定时区恰好为 `hour:00:00` 的时刻（Unix 秒）。
///
/// 结果严格晚于 `now`：正好处在整点时返回第二天的同一时刻。
pub(crate) fn next_daily_occurrence(now: i64, hour: u8, utc_offset_seconds: i32) -> i64 {
    let offset = i64::from(utc_offset_seconds);
    let local = now + offset;
    let day_start = local - local.rem_euclid(SECONDS_PER_DAY);
    let mut candidate = day_start + i64::from(hour.min(23)) * 3_600;
    if candidate <= local {
        candidate += SECONDS_PER_DAY;
    }
    candidate - offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    const BEIJING: i32 = 8 * 3_600;

    /// 2026-06-29T00:00:00+08:00。
    const BEIJING_MIDNIGHT: i64 = 1_782_662_400;

    /// 测试用的墙上时钟：平时跟随 tokio 的（可暂停的）单调时钟，可以额外前后拨动，
    /// 用来模拟休眠醒来与时钟调整。
    #[derive(Clone)]
    struct FakeWall {
        started: Instant,
        base_seconds: i64,
        jump_millis: Arc<AtomicI64>,
    }

    impl FakeWall {
        fn starting_at(base_seconds: i64) -> Self {
            Self {
                started: Instant::now(),
                base_seconds,
                jump_millis: Arc::new(AtomicI64::new(0)),
            }
        }

        fn jump(&self, seconds: i64) {
            self.jump_millis
                .fetch_add(seconds * 1_000, Ordering::SeqCst);
        }

        fn clock(&self) -> WallClock {
            let wall = self.clone();
            Arc::new(move || {
                let millis = wall.base_seconds * 1_000
                    + wall.started.elapsed().as_millis() as i64
                    + wall.jump_millis.load(Ordering::SeqCst);
                UNIX_EPOCH + Duration::from_millis(millis.max(0) as u64)
            })
        }
    }

    fn counter() -> (
        Arc<AtomicUsize>,
        impl FnMut() -> std::future::Ready<()> + Send + 'static,
    ) {
        let count = Arc::new(AtomicUsize::new(0));
        let handle = count.clone();
        (count, move || {
            handle.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        })
    }

    /// 让出执行权，直到定时器任务与它派生的执行任务都跑完当前这一步。
    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
    }

    async fn advance(seconds: u64) {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        settle().await;
    }

    #[test]
    fn next_daily_occurrence_follows_the_fixed_offset() {
        let eight = BEIJING_MIDNIGHT + 8 * 3_600;
        // 当天还没到点：今天。
        assert_eq!(next_daily_occurrence(BEIJING_MIDNIGHT, 8, BEIJING), eight);
        assert_eq!(next_daily_occurrence(eight - 1, 8, BEIJING), eight);
        // 正好到点或已过：明天。结果必须严格晚于 now，否则会在同一秒内反复触发。
        assert_eq!(
            next_daily_occurrence(eight, 8, BEIJING),
            eight + SECONDS_PER_DAY
        );
        assert_eq!(
            next_daily_occurrence(eight + 1, 8, BEIJING),
            eight + SECONDS_PER_DAY
        );
        // 0 点与 23 点：一天的两端。
        assert_eq!(
            next_daily_occurrence(BEIJING_MIDNIGHT, 0, BEIJING),
            BEIJING_MIDNIGHT + SECONDS_PER_DAY
        );
        assert_eq!(
            next_daily_occurrence(BEIJING_MIDNIGHT - 1, 0, BEIJING),
            BEIJING_MIDNIGHT
        );
        assert_eq!(
            next_daily_occurrence(BEIJING_MIDNIGHT, 23, BEIJING),
            BEIJING_MIDNIGHT + 23 * 3_600
        );
        // 超出范围的小时按 23 点处理，不会算出一天之外的时刻。
        assert_eq!(
            next_daily_occurrence(BEIJING_MIDNIGHT, 99, BEIJING),
            BEIJING_MIDNIGHT + 23 * 3_600
        );
    }

    #[test]
    fn next_daily_occurrence_handles_other_offsets() {
        // UTC：1970-01-02T00:00:00Z 之后的 08:00 就是当天 08:00。
        assert_eq!(
            next_daily_occurrence(SECONDS_PER_DAY, 8, 0),
            SECONDS_PER_DAY + 8 * 3_600
        );
        // 负偏移（UTC-5）与纪元之前的时间都按欧几里得余数取整到当地零点。
        let offset = -5 * 3_600;
        let next = next_daily_occurrence(0, 8, offset);
        assert_eq!(
            (next + i64::from(offset)).rem_euclid(SECONDS_PER_DAY),
            8 * 3_600
        );
        assert!(next > 0 && next <= SECONDS_PER_DAY);
        let before_epoch = next_daily_occurrence(-10, 0, 0);
        assert_eq!(before_epoch, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn interval_timer_waits_one_full_interval_then_repeats() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let _timer = Timer::every_with_clock("测试", Duration::from_secs(300), wall.clock(), run);

        advance(299).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "第一次触发在一个完整间隔之后"
        );
        advance(1).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        advance(900).await;
        assert_eq!(count.load(Ordering::SeqCst), 4, "之后每个间隔触发一次");
    }

    /// 被取代的实现正是在这里停摆：冻结超过一个间隔后只补跑一次，之后再不触发。
    #[tokio::test(start_paused = true)]
    async fn interval_timer_catches_up_once_after_a_freeze_and_keeps_running() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let _timer = Timer::every_with_clock("测试", Duration::from_secs(3_600), wall.clock(), run);

        advance(3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // 主机休眠 3 小时：墙上时钟前进，单调时钟没有走。
        wall.jump(3 * 3_600);
        advance(MAX_SLEEP.as_secs()).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "醒来后在一次轮询内补跑，而且只补一次"
        );

        advance(3 * 3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 5, "补跑之后按原间隔继续触发");
    }

    #[tokio::test(start_paused = true)]
    async fn missed_intervals_do_not_burst_after_executor_resume() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let _timer = Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), run);
        settle().await;

        // 一次推进整个运行时的时钟，定时器没有机会在中间的各周期执行。
        tokio::time::advance(Duration::from_secs(210)).await;
        settle().await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "错过三个周期也只补一次");
        advance(59).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        advance(1).await;
        assert_eq!(count.load(Ordering::SeqCst), 2, "补跑后从新间隔继续");
    }

    #[tokio::test(start_paused = true)]
    async fn interval_starts_counting_when_the_handle_is_created() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let _timer = Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), run);
        wall.jump(180);
        settle().await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "首次调度前的暂停也应补跑");
        advance(60).await;
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "timer interval must be greater than zero")]
    async fn zero_interval_is_rejected_without_starting_a_busy_loop() {
        Timer::every("测试", Duration::ZERO, || async {});
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_factory_does_not_stop_future_triggers() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let count = Arc::new(AtomicUsize::new(0));
        let handle = count.clone();
        let timer =
            Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), move || {
                if handle.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("test factory panic");
                }
                std::future::ready(())
            });
        advance(180).await;
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert!(timer.is_running());
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_run_does_not_stop_future_triggers() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let count = Arc::new(AtomicUsize::new(0));
        let handle = count.clone();
        let timer =
            Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), move || {
                let handle = handle.clone();
                async move {
                    handle.fetch_add(1, Ordering::SeqCst);
                    panic!("test run panic");
                }
            });
        advance(180).await;
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert!(timer.is_running());
    }

    #[tokio::test(start_paused = true)]
    async fn interval_timer_ignores_a_backward_clock_step() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let _timer = Timer::every_with_clock("测试", Duration::from_secs(600), wall.clock(), run);

        advance(300).await;
        wall.jump(-86_400);
        advance(299).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        advance(1).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "时钟被向后拨不能推迟触发");
        advance(600).await;
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_or_dropped_timer_does_not_fire_again() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let (count, run) = counter();
        let timer = Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), run);
        advance(60).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        timer.stop().await;
        advance(600).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);

        let (count, run) = counter();
        let timer = Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), run);
        drop(timer);
        advance(600).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_the_timer_does_not_cancel_a_run_in_progress() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let finished = Arc::new(AtomicUsize::new(0));
        let handle = finished.clone();
        let timer =
            Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), move || {
                let handle = handle.clone();
                async move {
                    tokio::time::sleep(Duration::from_secs(120)).await;
                    handle.fetch_add(1, Ordering::SeqCst);
                }
            });
        advance(60).await;
        timer.stop().await;
        advance(120).await;
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_runs_do_not_delay_later_triggers() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT);
        let started = Arc::new(AtomicUsize::new(0));
        let handle = started.clone();
        let _timer =
            Timer::every_with_clock("测试", Duration::from_secs(60), wall.clock(), move || {
                let handle = handle.clone();
                async move {
                    handle.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_secs(1_000)).await;
                }
            });
        advance(180).await;
        assert_eq!(started.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn daily_timer_fires_at_the_configured_hour_every_day() {
        // 北京时间 07:59:30 启动，8 点签到。
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + 8 * 3_600 - 30);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);

        advance(29).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        advance(1).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "到整点触发");
        advance(3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "同一天不重复触发");
        advance(23 * 3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 2, "第二天同一时刻再次触发");
    }

    #[tokio::test(start_paused = true)]
    async fn daily_target_is_not_lost_before_the_first_poll() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + 8 * 3_600 - 1);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);
        wall.jump(2);
        settle().await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "首次调度前跨过整点也要补跑"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn daily_catchup_after_several_days_only_runs_once_on_the_resume_day() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + 7 * 3_600);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);
        settle().await;
        wall.jump(3 * SECONDS_PER_DAY);
        advance(MAX_SLEEP.as_secs()).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "错过多天也只补一次");
        advance(3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "补跑当天的整点不重复");
        advance(SECONDS_PER_DAY as u64).await;
        assert_eq!(count.load(Ordering::SeqCst), 2, "第二天恢复正常整点执行");
    }

    #[tokio::test(start_paused = true)]
    async fn a_backward_clock_step_does_not_repeat_a_daily_run() {
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + 8 * 3_600 - 30);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);
        advance(30).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        wall.jump(-2 * 3_600);
        advance(2 * 3_600).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "重新跨过同一天整点不能重复执行"
        );
        advance(SECONDS_PER_DAY as u64).await;
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn daily_timer_catches_up_when_the_hour_passed_during_a_freeze() {
        // 北京时间 07:00 启动，随后主机休眠到 09:30。
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + 7 * 3_600);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);

        advance(60).await;
        wall.jump(2 * 3_600 + 29 * 60);
        advance(MAX_SLEEP.as_secs()).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "醒来后补跑错过的那一次");
        advance(12 * 3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "补跑之后当天不再触发");
        advance(12 * 3_600).await;
        assert_eq!(count.load(Ordering::SeqCst), 2, "第二天 8 点照常触发");
    }

    #[tokio::test(start_paused = true)]
    async fn daily_timer_recomputes_after_the_clock_is_set_back() {
        // 时钟错误地快了一天：以为是「明天」09:00，下一次触发被算到「后天」08:00。
        let wall = FakeWall::starting_at(BEIJING_MIDNIGHT + SECONDS_PER_DAY + 9 * 3_600);
        let (count, run) = counter();
        let _timer = Timer::daily_at_with_clock("测试", 8, BEIJING, wall.clock(), run);
        advance(60).await;

        // 校时后回到真实时间：今天 07:01。今天 8 点的签到不能被跳过。
        wall.jump(-SECONDS_PER_DAY - 2 * 3_600);
        advance(3_600).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "时钟回拨后按新的时间在今天 8 点触发"
        );
    }
}
