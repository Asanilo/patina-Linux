//! Precise browser activity. Product clients consume resolved boundaries and metadata.
use crate::activity::ActivityReadHealth;
use serde::{Deserialize, Serialize};

pub const MAX_WEB_HISTORY_FACTS: usize = 20_000;
pub const MAX_WEB_HISTORY_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_WEB_HISTORY_NAME_BYTES: usize = 1024;
pub const MAX_WEB_HISTORY_TITLE_BYTES: usize = 16_384;
pub const MAX_WEB_HISTORY_URL_BYTES: usize = 65_536;
pub const MAX_WEB_HISTORY_ICON_BYTES: usize = 32_768;
pub const BROWSER_BRIDGE_STALE_AFTER_MS: i64 = 75_000;

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebActivityUrlPrivacyMode {
    #[default]
    Full,
    StripQuery,
    DomainOnly,
}
impl WebActivityUrlPrivacyMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::StripQuery => "strip_query",
            Self::DomainOnly => "domain_only",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebHistorySnapshot {
    pub from_ms: i64,
    pub to_ms: i64,
    pub sampled_at_ms: i64,
    pub classification_revision: String,
    pub tracking_health: ActivityReadHealth,
    pub url_privacy: WebActivityUrlPrivacyMode,
    pub records: Vec<WebHistoryRecord>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebHistoryRecord {
    pub record_id: i64,
    pub browser_client_id: String,
    pub browser_kind: String,
    pub browser_exe_name: String,
    pub domain: String,
    pub normalized_domain: String,
    pub category: String,
    pub display_name_override: Option<String>,
    pub color_override: Option<String>,
    pub recording_enabled: bool,
    pub url: Option<String>,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub is_open: bool,
    pub is_live: bool,
}
