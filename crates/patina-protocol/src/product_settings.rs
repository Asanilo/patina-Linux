//! Shared product policy. Client presentation preferences and secrets do not
//! belong in this snapshot. Revision describes settings, never heartbeat time.
use serde::{Deserialize, Serialize};

pub const MAX_PRODUCT_SETTINGS_RESPONSE_BYTES: usize = 8192;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductSettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline_merge_gap_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_session_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_paused: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductSettingsCommitRequest {
    pub expected_revision: String,
    pub patch: ProductSettingsPatch,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductSettings {
    pub idle_timeout_secs: u64,
    pub timeline_merge_gap_secs: u64,
    pub min_session_secs: u64,
    pub tracking_paused: bool,
    pub audio_participation_enabled: bool,
    pub web_activity_enabled: bool,
    pub web_activity_port: u16,
    pub web_activity_token_present: bool,
    pub web_activity_url_privacy: crate::web_history::WebActivityUrlPrivacyMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductSettingsSnapshot {
    pub revision: String,
    pub sampled_at_ms: i64,
    pub settings: ProductSettings,
    pub last_heartbeat_ms: Option<i64>,
    pub last_successful_sample_ms: Option<i64>,
}
