//! Conditional changes to daemon-owned audio and browser resources. Secrets are
//! accepted only in explicit patches and never returned in a snapshot.
use crate::web_history::WebActivityUrlPrivacyMode;
use serde::{Deserialize, Serialize};

pub const MAX_RESOURCE_SETTINGS_RESPONSE_BYTES: usize = 8192;

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserResourceSettings {
    pub enabled: bool,
    pub port: u16,
    pub token_present: bool,
    pub url_privacy: WebActivityUrlPrivacyMode,
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceSettingsSnapshot {
    pub revision: String,
    pub sampled_at_ms: i64,
    pub audio_participation_enabled: bool,
    pub browser_activity: BrowserResourceSettings,
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserResourcePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_privacy: Option<WebActivityUrlPrivacyMode>,
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_participation_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_activity: Option<BrowserResourcePatch>,
}

impl ResourceSettingsPatch {
    pub fn changes_browser(&self) -> bool {
        self.browser_activity
            .as_ref()
            .is_some_and(|patch| *patch != BrowserResourcePatch::default())
    }
    pub fn is_empty(&self) -> bool {
        self.audio_participation_enabled.is_none() && !self.changes_browser()
    }
}

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSettingsCommitRequest {
    pub expected_revision: String,
    pub patch: ResourceSettingsPatch,
}
