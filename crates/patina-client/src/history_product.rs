use crate::{Client, ClientError};
use patina_protocol::{history::*, history_product::*};
use std::collections::{BTreeMap, BTreeSet};

impl Client {
    pub async fn history_product(
        &self,
        from_ms: i64,
        to_ms: i64,
        language: &str,
    ) -> Result<HistoryProductSnapshot, ClientError> {
        if !valid_range(from_ms, to_ms) || !matches!(language, "en-US" | "zh-CN") {
            return Err(ClientError::InvalidConfiguration(
                "invalid History product range or language".into(),
            ));
        }
        let snapshot = self.get_json_with_limits(&format!("/api/v1/activity/history-product?from_ms={from_ms}&to_ms={to_ms}&language={language}"),
            "History product",patina_protocol::read_budget::ANALYTICS.client,MAX_HISTORY_RESPONSE_BYTES).await?;
        validate(&snapshot, from_ms, to_ms)?;
        Ok(snapshot)
    }
}

fn validate(
    snapshot: &HistoryProductSnapshot,
    from_ms: i64,
    to_ms: i64,
) -> Result<(), ClientError> {
    crate::history::validate(&snapshot.history, from_ms, to_ms)?;
    let error = || ClientError::InvalidResponse("invalid History hourly projection".into());
    if snapshot.hours.len() != 24 {
        return Err(error());
    }
    let mut expected = BTreeMap::<&str, i64>::new();
    for record in &snapshot.history.records {
        let value = expected.entry(&record.category).or_default();
        *value = value
            .checked_add(record.end_ms - record.start_ms)
            .ok_or_else(error)?;
    }
    if expected.len() > MAX_HISTORY_HOUR_CATEGORIES {
        return Err(error());
    }
    let mut actual = BTreeMap::<&str, i64>::new();
    for (index, hour) in snapshot.hours.iter().enumerate() {
        if usize::from(hour.hour) != index
            || hour.active_ms < 0
            || hour.categories.len() > MAX_HISTORY_HOUR_CATEGORIES
        {
            return Err(error());
        }
        let mut seen = BTreeSet::new();
        let mut total = 0i64;
        for category in &hour.categories {
            if category.active_ms <= 0
                || !expected.contains_key(category.category.as_str())
                || !seen.insert(category.category.as_str())
            {
                return Err(error());
            }
            total = total.checked_add(category.active_ms).ok_or_else(error)?;
            let value = actual.entry(&category.category).or_default();
            *value = value.checked_add(category.active_ms).ok_or_else(error)?;
        }
        if total != hour.active_ms {
            return Err(error());
        }
    }
    if actual != expected {
        return Err(error());
    }
    Ok(())
}
