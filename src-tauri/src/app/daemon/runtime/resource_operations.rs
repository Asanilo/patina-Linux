//! The daemon owns accepted resource changes beyond a transport's lifetime.
use crate::engine::api::runtime_control::RuntimeControlError;
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Mutex;

#[derive(Default)]
pub(super) struct ResourceOperations {
    active: Arc<Mutex<()>>,
    closing: AtomicBool,
}

impl ResourceOperations {
    pub(super) async fn run<T: Send + 'static>(
        &self,
        operation: impl Future<Output = Result<T, RuntimeControlError>> + Send + 'static,
    ) -> Result<T, RuntimeControlError> {
        // Reject overlap instead of accumulating detached tasks behind a mutex.
        let permit = self.active.clone().try_lock_owned().map_err(|_| {
            RuntimeControlError::Conflict("a runtime resource change is already in progress".into())
        })?;
        if self.closing.load(Ordering::Acquire) {
            return Err(RuntimeControlError::Conflict(
                "runtime resources are shutting down".into(),
            ));
        }
        tokio::spawn(async move {
            let _permit = permit;
            operation.await
        })
        .await
        .map_err(|_| RuntimeControlError::Internal("runtime resource operation failed".into()))?
    }

    pub(super) async fn close_and_drain(&self) {
        self.closing.store(true, Ordering::Release);
        // Do not close listeners/storage while an accepted operation is publishing.
        let _permit = self.active.lock().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn cancelled_waiter_does_not_cancel_change_and_shutdown_drains_it() {
        let owner = Arc::new(ResourceOperations::default());
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let committed = Arc::new(AtomicBool::new(false));
        let caller = tokio::spawn({
            let owner = owner.clone();
            let committed = committed.clone();
            async move {
                owner
                    .run(async move {
                        started_tx.send(()).unwrap();
                        release_rx.await.unwrap();
                        committed.store(true, Ordering::Release);
                        Ok(())
                    })
                    .await
            }
        });
        started_rx.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(matches!(
            owner
                .run::<()>(async {
                    Err(RuntimeControlError::Internal(
                        "overlapping operation ran".into(),
                    ))
                })
                .await,
            Err(RuntimeControlError::Conflict(_))
        ));
        let closing = tokio::spawn({
            let owner = owner.clone();
            async move { owner.close_and_drain().await }
        });
        while !owner.closing.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        assert!(!closing.is_finished());
        release_tx.send(()).unwrap();
        closing.await.unwrap();
        assert!(committed.load(Ordering::Acquire));
        assert!(matches!(
            owner.run(async { Ok(()) }).await,
            Err(RuntimeControlError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn failed_operation_releases_admission() {
        let owner = ResourceOperations::default();
        assert!(owner
            .run::<()>(async { Err(RuntimeControlError::Internal("injected".into())) })
            .await
            .is_err());
        assert_eq!(owner.run(async { Ok(42) }).await.unwrap(), 42);
        owner.close_and_drain().await;
    }
}
