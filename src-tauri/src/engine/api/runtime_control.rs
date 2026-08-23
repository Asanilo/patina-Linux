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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeControlError {
    InvalidInput(String),
    Conflict(String),
    Internal(String),
}

pub trait ApiRuntimeControl: Send + Sync {
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
