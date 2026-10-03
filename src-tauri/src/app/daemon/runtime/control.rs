use super::web_activity::DaemonWebActivityControl;
use crate::engine::api::runtime_control::{
    validate_browser_activity_configuration, ApiRuntimeControl,
    BrowserActivityRuntimeConfiguration, DaemonServiceRestartResult, DaemonServiceRuntimeSnapshot,
    LocalApiPortApplyResult, LocalApiRuntimeSnapshot, LocalApiTokenRotationResult,
    RuntimeControlError, RuntimeControlFuture,
};
use std::sync::Arc;

const STORAGE_ERROR_PREFIX: &str = "storage:";

#[derive(Clone)]
pub(crate) struct DaemonApiRuntimeControl {
    resources: Arc<super::resource_operations::ResourceOperations>,
    context: crate::engine::runtime_context::RuntimeContext,
    web_activity: DaemonWebActivityControl,
    event_sink: std::sync::Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    api_listener: Arc<crate::engine::api::listener_owner::LocalApiListenerOwner>,
    api_credentials: crate::engine::api::auth::ApiCredentialStore,
    service_lifecycle: Arc<crate::app::daemon::service_lifecycle::DaemonServiceLifecycleOwner>,
    #[cfg(target_os = "linux")]
    audio_source: crate::platform::linux::audio::AudioSignalSource,
}

impl DaemonApiRuntimeControl {
    pub(crate) fn new(
        context: crate::engine::runtime_context::RuntimeContext,
        web_activity: DaemonWebActivityControl,
        event_sink: std::sync::Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
        api_listener: Arc<crate::engine::api::listener_owner::LocalApiListenerOwner>,
        api_credentials: crate::engine::api::auth::ApiCredentialStore,
        service_lifecycle: Arc<crate::app::daemon::service_lifecycle::DaemonServiceLifecycleOwner>,
        #[cfg(target_os = "linux")] audio_source: crate::platform::linux::audio::AudioSignalSource,
    ) -> Self {
        Self {
            resources: Arc::new(super::resource_operations::ResourceOperations::default()),
            context,
            web_activity,
            event_sink,
            api_listener,
            api_credentials,
            service_lifecycle,
            #[cfg(target_os = "linux")]
            audio_source,
        }
    }

    pub(super) async fn close_and_drain_resources(&self) {
        self.resources.close_and_drain().await;
    }

    fn emit_settings_changed(&self) {
        let _ = self.event_sink.emit(
            crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                reason: crate::domain::tracking::TRACKING_REASON_RUNTIME_SETTINGS_CHANGED
                    .to_string(),
                changed_at_ms: self.context.now_ms().max(0) as u64,
            },
        );
    }

    async fn apply_browser_configuration(
        &self,
        mut configuration: BrowserActivityRuntimeConfiguration,
    ) -> Result<BrowserActivityRuntimeConfiguration, RuntimeControlError> {
        configuration.token = configuration.token.trim().to_string();
        validate_browser_activity_configuration(&configuration)?;
        let bridge_settings = crate::domain::settings::WebActivityBridgeSettings {
            enabled: configuration.enabled,
            port: configuration.port,
            token: configuration.token.clone(),
        };
        let stored_settings = bridge_settings.clone();
        let url_privacy = configuration.url_privacy;
        let pool = self.context.pool().clone();
        let mut sealed = false;
        let mut changed_at_ms = 0;
        self.web_activity
            .apply_with_commit(bridge_settings, || async {
                changed_at_ms = self.context.now_ms();
                sealed =
                    crate::data::repositories::app_settings::save_web_activity_runtime_settings_at(
                        &pool,
                        &stored_settings,
                        url_privacy,
                        changed_at_ms,
                    )
                    .await
                    .map_err(|error| format!("{STORAGE_ERROR_PREFIX}{error}"))?;
                Ok(())
            })
            .await
            .map_err(map_browser_apply_error)?;
        if sealed {
            let _ = self.event_sink.emit(
                crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                    reason: crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON.to_string(),
                    changed_at_ms: changed_at_ms.max(0) as u64,
                },
            );
        }
        self.emit_settings_changed();
        Ok(configuration)
    }

    async fn local_api_snapshot_value(
        &self,
    ) -> Result<LocalApiRuntimeSnapshot, RuntimeControlError> {
        let port = self.api_listener.confirmed_port().await.ok_or_else(|| {
            RuntimeControlError::Internal("local API listener is not ready".into())
        })?;
        let token_path = self
            .api_credentials
            .token_path()
            .map_err(RuntimeControlError::Internal)?;
        let token_present = !self
            .api_credentials
            .token()
            .map_err(RuntimeControlError::Internal)?
            .trim()
            .is_empty();
        Ok(LocalApiRuntimeSnapshot {
            port,
            base_url: format!("http://127.0.0.1:{port}"),
            token_path: token_path.display().to_string(),
            token_present,
        })
    }
}

