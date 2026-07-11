use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::Settings;

use super::model::{
    Job, JobKind, JobPriority, MetadataScrapePayload, PushDispatchPayload,
    SubscriptionTransferPayload,
};

// 高/普通/低按 3:2:1 获得调度机会。游标跨调用保留，因此即使高优先级任务持续
// 到达，普通和低优先级队列也会稳定获得执行机会。
const PRIORITY_SLOTS: [JobPriority; 6] = [
    JobPriority::High,
    JobPriority::Normal,
    JobPriority::High,
    JobPriority::Low,
    JobPriority::High,
    JobPriority::Normal,
];

#[derive(Debug, Default)]
pub(crate) struct FairScheduler {
    high: FairPriorityQueue,
    normal: FairPriorityQueue,
    low: FairPriorityQueue,
    pending_ids: HashSet<String>,
    priority_cursor: usize,
}

impl FairScheduler {
    pub(crate) fn push(&mut self, job: Job) {
        // 优先级调整会再次发送同一任务 ID。先移除旧快照，再以最新优先级入队。
        if self.pending_ids.contains(&job.id) {
            self.remove(&job.id);
        }
        self.pending_ids.insert(job.id.clone());
        self.queue_mut(job.priority).push(job);
    }

    pub(crate) fn pop_next(&mut self, mut eligible: impl FnMut(&Job) -> bool) -> Option<Job> {
        for _ in 0..PRIORITY_SLOTS.len() {
            let priority = PRIORITY_SLOTS[self.priority_cursor];
            self.priority_cursor = (self.priority_cursor + 1) % PRIORITY_SLOTS.len();
            if let Some(job) = self.queue_mut(priority).pop_next(&mut eligible) {
                self.pending_ids.remove(&job.id);
                return Some(job);
            }
        }
        None
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending_ids.is_empty()
    }

    fn remove(&mut self, id: &str) {
        self.high.remove(id);
        self.normal.remove(id);
        self.low.remove(id);
        self.pending_ids.remove(id);
    }

    fn queue_mut(&mut self, priority: JobPriority) -> &mut FairPriorityQueue {
        match priority {
            JobPriority::High => &mut self.high,
            JobPriority::Normal => &mut self.normal,
            JobPriority::Low => &mut self.low,
        }
    }
}

#[derive(Debug, Default)]
struct FairPriorityQueue {
    groups: VecDeque<FairGroup>,
}

impl FairPriorityQueue {
    fn push(&mut self, job: Job) {
        let key = fairness_key(&job);
        if let Some(group) = self.groups.iter_mut().find(|group| group.key == key) {
            group.jobs.push_back(job);
            return;
        }
        self.groups.push_back(FairGroup {
            key,
            jobs: VecDeque::from([job]),
        });
    }

    fn pop_next(&mut self, eligible: &mut impl FnMut(&Job) -> bool) -> Option<Job> {
        let group_count = self.groups.len();
        for _ in 0..group_count {
            let mut group = self.groups.pop_front()?;
            let can_run = group.jobs.front().is_some_and(&mut *eligible);
            if can_run {
                let job = group.jobs.pop_front();
                if !group.jobs.is_empty() {
                    self.groups.push_back(group);
                }
                return job;
            }
            self.groups.push_back(group);
        }
        None
    }

    fn remove(&mut self, id: &str) {
        for group in &mut self.groups {
            group.jobs.retain(|job| job.id != id);
        }
        self.groups.retain(|group| !group.jobs.is_empty());
    }
}

#[derive(Debug)]
struct FairGroup {
    key: String,
    jobs: VecDeque<Job>,
}

