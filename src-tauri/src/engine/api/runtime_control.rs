use crate::domain::settings::WebActivityUrlPrivacyMode;
use std::future::Future;
use std::pin::Pin;

pub type RuntimeControlFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RuntimeControlError>> + Send + 'a>>;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BrowserActivityRuntimeConfiguration {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub url_privacy: WebActivityUrlPrivacyMode,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct LocalApiRuntimeSnapshot {
    pub port: u16,
    pub base_url: String,
    pub token_path: String,
    pub token_present: bool,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct LocalApiPortApplyResult {
    pub configuration: LocalApiRuntimeSnapshot,
    pub previous_port: u16,
    pub reconnect_required: bool,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
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
