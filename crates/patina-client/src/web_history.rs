use crate::{Client, ClientError};
use patina_protocol::{
    activity::{is_product_category, ActivityReadStatus},
    configuration::is_revision,
    history::{valid_range, MAX_SAFE_TIMESTAMP},
    web_history::*,
};
use std::{
    collections::{HashMap, HashSet},
};
impl Client {
    pub async fn web_history(
        &self,
        from_ms: i64,
        to_ms: i64,
        language: &str,
    ) -> Result<WebHistorySnapshot, ClientError> {
        if !valid_range(from_ms, to_ms) || !matches!(language, "en-US" | "zh-CN") {
            return Err(ClientError::InvalidConfiguration(
                "invalid web history range or language".into(),
            ));
        }
        let read:WebHistorySnapshot=self.get_json_with_limits(
            &format!("/api/v1/activity/web-history?from_ms={from_ms}&to_ms={to_ms}&language={language}"),
            "web history",patina_protocol::read_budget::WEB_HISTORY.client,MAX_WEB_HISTORY_RESPONSE_BYTES).await?;
        validate(&read, from_ms, to_ms)?;
        Ok(read)
    }
}
fn validate(read: &WebHistorySnapshot, from: i64, to: i64) -> Result<(), ClientError> {
    let error = || ClientError::InvalidResponse("invalid web history snapshot".into());
    if read.from_ms != from
        || read.to_ms != to
        || !(0..=MAX_SAFE_TIMESTAMP).contains(&read.sampled_at_ms)
        || !is_revision(&read.classification_revision)
        || !read.tracking_health.is_valid_at(read.sampled_at_ms)
        || read.records.len() > MAX_WEB_HISTORY_FACTS
    {
        return Err(error());
    }
    let mut seen = HashSet::new();
    let mut previous = None;
    let mut ends = HashMap::new();
    let mut domains = HashMap::new();
    for r in &read.records {
        let order = (r.start_ms, r.record_id, r.end_ms);
        let bounded =
            |value: &Option<String>, limit| value.as_ref().is_none_or(|v| v.len() <= limit);
        if !(1..=MAX_SAFE_TIMESTAMP).contains(&r.record_id)
            || !seen.insert(r.record_id)
            || previous.is_some_and(|last| order < last)
            || r.start_ms < from
            || r.end_ms > to
            || r.end_ms <= r.start_ms
            || [
                &r.browser_client_id,
                &r.browser_kind,
                &r.browser_exe_name,
                &r.domain,
                &r.normalized_domain,
            ]
            .iter()
            .any(|s| s.is_empty() || s.len() > MAX_WEB_HISTORY_NAME_BYTES)
            || !is_product_category(&r.category)
            || !bounded(&r.display_name_override, 4096)
            || r.display_name_override
                .as_ref()
                .is_some_and(|s| s.trim().is_empty())
            || r.color_override.as_ref().is_some_and(|s| {
                s.len() != 7
                    || !s.starts_with('#')
                    || !s[1..].bytes().all(|b| b.is_ascii_hexdigit())
            })
            || !bounded(&r.url, MAX_WEB_HISTORY_URL_BYTES)
            || !bounded(&r.title, MAX_WEB_HISTORY_TITLE_BYTES)
            || !bounded(&r.favicon_url, MAX_WEB_HISTORY_ICON_BYTES)
            || (r.is_open
                && (read.tracking_health.status == ActivityReadStatus::Unavailable
                    || r.end_ms > read.tracking_health.live_cutoff_ms))
            || (r.is_live
                && (!r.is_open
                    || read.tracking_health.status != ActivityReadStatus::Healthy
                    || r.end_ms != read.sampled_at_ms))
            || (read.url_privacy == WebActivityUrlPrivacyMode::DomainOnly && r.url.is_some())
            || (read.url_privacy == WebActivityUrlPrivacyMode::StripQuery
                && r.url.as_ref().is_some_and(|s| s.contains(['?', '#'])))
        {
            return Err(error());
        }
        let metadata = (
            &r.category,
            &r.display_name_override,
            &r.color_override,
            r.recording_enabled,
        );
        if domains
            .insert(&r.normalized_domain, metadata)
            .is_some_and(|prior| prior != metadata)
        {
            return Err(error());
        }
        let key = (
            &r.browser_client_id,
            &r.browser_kind,
            &r.browser_exe_name,
            &r.normalized_domain,
        );
        if ends
            .insert(key, r.end_ms)
            .is_some_and(|end| r.start_ms < end)
        {
            return Err(error());
        }
        previous = Some(order);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use patina_protocol::activity::ActivityReadHealth;
    fn fixture() -> WebHistorySnapshot {
        WebHistorySnapshot {
            from_ms: 0,
            to_ms: 1000,
            sampled_at_ms: 1000,
            classification_revision: "a".repeat(64),
            tracking_health: ActivityReadHealth {
                status: ActivityReadStatus::Healthy,
                last_heartbeat_ms: Some(1000),
                live_cutoff_ms: 1000,
                stale_after_ms: 8000,
            },
            url_privacy: WebActivityUrlPrivacyMode::StripQuery,
            records: vec![WebHistoryRecord {
                record_id: 1,
                browser_client_id: "one".into(),
                browser_kind: "firefox".into(),
                browser_exe_name: "firefox".into(),
                domain: "example.com".into(),
                normalized_domain: "example.com".into(),
                category: "development".into(),
                display_name_override: None,
                color_override: None,
                recording_enabled: true,
                url: Some("https://example.com/path".into()),
                title: Some("Title".into()),
                favicon_url: None,
                start_ms: 10,
                end_ms: 20,
                is_open: false,
                is_live: false,
            }],
        }
    }
    #[test]
    fn validates_web_snapshot_scope_privacy_source_overlap_and_live_boundary() {
        let good = fixture();
        assert!(validate(&good, 0, 1000).is_ok());
        for mutate in [
            |v: &mut WebHistorySnapshot| v.from_ms += 1,
            |v: &mut WebHistorySnapshot| v.records[0].record_id = 0,
            |v: &mut WebHistorySnapshot| {
                v.records[0].url = Some("https://example.com?secret".into())
            },
            |v: &mut WebHistorySnapshot| v.url_privacy = WebActivityUrlPrivacyMode::DomainOnly,
            |v: &mut WebHistorySnapshot| v.records[0].is_live = true,
            |v: &mut WebHistorySnapshot| v.records[0].title = Some("中".repeat(6000)),
            |v: &mut WebHistorySnapshot| {
                v.records.push(WebHistoryRecord {
                    record_id: 2,
                    ..v.records[0].clone()
                })
            },
            |v: &mut WebHistorySnapshot| v.records.push(v.records[0].clone()),
        ] {
            let mut invalid = good.clone();
            mutate(&mut invalid);
            assert!(validate(&invalid, 0, 1000).is_err());
        }
        let mut separate = good.clone();
        separate.records.push(WebHistoryRecord {
            record_id: 2,
            browser_client_id: "other".into(),
            ..good.records[0].clone()
        });
        assert!(validate(&separate, 0, 1000).is_ok());
        separate.records[1].category = "music".into();
        assert!(validate(&separate, 0, 1000).is_err());
    }
}
