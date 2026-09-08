use sqlx::{Pool, Row, Sqlite};

const APP_OVERRIDE_KEY_PREFIX: &str = "__app_override::";
const LEGACY_APP_CATEGORY_KEY_PREFIX: &str = "__app_category::";
const LEGACY_APP_EXCLUDED_KEY_PREFIX: &str = "__app_excluded::";
const MAX_EXE_NAME_LEN: usize = 256;
const MAX_DISPLAY_NAME_LEN: usize = 256;
const MAX_CATEGORY_LEN: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedAppConfiguration {
    pub exe_name: String,
    pub display_name: String,
    pub category: Option<String>,
    pub excluded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppOverrideUpdate {
    Category(String),
    DisplayName(String),
    Excluded(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppOverrideUpdateError {
    InvalidInput(String),
    Storage(String),
}

impl std::fmt::Display for AppOverrideUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) | Self::Storage(message) => formatter.write_str(message),
        }
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAppOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_title: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

pub async fn load_observed_app_configurations(
    pool: &Pool<Sqlite>,
) -> Result<Vec<ObservedAppConfiguration>, String> {
    let recorded_apps = super::activity_read_model::load_recorded_apps(pool).await?;
    let setting_rows = sqlx::query(
        "SELECT key, value FROM settings
         WHERE key LIKE ? OR key LIKE ? OR key LIKE ?",
    )
    .bind(format!("{APP_OVERRIDE_KEY_PREFIX}%"))
    .bind(format!("{LEGACY_APP_CATEGORY_KEY_PREFIX}%"))
    .bind(format!("{LEGACY_APP_EXCLUDED_KEY_PREFIX}%"))
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load app classification settings: {error}"))?;

    let mut overrides = std::collections::HashMap::<String, StoredAppOverride>::new();
    let mut legacy_categories = std::collections::HashMap::<String, String>::new();
    let mut legacy_excluded = std::collections::HashSet::<String>::new();
    for row in setting_rows {
        let key = row.try_get::<String, _>("key").unwrap_or_default();
        let value = row.try_get::<String, _>("value").unwrap_or_default();
        if let Some(exe_name) = key.strip_prefix(APP_OVERRIDE_KEY_PREFIX) {
            if let Ok(value) = serde_json::from_str::<StoredAppOverride>(&value) {
                overrides.insert(canonical_app_key(exe_name), value);
            }
        } else if let Some(exe_name) = key.strip_prefix(LEGACY_APP_CATEGORY_KEY_PREFIX) {
            legacy_categories.insert(canonical_app_key(exe_name), value);
        } else if let Some(exe_name) = key.strip_prefix(LEGACY_APP_EXCLUDED_KEY_PREFIX) {
            if crate::domain::settings::parse_boolean_setting(&value, false) {
                legacy_excluded.insert(canonical_app_key(exe_name));
            }
        }
    }

    Ok(recorded_apps
        .into_iter()
        .map(|recorded| {
            let exe_name = recorded.exe_name;
            let app_key = canonical_app_key(&exe_name);
            let override_value = overrides.get(&app_key);
            ObservedAppConfiguration {
                display_name: override_value
                    .and_then(|value| value.display_name.clone())
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| (!recorded.app_name.trim().is_empty()).then_some(recorded.app_name))
                    .unwrap_or_else(|| exe_name.clone()),
                category: override_value
                    .and_then(|value| value.category.clone())
                    .or_else(|| legacy_categories.get(&app_key).cloned()),
                excluded: override_value
                    .and_then(|value| value.track)
                    .is_some_and(|track| !track)
                    || legacy_excluded.contains(&app_key),
                exe_name,
            }
        })
        .collect())
}

