use crate::domain::tools::{ToolAlert, ToolsRuntimeSnapshot};
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use crate::engine::tools::{ToolsRuntimeOwner, ToolsRuntimeSink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(crate) struct DaemonToolsRuntimeSink {
    event_sink: Arc<dyn RuntimeEventSink>,
    ready: Arc<AtomicBool>,
}

impl DaemonToolsRuntimeSink {
    pub(crate) fn new(event_sink: Arc<dyn RuntimeEventSink>, ready: Arc<AtomicBool>) -> Self {
        Self { event_sink, ready }
    }
}

impl ToolsRuntimeSink for DaemonToolsRuntimeSink {
    fn snapshot_changed(&self, snapshot: &ToolsRuntimeSnapshot) {
        self.ready.store(true, Ordering::Release);
        if let Err(error) = self.event_sink.emit(RuntimeEvent::ToolsRuntimeChanged {
            changed_at_ms: snapshot.sampled_at_ms.max(0) as u64,
        }) {
            eprintln!("[patinad] failed to emit Tools runtime event: {error}");
        }
    }

    fn alert(&self, alert: &ToolAlert) {
        #[cfg(target_os = "linux")]
        if let Err(error) = crate::platform::linux::notifications::send(&alert.title, &alert.body) {
            eprintln!("[patinad] {error}");
        }
        if let Err(error) = self.event_sink.emit(RuntimeEvent::ToolAlert {
            alert: alert.clone(),
        }) {
            eprintln!("[patinad] failed to emit Tools alert event: {error}");
        }
    }
}

pub(super) struct DaemonToolsTask {
    ready: Arc<AtomicBool>,
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl DaemonToolsTask {
    pub(super) fn start(owner: ToolsRuntimeOwner, ready: Arc<AtomicBool>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task_ready = ready.clone();
        let handle = tokio::spawn(async move {
            if let Err(error) = owner.run_with_shutdown(shutdown_rx).await {
                eprintln!("[patinad] Tools runtime stopped: {error}");
            }
            task_ready.store(false, Ordering::Release);
        });
        Self {
            ready,
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
        self.ready.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_snapshot_marks_runtime_ready_and_emits_typed_event() {
        let event_sink = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let ready = Arc::new(AtomicBool::new(false));
        let sink = DaemonToolsRuntimeSink::new(event_sink.clone(), ready.clone());
        let snapshot = ToolsRuntimeSnapshot {
            sampled_at_ms: 1_234,
            ..ToolsRuntimeSnapshot::default()
        };

        sink.snapshot_changed(&snapshot);

        assert!(ready.load(Ordering::Acquire));
        assert_eq!(
            event_sink.events(),
            vec![RuntimeEvent::ToolsRuntimeChanged {
                changed_at_ms: 1_234
            }]
        );
    }
}
