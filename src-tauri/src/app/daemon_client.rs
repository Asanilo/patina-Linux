use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

// Stage 2H.3b.2 builds this adapter before Stage 2H.3b.3 wires the explicit
// desktop client mode. Keep the preview implementation compiled and tested.
#[allow(dead_code)]
pub mod runtime;

#[derive(Clone, Debug)]
pub struct PatinadClientState {
    client: Arc<RwLock<Option<crate::platform::daemon_client::PatinadClient>>>,
    revision_tx: tokio::sync::watch::Sender<u64>,
}

impl Default for PatinadClientState {
    fn default() -> Self {
        let (revision_tx, _) = tokio::sync::watch::channel(0);
        Self {
            client: Arc::new(RwLock::new(None)),
            revision_tx,
        }
    }
}

impl PatinadClientState {
    pub fn install(&self, client: crate::platform::daemon_client::PatinadClient) {
        {
            match self.client.write() {
                Ok(mut current) => *current = Some(client),
                Err(poisoned) => *poisoned.into_inner() = Some(client),
            }
        }
        self.revision_tx.send_modify(|revision| {
            *revision = revision.saturating_add(1);
        });
    }

    pub fn require(&self) -> Result<crate::platform::daemon_client::PatinadClient, String> {
        let client = match self.client.read() {
            Ok(current) => current.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        client.ok_or_else(|| "patinad client is not configured for this profile".to_string())
    }

    pub fn replace_configuration(&self, port: u16, token: String) -> Result<(), String> {
        let client = crate::platform::daemon_client::PatinadClient::new(port, token)
            .map_err(|error| error.to_string())?;
        self.install(client);
        Ok(())
    }

    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.revision_tx.subscribe()
    }
}

pub fn command_client<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<crate::platform::daemon_client::PatinadClient>, String> {
    if app
        .state::<crate::app::runtime::DesktopRuntimeMode>()
        .owns_embedded_runtime()
    {
        return Ok(None);
    }
    app.try_state::<PatinadClientState>()
        .ok_or_else(|| "patinad client state is unavailable".to_string())?
        .require()
        .map(Some)
}

