use super::web_activity::DaemonWebActivityControl;
use crate::engine::api::runtime_control::{
    ApiRuntimeControl, BrowserActivityRuntimeConfiguration, RuntimeControlError,
    RuntimeControlFuture,
};

const MAX_WEB_ACTIVITY_TOKEN_LEN: usize = 512;
const STORAGE_ERROR_PREFIX: &str = "storage:";

pub(crate) struct DaemonApiRuntimeControl {
    context: crate::engine::runtime_context::RuntimeContext,
    web_activity: DaemonWebActivityControl,
    event_sink: std::sync::Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
    #[cfg(target_os = "linux")]
    audio_source: crate::platform::linux::audio::AudioSignalSource,
}

impl DaemonApiRuntimeControl {
    pub(crate) fn new(
        context: crate::engine::runtime_context::RuntimeContext,
        web_activity: DaemonWebActivityControl,
        event_sink: std::sync::Arc<dyn crate::engine::runtime_event::RuntimeEventSink>,
        #[cfg(target_os = "linux")] audio_source: crate::platform::linux::audio::AudioSignalSource,
    ) -> Self {
        Self {
            context,
            web_activity,
            event_sink,
            #[cfg(target_os = "linux")]
            audio_source,
        }
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
}

impl ApiRuntimeControl for DaemonApiRuntimeControl {
    fn set_audio_participation_enabled(&self, enabled: bool) -> RuntimeControlFuture<'_, bool> {
        Box::pin(async move {
            #[cfg(not(target_os = "linux"))]
            return Err(RuntimeControlError::InvalidInput(
                "audio participation is only supported on Linux".to_string(),
            ));

            #[cfg(target_os = "linux")]
            {
            crate::data::repositories::app_settings::save_audio_participation_enabled(
                self.context.pool(),
                enabled,
            )
            .await
            .map_err(RuntimeControlError::Internal)?;
            self.audio_source.set_enabled(enabled);
            self.emit_settings_changed();
            Ok(enabled)
            }
        })
    }

    fn configure_browser_activity(
        &self,
        mut configuration: BrowserActivityRuntimeConfiguration,
    ) -> RuntimeControlFuture<'_, BrowserActivityRuntimeConfiguration> {
        Box::pin(async move {
            configuration.token = configuration.token.trim().to_string();
            validate_browser_configuration(&configuration)?;
            let bridge_settings = crate::domain::settings::WebActivityBridgeSettings {
                enabled: configuration.enabled,
                port: configuration.port,
                token: configuration.token.clone(),
            };
            let stored_settings = bridge_settings.clone();
            let url_privacy = configuration.url_privacy;
            let pool = self.context.pool().clone();
            self.web_activity
                .apply_with_commit(bridge_settings, move || async move {
                    crate::data::repositories::app_settings::save_web_activity_runtime_settings(
                        &pool,
                        &stored_settings,
                        url_privacy,
                    )
                    .await
                    .map_err(|error| format!("{STORAGE_ERROR_PREFIX}{error}"))
                })
                .await
                .map_err(map_browser_apply_error)?;

            if !configuration.enabled {
                let changed_at_ms = self.context.now_ms();
                match crate::engine::web_activity::seal_active_segment(
                    self.context.pool(),
                    changed_at_ms,
                )
                .await
                {
                    Ok(true) => {
                        let _ = self.event_sink.emit(
                            crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                                reason: crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON
                                    .to_string(),
                                changed_at_ms: changed_at_ms.max(0) as u64,
                            },
                        );
                    }
                    Ok(false) => {}
                    Err(error) => {
                        return Err(RuntimeControlError::Internal(format!(
                            "browser activity was disabled but the active segment could not be sealed: {error}"
                        )))
                    }
                }
            }
            self.emit_settings_changed();
            Ok(configuration)
        })
    }
}

fn validate_browser_configuration(
    configuration: &BrowserActivityRuntimeConfiguration,
) -> Result<(), RuntimeControlError> {
    if crate::domain::settings::parse_web_activity_port(&configuration.port.to_string()).is_none() {
        return Err(RuntimeControlError::InvalidInput(
            "browser activity port must be between 1024 and 65535".to_string(),
        ));
    }
    let token = configuration.token.trim();
    if token.len() > MAX_WEB_ACTIVITY_TOKEN_LEN || token.chars().any(char::is_control) {
        return Err(RuntimeControlError::InvalidInput(
            "browser activity token is invalid".to_string(),
        ));
    }
    if configuration.enabled && token.is_empty() {
        return Err(RuntimeControlError::InvalidInput(
            "browser activity token is required when synchronization is enabled".to_string(),
        ));
    }
    Ok(())
}

fn map_browser_apply_error(error: String) -> RuntimeControlError {
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
    use std::sync::Arc;

    async fn test_control() -> (
        sqlx::SqlitePool,
        DaemonApiRuntimeControl,
        DaemonWebActivityControl,
        Arc<crate::engine::runtime_event::MemoryRuntimeEventSink>,
        crate::platform::linux::audio::AudioSignalSource,
    ) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
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
        let control = DaemonApiRuntimeControl::new(
            context,
            web_control.clone(),
            event_sink,
            audio_source.clone(),
        );
        (pool, control, web_control, sink, audio_source)
    }

    fn available_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
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
            validate_browser_configuration(&missing),
            Err(RuntimeControlError::InvalidInput(_))
        ));

        let control_character = BrowserActivityRuntimeConfiguration {
            token: "line\nbreak".to_string(),
            ..missing
        };
        assert!(matches!(
            validate_browser_configuration(&control_character),
            Err(RuntimeControlError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn audio_setting_is_persisted_before_the_live_source_changes() {
        let (pool, control, web_control, sink, audio_source) = test_control().await;

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
        pool.close().await;
    }

    #[tokio::test]
    async fn browser_port_conflict_preserves_the_live_and_persisted_configuration() {
        let (pool, control, web_control, _sink, _audio_source) = test_control().await;
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
        pool.close().await;
    }
}
