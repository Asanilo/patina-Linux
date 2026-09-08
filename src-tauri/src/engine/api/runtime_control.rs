use crate::domain::settings::WebActivityUrlPrivacyMode;
use std::future::Future;
use std::pin::Pin;

const MAX_WEB_ACTIVITY_TOKEN_LEN: usize = 512;

pub type RuntimeControlFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RuntimeControlError>> + Send + 'a>>;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BrowserActivityRuntimeConfiguration {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub url_privacy: WebActivityUrlPrivacyMode,
}

pub fn validate_browser_activity_configuration(
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct LocalApiRuntimeSnapshot {
    pub port: u16,
    pub base_url: String,
    pub token_path: String,
    pub token_present: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct LocalApiPortApplyResult {
    pub configuration: LocalApiRuntimeSnapshot,
    pub previous_port: u16,
    pub reconnect_required: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct LocalApiTokenRotationResult {
    pub configuration: LocalApiRuntimeSnapshot,
    pub reauthentication_required: bool,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct DaemonServiceRestartSnapshot {
    pub request_id: String,
    pub status: String,
    pub requested_at_ms: i64,
    pub requested_instance_id: String,
    pub completed_at_ms: Option<i64>,
    pub completed_instance_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct DaemonServiceRuntimeSnapshot {
    pub service_name: String,
    pub managed_by_systemd: bool,
    pub instance_id: String,
    pub restart: Option<DaemonServiceRestartSnapshot>,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct DaemonServiceRestartResult {
    pub service: DaemonServiceRuntimeSnapshot,
    pub reconnect_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeControlError {
    InvalidInput(String),
    Conflict(String),
    Internal(String),
}

pub trait ApiRuntimeControl: Send + Sync {
    fn daemon_service_managed(&self) -> bool;

    fn daemon_service_snapshot(&self) -> RuntimeControlFuture<'_, DaemonServiceRuntimeSnapshot>;

    fn request_daemon_service_restart(
        &self,
    ) -> RuntimeControlFuture<'_, DaemonServiceRestartResult>;

    fn set_audio_participation_enabled(&self, enabled: bool) -> RuntimeControlFuture<'_, bool>;

    fn configure_browser_activity(
        &self,
        configuration: BrowserActivityRuntimeConfiguration,
    ) -> RuntimeControlFuture<'_, BrowserActivityRuntimeConfiguration>;

    fn local_api_snapshot(&self) -> RuntimeControlFuture<'_, LocalApiRuntimeSnapshot>;

    fn apply_local_api_port(
        &self,
        context: crate::engine::api::context::ApiRuntimeContext,
        port: u16,
    ) -> RuntimeControlFuture<'_, LocalApiPortApplyResult>;

    fn rotate_local_api_token(&self) -> RuntimeControlFuture<'_, LocalApiTokenRotationResult>;
}