pub async fn route_owned_app_settings<R: Runtime>(
    app: &AppHandle<R>,
    client: &crate::platform::daemon_client::PatinadClient,
    mutations: Vec<crate::data::repositories::app_settings::AppSettingMutation>,
) -> Result<Vec<crate::data::repositories::app_settings::AppSettingMutation>, String> {
    crate::data::repositories::app_settings::validate_app_setting_mutations(&mutations)?;
    if mutations
        .iter()
        .any(|mutation| matches!(mutation.key.as_str(), "local_api_port" | "local_api_token"))
    {
        return Err("local API settings require the dedicated configuration commands".to_string());
    }

    let has_browser_settings = mutations
        .iter()
        .any(|mutation| is_browser_runtime_setting(&mutation.key));
    let mut browser_configuration = if has_browser_settings {
        let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(app).await?;
        let current =
            crate::data::repositories::app_settings::load_runtime_activity_settings(&pool)
                .await
                .map_err(|error| format!("failed to load browser activity settings: {error}"))?;
        Some(
            crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration {
                enabled: current.web_activity_bridge.enabled,
                port: current.web_activity_bridge.port,
                token: current.web_activity_bridge.token,
                url_privacy: current.web_activity_url_privacy,
            },
        )
    } else {
        None
    };
    let mut afk_threshold = None;
    let mut tracking_paused = None;
    let mut audio_participation_enabled = None;
    let mut remaining = Vec::new();

    for mutation in mutations {
        match mutation.key.as_str() {
            "idle_timeout_secs" => {
                let seconds = mutation
                    .value
                    .parse::<u64>()
                    .map_err(|_| "idle timeout setting must be an integer".to_string())?;
                if !(60..=86_400).contains(&seconds) {
                    return Err("idle timeout setting must be between 60 and 86400".to_string());
                }
                afk_threshold = Some(seconds);
            }
            "tracking_paused" => {
                tracking_paused = Some(crate::domain::settings::parse_boolean_setting(
                    &mutation.value,
                    false,
                ));
            }
            "audio_participation_enabled" => {
                audio_participation_enabled = Some(crate::domain::settings::parse_boolean_setting(
                    &mutation.value,
                    true,
                ));
            }
            "web_activity_enabled" => {
                require_browser_configuration(&mut browser_configuration)?.enabled =
                    crate::domain::settings::parse_boolean_setting(&mutation.value, false);
            }
            "web_activity_port" => {
                require_browser_configuration(&mut browser_configuration)?.port =
                    crate::domain::settings::parse_web_activity_port(&mutation.value)
                        .ok_or_else(|| "browser activity port is invalid".to_string())?;
            }
            "web_activity_token" => {
                require_browser_configuration(&mut browser_configuration)?.token = mutation.value;
            }
            "web_activity_url_privacy" => {
                require_browser_configuration(&mut browser_configuration)?.url_privacy =
                    crate::domain::settings::parse_web_activity_url_privacy(Some(&mutation.value));
            }
            _ => remaining.push(mutation),
        }
    }

    if let Some(configuration) = browser_configuration.as_mut() {
        configuration.token = configuration.token.trim().to_string();
        crate::engine::api::runtime_control::validate_browser_activity_configuration(configuration)
            .map_err(runtime_control_error_message)?;
    }

    if let Some(seconds) = afk_threshold {
        client
            .set_afk_threshold(seconds)
            .await
            .map_err(|error| error.to_string())?;
    }
    if let Some(paused) = tracking_paused {
        client
            .set_tracking_paused(paused)
            .await
            .map_err(|error| error.to_string())?;
    }
    if let Some(enabled) = audio_participation_enabled {
        client
            .set_audio_participation_enabled(enabled)
            .await
            .map_err(|error| error.to_string())?;
    }
    if let Some(configuration) = browser_configuration {
        client
            .configure_browser_activity(configuration)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(remaining)
}

fn require_browser_configuration(
    configuration: &mut Option<
        crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration,
    >,
) -> Result<&mut crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration, String> {
    configuration
        .as_mut()
        .ok_or_else(|| "browser activity configuration is unavailable".to_string())
}

fn runtime_control_error_message(
    error: crate::engine::api::runtime_control::RuntimeControlError,
) -> String {
    match error {
        crate::engine::api::runtime_control::RuntimeControlError::InvalidInput(message)
        | crate::engine::api::runtime_control::RuntimeControlError::Conflict(message)
        | crate::engine::api::runtime_control::RuntimeControlError::Internal(message) => message,
    }
}

fn is_browser_runtime_setting(key: &str) -> bool {
    matches!(
        key,
        "web_activity_enabled"
            | "web_activity_port"
            | "web_activity_token"
            | "web_activity_url_privacy"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct DaemonClientDiagnosticsSnapshot {
    pub base_url: String,
    pub status: String,
    pub server_version: Option<String>,
    pub protocol_version: Option<u32>,
    pub tracking_ready: bool,
    pub event_stream_available: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

pub async fn diagnose(port: u16, token: String) -> DaemonClientDiagnosticsSnapshot {
    let client = match crate::platform::daemon_client::PatinadClient::new(port, token) {
        Ok(client) => client,
        Err(error) => return failure_snapshot(format!("http://127.0.0.1:{port}"), error),
    };
    diagnose_client(client).await
}

pub async fn diagnose_client(
    client: crate::platform::daemon_client::PatinadClient,
) -> DaemonClientDiagnosticsSnapshot {
    let base_url = client.base_url().to_string();

    match client.negotiate_tracking_owner().await {
        Ok(negotiated) => DaemonClientDiagnosticsSnapshot {
            base_url,
            status: if negotiated.tracking_ready {
                "ready"
            } else {
                "starting"
            }
            .to_string(),
            server_version: Some(negotiated.server_version),
            protocol_version: Some(negotiated.protocol_version),
            tracking_ready: negotiated.tracking_ready,
            event_stream_available: negotiated.event_stream_available,
            error_code: None,
            error_message: None,
        },
        Err(error) => failure_snapshot(base_url, error),
    }
}

fn failure_snapshot(
    base_url: String,
    error: crate::platform::daemon_client::PatinadClientError,
) -> DaemonClientDiagnosticsSnapshot {
    DaemonClientDiagnosticsSnapshot {
        base_url,
        status: "unavailable".to_string(),
        server_version: None,
        protocol_version: None,
        tracking_ready: false,
        event_stream_available: false,
        error_code: Some(error.code().to_string()),
        error_message: Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::api::surface::ApiSurface;
    use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
    use crate::platform::daemon_client::{PatinadClient, PatinadClientError, PatinadStreamEvent};
    use sha2::Digest;

    const TEST_TOKEN: &str = "patina_api_daemon-client-test";
    static TEST_RUNTIME_SEQUENCE: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    #[tokio::test]
    async fn invalid_configuration_is_sanitized_for_diagnostics() {
        let snapshot = super::diagnose(0, "secret-that-must-not-leak".to_string()).await;

        assert_eq!(snapshot.status, "unavailable");
        assert_eq!(
            snapshot.error_code.as_deref(),
            Some("invalid-configuration")
        );
        assert!(!snapshot
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("secret-that-must-not-leak"));
    }

    #[tokio::test]
    async fn client_negotiates_against_the_real_daemon_api_transport() {
        let runtime = TestApiRuntime::start(ApiSurface::DaemonTracking).await;
        let client = PatinadClient::new(runtime.port, TEST_TOKEN).unwrap();

        let negotiated = client.negotiate_tracking_owner().await.unwrap();

        assert_eq!(
            negotiated.protocol_version,
            crate::engine::api::protocol::CURRENT_PROTOCOL_VERSION
        );
        assert!(!negotiated.tracking_ready);
        assert!(negotiated.event_stream_available);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn client_distinguishes_bad_credentials_from_the_wrong_runtime_host() {
        let daemon = TestApiRuntime::start(ApiSurface::DaemonTracking).await;
        let unauthorized = PatinadClient::new(daemon.port, "wrong-token")
            .unwrap()
            .negotiate_tracking_owner()
            .await
            .unwrap_err();
        assert_eq!(unauthorized, PatinadClientError::Unauthorized);
        daemon.shutdown().await;

        let desktop = TestApiRuntime::start(ApiSurface::Desktop).await;
        let wrong_host = PatinadClient::new(desktop.port, TEST_TOKEN)
            .unwrap()
            .negotiate_tracking_owner()
            .await
            .unwrap_err();
        assert_eq!(wrong_host.code(), "wrong-runtime-host");
        desktop.shutdown().await;
    }

    #[tokio::test]
    async fn client_routes_tracker_and_classification_writes_through_daemon_transport() {
        let runtime = TestApiRuntime::start_tracking().await;
        let client = PatinadClient::new(runtime.port, TEST_TOKEN).unwrap();

        client.set_afk_threshold(600).await.unwrap();
        assert_eq!(
            client.tracker_settings().await.unwrap().idle_timeout_secs,
            600
        );

        let key = "__category_label_override::focus";
        client
            .commit_classification_settings(vec![
                crate::engine::api::types::ClassificationMutationRequest {
                    key: key.to_string(),
                    value: Some("Focus".to_string()),
                },
            ])
            .await
            .unwrap();
        client
            .commit_app_settings(vec![crate::engine::api::types::AppSettingMutationRequest {
                key: "theme_mode".to_string(),
                value: "dark".to_string(),
            }])
            .await
            .unwrap();
        assert!(matches!(
            client
                .commit_app_settings(vec![crate::engine::api::types::AppSettingMutationRequest {
                    key: "not_allowed".to_string(),
                    value: "1".to_string(),
                },])
                .await
                .unwrap_err(),
            PatinadClientError::Http { status: 400, .. }
        ));
        let rejected = client.set_afk_threshold(10).await.unwrap_err();
        assert!(matches!(
            rejected,
            PatinadClientError::Http { status: 400, .. }
        ));
        assert_eq!(
            client
                .delete_tracking_data_before(1)
                .await
                .unwrap()
                .sessions_deleted,
            0
        );
        assert_eq!(
            client
                .clear_window_titles()
                .await
                .unwrap()
                .sessions_redacted,
            1
        );

        let tools = client
            .start_timer(crate::engine::api::types::StartTimerRequest {
                mode: crate::domain::tools::TimerMode::Stopwatch,
                duration_ms: None,
                label: Some("Transport test".to_string()),
            })
            .await
            .unwrap();
        assert_eq!(
            tools
                .current_timer
                .as_ref()
                .and_then(|timer| timer.label.as_deref()),
            Some("Transport test")
        );
        assert!(client
            .tools_snapshot()
            .await
            .unwrap()
            .current_timer
            .is_some());
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn client_commits_lists_and_deletes_a_staged_activity_import() {
        const CSV: &[u8] = b"record_type,start_time,end_time,duration_ms,exe_name,app_name,title,category\nexact_session,2026-01-15T09:00:00+08:00,2026-01-15T09:30:00+08:00,1800000,org.example.Editor,Editor,Work,Development\n";

        let runtime = TestApiRuntime::start_tracking().await;
        let client = PatinadClient::new(runtime.port, TEST_TOKEN).unwrap();
        let staging_root = runtime.root.join("activity-import-staging");
        let ticket =
            crate::platform::activity_import_staging::stage_bytes(&staging_root, CSV).unwrap();
        let fingerprint = format!("{:x}", sha2::Sha256::digest(CSV));

        let report = client
            .commit_staged_activity_import(
                &crate::engine::api::types::StagedActivityImportCommitRequest {
                    ticket,
                    source_name: "activity.csv".to_string(),
                    expected_fingerprint: fingerprint,
                },
            )
            .await
            .unwrap();
        assert_eq!(report.imported_records, 1);

        let batches = client.activity_import_batches().await.unwrap();
        assert_eq!(batches.len(), 1);
        let deleted = client
            .delete_activity_import_batch(&batches[0].id)
            .await
            .unwrap();
        assert_eq!(deleted.deleted_exact_sessions, 1);
        assert!(client.activity_import_batches().await.unwrap().is_empty());

        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn runtime_adapter_reads_current_and_active_state_and_follows_sse() {
        let runtime = TestApiRuntime::start_tracking().await;
        let client = PatinadClient::new(runtime.port, TEST_TOKEN).unwrap();
        let state = std::sync::Arc::new(super::runtime::PatinadRuntimeState::default());
        let adapter = super::runtime::PatinadRuntimeAdapter::new(client, state.clone());
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move { adapter.run(shutdown_rx).await });

        wait_for_runtime(&state, "ghostty").await;
        let initial = state.snapshot();
        let initial_runtime = initial.runtime.unwrap();
        assert_eq!(initial_runtime.current_window.sampled_at_ms, 1_000);
        assert_eq!(
            initial_runtime
                .current_window
                .runtime_snapshot
                .window
                .exe_name,
            "ghostty"
        );
        assert_eq!(
            initial_runtime.current_window.runtime_snapshot.probe_status,
            crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeStatus::Ok
        );
        assert_eq!(initial_runtime.active_session.unwrap().exe_name, "ghostty");
        assert!(initial_runtime.coherent);

        runtime.replace_tracking_snapshot(tracking_snapshot("obsidian", 2_000));
        runtime
            .replace_active_session("Obsidian", "obsidian", 1_900)
            .await;
        runtime
            .event_hub
            .emit(RuntimeEvent::TrackingDataChanged {
                reason: "session-transition".to_string(),
                changed_at_ms: 2_000,
            })
            .unwrap();

        wait_for_runtime(&state, "obsidian").await;
        let updated = state.snapshot().runtime.unwrap();
        assert_eq!(updated.active_session.unwrap().exe_name, "obsidian");
        assert!(updated.last_event_sequence.is_some());
        assert!(updated.coherent);

        shutdown_tx.send(true).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("runtime adapter should honor shutdown")
            .unwrap();
        assert_eq!(
            state.snapshot().connection_status,
            super::runtime::PatinadRuntimeConnectionStatus::Stopped
        );
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn runtime_adapter_reconnects_with_replaced_client_configuration() {
        let first_runtime = TestApiRuntime::start_tracking().await;
        let second_runtime = TestApiRuntime::start_tracking().await;
        second_runtime.replace_tracking_snapshot(tracking_snapshot("obsidian", 2_000));
        second_runtime
            .replace_active_session("Obsidian", "obsidian", 1_900)
            .await;

        let client_state = super::PatinadClientState::default();
        client_state.install(PatinadClient::new(first_runtime.port, TEST_TOKEN).unwrap());
        let output = std::sync::Arc::new(super::runtime::PatinadRuntimeState::default());
        let adapter = super::runtime::PatinadRuntimeAdapter::new_with_client_state(
            client_state.clone(),
            output.clone(),
        );
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move { adapter.run(shutdown_rx).await });

        wait_for_runtime(&output, "ghostty").await;
        client_state
            .replace_configuration(second_runtime.port, TEST_TOKEN.to_string())
            .unwrap();
        first_runtime.shutdown().await;
        wait_for_runtime(&output, "obsidian").await;

        shutdown_tx.send(true).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("runtime adapter should stop after reconnect")
            .unwrap();
        second_runtime.shutdown().await;
    }

    #[tokio::test]
    async fn event_stream_reports_replay_gap_for_a_stale_cursor() {
        let runtime = TestApiRuntime::start_tracking().await;
        for sequence in 0..300 {
            runtime
                .event_hub
                .emit(RuntimeEvent::TrackingDataChanged {
                    reason: "replay-fill".to_string(),
                    changed_at_ms: 2_000 + sequence,
                })
                .unwrap();
        }
        let client = PatinadClient::new(runtime.port, TEST_TOKEN).unwrap();
        let mut events = client.open_event_stream(Some(0)).await.unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.next_event())
            .await
            .expect("resync event should arrive")
            .unwrap()
            .unwrap();

        assert!(matches!(
            event,
            PatinadStreamEvent::ResyncRequired { ref reason, missed: None }
                if reason == "replay-gap"
        ));
        runtime.shutdown().await;
    }

    async fn wait_for_runtime(state: &super::runtime::PatinadRuntimeState, expected_exe: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if state
                    .snapshot()
                    .runtime
                    .as_ref()
                    .is_some_and(|runtime| runtime.current_window.exe_name == expected_exe)
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for daemon runtime `{expected_exe}`"));
    }

    fn tracking_snapshot(
        exe_name: &str,
        sampled_at_ms: i64,
    ) -> crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot {
        crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot {
            generation: 0,
            window: crate::platform::linux::foreground::WindowInfo {
                hwnd: "0x100".to_string(),
                root_owner_hwnd: "0x100".to_string(),
                process_id: 42,
                window_class: exe_name.to_string(),
                title: "Window".to_string(),
                exe_name: exe_name.to_string(),
                process_path: format!("/usr/bin/{exe_name}"),
                is_afk: false,
                idle_time_ms: 0,
            },
            status: crate::domain::tracking::TrackingStatusSnapshot::default(),
            sampled_at_ms,
            probe_status: crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics:
                crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeDiagnostics::default(
                ),
        }
    }

    struct TestRuntimeState {
        tracking:
            std::sync::Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        tools_ready: bool,
    }

    impl crate::engine::api::context::ApiRuntimeStateProvider for TestRuntimeState {
        fn tracking_snapshot(
            &self,
        ) -> Option<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot> {
            self.tracking.snapshot()
        }

        fn web_activity_snapshot(
            &self,
            _settings: &crate::domain::settings::WebActivitySettings,
            _now_ms: i64,
        ) -> Option<crate::domain::web_activity::WebActivityBridgeSnapshot> {
            None
        }

        fn tools_runtime_ready(&self) -> bool {
            self.tools_ready
        }
    }

    struct TestToolsSink;

    impl crate::engine::tools::ToolsRuntimeSink for TestToolsSink {
        fn snapshot_changed(&self, _snapshot: &crate::domain::tools::ToolsRuntimeSnapshot) {}

        fn alert(&self, _alert: &crate::domain::tools::ToolAlert) {}
    }

    struct TestApiRuntime {
        root: std::path::PathBuf,
        pool: sqlx::SqlitePool,
        handle: crate::engine::api::server::ApiServerHandle,
        event_hub: std::sync::Arc<crate::engine::runtime_event::RuntimeEventHub>,
        tracking_state: Option<
            std::sync::Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        >,
        port: u16,
    }

    impl TestApiRuntime {
        async fn start(surface: ApiSurface) -> Self {
            Self::start_with_tracking(surface, false).await
        }

        async fn start_tracking() -> Self {
            Self::start_with_tracking(ApiSurface::DaemonTracking, true).await
        }

        async fn start_with_tracking(surface: ApiSurface, tracking: bool) -> Self {
            let root = std::env::temp_dir().join(format!(
                "patina-daemon-client-{}-{}",
                std::process::id(),
                TEST_RUNTIME_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
                &root.join("patina.db"),
                true,
            )
            .await
            .unwrap();
            let event_hub =
                std::sync::Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(
                    crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
                ));
            let tracking_state = tracking.then(|| {
                let state = std::sync::Arc::new(
                    crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default(
                    ),
                );
                state.replace(tracking_snapshot("ghostty", 1_000));
                state
            });
            if tracking {
                crate::data::repositories::sessions::start_session(
                    &pool, "Ghostty", "ghostty", "Window", 900, 900,
                )
                .await
                .unwrap();
            }
            let context = if tracking {
                let event_sink: std::sync::Arc<dyn crate::engine::runtime_event::RuntimeEventSink> =
                    event_hub.clone();
                let context = crate::engine::api::context::ApiRuntimeContext::with_state_and_events(
                    crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::sync::Arc::new(TestRuntimeState {
                        tracking: tracking_state.clone().unwrap(),
                        tools_ready: true,
                    }),
                    Some(event_sink),
                );
                let tools_owner =
                    std::sync::Arc::new(crate::engine::tools::ToolsRuntimeOwner::new(
                        crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
                        std::sync::Arc::new(TestToolsSink),
                    ));
                let context = context.with_tools_owner(tools_owner);
                let import_owner = std::sync::Arc::new(
                    crate::app::daemon::activity_import::DaemonActivityImportOwner::new(
                        crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
                        root.join("activity-import-staging"),
                    ),
                );
                context.with_activity_import_owner(import_owner)
            } else {
                crate::engine::api::context::ApiRuntimeContext::new(
                    crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
                )
            };
            let credentials = crate::engine::api::auth::ApiCredentialStore::new();
            credentials
                .initialize_at(&root.join("api_token"), Some(TEST_TOKEN))
                .unwrap();
            let server = if surface.has_event_stream() {
                crate::engine::api::server::prepare_standalone_server_with_events(
                    0,
                    credentials,
                    context,
                    surface,
                    event_hub.clone(),
                )
                .await
                .unwrap()
            } else {
                crate::engine::api::server::prepare_standalone_server(
                    0,
                    credentials,
                    context,
                    surface,
                )
                .await
                .unwrap()
            };
            let port = server.port();
            let handle = server.start();
            Self {
                root,
                pool,
                handle,
                event_hub,
                tracking_state,
                port,
            }
        }

        fn replace_tracking_snapshot(
            &self,
            snapshot: crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot,
        ) {
            self.tracking_state.as_ref().unwrap().replace(snapshot);
        }

        async fn replace_active_session(&self, app_name: &str, exe_name: &str, start_time: i64) {
            crate::data::repositories::sessions::start_session(
                &self.pool, app_name, exe_name, "Window", start_time, start_time,
            )
            .await
            .unwrap();
        }

        async fn shutdown(self) {
            self.handle.shutdown().await;
            self.pool.close().await;
            std::fs::remove_dir_all(self.root).unwrap();
        }
    }
}
