use serde::Serialize;

// Stage 2H.3b.2 builds this adapter before Stage 2H.3b.3 wires the explicit
// desktop client mode. Keep the preview implementation compiled and tested.
#[allow(dead_code)]
pub mod runtime;

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
            false
        }
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
                crate::engine::api::context::ApiRuntimeContext::with_state_and_events(
                    crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::sync::Arc::new(TestRuntimeState {
                        tracking: tracking_state.clone().unwrap(),
                    }),
                    Some(event_sink),
                )
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
