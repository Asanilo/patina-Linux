use super::restart::wait_for_restart;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(super) struct DaemonTrackingTasks {
    shutdown_tx: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
    context: crate::engine::runtime_context::RuntimeContext,
    health: Arc<crate::engine::tracking::watchdog::RuntimeHealthState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
}

impl DaemonTrackingTasks {
    pub(super) fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
        #[cfg(target_os = "linux")] audio_source: crate::platform::linux::audio::AudioSignalSource,
        #[cfg(target_os = "linux")] media_source: crate::platform::linux::media::MediaSignalSource,
    ) -> Self {
        let health = Arc::new(crate::engine::tracking::watchdog::RuntimeHealthState::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let tracking_handle = tokio::spawn(run_tracking_restart_loop(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            snapshot,
            #[cfg(target_os = "linux")]
            audio_source,
            #[cfg(target_os = "linux")]
            media_source,
            shutdown_rx.clone(),
        ));
        let watchdog_handle = tokio::spawn(run_watchdog_restart_loop(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            shutdown_rx,
        ));
        Self {
            shutdown_tx,
            handles: vec![tracking_handle, watchdog_handle],
            context,
            health,
            event_sink,
        }
    }

    pub(super) async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        for mut handle in self.handles {
            if tokio::time::timeout(std::time::Duration::from_secs(5), &mut handle)
                .await
                .is_err()
            {
                handle.abort();
                let _ = handle.await;
            }
        }
        let seal_at_ms = self
            .health
            .snapshot()
            .last_successful_sample_ms
            .unwrap_or_else(|| self.context.now_ms());
        let data = crate::data::tracking_runtime::TrackingRuntimeDataStore::new(
            self.context.pool().clone(),
        );
        match data.end_active_sessions(seal_at_ms).await {
            Ok(true) => {
                let _ = self.event_sink.emit(
                    crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                        reason: crate::domain::tracking::TRACKING_REASON_RUNTIME_SHUTDOWN_SEALED
                            .to_string(),
                        changed_at_ms: seal_at_ms.max(0) as u64,
                    },
                );
            }
            Ok(false) => {}
            Err(error) => eprintln!("[patinad] failed to seal session during shutdown: {error}"),
        }
    }
}

async fn run_tracking_restart_loop(
    context: crate::engine::runtime_context::RuntimeContext,
    health: Arc<crate::engine::tracking::watchdog::RuntimeHealthState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
    #[cfg(target_os = "linux")] audio_source: crate::platform::linux::audio::AudioSignalSource,
    #[cfg(target_os = "linux")] media_source: crate::platform::linux::media::MediaSignalSource,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut retry_secs = 2_u64;
    loop {
        let result = crate::engine::tracking::runtime::run_with_context(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            snapshot.clone(),
            #[cfg(target_os = "linux")]
            audio_source.clone(),
            #[cfg(target_os = "linux")]
            media_source.clone(),
            shutdown.clone(),
        )
        .await;
        if *shutdown.borrow() {
            return;
        }
        if let Err(error) = result {
            eprintln!("[patinad] tracking runtime stopped: {error}");
        }
        if wait_for_restart(&mut shutdown, retry_secs).await {
            return;
        }
        retry_secs = retry_secs.saturating_mul(2).min(30);
    }
}

async fn run_watchdog_restart_loop(
    context: crate::engine::runtime_context::RuntimeContext,
    health: Arc<crate::engine::tracking::watchdog::RuntimeHealthState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut retry_secs = 2_u64;
    loop {
        let result = crate::engine::tracking::watchdog::watch_with_shutdown(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            shutdown.clone(),
        )
        .await;
        if *shutdown.borrow() {
            return;
        }
        if let Err(error) = result {
            eprintln!("[patinad] tracking watchdog stopped: {error}");
        }
        if wait_for_restart(&mut shutdown, retry_secs).await {
            return;
        }
        retry_secs = retry_secs.saturating_mul(2).min(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::daemon::prepare_sqlite_runtime_at_path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "patina-daemon-tracking-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn shutdown_seals_active_session_at_last_successful_sample() {
        let root = temp_root("tracking-seal");
        let db_path = root.join("data/Patina Dev/patina.db");
        let sqlite = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();
        crate::data::repositories::sessions::start_session(
            &sqlite.pool,
            "Ghostty",
            "ghostty",
            "patinad",
            1_000,
            1_000,
        )
        .await
        .unwrap();
        let context = crate::engine::runtime_context::RuntimeContext::system(sqlite.pool.clone());
        let health = Arc::new(crate::engine::tracking::watchdog::RuntimeHealthState::default());
        health.note_successful_sample(5_000);
        let sink = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);
        let tasks = DaemonTrackingTasks {
            shutdown_tx,
            handles: Vec::new(),
            context,
            health,
            event_sink: sink.clone(),
        };

        tasks.shutdown().await;

        let sessions = crate::data::repositories::sessions::fetch_all_for_backup(&sqlite.pool)
            .await
            .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].end_time, Some(5_000));
        assert_eq!(
            sink.events(),
            vec![
                crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                    reason: crate::domain::tracking::TRACKING_REASON_RUNTIME_SHUTDOWN_SEALED
                        .to_string(),
                    changed_at_ms: 5_000,
                }
            ]
        );

        sqlite.pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
