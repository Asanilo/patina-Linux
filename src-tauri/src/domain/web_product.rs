//! Browser product metadata and trusted live boundaries; no I/O or client clocks.
use super::activity_read_policy::trim_js;
use patina_protocol::{
    activity::{ActivityReadHealth, ActivityReadStatus},
    configuration::ClassificationEntry,
    web_history::{WebActivityUrlPrivacyMode, BROWSER_BRIDGE_STALE_AFTER_MS},
};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct WebDomainMetadata {
    pub category: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    pub recording_enabled: bool,
}
impl Default for WebDomainMetadata {
    fn default() -> Self {
        Self {
            category: "other".into(),
            display_name: None,
            color: None,
            recording_enabled: true,
        }
    }
}
#[derive(Default)]
pub struct WebProductPolicy {
    overrides: HashMap<String, WebDomainMetadata>,
}
impl WebProductPolicy {
    pub fn from_entries(entries: &[ClassificationEntry], language: &str) -> Self {
        let mut policy = Self::default();
        for entry in entries {
            let Some(key) = entry
                .key
                .strip_prefix(super::web_activity::WEB_DOMAIN_OVERRIDE_KEY_PREFIX)
                .and_then(super::web_activity::normalize_domain)
            else {
                continue;
            };
            let Ok(Value::Object(value)) = serde_json::from_str(&entry.value) else {
                continue;
            };
            // Recording state has its own runtime contract. Malformed display
            // metadata must not make a disabled domain appear enabled.
            let recording_enabled = value.get("enabled") != Some(&Value::Bool(false));
            policy.overrides.insert(
                key.clone(),
                WebDomainMetadata {
                    recording_enabled,
                    ..WebDomainMetadata::default()
                },
            );
            if ["category", "displayName", "color"].iter().any(|key| {
                value
                    .get(*key)
                    .is_some_and(|v| !v.is_null() && !v.is_string())
            }) {
                continue;
            }
            let Ok(category) = super::product_classification::normalize_category(
                value.get("category").and_then(Value::as_str).unwrap_or(""),
                language,
            ) else {
                continue;
            };
            let name = value
                .get("displayName")
                .and_then(Value::as_str)
                .map(trim_js)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let color = value
                .get("color")
                .and_then(Value::as_str)
                .map(trim_js)
                .map(|s| s.strip_prefix('#').unwrap_or(s))
                .filter(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(|s| format!("#{}", s.to_ascii_uppercase()));
            policy.overrides.insert(
                key,
                WebDomainMetadata {
                    category: category.unwrap_or_else(|| "other".into()),
                    display_name: name,
                    color,
                    recording_enabled,
                },
            );
        }
        policy
    }
    pub fn metadata(&self, domain: &str) -> WebDomainMetadata {
        self.overrides.get(domain).cloned().unwrap_or_default()
    }
}

pub fn apply_url_privacy(url: Option<String>, mode: WebActivityUrlPrivacyMode) -> Option<String> {
    match mode {
        WebActivityUrlPrivacyMode::Full => url,
        WebActivityUrlPrivacyMode::DomainOnly => None,
        WebActivityUrlPrivacyMode::StripQuery => {
            url.map(|url| url.split(['?', '#']).next().unwrap_or("").to_owned())
        }
    }
}

/// A known closed parent always bounds the child. Legacy unlinked open rows
/// cannot extrapolate beyond their last persisted browser observation.
pub fn confirmed_end(
    start: i64,
    end: Option<i64>,
    observed: i64,
    native: Option<Option<i64>>,
    health: &ActivityReadHealth,
    now: i64,
) -> Option<(i64, bool)> {
    if let Some(end) = end {
        let end = native.flatten().map_or(end, |parent| end.min(parent));
        return (end > start).then_some((end, false));
    }
    if observed < start
        || observed <= 0
        || observed > now.saturating_add(super::activity_read_health::HEARTBEAT_STALE_AFTER_MS)
        || health.live_cutoff_ms <= 0
    {
        return None;
    }
    let observed = observed.min(now);
    let browser_fresh = now - observed <= BROWSER_BRIDGE_STALE_AFTER_MS;
    let mut cutoff = health.live_cutoff_ms;
    if !browser_fresh || native.is_none() {
        cutoff = cutoff.min(observed);
    }
    if let Some(Some(parent_end)) = native {
        cutoff = cutoff.min(parent_end);
    }
    let live = native == Some(None)
        && browser_fresh
        && health.status == ActivityReadStatus::Healthy
        && cutoff == now;
    (cutoff > start).then_some((cutoff, live))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_normalizes_valid_values_without_changing_recording_state() {
        let entries = vec![ClassificationEntry {key:"__web_domain_override::Example.COM".into(),value:r###"{"category":"custom:Deep%20Work","displayName":"  Research  ","color":"##aabbcc","enabled":false}"###.into()},
            ClassificationEntry {key:"__web_domain_override::invalid".into(),value:r#"{"displayName":17,"enabled":false}"#.into()}];
        let policy = WebProductPolicy::from_entries(&entries, "en-US");
        let value = policy.metadata("example.com");
        assert_eq!(value.category, "custom:Deep%20Work");
        assert_eq!(value.display_name.as_deref(), Some("Research"));
        assert!(value.color.is_none());
        assert!(!value.recording_enabled);
        assert!(!policy.metadata("invalid").recording_enabled);
        assert_eq!(policy.metadata("unknown").category, "other");
    }
    #[test]
    fn browser_and_native_evidence_bound_open_records() {
        let healthy = super::super::activity_read_health::resolve_read_health(Some(90000), 90000);
        assert_eq!(
            confirmed_end(1000, None, 20000, Some(None), &healthy, 90000),
            Some((90000, true))
        );
        assert_eq!(
            confirmed_end(1000, None, 10000, Some(None), &healthy, 90000),
            Some((10000, false))
        );
        assert_eq!(
            confirmed_end(1000, None, 20000, None, &healthy, 90000),
            Some((20000, false))
        );
        assert_eq!(
            confirmed_end(1000, None, 20000, Some(Some(30000)), &healthy, 90000),
            Some((30000, false))
        );
        assert_eq!(
            confirmed_end(1000, None, 100000, Some(None), &healthy, 90000),
            None
        );
        let stale = super::super::activity_read_health::resolve_read_health(Some(50000), 90000);
        assert_eq!(
            confirmed_end(1000, None, 20000, Some(None), &stale, 90000),
            Some((50000, false))
        );
        let absent = super::super::activity_read_health::resolve_read_health(None, 90000);
        assert_eq!(
            confirmed_end(1000, None, 20000, Some(None), &absent, 90000),
            None
        );
        assert_eq!(
            confirmed_end(1000, Some(2000), 0, None, &absent, 90000),
            Some((2000, false))
        );
    }
}