impl ApiRuntimeControl for DaemonApiRuntimeControl {
    fn daemon_service_managed(&self) -> bool {
        self.service_lifecycle.managed_by_systemd()
    }

    fn daemon_service_snapshot(&self) -> RuntimeControlFuture<'_, DaemonServiceRuntimeSnapshot> {
        Box::pin(async move { Ok(self.service_lifecycle.snapshot()) })
    }

    fn request_daemon_service_restart(
        &self,
    ) -> RuntimeControlFuture<'_, DaemonServiceRestartResult> {
        Box::pin(async move {
            self.service_lifecycle
                .request_restart(self.context.now_ms())
        })
    }

    fn set_audio_participation_enabled(&self, enabled: bool) -> RuntimeControlFuture<'_, bool> {
        let control = self.clone();
        Box::pin(async move {
            let resources = control.resources.clone();
            resources
                .run(async move {
                    #[cfg(not(target_os = "linux"))]
                    return Err(RuntimeControlError::InvalidInput(
                        "audio participation is only supported on Linux".to_string(),
                    ));

                    #[cfg(target_os = "linux")]
                    {
                        crate::data::repositories::app_settings::save_audio_participation_enabled(
                            control.context.pool(),
                            enabled,
                        )
                        .await
                        .map_err(RuntimeControlError::Internal)?;
                        control.audio_source.set_enabled(enabled);
                        control.emit_settings_changed();
                        Ok(enabled)
                    }
                })
                .await
        })
    }

    fn configure_browser_activity(
        &self,
        configuration: BrowserActivityRuntimeConfiguration,
    ) -> RuntimeControlFuture<'_, BrowserActivityRuntimeConfiguration> {
        let control = self.clone();
        Box::pin(async move {
            let resources = control.resources.clone();
            resources
                .run(async move { control.apply_browser_configuration(configuration).await })
                .await
        })
    }

    fn local_api_snapshot(&self) -> RuntimeControlFuture<'_, LocalApiRuntimeSnapshot> {
        Box::pin(self.local_api_snapshot_value())
    }

    fn apply_local_api_port(
        &self,
        context: crate::engine::api::context::ApiRuntimeContext,
        port: u16,
    ) -> RuntimeControlFuture<'_, LocalApiPortApplyResult> {
        let control = self.clone();
        Box::pin(async move {
            let resources = control.resources.clone();
            resources
                .run(async move {
                    let port = crate::domain::settings::parse_local_api_port(&port.to_string())
                        .ok_or_else(|| {
                            RuntimeControlError::InvalidInput(
                                "local API port must be between 1024 and 65535".to_string(),
                            )
                        })?;
                    let previous_port =
                        control.api_listener.confirmed_port().await.ok_or_else(|| {
                            RuntimeControlError::Internal("local API listener is not ready".into())
                        })?;
                    let pool = control.context.pool().clone();
                    control
                        .api_listener
                        .apply_port_with_commit(port, context, move |confirmed_port| async move {
                            crate::data::repositories::app_settings::save_local_api_port(
                                &pool,
                                confirmed_port,
                            )
                            .await
                            .map_err(|error| format!("{STORAGE_ERROR_PREFIX}{error}"))
                        })
                        .await
                        .map_err(map_local_api_apply_error)?;
                    let configuration = control.local_api_snapshot_value().await?;
                    Ok(LocalApiPortApplyResult {
                        reconnect_required: configuration.port != previous_port,
                        previous_port,
                        configuration,
                    })
                })
                .await
        })
    }

    fn rotate_local_api_token(&self) -> RuntimeControlFuture<'_, LocalApiTokenRotationResult> {
        let control = self.clone();
        Box::pin(async move {
            let resources = control.resources.clone();
            resources
                .run(async move {
                    control
                        .api_credentials
                        .rotate()
                        .map_err(RuntimeControlError::Internal)?;
                    Ok(LocalApiTokenRotationResult {
                        configuration: control.local_api_snapshot_value().await?,
                        reauthentication_required: true,
                    })
                })
                .await
        })
    }
}

