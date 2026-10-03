use crate::{Client, ClientError};
use patina_protocol::{
    activity::{is_product_category, ActivityReadStatus},
    configuration::is_revision,
    history::*,
};
use std::{collections::HashSet, time::Duration};

impl Client {
    pub async fn exact_history(
        &self,
        from_ms: i64,
        to_ms: i64,
        language: &str,
    ) -> Result<ExactHistorySnapshot, ClientError> {
        if !valid_range(from_ms, to_ms) || !matches!(language, "en-US" | "zh-CN") {
            return Err(ClientError::InvalidConfiguration(
                "invalid exact history range or language".into(),
            ));
        }
        let snapshot: ExactHistorySnapshot = self
            .get_json_with_limits(
                &format!(
                    "/api/v1/activity/history?from_ms={from_ms}&to_ms={to_ms}&language={language}"
                ),
                "exact history",
                Duration::from_secs(35),
                MAX_HISTORY_RESPONSE_BYTES,
            )
            .await?;
        validate(&snapshot, from_ms, to_ms)?;
        Ok(snapshot)
    }
}

fn validate(snapshot: &ExactHistorySnapshot, from_ms: i64, to_ms: i64) -> Result<(), ClientError> {
    let error = || ClientError::InvalidResponse("invalid exact history snapshot".into());
    if snapshot.from_ms != from_ms
        || snapshot.to_ms != to_ms
        || !is_revision(&snapshot.configuration_revision)
        || !(0..=MAX_SAFE_TIMESTAMP).contains(&snapshot.sampled_at_ms)
        || !snapshot.tracking_health.is_valid_at(snapshot.sampled_at_ms)
        || snapshot.records.len() > MAX_HISTORY_RECORDS
    {
        return Err(error());
    }
    let mut seen = HashSet::new();
    let mut previous = None;
    let mut samples = 0;
    for record in &snapshot.records {
        let order = (
            record.start_ms,
            record.origin,
            record.record_id,
            record.end_ms,
        );
        if previous.is_some_and(|before| before > order) || !seen.insert(order) {
            return Err(error());
        }
        previous = Some(order);
        if record.record_id <= 0
            || record.record_id > MAX_SAFE_TIMESTAMP
            || record.app_key.is_empty()
            || record.app_key.len() > MAX_HISTORY_NAME_BYTES
            || record.exe_name.is_empty()
            || record.exe_name.len() > MAX_HISTORY_NAME_BYTES
            || record.app_name.len() > MAX_HISTORY_NAME_BYTES
            || record.window_title.len() > MAX_HISTORY_TITLE_BYTES
            || !is_product_category(&record.category)
            || record
                .display_name_override
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > 4096)
            || record.start_ms < from_ms
            || record.end_ms > to_ms
            || record.end_ms <= record.start_ms
            || record.continuity_start_ms < -MAX_SAFE_TIMESTAMP
            || record.continuity_start_ms > record.start_ms
            || (record.origin == ExactActivityOrigin::ImportExact
                && (record.is_open || !record.title_samples.is_empty()))
            || (record.is_open
                && (snapshot.tracking_health.status == ActivityReadStatus::Unavailable
                    || record.end_ms > snapshot.tracking_health.live_cutoff_ms))
        {
            return Err(error());
        }
        let mut previous_sample = None;
        for sample in &record.title_samples {
            samples += 1;
            if samples > MAX_HISTORY_TITLE_SAMPLES
                || sample.title.len() > MAX_HISTORY_TITLE_BYTES
                || sample.start_ms < record.start_ms
                || sample.end_ms > record.end_ms
                || sample.end_ms <= sample.start_ms
                || previous_sample.is_some_and(|start| sample.start_ms < start)
            {
                return Err(error());
            }
            previous_sample = Some(sample.start_ms);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_protocol::activity::ActivityReadHealth;
    #[test]
    fn rejects_out_of_scope_samples_duplicates_and_untrusted_open_growth() {
        let mut snapshot = ExactHistorySnapshot {
            from_ms: 0,
            to_ms: 100,
            sampled_at_ms: 100,
            configuration_revision: "a".repeat(64),
            tracking_health: ActivityReadHealth {
                status: ActivityReadStatus::Unavailable,
                last_heartbeat_ms: None,
                live_cutoff_ms: 0,
                stale_after_ms: 8000,
            },
            records: vec![ExactActivityRecord {
                origin: ExactActivityOrigin::Native,
                record_id: 1,
                app_key: "editor".into(),
                exe_name: "editor".into(),
                app_name: "Editor".into(),
                category: "other".into(),
                display_name_override: None,
                window_title: "Caption".into(),
                start_ms: 10,
                end_ms: 50,
                continuity_start_ms: 0,
                is_open: false,
                title_samples: vec![],
            }],
        };
        assert!(validate(&snapshot, 0, 100).is_ok());
        assert!(validate(&snapshot, 1, 100).is_err());
        snapshot.records[0].is_open = true;
        assert!(validate(&snapshot, 0, 100).is_err());
        snapshot.records[0].is_open = false;
        snapshot.records[0].title_samples.push(ExactTitleSample {
            title: "outside".into(),
            start_ms: 0,
            end_ms: 30,
        });
        assert!(validate(&snapshot, 0, 100).is_err());
        snapshot.records[0].title_samples.clear();
        snapshot.records.push(snapshot.records[0].clone());
        assert!(validate(&snapshot, 0, 100).is_err());
    }
}
