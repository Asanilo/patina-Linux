use super::{
    map_browser_apply_error, validate_browser_activity_configuration,
    BrowserActivityRuntimeConfiguration, DaemonApiRuntimeControl, RuntimeControlError,
    STORAGE_ERROR_PREFIX,
};
use crate::data::repositories::resource_settings as repository;
use patina_protocol::resource_settings::{ResourceSettingsCommitRequest, ResourceSettingsSnapshot};

const CONFLICT: &str = "resource-settings-conflict: settings changed since they were read";
#[cfg(all(test, target_os = "linux"))]
mod tests;

impl DaemonApiRuntimeControl {
    pub(super) async fn apply_resource_patch(
        &self,
        request: ResourceSettingsCommitRequest,
    ) -> Result<ResourceSettingsSnapshot, RuntimeControlError> {
        if !patina_protocol::configuration::is_revision(&request.expected_revision) {
            return Err(RuntimeControlError::InvalidInput(
                "invalid resource settings revision".into(),
            ));
        }
        #[cfg(not(target_os = "linux"))]
        if request.patch.audio_participation_enabled.is_some() {
            return Err(RuntimeControlError::InvalidInput(
                "audio participation is only supported on Linux".into(),
            ));
        }
        let current = repository::load_state(self.context.pool(), self.context.now_ms())
            .await
            .map_err(RuntimeControlError::Internal)?;
        if current.snapshot.revision != request.expected_revision {
            return Err(RuntimeControlError::Conflict(CONFLICT.into()));
        }
        let mut browser = current.browser;
        let mut privacy = current.snapshot.browser_activity.url_privacy;
        if let Some(patch) = request.patch.browser_activity.as_ref() {
            if let Some(enabled) = patch.enabled {
                browser.enabled = enabled;
            }
            if let Some(port) = patch.port {
                browser.port = port;
            }
            if let Some(token) = patch.token.as_ref() {
                browser.token = token.trim().into();
            }
            if let Some(value) = patch.url_privacy {
                privacy = value;
            }
        }
        let changes_browser = request.patch.changes_browser();
        if changes_browser {
            validate_browser_activity_configuration(&BrowserActivityRuntimeConfiguration {
                enabled: browser.enabled,
                port: browser.port,
                token: browser.token.clone(),
                url_privacy: privacy,
            })?;
        }
        let mut changed_at_ms = self.context.now_ms();
        let result = if changes_browser {
            let mut committed = None;
            self.web_activity
                .apply_with_commit(browser.clone(), || async {
                    changed_at_ms = self.context.now_ms();
                    committed = Some(
                        repository::commit(
                            self.context.pool(),
                            &request.expected_revision,
                            request.patch.audio_participation_enabled,
                            Some((&browser, privacy)),
                            changed_at_ms,
                        )
                        .await
                        .map_err(|error| match error {
                            repository::CommitError::Conflict => CONFLICT.into(),
                            repository::CommitError::Storage(message) => {
                                format!("{STORAGE_ERROR_PREFIX}{message}")
                            }
                        })?,
                    );
                    Ok(())
                })
                .await
                .map_err(|error| {
                    if error == CONFLICT {
                        RuntimeControlError::Conflict(error)
                    } else {
                        map_browser_apply_error(error)
                    }
                })?;
            committed.ok_or_else(|| {
                RuntimeControlError::Internal("resource commit result is missing".into())
            })?
        } else {
            repository::commit(
                self.context.pool(),
                &request.expected_revision,
                request.patch.audio_participation_enabled,
                None,
                changed_at_ms,
            )
            .await
            .map_err(|error| match error {
                repository::CommitError::Conflict => RuntimeControlError::Conflict(CONFLICT.into()),
                repository::CommitError::Storage(message) => RuntimeControlError::Internal(message),
            })?
        };
        #[cfg(target_os = "linux")]
        if let Some(enabled) = request.patch.audio_participation_enabled {
            self.audio_source.set_enabled(enabled);
        }
        if result.sealed {
            let _ = self.event_sink.emit(
                crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                    reason: crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON.into(),
                    changed_at_ms: changed_at_ms.max(0) as u64,
                },
            );
        }
        if !request.patch.is_empty() {
            self.emit_settings_changed();
        }
        Ok(result.snapshot)
    }
}
