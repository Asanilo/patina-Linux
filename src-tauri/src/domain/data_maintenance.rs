use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrackingDataCleanupResult {
    pub title_samples_deleted: u64,
    pub sessions_deleted: u64,
    pub web_activity_segments_deleted: u64,
    pub imported_exact_sessions_deleted: u64,
    pub imported_time_buckets_deleted: u64,
    pub import_batches_deleted: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WindowTitleCleanupResult {
    pub title_samples_deleted: u64,
    pub sessions_redacted: u64,
    pub imported_exact_sessions_redacted: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppTrackingDataCleanupResult {
    pub sessions_deleted: u64,
    pub imported_exact_sessions_deleted: u64,
    pub imported_time_buckets_deleted: u64,
    pub import_batches_deleted: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WebDomainCleanupResult {
    pub web_activity_segments_deleted: u64,
}

pub fn normalize_web_domain_cleanup(domain: &str) -> Result<String, String> {
    let domain = crate::domain::web_activity::normalize_domain(domain)
        .ok_or_else(|| "web history cleanup requires an exact domain".to_string())?;
    if domain.len() > 253 || domain.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("web history cleanup contains an invalid domain".to_string());
    }
    Ok(domain)
}

pub fn validate_app_tracking_data_cleanup(
    exe_names: &[String],
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
) -> Result<(), String> {
    if exe_names.is_empty() || exe_names.len() > 512 {
        return Err("application cleanup requires between 1 and 512 executable names".to_string());
    }
    if exe_names.iter().any(|value| {
        let value = value.trim();
        value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
    }) {
        return Err("application cleanup contains an invalid executable name".to_string());
    }
    match (start_time_ms, end_time_ms) {
        (None, None) => Ok(()),
        (Some(start), Some(end)) if start >= 0 && end > start => Ok(()),
        _ => Err("application cleanup requires a valid complete time range".to_string()),
    }
}
