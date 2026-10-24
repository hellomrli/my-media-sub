use std::future::Future;

use tokio::sync::OwnedMutexGuard;

use crate::error::{AppError, Result};

/// Once dispatched, a commit owns the serialization lock until both the disk
/// write and memory update finish, even if its caller is canceled. Merely
/// holding a borrowed guard around spawn_blocking does not provide this guarantee.
pub(crate) async fn finish_commit<T, F>(guard: OwnedMutexGuard<()>, commit: F) -> Result<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    tokio::spawn(async move {
        let _guard = guard;
        let result = commit.await;
        if let Err(error) = &result {
            // A canceled caller can no longer observe the returned error.
            tracing::error!(%error, "Store commit failed");
        }
        result
    })
    .await
    .map_err(|error| AppError::Database(format!("Store commit task failed: {error}")))?
}
