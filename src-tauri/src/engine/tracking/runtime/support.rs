use crate::domain::tracking::TrackingDataChangedPayload;
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use tauri::{AppHandle, Emitter, Runtime};

struct TauriRuntimeEventSink<'a, R: Runtime>(&'a AppHandle<R>);

impl<R: Runtime> RuntimeEventSink for TauriRuntimeEventSink<'_, R> {
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

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

pub fn emit_tracking_data_changed<R: Runtime>(
    app: &AppHandle<R>,
    reason: &str,
    changed_at_ms: u64,
) -> Result<(), String> {
    TauriRuntimeEventSink(app).emit(RuntimeEvent::TrackingDataChanged {
        reason: reason.to_string(),
        changed_at_ms,
    })
}

pub(super) fn log_tracker_error(message: impl AsRef<str>) {
    eprintln!("[tracker] {}", message.as_ref());
}