fn map_browser_apply_error(error: String) -> RuntimeControlError {
    match error.strip_prefix(STORAGE_ERROR_PREFIX) {
        Some(storage_error) => RuntimeControlError::Internal(storage_error.to_string()),
        None => RuntimeControlError::Conflict(error),
    }
}

fn map_local_api_apply_error(error: String) -> RuntimeControlError {
    match error.strip_prefix(STORAGE_ERROR_PREFIX) {
        Some(storage_error) => RuntimeControlError::Internal(storage_error.to_string()),
        None => RuntimeControlError::Conflict(error),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::engine::api::runtime_control::ApiRuntimeControl;
    use sqlx::Executor;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    async fn test_control() -> (
        sqlx::SqlitePool,
        Arc<DaemonApiRuntimeControl>,
        DaemonWebActivityControl,
        Arc<crate::engine::runtime_event::MemoryRuntimeEventSink>,
        crate::platform::linux::audio::AudioSignalSource,
        Arc<crate::engine::api::listener_owner::LocalApiListenerOwner>,
        crate::engine::api::auth::ApiCredentialStore,
        Arc<crate::app::daemon::service_lifecycle::DaemonServiceLifecycleOwner>,
        std::path::PathBuf,
    ) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::WEB_ACTIVITY_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL)
            .await
            .unwrap();
        let context = crate::engine::runtime_context::RuntimeContext::system(pool.clone());
        let tracking = Arc::new(
            crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default(),
        );
        let web_state = Arc::new(crate::engine::web_activity::WebActivityRuntimeState::default());
        let sink = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink> = sink.clone();
        let web_control =
            DaemonWebActivityControl::new(context.clone(), tracking, web_state, event_sink.clone());
        let audio_source = crate::platform::linux::audio::AudioSignalSource::new(true);
        let token_path = std::env::temp_dir().join(format!(
            "patina-daemon-runtime-control-token-{}-{}",
            std::process::id(),
            TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let api_credentials = crate::engine::api::auth::ApiCredentialStore::new();
        api_credentials
            .initialize_at(&token_path, Some("test-token"))
            .unwrap();
        let api_listener = Arc::new(
            crate::engine::api::listener_owner::LocalApiListenerOwner::new(
                api_credentials.clone(),
                crate::engine::api::surface::ApiSurface::DaemonTracking,
                Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(8)),
            ),
        );
        let service_lifecycle = Arc::new(
            crate::app::daemon::service_lifecycle::DaemonServiceLifecycleOwner::new(
                token_path.parent().unwrap(),
                false,
                1_000,
            )
            .unwrap(),
        );
        let control = Arc::new(DaemonApiRuntimeControl::new(
            context,
            web_control.clone(),
            event_sink,
            api_listener.clone(),
            api_credentials.clone(),
            service_lifecycle.clone(),
            audio_source.clone(),
        ));
        (
            pool,
            control,
            web_control,
            sink,
            audio_source,
            api_listener,
            api_credentials,
            service_lifecycle,
            token_path,
        )
    }

    fn available_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn abandoned_browser_change_finishes_before_resource_shutdown() {
        let (pool, control, web_control, sink, _, api_listener, _, _, token_path) =
            test_control().await;
        let held_connection = pool.acquire().await.unwrap();
        let port = available_port();
        let mut change = control.configure_browser_activity(BrowserActivityRuntimeConfiguration {
            enabled: true,
            port,
            token: "test-browser-token".into(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly,
        });
        // Poll until the operation is submitted, then drop only its transport waiter.
        std::future::poll_fn(|cx| {
            assert!(change.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(change);
        assert!(matches!(
            control.set_audio_participation_enabled(false).await,
            Err(RuntimeControlError::Conflict(_))
        ));
        drop(held_connection);
        control.close_and_drain_resources().await;
        let stored =
            crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
                .await
                .unwrap();
        assert!(stored.enabled);
        assert_eq!(stored.port, port);
        // Both the committed configuration and real listener survived caller cancellation.
        let connection = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        drop(connection);
        assert_eq!(sink.events().len(), 1);
        assert!(matches!(
            control.set_audio_participation_enabled(false).await,
            Err(RuntimeControlError::Conflict(_))
        ));
        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }

    #[tokio::test]
    async fn failed_browser_seal_preserves_settings_listener_and_events() {
        let (pool, control, web_control, sink, _, api_listener, _, _, token_path) =
            test_control().await;
        let configuration = BrowserActivityRuntimeConfiguration {
            enabled: true,
            port: available_port(),
            token: "browser-token".into(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
        };
        control
            .configure_browser_activity(configuration.clone())
            .await
            .unwrap();
        pool.execute("INSERT INTO web_activity_segments
            (browser_client_id, browser_kind, browser_exe_name, domain, normalized_domain,
             start_time, source, created_at, updated_at)
            VALUES ('test','chrome','chrome','example.org','example.org',1000,'browser-extension',1000,1000)")
            .await.unwrap();
        pool.execute(
            "CREATE TRIGGER reject_browser_seal BEFORE UPDATE OF end_time ON web_activity_segments
            BEGIN SELECT RAISE(ABORT, 'injected browser seal failure'); END",
        )
        .await
        .unwrap();
        let disabled = BrowserActivityRuntimeConfiguration {
            enabled: false,
            ..configuration.clone()
        };
        assert!(matches!(
            control.configure_browser_activity(disabled.clone()).await,
            Err(RuntimeControlError::Internal(_))
        ));
        let stored =
            crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
                .await
                .unwrap();
        assert!(stored.enabled);
        assert_eq!(sink.events().len(), 1);
        let connection = tokio::net::TcpStream::connect(("127.0.0.1", configuration.port))
            .await
            .unwrap();
        drop(connection);
        let records = crate::data::repositories::web_activity::query_segments(
            &pool,
            &Default::default(),
            control.context.now_ms(),
        )
        .await
        .unwrap();
        let open = records.iter().filter(|row| row.end_time.is_none()).count();
        assert_eq!(open, 1);
        pool.execute("DROP TRIGGER reject_browser_seal")
            .await
            .unwrap();
        control.configure_browser_activity(disabled).await.unwrap();
        let records = crate::data::repositories::web_activity::query_segments(
            &pool,
            &Default::default(),
            control.context.now_ms(),
        )
        .await
        .unwrap();
        let open = records.iter().filter(|row| row.end_time.is_none()).count();
        assert_eq!(open, 0);
        assert!(
            !crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
                .await
                .unwrap()
                .enabled
        );
        assert_eq!(sink.events().len(), 3);
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", configuration.port))
                .await
                .is_err()
        );
        control.close_and_drain_resources().await;
        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }

    #[test]
    fn enabled_browser_configuration_requires_a_safe_token() {
        let missing = BrowserActivityRuntimeConfiguration {
            enabled: true,
            port: 12_345,
            token: "  ".to_string(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
        };
        assert!(matches!(
            validate_browser_activity_configuration(&missing),
            Err(RuntimeControlError::InvalidInput(_))
        ));

        let control_character = BrowserActivityRuntimeConfiguration {
            token: "line\nbreak".to_string(),
            ..missing
        };
        assert!(matches!(
            validate_browser_activity_configuration(&control_character),
            Err(RuntimeControlError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn audio_setting_is_persisted_before_the_live_source_changes() {
        let (pool, control, web_control, sink, audio_source, api_listener, _, _, token_path) =
            test_control().await;

        assert!(!control
            .set_audio_participation_enabled(false)
            .await
            .unwrap());
        assert!(!audio_source.is_enabled());
        assert!(
            !crate::data::repositories::app_settings::load_audio_participation_enabled(&pool)
                .await
                .unwrap()
        );
        assert_eq!(sink.events().len(), 1);

        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }

    #[tokio::test]
    async fn typed_client_applies_runtime_settings_through_the_daemon_api() {
        let (
            pool,
            control,
            web_control,
            _sink,
            audio_source,
            api_listener,
            credentials,
            _,
            token_path,
        ) = test_control().await;
        let context = crate::engine::api::context::ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        )
        .with_runtime_control(control);
        let api_port = api_listener.start(0, context).await.unwrap();
        let client = crate::platform::daemon_client::PatinadClient::new(
            api_port,
            credentials.token().unwrap(),
        )
        .unwrap();
        let bridge_port = available_port();

        let applied = client
            .configure_browser_activity(BrowserActivityRuntimeConfiguration {
                enabled: true,
                port: bridge_port,
                token: "browser-token".to_string(),
                url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly,
            })
            .await
            .unwrap();
        assert!(applied.enabled);
        assert_eq!(applied.port, bridge_port);
        assert_eq!(
            applied.url_privacy,
            crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly
        );

        client.set_audio_participation_enabled(false).await.unwrap();
        assert!(!audio_source.is_enabled());

        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }

    #[tokio::test]
    async fn browser_port_conflict_preserves_the_live_and_persisted_configuration() {
        let (pool, control, web_control, _sink, _audio_source, api_listener, _, _, token_path) =
            test_control().await;
        let old_port = available_port();
        let original = BrowserActivityRuntimeConfiguration {
            enabled: true,
            port: old_port,
            token: " old-token ".to_string(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::StripQuery,
        };
        let applied = control.configure_browser_activity(original).await.unwrap();
        assert_eq!(applied.token, "old-token");

        let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let error = control
            .configure_browser_activity(BrowserActivityRuntimeConfiguration {
                enabled: true,
                port: occupied_port,
                token: "new-token".to_string(),
                url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly,
            })
            .await
            .unwrap_err();
        assert!(matches!(error, RuntimeControlError::Conflict(_)));

        let stored =
            crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
                .await
                .unwrap();
        assert_eq!(stored.port, old_port);
        assert_eq!(stored.token, "old-token");
        assert!(stored.enabled);

        drop(occupied);
        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }

    #[tokio::test]
    async fn local_api_port_and_token_changes_apply_to_the_live_owner() {
        let (
            pool,
            control,
            web_control,
            _sink,
            _audio_source,
            api_listener,
            credentials,
            _service_lifecycle,
            token_path,
        ) = test_control().await;
        let context = crate::engine::api::context::ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        )
        .with_runtime_control(control.clone());
        let old_port = api_listener.start(0, context.clone()).await.unwrap();
        let new_port = available_port();
        let old_token = credentials.token().unwrap();
        let old_client =
            crate::platform::daemon_client::PatinadClient::new(old_port, old_token.clone())
                .unwrap();

        let applied = old_client.apply_local_api_port(new_port).await.unwrap();
        assert_eq!(applied.previous_port, old_port);
        assert_eq!(applied.configuration.port, new_port);
        assert!(applied.reconnect_required);
        assert_eq!(
            crate::data::repositories::app_settings::load_local_api_settings(&pool)
                .await
                .unwrap()
                .port,
            new_port
        );

        let new_client =
            crate::platform::daemon_client::PatinadClient::new(new_port, old_token.clone())
                .unwrap();
        let rotated = new_client.rotate_local_api_token().await.unwrap();
        assert!(rotated.reauthentication_required);
        let new_token = credentials.token().unwrap();
        assert_ne!(new_token, old_token);
        assert!(!credentials.validate(Some(&format!("Bearer {old_token}"))));
        assert!(matches!(
            new_client.local_api_configuration().await.unwrap_err(),
            crate::platform::daemon_client::PatinadClientError::Unauthorized
        ));
        let reauthenticated =
            crate::platform::daemon_client::PatinadClient::new(new_port, new_token)
                .unwrap()
                .local_api_configuration()
                .await
                .unwrap();
        assert_eq!(reauthenticated.port, new_port);

        web_control.shutdown().await;
        api_listener.shutdown().await;
        pool.close().await;
        let _ = std::fs::remove_file(token_path);
    }
}
