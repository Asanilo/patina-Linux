use super::restart::wait_for_restart;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(super) struct DaemonPowerTask {
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl DaemonPowerTask {
    pub(super) fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = tokio::spawn(run_power_restart_loop(context, event_sink, shutdown_rx));
        Self {
            shutdown_tx,
            handle,
        }
    }

    pub(super) async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut handle = self.handle;
        if tokio::time::timeout(std::time::Duration::from_secs(5), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            let _ = handle.await;
        }
    }
}

async fn run_power_restart_loop(
    context: crate::engine::runtime_context::RuntimeContext,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut retry_secs = 2_u64;
    loop {
        let result =
            run_power_watch_attempt(context.clone(), event_sink.clone(), shutdown.clone()).await;
        if *shutdown.borrow() {
            return;
        }
        if let Err(error) = result {
            eprintln!("[patinad] power watcher stopped: {error}");
        }
        if wait_for_restart(&mut shutdown, retry_secs).await {
            return;
        }
        retry_secs = retry_secs.saturating_mul(2).min(30);
    }
}

async fn run_power_watch_attempt(
    context: crate::engine::runtime_context::RuntimeContext,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(16);
    let watcher = crate::platform::linux::power::watch_systemd_logind(shutdown.clone(), event_tx);
    tokio::pin!(watcher);

    loop {
        tokio::select! {
            result = &mut watcher => return result,
            changed = shutdown.changed() => {
                let _ = changed;
                return Ok(());
            }
            event = event_rx.recv() => {
                let event = event.ok_or_else(|| "power lifecycle event channel closed".to_string())?;
                if event.state == "ready" {
                    println!("[patinad] power watcher ready");
                    continue;
                }
                if let Err(error) = crate::engine::tracking::runtime::handle_power_lifecycle_event_with_context(
                    &context,
                    event_sink.as_ref(),
                    &event.state,
                    event.timestamp_ms as i64,
                ).await {
                    eprintln!("[patinad] power lifecycle handling failed: {error}");
                }
            }
        }
    }
}
