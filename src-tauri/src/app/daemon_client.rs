use serde::Serialize;

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
    use crate::platform::daemon_client::{PatinadClient, PatinadClientError};

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

        assert_eq!(negotiated.protocol_version, 1);
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

    struct TestApiRuntime {
        root: std::path::PathBuf,
        pool: sqlx::SqlitePool,
        handle: crate::engine::api::server::ApiServerHandle,
        port: u16,
    }

    impl TestApiRuntime {
        async fn start(surface: ApiSurface) -> Self {
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
            let context = crate::engine::api::context::ApiRuntimeContext::new(
                crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
            );
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
                    std::sync::Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(
                        crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
                    )),
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
                port,
            }
        }

        async fn shutdown(self) {
            self.handle.shutdown().await;
            self.pool.close().await;
            std::fs::remove_dir_all(self.root).unwrap();
        }
    }
}
