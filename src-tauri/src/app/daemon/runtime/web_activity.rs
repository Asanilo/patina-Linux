use crate::domain::settings::WebActivityBridgeSettings;
use crate::platform::web_activity_bridge::{
    WebActivityBridgeHttpHandler, WebActivityBridgeReadinessHandler, WebActivityBridgeRuntimeState,
};
use std::future::Future;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub(crate) struct DaemonWebActivityControl {
    context: crate::engine::runtime_context::RuntimeContext,
    tracking_snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
    state: Arc<crate::engine::web_activity::WebActivityRuntimeState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    runtime: Arc<WebActivityBridgeRuntimeState>,
}

impl DaemonWebActivityControl {
    pub(crate) fn new(
        context: crate::engine::runtime_context::RuntimeContext,
        tracking_snapshot: Arc<
            crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState,
        >,
        state: Arc<crate::engine::web_activity::WebActivityRuntimeState>,
        event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    ) -> Self {
        Self {
            context,
            tracking_snapshot,
            state,
            event_sink,
            runtime: Arc::new(WebActivityBridgeRuntimeState::default()),
        }
    }

    pub(super) async fn apply(&self, settings: WebActivityBridgeSettings) -> Result<bool, String> {
        self.runtime
            .update_with_retry(settings, self.handler(), self.readiness_handler())
            .await
    }

    pub(super) async fn apply_with_commit<F, Fut>(
        &self,
        settings: WebActivityBridgeSettings,
        commit: F,
    ) -> Result<bool, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        self.runtime
            .update_with_commit(settings, self.handler(), self.readiness_handler(), commit)
            .await
    }

    pub(super) async fn shutdown(&self) {
        self.runtime.shutdown().await;
        self.state.set_listening(false);
    }

    fn handler(&self) -> WebActivityBridgeHttpHandler {
        let context = self.context.clone();
        let state = self.state.clone();
        let tracking = self.tracking_snapshot.clone();
        let events = self.event_sink.clone();
        Arc::new(move |request| {
            let context = context.clone();
            let state = state.clone();
            let tracking = tracking.clone();
            let events = events.clone();
            Box::pin(async move {
                crate::engine::web_activity::handle_http_request(
                    &context,
                    state.as_ref(),
                    tracking.snapshot(),
                    events.as_ref(),
                    request,
                )
                .await
            })
        })
    }

    fn readiness_handler(&self) -> WebActivityBridgeReadinessHandler {
        let state = self.state.clone();
        Arc::new(move |listening| state.set_listening(listening))
    }
}

pub(super) struct DaemonWebActivityTask {
    control: DaemonWebActivityControl,
    shutdown_tx: watch::Sender<bool>,
    event_handle: JoinHandle<()>,
    context: crate::engine::runtime_context::RuntimeContext,
    tracking_snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
}

