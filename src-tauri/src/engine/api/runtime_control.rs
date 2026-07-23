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
}