pub async fn update_app_override(
    pool: &Pool<Sqlite>,
    exe_name: &str,
    update: AppOverrideUpdate,
    updated_at_ms: i64,
) -> Result<(), AppOverrideUpdateError> {
    let app_key = validate_app_key(exe_name)?;
    let override_key = format!("{APP_OVERRIDE_KEY_PREFIX}{app_key}");
    let legacy_category_key = format!("{LEGACY_APP_CATEGORY_KEY_PREFIX}{app_key}");
    let legacy_excluded_key = format!("{LEGACY_APP_EXCLUDED_KEY_PREFIX}{app_key}");
    let mut tx = pool.begin().await.map_err(|error| {
        AppOverrideUpdateError::Storage(format!(
            "failed to start app override transaction: {error}"
        ))
    })?;

    let stored = sqlx::query("SELECT value FROM settings WHERE key = ? LIMIT 1")
        .bind(&override_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| {
            AppOverrideUpdateError::Storage(format!("failed to load app override: {error}"))
        })?
        .and_then(|row| row.try_get::<String, _>("value").ok());
    let mut override_value = match stored {
        Some(value) => serde_json::from_str::<StoredAppOverride>(&value).map_err(|error| {
            AppOverrideUpdateError::Storage(format!("stored app override is invalid JSON: {error}"))
        })?,
        None => StoredAppOverride::default(),
    };

    if override_value.category.is_none() {
        override_value.category =
            load_setting_in_transaction(&mut tx, &legacy_category_key).await?;
    }
    if override_value.track.is_none()
        && load_setting_in_transaction(&mut tx, &legacy_excluded_key)
            .await?
            .is_some_and(|value| crate::domain::settings::parse_boolean_setting(&value, false))
    {
        override_value.track = Some(false);
    }

    match update {
        AppOverrideUpdate::Category(category) => {
            let category = category.trim();
            if category.eq_ignore_ascii_case("other") {
                override_value.category = None;
            } else {
                validate_text_value("category", category, MAX_CATEGORY_LEN)?;
                override_value.category = Some(category.to_string());
            }
        }
        AppOverrideUpdate::DisplayName(display_name) => {
            let display_name = display_name.trim();
            validate_text_value("display name", display_name, MAX_DISPLAY_NAME_LEN)?;
            override_value.display_name = Some(display_name.to_string());
        }
        AppOverrideUpdate::Excluded(excluded) => {
            override_value.track = excluded.then_some(false);
        }
    }
    override_value.updated_at = Some(updated_at_ms.max(0));
    override_value.enabled = Some(true);

    if has_meaningful_override(&override_value) {
        let value = serde_json::to_string(&override_value).map_err(|error| {
            AppOverrideUpdateError::Storage(format!("failed to serialize app override: {error}"))
        })?;
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(&override_key)
        .bind(value)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppOverrideUpdateError::Storage(format!("failed to save app override: {error}"))
        })?;
    } else {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(&override_key)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                AppOverrideUpdateError::Storage(format!(
                    "failed to delete empty app override: {error}"
                ))
            })?;
    }
    sqlx::query("DELETE FROM settings WHERE key = ? OR key = ?")
        .bind(legacy_category_key)
        .bind(legacy_excluded_key)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            AppOverrideUpdateError::Storage(format!(
                "failed to remove legacy app settings: {error}"
            ))
        })?;
    tx.commit().await.map_err(|error| {
        AppOverrideUpdateError::Storage(format!(
            "failed to commit app override transaction: {error}"
        ))
    })
}

async fn load_setting_in_transaction(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    key: &str,
) -> Result<Option<String>, AppOverrideUpdateError> {
    sqlx::query("SELECT value FROM settings WHERE key = ? LIMIT 1")
        .bind(key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| {
            AppOverrideUpdateError::Storage(format!("failed to load legacy app setting: {error}"))
        })
        .map(|row| row.and_then(|row| row.try_get::<String, _>("value").ok()))
}

fn canonical_app_key(exe_name: &str) -> String {
    exe_name.trim().trim_matches('"').to_ascii_lowercase()
}

fn validate_app_key(exe_name: &str) -> Result<String, AppOverrideUpdateError> {
    let app_key = canonical_app_key(exe_name);
    if app_key.is_empty()
        || app_key.len() > MAX_EXE_NAME_LEN
        || app_key.chars().any(char::is_control)
    {
        return Err(AppOverrideUpdateError::InvalidInput(
            "invalid app executable name".to_string(),
        ));
    }
    Ok(app_key)
}

