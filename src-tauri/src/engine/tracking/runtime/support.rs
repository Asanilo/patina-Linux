use crate::domain::tracking::TrackingDataChangedPayload;
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use tauri::{AppHandle, Emitter, Runtime};

pub struct TauriRuntimeEventSink<R: Runtime>(AppHandle<R>);

impl<R: Runtime> TauriRuntimeEventSink<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self(app)
    }
}

impl<R: Runtime> RuntimeEventSink for TauriRuntimeEventSink<R> {
    fn emit(&self, event: RuntimeEvent) -> Result<(), String> {
        match event {
            RuntimeEvent::TrackingDataChanged {
                reason,
                changed_at_ms,
            } => self
                .0
                .emit(
                    "tracking-data-changed",
                    TrackingDataChangedPayload::new(reason, changed_at_ms),
                )
                .map_err(|error| error.to_string()),
        }
    }
}

pub fn emit_tracking_data_changed<R: Runtime>(
    app: &AppHandle<R>,
    reason: &str,
    changed_at_ms: u64,
) -> Result<(), String> {
    TauriRuntimeEventSink::new(app.clone()).emit(RuntimeEvent::TrackingDataChanged {
        reason: reason.to_string(),
        changed_at_ms,
    })
}

pub(super) fn log_tracker_error(message: impl AsRef<str>) {
    eprintln!("[tracker] {}", message.as_ref());
}
