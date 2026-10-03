use crate::{Client, ClientError};
use patina_protocol::activity::DailyProductSnapshot;
use std::{collections::HashSet, time::Duration};

impl Client {
    pub async fn daily_product(
        &self,
        from: &str,
        to: &str,
        language: &str,
    ) -> Result<DailyProductSnapshot, ClientError> {
        fn date(value: &str) -> bool {
            value.len() == 10
                && value.bytes().enumerate().all(|(i, b)| {
                    if i == 4 || i == 7 {
                        b == b'-'
                    } else {
                        b.is_ascii_digit()
                    }
                })
        }
        if !date(from) || !date(to) || !matches!(language, "en-US" | "zh-CN") {
            return Err(ClientError::InvalidConfiguration(
                "invalid product dates or language".into(),
            ));
        }
        let snapshot: DailyProductSnapshot = self
            .get_json_with_limits(
                &format!("/api/v1/activity/daily-product?from={from}&to={to}&language={language}"),
                "daily product snapshot",
                Duration::from_secs(35),
                4 * 1024 * 1024,
            )
            .await?;
        validate(&snapshot)?;
        Ok(snapshot)
    }
}

fn validate(snapshot: &DailyProductSnapshot) -> Result<(), ClientError> {
    let error = || ClientError::InvalidResponse("invalid daily product snapshot".into());
    if !patina_protocol::configuration::is_revision(&snapshot.configuration_revision)
        || !snapshot.tracking_health.is_valid_at(snapshot.sampled_at_ms)
        || snapshot.days.is_empty()
        || snapshot.days.len() > 378
        || snapshot.applications.len() > 4096
    {
        return Err(error());
    }
    let mut keys = HashSet::new();
    let mut count = 0;
    let mut previous = None;
    for day in &snapshot.days {
        let width = day.end_ms.checked_sub(day.start_ms).ok_or_else(error)?;
        if width <= 0 || width > 48 * 3_600_000 || previous.is_some_and(|end| end != day.start_ms) {
            return Err(error());
        }
        previous = Some(day.end_ms);
        let mut total = 0i64;
        let mut seen = HashSet::new();
        for app in &day.apps {
            count += 1;
            if count > 50_000
                || app.app_key.is_empty()
                || app.app_key.len() > 1024
                || app.active_ms <= 0
                || !seen.insert(&app.app_key)
            {
                return Err(error());
            }
            keys.insert(&app.app_key);
            total = total.checked_add(app.active_ms).ok_or_else(error)?;
        }
        if total != day.active_ms {
            return Err(error());
        }
    }
    if keys.len() != snapshot.applications.len() {
        return Err(error());
    }
    for app in &snapshot.applications {
        if !keys.remove(&app.app_key)
            || app.app_name.len() > 1024
            || app.exe_name.is_empty()
            || app.exe_name.len() > 1024
            || !patina_protocol::activity::is_product_category(&app.category)
            || app
                .display_name_override
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.len() > 4096)
        {
            return Err(error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_protocol::activity::{DailyProductAppTotal, DailyProductDay, ProductAppIdentity};

    #[test]
    fn rejects_inconsistent_totals_identities_and_legacy_category_values() {
        let valid = DailyProductSnapshot {
            tracking_health: patina_protocol::activity::ActivityReadHealth {
                status: patina_protocol::activity::ActivityReadStatus::Unavailable,
                last_heartbeat_ms: None,
                live_cutoff_ms: 0,
                stale_after_ms: 8000,
            },
            sampled_at_ms: 1000,
            configuration_revision: "a".repeat(64),
            days: vec![DailyProductDay {
                start_ms: 0,
                end_ms: 1000,
                active_ms: 37,
                apps: vec![DailyProductAppTotal {
                    app_key: "editor".into(),
                    active_ms: 37,
                }],
            }],
            applications: vec![ProductAppIdentity {
                app_key: "editor".into(),
                app_name: "Editor".into(),
                exe_name: "editor".into(),
                category: "development".into(),
                display_name_override: None,
            }],
        };
        assert!(validate(&valid).is_ok());
        let mut wrong_health = valid.clone();
        wrong_health.tracking_health.live_cutoff_ms = 1000;
        assert!(validate(&wrong_health).is_err());
        let mut invalid = valid.clone();
        invalid.days[0].active_ms = 38;
        assert!(validate(&invalid).is_err());
        invalid = valid.clone();
        invalid.applications.clear();
        assert!(validate(&invalid).is_err());
        invalid = valid.clone();
        invalid.days.push(invalid.days[0].clone());
        assert!(validate(&invalid).is_err());
        invalid = valid.clone();
        let duplicate = invalid.days[0].apps[0].clone();
        invalid.days[0].apps.push(duplicate);
        invalid.days[0].active_ms = 74;
        assert!(validate(&invalid).is_err());
        for category in ["system", "Imported category", "custom:"] {
            invalid = valid.clone();
            invalid.applications[0].category = category.into();
            assert!(validate(&invalid).is_err());
        }
    }
}
