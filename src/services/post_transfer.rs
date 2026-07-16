use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;

use crate::models::{Settings, Subscription};
use crate::providers::DriveItem;

/// 转存完成后的只读上下文。模块获得自己的 Arc/Vec 快照，不能直接修改
/// SubscriptionStore，从类型边界上避免插件破坏核心转存事务。
#[derive(Debug, Clone)]
pub struct PostTransferContext {
    pub settings: Arc<Settings>,
    pub subscription: Arc<Subscription>,
    pub target_dir: String,
    pub files: Arc<Vec<DriveItem>>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PostTransferOutcome {
    pub module: &'static str,
    pub status: PostTransferStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PostTransferStatus {
    Succeeded,
    Failed,
    Skipped,
}

pub type PostTransferFuture<'a> = Pin<Box<dyn Future<Output = PostTransferOutcome> + Send + 'a>>;

/// Rust 原生的对象安全模块接口。STRM、媒体库刷新、索引通知等后续能力
/// 只需实现该 trait，并在注册表中挂载。
pub trait PostTransferModule: Send + Sync {
    fn id(&self) -> &'static str;
    fn run<'a>(&'a self, context: PostTransferContext) -> PostTransferFuture<'a>;
}

#[derive(Clone, Default)]
pub struct PostTransferRegistry {
    modules: Vec<Arc<dyn PostTransferModule>>,
}

impl PostTransferRegistry {
    pub fn with_defaults() -> Self {
        Self {
            modules: vec![Arc::new(MediaLibraryRefreshModule)],
        }
    }

    #[cfg(test)]
    pub fn new(modules: Vec<Arc<dyn PostTransferModule>>) -> Self {
        Self { modules }
    }

    pub async fn run_all(&self, context: PostTransferContext) -> Vec<PostTransferOutcome> {
        let mut outcomes = Vec::with_capacity(self.modules.len());
        for module in &self.modules {
            let module_id = module.id();
            let outcome = module.run(context.clone()).await;
            debug_assert_eq!(outcome.module, module_id);
            outcomes.push(outcome);
        }
        outcomes
    }
}

struct MediaLibraryRefreshModule;

impl PostTransferModule for MediaLibraryRefreshModule {
    fn id(&self) -> &'static str {
        "media_library_refresh"
    }

    fn run<'a>(&'a self, context: PostTransferContext) -> PostTransferFuture<'a> {
        Box::pin(async move {
            match crate::services::media_library::refresh_media_library(
                &context.settings,
                &context.subscription,
                context.reason,
            )
            .await
            {
                None => PostTransferOutcome {
                    module: self.id(),
                    status: PostTransferStatus::Skipped,
                    message: "媒体库刷新未启用".to_string(),
                },
                Some(report) if report.success => PostTransferOutcome {
                    module: self.id(),
                    status: PostTransferStatus::Succeeded,
                    message: format!("{} 刷新成功", report.provider),
                },
                Some(report) => PostTransferOutcome {
                    module: self.id(),
                    status: PostTransferStatus::Failed,
                    message: report
                        .error
                        .unwrap_or_else(|| format!("{} 刷新失败", report.provider)),
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ProbeModule;

    impl PostTransferModule for ProbeModule {
        fn id(&self) -> &'static str {
            "probe"
        }

        fn run<'a>(&'a self, context: PostTransferContext) -> PostTransferFuture<'a> {
            Box::pin(async move {
                PostTransferOutcome {
                    module: self.id(),
                    status: PostTransferStatus::Succeeded,
                    message: format!("{}:{}", context.subscription.id, context.files.len()),
                }
            })
        }
    }

    #[tokio::test]
    async fn registry_runs_typed_modules_with_read_only_snapshots() {
        let registry = PostTransferRegistry::new(vec![Arc::new(ProbeModule)]);
        let outcomes = registry
            .run_all(PostTransferContext {
                settings: Arc::new(Settings::default()),
                subscription: Arc::new(
                    serde_json::from_value(serde_json::json!({
                        "id": "sub-1",
                        "title": "测试订阅",
                        "url": "https://pan.quark.cn/s/test",
                        "created_at": 1,
                        "updated_at": 1,
                        "last_checked_at": 1
                    }))
                    .unwrap(),
                ),
                target_dir: "/media".to_string(),
                files: Arc::new(Vec::new()),
                reason: "test",
            })
            .await;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].module, "probe");
        assert_eq!(outcomes[0].status, PostTransferStatus::Succeeded);
    }
}