fn validate_text_value(
    label: &str,
    value: &str,
    max_len: usize,
) -> Result<(), AppOverrideUpdateError> {
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(AppOverrideUpdateError::InvalidInput(format!(
            "invalid {label}"
        )));
    }
    Ok(())
}

fn has_meaningful_override(value: &StoredAppOverride) -> bool {
    value.category.is_some()
        || value.display_name.is_some()
        || value.color.is_some()
        || value.track == Some(false)
        || value.capture_title == Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema as db_schema;
    use sqlx::{Executor, Row, SqlitePool};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    async fn load_setting(pool: &SqlitePool, key: &str) -> Option<String> {
        sqlx::query("SELECT value FROM settings WHERE key = ? LIMIT 1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .unwrap()
            .and_then(|row| row.try_get::<String, _>("value").ok())
    }

    #[test]
    fn app_override_updates_preserve_fields_and_use_current_storage_shape() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            sqlx::query(
                "INSERT INTO sessions
                 (app_name, exe_name, window_title, start_time, end_time, duration)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind("Ghostty")
            .bind("Ghostty")
            .bind("shell")
            .bind(1_000_i64)
            .bind(2_000_i64)
            .bind(1_000_i64)
            .execute(&pool)
            .await
            .unwrap();
            let key = "__app_override::ghostty";
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(
                    r##"{"category":"development","color":"#112233","captureTitle":false,"enabled":true}"##,
                )
                .execute(&pool)
                .await
                .unwrap();

            update_app_override(
                &pool,
                "Ghostty",
                AppOverrideUpdate::DisplayName(" Terminal ".to_string()),
                5_000,
            )
            .await
            .unwrap();
            update_app_override(&pool, "ghostty", AppOverrideUpdate::Excluded(true), 6_000)
                .await
                .unwrap();

            let stored = load_setting(&pool, key).await.unwrap();
            let value: serde_json::Value = serde_json::from_str(&stored).unwrap();
            assert_eq!(value["displayName"], "Terminal");
            assert_eq!(value["category"], "development");
            assert_eq!(value["color"], "#112233");
            assert_eq!(value["captureTitle"], false);
            assert_eq!(value["track"], false);
            assert_eq!(value["updatedAt"], 6_000);
            assert!(value.get("display_name").is_none());

            let apps = load_observed_app_configurations(&pool).await.unwrap();
            assert_eq!(
                apps,
                vec![ObservedAppConfiguration {
                    exe_name: "Ghostty".to_string(),
                    display_name: "Terminal".to_string(),
                    category: Some("development".to_string()),
                    excluded: true,
                }]
            );
        });
    }

    #[test]
    fn app_override_update_migrates_legacy_api_keys_atomically() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?), (?, ?)")
                .bind("__app_category::zen")
                .bind("research")
                .bind("__app_excluded::zen")
                .bind("1")
                .execute(&pool)
                .await
                .unwrap();

            update_app_override(
                &pool,
                "Zen",
                AppOverrideUpdate::DisplayName("Zen Browser".to_string()),
                7_000,
            )
            .await
            .unwrap();

            let stored = load_setting(&pool, "__app_override::zen").await.unwrap();
            let value: serde_json::Value = serde_json::from_str(&stored).unwrap();
            assert_eq!(value["displayName"], "Zen Browser");
            assert_eq!(value["category"], "research");
            assert_eq!(value["track"], false);
            assert_eq!(load_setting(&pool, "__app_category::zen").await, None);
            assert_eq!(load_setting(&pool, "__app_excluded::zen").await, None);
        });
    }

    #[test]
    fn invalid_stored_override_is_not_overwritten() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let key = "__app_override::ghostty";
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(key)
                .bind("{invalid")
                .execute(&pool)
                .await
                .unwrap();

            let result = update_app_override(
                &pool,
                "ghostty",
                AppOverrideUpdate::DisplayName("Terminal".to_string()),
                8_000,
            )
            .await;

            assert!(result.is_err());
            assert_eq!(load_setting(&pool, key).await, Some("{invalid".to_string()));
        });
    }
}
