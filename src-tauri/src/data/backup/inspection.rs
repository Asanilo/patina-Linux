//! Per-process admission control for read-only archive work, not restore writes.
use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;

pub(super) static GATE: LazyLock<Gate> = LazyLock::new(Gate::new);
const BUSY: &str = "backup inspection is busy; retry after the current operation";

pub(super) struct Gate {
    active: Arc<Semaphore>,
}

impl Gate {
    fn new() -> Self {
        Self {
            active: Arc::new(Semaphore::new(1)),
        }
    }

    pub(super) async fn run<T, F>(&self, work: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let active = self
            .active
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| BUSY.to_string())?;
        // A cancelled caller must not release capacity while its blocking worker still runs.
        tauri::async_runtime::spawn_blocking(move || {
            let _active = active;
            work()
        })
        .await
        .map_err(|error| format!("backup inspection worker failed: {error}"))?
    }

    #[cfg(test)]
    fn run_sync<T>(&self, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let _active = self.active.try_acquire().map_err(|_| BUSY.to_string())?;
        work()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn serializes_work_and_keeps_permits_until_cancelled_worker_finishes() {
        tauri::async_runtime::block_on(async {
            let gate = Arc::new(Gate::new());
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let first_gate = gate.clone();
            let first = tokio::spawn(async move {
                first_gate
                    .run(move || {
                        started_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        Ok(())
                    })
                    .await
            });
            started_rx.await.unwrap();
            first.abort();
            assert!(first.await.unwrap_err().is_cancelled());
            assert!(gate.run_sync(|| Ok(())).unwrap_err().contains("busy"));
            let queued_gate = gate.clone();
            let queued = tokio::spawn(async move { queued_gate.run(|| Ok(())).await });
            tokio::task::yield_now().await;
            assert!(!queued.is_finished());
            queued.abort();
            assert!(queued.await.unwrap_err().is_cancelled());
            release_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if gate.run_sync(|| Ok(())).is_ok() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(gate
                .run(|| Err::<(), _>("invalid archive".into()))
                .await
                .is_err());
            assert!(gate.run(|| Ok(())).await.is_ok());
        });
    }
}
