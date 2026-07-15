use crate::data::tracking_runtime::TrackingRuntimeDataStore;
use crate::domain::tracking::TRACKING_REASON_WATCHDOG_SEALED;
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

const TRACKER_WATCHDOG_POLL_MS: u64 = 1_000;
const TRACKER_STALL_SEAL_AFTER_MS: i64 = 8_000;

#[derive(Debug, Default)]
pub struct RuntimeHealthState {
    last_heartbeat_ms: AtomicI64,
    last_successful_sample_ms: AtomicI64,
    last_watchdog_seal_sample_ms: AtomicI64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeHealthSnapshot {
    pub last_heartbeat_ms: Option<i64>,
    pub last_successful_sample_ms: Option<i64>,
    pub last_watchdog_seal_sample_ms: Option<i64>,
}

impl RuntimeHealthState {
    pub fn note_heartbeat(&self, timestamp_ms: i64) {
        self.last_heartbeat_ms
            .store(timestamp_ms, Ordering::Relaxed);
    }

    pub fn note_successful_sample(&self, timestamp_ms: i64) {
        self.last_successful_sample_ms
            .store(timestamp_ms, Ordering::Relaxed);
    }

    fn last_successful_sample_ms(&self) -> Option<i64> {
        let timestamp_ms = self.last_successful_sample_ms.load(Ordering::Relaxed);
        (timestamp_ms > 0).then_some(timestamp_ms)
    }

    fn note_watchdog_seal(&self, timestamp_ms: i64) {
        self.last_watchdog_seal_sample_ms
            .store(timestamp_ms, Ordering::Relaxed);
    }

    fn last_watchdog_seal_sample_ms(&self) -> Option<i64> {
        let timestamp_ms = self.last_watchdog_seal_sample_ms.load(Ordering::Relaxed);
        (timestamp_ms > 0).then_some(timestamp_ms)
    }

    pub fn snapshot(&self) -> RuntimeHealthSnapshot {
        RuntimeHealthSnapshot {
            last_heartbeat_ms: self.last_heartbeat_ms(),
            last_successful_sample_ms: self.last_successful_sample_ms(),
            last_watchdog_seal_sample_ms: self.last_watchdog_seal_sample_ms(),
        }
    }

    fn last_heartbeat_ms(&self) -> Option<i64> {
        let timestamp_ms = self.last_heartbeat_ms.load(Ordering::Relaxed);
        (timestamp_ms > 0).then_some(timestamp_ms)
    }
}

pub async fn watch(
    context: RuntimeContext,
    health_state: Arc<RuntimeHealthState>,
    event_sink: Arc<dyn RuntimeEventSink>,
) -> Result<(), String> {
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    watch_with_shutdown(context, health_state, event_sink, shutdown_rx).await
}

pub async fn watch_with_shutdown(
    context: RuntimeContext,
    health_state: Arc<RuntimeHealthState>,
    event_sink: Arc<dyn RuntimeEventSink>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        run_iteration(&context, &health_state, event_sink.as_ref()).await;
        tokio::select! {
            _ = sleep(Duration::from_millis(TRACKER_WATCHDOG_POLL_MS)) => {}
            _ = shutdown.changed() => return Ok(()),
        }
    }
}

pub(crate) async fn run_iteration(
    context: &RuntimeContext,
    health_state: &RuntimeHealthState,
    event_sink: &dyn RuntimeEventSink,
) {
    let last_successful_sample_ms = health_state.last_successful_sample_ms();
    if should_watchdog_seal(
        last_successful_sample_ms,
        health_state.last_watchdog_seal_sample_ms(),
        context.now_ms(),
    ) {
        let data = TrackingRuntimeDataStore::new(context.pool().clone());
        seal_stale_session(
            &data,
            health_state,
            event_sink,
            last_successful_sample_ms.unwrap_or_default(),
        )
        .await;
    }
}

