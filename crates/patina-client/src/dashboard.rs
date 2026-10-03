use crate::{Client, ClientError};
use patina_protocol::{activity::DailyProductSnapshot, dashboard::DashboardProductSnapshot};
use std::{collections::BTreeMap, time::Duration};

impl Client {
    pub async fn dashboard(
        &self,
        date: &str,
        language: &str,
    ) -> Result<DashboardProductSnapshot, ClientError> {
        if date.len() != 10
            || !date.bytes().enumerate().all(|(index, byte)| {
                if index == 4 || index == 7 {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit()
                }
            })
            || !matches!(language, "en-US" | "zh-CN")
        {
            return Err(ClientError::InvalidConfiguration(
                "invalid Dashboard date or language".into(),
            ));
        }
        let snapshot: DashboardProductSnapshot = self
            .get_json_with_limits(
                &format!("/api/v1/activity/dashboard?date={date}&language={language}"),
                "Dashboard product snapshot",
                Duration::from_secs(35),
                4 * 1024 * 1024,
            )
            .await?;
        validate(&snapshot)?;
        Ok(snapshot)
    }
}

fn validate(snapshot: &DashboardProductSnapshot) -> Result<(), ClientError> {
    crate::activity::validate(&DailyProductSnapshot {
        sampled_at_ms: snapshot.sampled_at_ms,
        tracking_health: snapshot.tracking_health.clone(),
        configuration_revision: snapshot.configuration_revision.clone(),
        days: vec![snapshot.previous.clone(), snapshot.current.clone()],
        applications: snapshot.applications.clone(),
    })?;
    let error = || ClientError::InvalidResponse("inconsistent Dashboard hourly quantities".into());
    if snapshot.hours.len() != 24 {
        return Err(error());
    }
    let identities: BTreeMap<_, _> = snapshot
        .applications
        .iter()
        .map(|app| (&app.app_key, &app.category))
        .collect();
    let mut expected = BTreeMap::<&str, i64>::new();
    for app in &snapshot.current.apps {
        let category = identities.get(&app.app_key).ok_or_else(error)?.as_str();
        let entry = expected.entry(category).or_default();
        *entry = entry.checked_add(app.active_ms).ok_or_else(error)?;
    }
    let mut actual = BTreeMap::<&str, i64>::new();
    for (index, hour) in snapshot.hours.iter().enumerate() {
        if usize::from(hour.hour) != index || hour.categories.len() > 4096 {
            return Err(error());
        }
        let mut seen = std::collections::HashSet::new();
        let mut total = 0i64;
        for category in &hour.categories {
            if category.active_ms <= 0
                || !seen.insert(&category.category)
                || !patina_protocol::activity::is_product_category(&category.category)
            {
                return Err(error());
            }
            total = total.checked_add(category.active_ms).ok_or_else(error)?;
            let entry = actual.entry(&category.category).or_default();
            *entry = entry.checked_add(category.active_ms).ok_or_else(error)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use patina_protocol::{
        activity::{
            ActivityReadHealth, ActivityReadStatus, DailyProductAppTotal, DailyProductDay,
            ProductAppIdentity,
        },
        dashboard::{CategoryTotal, DashboardHour},
    };
    #[test]
    fn hourly_projection_must_conserve_the_confirmed_category_totals() {
        let mut snapshot = DashboardProductSnapshot {
            sampled_at_ms: 2000,
            configuration_revision: "0".repeat(64),
            tracking_health: ActivityReadHealth {
                status: ActivityReadStatus::Unavailable,
                last_heartbeat_ms: None,
                live_cutoff_ms: 0,
                stale_after_ms: 8000,
            },
            previous: DailyProductDay {
                start_ms: 0,
                end_ms: 1000,
                active_ms: 0,
                apps: vec![],
            },
            current: DailyProductDay {
                start_ms: 1000,
                end_ms: 2000,
                active_ms: 1,
                apps: vec![DailyProductAppTotal {
                    app_key: "editor".into(),
                    active_ms: 1,
                }],
            },
            applications: vec![ProductAppIdentity {
                app_key: "editor".into(),
                exe_name: "editor".into(),
                app_name: "Editor".into(),
                category: "development".into(),
                display_name_override: None,
            }],
            hours: (0..24)
                .map(|hour| DashboardHour {
                    hour,
                    active_ms: 0,
                    categories: vec![],
                })
                .collect(),
        };
        assert!(validate(&snapshot).is_err());
        snapshot.hours[1].active_ms = 1;
        snapshot.hours[1].categories = vec![CategoryTotal {
            category: "development".into(),
            active_ms: 1,
        }];
        assert!(validate(&snapshot).is_ok());
        snapshot.hours[1].categories[0].category = "music".into();
        assert!(validate(&snapshot).is_err());
        snapshot.hours[1].categories[0].category = "development".into();
        snapshot.hours[2].hour = 1;
        assert!(validate(&snapshot).is_err());
    }
}
