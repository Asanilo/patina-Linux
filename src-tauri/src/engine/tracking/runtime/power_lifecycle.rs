use crate::data::tracking_runtime::{TrackingRuntimeDataError, TrackingRuntimeDataStore};

pub(super) async fn apply_power_lifecycle_event(
    data: &TrackingRuntimeDataStore,
    state: &str,
    timestamp_ms: i64,
) -> Result<Option<&'static str>, TrackingRuntimeDataError> {
    let should_end_active_session = matches!(state, "lock" | "suspend" | "shutdown");

    if !should_end_active_session {
        return Ok(None);
    }

    if data.end_active_sessions(timestamp_ms).await? {
        return Ok(Some(match state {
            "lock" => "session-ended-lock",
            "suspend" => "session-ended-suspend",
            "shutdown" => "session-ended-shutdown",
            _ => "session-ended-system",
        }));
    }

    Ok(None)
}