async fn seal_stale_session(
    data: &TrackingRuntimeDataStore,
    health_state: &RuntimeHealthState,
    event_sink: &dyn RuntimeEventSink,
    sample_time_ms: i64,
) {
    match data.end_active_sessions(sample_time_ms).await {
        Ok(did_seal) => {
            health_state.note_watchdog_seal(sample_time_ms);

            if did_seal {
                log_watchdog_error(format!(
                    "watchdog sealed stale active session at {} after tracker stall",
                    sample_time_ms
                ));
                let _ = event_sink.emit(RuntimeEvent::TrackingDataChanged {
                    reason: TRACKING_REASON_WATCHDOG_SEALED.to_string(),
                    changed_at_ms: sample_time_ms as u64,
                });
            }
        }
        Err(error) => {
            log_watchdog_error(format!("watchdog failed to seal stale session: {error}"));
        }
    }
}

pub(crate) fn should_watchdog_seal(
    last_successful_sample_ms: Option<i64>,
    last_watchdog_seal_sample_ms: Option<i64>,
    now_ms: i64,
) -> bool {
    let Some(last_successful_sample_ms) = last_successful_sample_ms else {
        return false;
    };

    if last_watchdog_seal_sample_ms == Some(last_successful_sample_ms) {
        return false;
    }

    now_ms.saturating_sub(last_successful_sample_ms) > TRACKER_STALL_SEAL_AFTER_MS
}

fn log_watchdog_error(message: impl AsRef<str>) {
    eprintln!("[tracker] {}", message.as_ref());
}

#[cfg(test)]
mod tests {
    use super::{run_iteration, RuntimeHealthState};
    use crate::engine::runtime_context::{RuntimeClock, RuntimeContext};
    use crate::engine::runtime_event::{MemoryRuntimeEventSink, RuntimeEvent};
    use sqlx::{Executor, Row, SqlitePool};
    use std::sync::Arc;

    struct FixedClock(i64);

    impl RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[test]
    fn runtime_health_snapshot_starts_empty() {
        let state = RuntimeHealthState::default();

        assert_eq!(state.snapshot().last_heartbeat_ms, None);
        assert_eq!(state.snapshot().last_successful_sample_ms, None);
        assert_eq!(state.snapshot().last_watchdog_seal_sample_ms, None);
    }

    #[test]
    fn runtime_health_snapshot_tracks_heartbeat() {
        let state = RuntimeHealthState::default();

        state.note_heartbeat(12_000);

        assert_eq!(state.snapshot().last_heartbeat_ms, Some(12_000));
        assert_eq!(state.snapshot().last_successful_sample_ms, None);
    }

    #[test]
    fn runtime_health_snapshot_tracks_successful_sample() {
        let state = RuntimeHealthState::default();

        state.note_successful_sample(13_000);

        assert_eq!(state.snapshot().last_heartbeat_ms, None);
        assert_eq!(state.snapshot().last_successful_sample_ms, Some(13_000));
    }

    #[test]
    fn runtime_health_snapshot_tracks_watchdog_seal_separately() {
        let state = RuntimeHealthState::default();

        state.note_successful_sample(14_000);
        state.note_watchdog_seal(14_000);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.last_successful_sample_ms, Some(14_000));
        assert_eq!(snapshot.last_watchdog_seal_sample_ms, Some(14_000));
    }

    #[tokio::test]
    async fn iteration_seals_stale_session_and_emits_one_runtime_event() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        crate::data::repositories::sessions::start_session(
            &pool, "Ghostty", "ghostty", "patina", 1_000, 1_000,
        )
        .await
        .unwrap();
        let context = RuntimeContext::new(pool.clone(), Arc::new(FixedClock(20_000)));
        let health = RuntimeHealthState::default();
        health.note_successful_sample(5_000);
        let sink = MemoryRuntimeEventSink::default();

        run_iteration(&context, &health, &sink).await;

        let row = sqlx::query("SELECT end_time FROM sessions LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<Option<i64>, _>("end_time"), Some(5_000));
        assert_eq!(
            sink.events(),
            vec![RuntimeEvent::TrackingDataChanged {
                reason: crate::domain::tracking::TRACKING_REASON_WATCHDOG_SEALED.to_string(),
                changed_at_ms: 5_000,
            }]
        );
        pool.close().await;
    }
}