impl DaemonWebActivityTask {
    pub(super) async fn start(
        control: DaemonWebActivityControl,
        event_hub: Arc<crate::engine::runtime_event::RuntimeEventHub>,
    ) -> Self {
        let context = control.context.clone();
        let tracking_snapshot = control.tracking_snapshot.clone();
        let state = control.state.clone();
        let event_sink = control.event_sink.clone();
        let startup_at_ms = context.now_ms();
        match crate::engine::web_activity::repair_active_segment_after_restart(
            context.pool(),
            startup_at_ms,
        )
        .await
        {
            Ok(Some(repaired_at_ms)) => {
                emit_web_activity_event(event_sink.as_ref(), repaired_at_ms)
            }
            Ok(None) => {}
            Err(error) => eprintln!("[patinad] failed to repair active web activity: {error}"),
        }

        let settings = crate::data::repositories::app_settings::load_web_activity_bridge_settings(
            context.pool(),
        )
        .await
        .unwrap_or_else(|error| {
            eprintln!("[patinad] failed to load browser activity bridge settings: {error}");
            crate::domain::settings::WebActivityBridgeSettings::default()
        });
        match control.apply(settings.clone()).await {
            Ok(true) => println!(
                "[patinad] browser activity bridge listening on http://127.0.0.1:{}",
                settings.port
            ),
            Ok(false) => {}
            Err(error) => {
                state.set_listening(false);
                eprintln!("[patinad] failed to apply browser activity bridge settings: {error}");
            }
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let event_handle = tokio::spawn(run_web_activity_event_sync(
            context.clone(),
            tracking_snapshot.clone(),
            state.clone(),
            event_hub,
            event_sink.clone(),
            shutdown_rx,
        ));
        Self {
            control,
            shutdown_tx,
            event_handle,
            context,
            tracking_snapshot,
            event_sink,
        }
    }

    pub(super) async fn shutdown(self) {
        self.control.shutdown().await;
        let _ = self.shutdown_tx.send(true);
        let mut event_handle = self.event_handle;
        if tokio::time::timeout(std::time::Duration::from_secs(5), &mut event_handle)
            .await
            .is_err()
        {
            event_handle.abort();
            let _ = event_handle.await;
        }
        let seal_at_ms = self
            .tracking_snapshot
            .snapshot()
            .map(|snapshot| snapshot.sampled_at_ms)
            .unwrap_or_else(|| self.context.now_ms());
        match crate::engine::web_activity::seal_active_segment(self.context.pool(), seal_at_ms)
            .await
        {
            Ok(true) => emit_web_activity_event(self.event_sink.as_ref(), seal_at_ms),
            Ok(false) => {}
            Err(error) => {
                eprintln!("[patinad] failed to seal web activity during shutdown: {error}")
            }
        }
    }
}

async fn run_web_activity_event_sync(
    context: crate::engine::runtime_context::RuntimeContext,
    tracking_snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
    state: Arc<crate::engine::web_activity::WebActivityRuntimeState>,
    event_hub: Arc<crate::engine::runtime_event::RuntimeEventHub>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut subscription = event_hub.subscribe_after(None);
    let start_at = tokio::time::Instant::now()
        + crate::engine::web_activity::WEB_ACTIVITY_STALE_CHECK_INTERVAL;
    let mut stale_interval = tokio::time::interval_at(
        start_at,
        crate::engine::web_activity::WEB_ACTIVITY_STALE_CHECK_INTERVAL,
    );
    stale_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return;
            }
            event = subscription.receiver.recv() => {
                let envelope = match event {
                    Ok(envelope) => envelope,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        sync_web_activity_boundary(
                            &context,
                            tracking_snapshot.as_ref(),
                            event_sink.as_ref(),
                            context.now_ms(),
                        ).await;
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                };
                let crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged { reason, changed_at_ms } = envelope.event else {
                    continue;
                };
                if reason == crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON {
                    continue;
                }
                sync_web_activity_boundary(
                    &context,
                    tracking_snapshot.as_ref(),
                    event_sink.as_ref(),
                    changed_at_ms.min(i64::MAX as u64) as i64,
                ).await;
            }
            _ = stale_interval.tick() => {
                match crate::engine::web_activity::seal_stale_active_segment(
                    context.pool(),
                    state.as_ref(),
                    context.now_ms(),
                ).await {
                    Ok(Some(sealed_at_ms)) => {
                        emit_web_activity_event(event_sink.as_ref(), sealed_at_ms)
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("[patinad] stale web activity watchdog failed: {error}")
                    }
                }
            }
        }
    }
}

async fn sync_web_activity_boundary(
    context: &crate::engine::runtime_context::RuntimeContext,
    tracking_snapshot: &crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState,
    event_sink: &dyn crate::engine::runtime_event::RuntimeEventSink,
    changed_at_ms: i64,
) {
    match crate::engine::web_activity::seal_if_tracking_inactive(
        context.pool(),
        tracking_snapshot.snapshot(),
        changed_at_ms,
    )
    .await
    {
        Ok(true) => emit_web_activity_event(event_sink, changed_at_ms),
        Ok(false) => {}
        Err(error) => eprintln!("[patinad] failed to sync web activity boundary: {error}"),
    }
}

fn emit_web_activity_event(
    event_sink: &dyn crate::engine::runtime_event::RuntimeEventSink,
    changed_at_ms: i64,
) {
    let _ = event_sink.emit(
        crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
            reason: crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON.to_string(),
            changed_at_ms: changed_at_ms.max(0) as u64,
        },
    );
}