fn fairness_key(job: &Job) -> String {
    job_resource(job)
        .subscription_id
        .map(|id| format!("subscription:{id}"))
        .unwrap_or_else(|| format!("kind:{:?}", job.kind))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum JobClass {
    Transfer,
    Metadata,
    Push,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobResource {
    pub(crate) class: JobClass,
    subscription_id: Option<String>,
    all_subscriptions: bool,
}

pub(crate) fn job_resource(job: &Job) -> JobResource {
    match job.kind {
        JobKind::ManualTransfer => JobResource {
            class: JobClass::Transfer,
            subscription_id: None,
            all_subscriptions: false,
        },
        JobKind::SubscriptionTransfer => JobResource {
            class: JobClass::Transfer,
            subscription_id: serde_json::from_value::<SubscriptionTransferPayload>(
                job.payload.clone(),
            )
            .ok()
            .map(|payload| payload.subscription_id),
            all_subscriptions: false,
        },
        JobKind::MetadataScrape => {
            let subscription_id =
                serde_json::from_value::<MetadataScrapePayload>(job.payload.clone())
                    .ok()
                    .and_then(|payload| payload.subscription_id);
            JobResource {
                class: JobClass::Metadata,
                all_subscriptions: subscription_id.is_none(),
                subscription_id,
            }
        }
        JobKind::PushDispatch => JobResource {
            class: JobClass::Push,
            subscription_id: serde_json::from_value::<PushDispatchPayload>(job.payload.clone())
                .ok()
                .and_then(|payload| payload.subscription_id),
            all_subscriptions: false,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobConcurrencyLimits {
    global: usize,
    transfer: usize,
    metadata: usize,
    push: usize,
}

impl JobConcurrencyLimits {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        let global = settings.job_max_concurrency.max(1);
        Self {
            global,
            transfer: settings.job_transfer_max_concurrency.clamp(1, global),
            metadata: settings.job_metadata_max_concurrency.clamp(1, global),
            push: settings.job_push_max_concurrency.clamp(1, global),
        }
    }

    #[cfg(test)]
    fn new(global: usize, transfer: usize, metadata: usize, push: usize) -> Self {
        Self {
            global,
            transfer,
            metadata,
            push,
        }
    }

    fn class_limit(self, class: JobClass) -> usize {
        match class {
            JobClass::Transfer => self.transfer,
            JobClass::Metadata => self.metadata,
            JobClass::Push => self.push,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RunningJobs {
    total: usize,
    by_class: HashMap<JobClass, usize>,
    subscriptions: HashMap<String, usize>,
    all_subscriptions: usize,
}

impl RunningJobs {
    pub(crate) fn can_start(&self, job: &Job, limits: JobConcurrencyLimits) -> bool {
        let resource = job_resource(job);
        if self.total >= limits.global
            || self.by_class.get(&resource.class).copied().unwrap_or(0)
                >= limits.class_limit(resource.class)
        {
            return false;
        }

        if resource.all_subscriptions {
            return self.all_subscriptions == 0 && self.subscriptions.is_empty();
        }
        if self.all_subscriptions > 0 {
            return false;
        }
        resource
            .subscription_id
            .as_ref()
            .is_none_or(|id| !self.subscriptions.contains_key(id))
    }

    pub(crate) fn start(&mut self, job: &Job) {
        let resource = job_resource(job);
        self.total += 1;
        *self.by_class.entry(resource.class).or_default() += 1;
        if resource.all_subscriptions {
            self.all_subscriptions += 1;
        }
        if let Some(id) = resource.subscription_id {
            *self.subscriptions.entry(id).or_default() += 1;
        }
    }

    pub(crate) fn finish(&mut self, job: &Job) {
        let resource = job_resource(job);
        self.total = self.total.saturating_sub(1);
        decrement(&mut self.by_class, &resource.class);
        if resource.all_subscriptions {
            self.all_subscriptions = self.all_subscriptions.saturating_sub(1);
        }
        if let Some(id) = resource.subscription_id {
            decrement(&mut self.subscriptions, &id);
        }
    }
}

fn decrement<K: Eq + std::hash::Hash>(counts: &mut HashMap<K, usize>, key: &K) {
    if let Some(count) = counts.get_mut(key) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::jobs::model::JobStatus;

    fn job(id: &str, kind: JobKind, priority: JobPriority, payload: serde_json::Value) -> Job {
        Job {
            id: id.to_string(),
            kind,
            priority,
            attempt: 1,
            next_attempt_at: None,
            error_class: None,
            status: JobStatus::Queued,
            progress: 0,
            title: id.to_string(),
            message: "queued".to_string(),
            payload,
            idempotency_key: None,
            result: None,
            error: None,
            created_at: 1,
            updated_at: 1,
            started_at: None,
            finished_at: None,
        }
    }

    fn subscription_job(id: &str, subscription_id: &str, priority: JobPriority) -> Job {
        job(
            id,
            JobKind::SubscriptionTransfer,
            priority,
            json!({"subscription_id": subscription_id, "file_names": []}),
        )
    }

    #[test]
    fn weighted_priority_schedule_does_not_starve_low_priority() {
        let mut scheduler = FairScheduler::default();
        for id in ["h1", "h2", "h3", "h4"] {
            scheduler.push(subscription_job(id, id, JobPriority::High));
        }
        scheduler.push(subscription_job("normal", "normal", JobPriority::Normal));
        scheduler.push(subscription_job("low", "low", JobPriority::Low));

        let order = (0..6)
            .map(|_| scheduler.pop_next(|_| true).unwrap().id)
            .collect::<Vec<_>>();

        assert_eq!(order, ["h1", "normal", "h2", "low", "h3", "h4"]);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn equal_priority_round_robins_between_subscriptions() {
        let mut scheduler = FairScheduler::default();
        scheduler.push(subscription_job("a1", "a", JobPriority::Normal));
        scheduler.push(subscription_job("a2", "a", JobPriority::Normal));
        scheduler.push(subscription_job("b1", "b", JobPriority::Normal));

        let order = (0..3)
            .map(|_| scheduler.pop_next(|_| true).unwrap().id)
            .collect::<Vec<_>>();

        assert_eq!(order, ["a1", "b1", "a2"]);
    }

    #[test]
    fn repeated_push_replaces_pending_priority_snapshot() {
        let mut scheduler = FairScheduler::default();
        let mut queued = subscription_job("same", "a", JobPriority::Low);
        scheduler.push(queued.clone());
        queued.priority = JobPriority::High;
        scheduler.push(queued);

        let selected = scheduler.pop_next(|_| true).unwrap();
        assert_eq!(selected.priority, JobPriority::High);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn layered_limits_cover_global_class_and_subscription() {
        let limits = JobConcurrencyLimits::new(3, 1, 2, 2);
        let first = subscription_job("a1", "a", JobPriority::Normal);
        let same_subscription = subscription_job("a2", "a", JobPriority::Normal);
        let other_transfer = subscription_job("c1", "c", JobPriority::Normal);
        let metadata = job(
            "metadata",
            JobKind::MetadataScrape,
            JobPriority::Low,
            json!({"subscription_id": "b"}),
        );
        let push = job(
            "push",
            JobKind::PushDispatch,
            JobPriority::High,
            json!({"event":"x","title":"x","message":"x","level":"info","subscription_id":"d"}),
        );
        let mut running = RunningJobs::default();

        assert!(running.can_start(&first, limits));
        running.start(&first);
        assert!(!running.can_start(&same_subscription, limits));
        assert!(!running.can_start(&other_transfer, limits));
        assert!(running.can_start(&metadata, limits));
        running.start(&metadata);
        assert!(running.can_start(&push, limits));
        running.start(&push);
        assert!(!running.can_start(
            &job(
                "extra",
                JobKind::PushDispatch,
                JobPriority::High,
                json!({"event":"x","title":"x","message":"x","level":"info"}),
            ),
            limits
        ));
        running.finish(&first);
        assert!(running.can_start(&other_transfer, limits));
    }

    #[test]
    fn bulk_metadata_is_exclusive_with_subscription_scoped_jobs() {
        let limits = JobConcurrencyLimits::new(4, 2, 2, 2);
        let bulk = job(
            "bulk",
            JobKind::MetadataScrape,
            JobPriority::Low,
            json!({"subscription_id": null}),
        );
        let transfer = subscription_job("transfer", "a", JobPriority::Normal);
        let mut running = RunningJobs::default();

        running.start(&transfer);
        assert!(!running.can_start(&bulk, limits));
        running.finish(&transfer);
        assert!(running.can_start(&bulk, limits));
        running.start(&bulk);
        assert!(!running.can_start(&transfer, limits));
    }
}
