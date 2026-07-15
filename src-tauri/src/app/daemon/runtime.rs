use super::DaemonSqliteRuntime;
use crate::app::runtime_lease::RuntimeLease;
use crate::engine::api::server::ApiServerHandle;
use crate::engine::runtime_event::RuntimeEventHub;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub struct DaemonTrackingTasks {
    shutdown_tx: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
    context: crate::engine::runtime_context::RuntimeContext,
    health: Arc<crate::engine::tracking::watchdog::RuntimeHealthState>,
    event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
}

pub struct DaemonBackgroundTasks {
    #[cfg(target_os = "linux")]
    power: DaemonPowerTask,
    tracking: DaemonTrackingTasks,
}

impl DaemonBackgroundTasks {
    pub fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    ) -> Self {
        #[cfg(target_os = "linux")]
        let power = DaemonPowerTask::start(context.clone(), event_sink.clone());
        let tracking = DaemonTrackingTasks::start(context, snapshot, event_sink);
        Self {
            #[cfg(target_os = "linux")]
            power,
            tracking,
        }
    }

    async fn shutdown(self) {
        #[cfg(target_os = "linux")]
        self.power.shutdown().await;
        self.tracking.shutdown().await;
    }
}

#[cfg(target_os = "linux")]
pub struct DaemonPowerTask {
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

#[cfg(target_os = "linux")]
impl DaemonPowerTask {
    fn start(
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

    async fn shutdown(self) {
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

impl DaemonTrackingTasks {
    pub fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    ) -> Self {
        let health = Arc::new(crate::engine::tracking::watchdog::RuntimeHealthState::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let tracking_handle = tokio::spawn(run_tracking_restart_loop(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            snapshot,
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

    async fn shutdown(self) {
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
    mut shutdown: watch::Receiver<bool>,
) {
    let mut retry_secs = 2_u64;
    loop {
        let result = crate::engine::tracking::runtime::run_with_context(
            context.clone(),
            health.clone(),
            event_sink.clone(),
            snapshot.clone(),
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

async fn wait_for_restart(shutdown: &mut watch::Receiver<bool>, retry_secs: u64) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(retry_secs)) => false,
        _ = shutdown.changed() => true,
    }
}

pub struct DaemonRuntime {
    api_server: Option<ApiServerHandle>,
    event_hub: Option<Arc<RuntimeEventHub>>,
    background_tasks: Option<DaemonBackgroundTasks>,
    sqlite: Option<DaemonSqliteRuntime>,
    lease: Option<RuntimeLease>,
}

impl DaemonRuntime {
    pub fn new(
        api_server: Option<ApiServerHandle>,
        event_hub: Arc<RuntimeEventHub>,
        background_tasks: Option<DaemonBackgroundTasks>,
        sqlite: DaemonSqliteRuntime,
        lease: RuntimeLease,
    ) -> Self {
        Self {
            api_server,
            event_hub: Some(event_hub),
            background_tasks,
            sqlite: Some(sqlite),
            lease: Some(lease),
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(tasks) = self.background_tasks.take() {
            tasks.shutdown().await;
        }
        if let Some(event_hub) = self.event_hub.as_ref() {
            event_hub.shutdown();
        }
        if let Some(server) = self.api_server.take() {
            server.shutdown().await;
        }
        drop(self.event_hub.take());
        if let Some(sqlite) = self.sqlite.take() {
            sqlite.pool.close().await;
        }
        drop(self.lease.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::daemon::prepare_sqlite_runtime_at_path;
    use crate::app::runtime_lease::{acquire_runtime_lease, RuntimeRole};
    use crate::platform::app_paths::AppProfile;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "patina-daemon-runtime-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn credentials(path: &std::path::Path) -> crate::engine::api::auth::ApiCredentialStore {
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        credentials
            .initialize_at(path, Some("runtime-test-token"))
            .unwrap();
        credentials
    }

    #[tokio::test]
    async fn graceful_shutdown_releases_listener_pool_and_lease() {
        let root = temp_root("full");
        let control_root = root.join("config/Patina Dev");
        let db_path = root.join("data/Patina Dev/patina.db");
        let lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();
        let sqlite = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();
        let context = crate::engine::api::context::ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(sqlite.pool.clone()),
        );
        let event_hub = Arc::new(RuntimeEventHub::new(
            crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
        ));
        let server = crate::engine::api::server::prepare_standalone_server_with_events(
            0,
            credentials(&root.join("data/Patina Dev/api_token")),
            context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            event_hub.clone(),
        )
        .await
        .unwrap();
        let port = server.port();
        let handle = server.start();
        let mut stalled = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut stalled,
            b"GET /api/v1/health HTTP/1.1\r\nAuthorization:",
        )
        .await
        .unwrap();

        DaemonRuntime::new(Some(handle), event_hub, None, sqlite, lease)
            .shutdown()
            .await;

        let rebound = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap();
        drop(rebound);
        let reopened = prepare_sqlite_runtime_at_path(&db_path, false)
            .await
            .unwrap();
        reopened.pool.close().await;
        let next_lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        drop(next_lease);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn shutdown_is_safe_when_api_was_not_started() {
        let root = temp_root("no-api");
        let control_root = root.join("config/Patina Dev");
        let db_path = root.join("data/Patina Dev/patina.db");
        let lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();
        let sqlite = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();

        let event_hub = Arc::new(RuntimeEventHub::new(
            crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
        ));
        let background_tasks = DaemonBackgroundTasks::start(
            crate::engine::runtime_context::RuntimeContext::system(sqlite.pool.clone()),
            Arc::new(
                crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default(),
            ),
            event_hub.clone(),
        );
        DaemonRuntime::new(None, event_hub, Some(background_tasks), sqlite, lease)
            .shutdown()
            .await;

        let next_lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        drop(next_lease);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn tracking_shutdown_seals_active_session_at_last_successful_sample() {
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
