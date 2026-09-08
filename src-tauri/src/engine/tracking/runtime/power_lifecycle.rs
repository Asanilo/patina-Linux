use crate::data::tracking_runtime::{TrackingRuntimeDataError, TrackingRuntimeDataStore};
use crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState;

pub(super) async fn flush_pending_power_stop(
    data: &TrackingRuntimeDataStore,
    runtime_state: &TrackingRuntimeSnapshotState,
) -> Result<Option<(&'static str, i64)>, TrackingRuntimeDataError> {
    let Some(stop) = runtime_state.pending_stop() else {
        return Ok(None);
    };
    let reason = apply_power_lifecycle_event(data, stop.2, stop.1).await?;
    runtime_state.acknowledge_stop(stop);
    runtime_state.invalidate_activity();
    Ok(reason.map(|reason| (reason, stop.1)))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{repositories::sessions, schema};
    use sqlx::{Executor, SqlitePool};

    #[test]
    fn resumed_tracking_drains_the_old_stop_once_before_accepting_new_samples() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            let data = TrackingRuntimeDataStore::new(pool.clone());
            let state = TrackingRuntimeSnapshotState::default();
            sessions::start_session(&pool, "Browser", "zen", "", 1_000, 1_000)
                .await
                .unwrap();

            let old_generation = state.lifecycle_generation();
            state.note_power_event("lock", 5_000);
            state.note_power_event("unlock", 15_000);
            assert!(!state.accepts_sample(old_generation));
            assert!(!state.accepts_sample(state.lifecycle_generation()));

            let _guard = state.lock_transition().await;
            assert_eq!(
                flush_pending_power_stop(&data, &state).await.unwrap(),
                Some(("session-ended-lock", 5_000))
            );
            assert!(state.accepts_sample(state.lifecycle_generation()));
            assert_eq!(flush_pending_power_stop(&data, &state).await.unwrap(), None);

            let end_time: i64 = sqlx::query_scalar("SELECT end_time FROM sessions WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(end_time, 5_000);
        });
    }
}
