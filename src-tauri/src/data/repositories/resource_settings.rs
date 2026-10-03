//! Confidential configuration is merged only at the data/runtime boundary.
use crate::domain::settings::{self, WebActivityBridgeSettings};
use patina_protocol::resource_settings::{BrowserResourceSettings, ResourceSettingsSnapshot};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

const GENERATION_KEY: &str = "__runtime_resource_generation";
const KEYS: &[&str] = &[
    "audio_participation_enabled",
    "web_activity_enabled",
    "web_activity_port",
    "web_activity_token",
    "web_activity_url_privacy",
    GENERATION_KEY,
];

pub(crate) struct ResourceState {
    pub snapshot: ResourceSettingsSnapshot,
    pub browser: WebActivityBridgeSettings,
}

pub async fn load_snapshot(
    pool: &SqlitePool,
    now_ms: i64,
) -> Result<ResourceSettingsSnapshot, String> {
    Ok(load_state(pool, now_ms).await?.snapshot)
}

pub(crate) async fn load_state(pool: &SqlitePool, now_ms: i64) -> Result<ResourceState, String> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        let state = read_state(&mut tx, now_ms).await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(state)
    })
    .await
    .map_err(|_| "resource settings read exceeded its time budget".to_string())?
}

async fn read_state(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<ResourceState, String> {
    let sql = format!(
        "SELECT key, length(CAST(value AS BLOB)) AS bytes,
        CASE WHEN length(CAST(value AS BLOB)) <= 4096 THEN value ELSE '' END AS value
        FROM settings WHERE key IN ({})",
        vec!["?"; KEYS.len()].join(",")
    );
    let mut query = sqlx::query(&sql);
    for key in KEYS {
        query = query.bind(key);
    }
    let mut values = BTreeMap::<String, String>::new();
    for row in query
        .fetch_all(&mut *connection)
        .await
        .map_err(|e| e.to_string())?
    {
        if row.get::<i64, _>("bytes") > 4096 {
            return Err("resource setting exceeds its value budget".into());
        }
        values.insert(row.get("key"), row.get("value"));
    }
    let raw = |key: &str| values.get(key).map(String::as_str);
    let browser = WebActivityBridgeSettings::from_storage_values(
        raw("web_activity_port"),
        raw("web_activity_enabled"),
        raw("web_activity_token"),
    );
    let audio = settings::parse_audio_participation_enabled(raw("audio_participation_enabled"));
    let public = BrowserResourceSettings {
        enabled: browser.enabled,
        port: browser.port,
        token_present: !browser.token.is_empty(),
        url_privacy: settings::parse_web_activity_url_privacy(raw("web_activity_url_privacy")),
    };
    let generation = parse_generation(raw(GENERATION_KEY))?;
    let mut hash = Sha256::new();
    hash.update(b"patina.resource-settings.v1\0");
    // Generation detects credential rotation without exposing a hash of a secret.
    hash.update(generation.to_be_bytes());
    hash.update([u8::from(audio)]);
    hash.update(serde_json::to_vec(&public).map_err(|e| e.to_string())?);
    Ok(ResourceState {
        browser,
        snapshot: ResourceSettingsSnapshot {
            revision: format!("{:x}", hash.finalize()),
            sampled_at_ms: now_ms,
            audio_participation_enabled: audio,
            browser_activity: public,
        },
    })
}

fn parse_generation(value: Option<&str>) -> Result<u64, String> {
    value.map_or(Ok(0), |value| {
        value
            .parse()
            .map_err(|_| "invalid resource settings generation".into())
    })
}

pub(crate) async fn advance_generation(tx: &mut Transaction<'_, Sqlite>) -> Result<(), String> {
    let value: Option<String> = sqlx::query_scalar("SELECT CASE WHEN length(value)<=20 THEN value ELSE 'invalid' END FROM settings WHERE key=?")
        .bind(GENERATION_KEY).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
    let next = parse_generation(value.as_deref())?
        .checked_add(1)
        .ok_or("resource settings generation exhausted")?;
    sqlx::query("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
        .bind(GENERATION_KEY).bind(next.to_string()).execute(&mut **tx).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    Conflict,
    Storage(String),
}

pub struct CommitResult {
    pub snapshot: ResourceSettingsSnapshot,
    pub sealed: bool,
}

pub(crate) async fn commit(
    pool: &SqlitePool,
    expected_revision: &str,
    audio: Option<bool>,
    browser: Option<(
        &WebActivityBridgeSettings,
        settings::WebActivityUrlPrivacyMode,
    )>,
    now_ms: i64,
) -> Result<CommitResult, CommitError> {
    use CommitError::{Conflict, Storage};
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|e| Storage(e.to_string()))?;
    if read_state(&mut tx, now_ms)
        .await
        .map_err(Storage)?
        .snapshot
        .revision
        != expected_revision
    {
        return Err(Conflict);
    }
    let mut mutations = Vec::new();
    let mut add = |key: &str, value: String| {
        mutations.push(super::app_settings::AppSettingMutation {
            key: key.into(),
            value,
        })
    };
    if let Some(value) = audio {
        add(
            "audio_participation_enabled",
            if value { "1" } else { "0" }.into(),
        );
    }
    if let Some((settings, privacy)) = browser {
        add(
            "web_activity_enabled",
            if settings.enabled { "1" } else { "0" }.into(),
        );
        add("web_activity_port", settings.port.to_string());
        add("web_activity_token", settings.token.clone());
        add("web_activity_url_privacy", privacy.as_str().into());
    }
    super::app_settings::validate_app_setting_mutations(&mutations).map_err(Storage)?;
    super::app_settings::apply_app_settings_tx(&mut tx, &mutations, now_ms)
        .await
        .map_err(Storage)?;
    let sealed = if browser.is_some_and(|(settings, _)| !settings.enabled) {
        super::web_activity::end_active_segment_tx(&mut tx, now_ms)
            .await
            .map_err(|e| Storage(e.to_string()))?
    } else {
        false
    };
    let snapshot = read_state(&mut tx, now_ms).await.map_err(Storage)?.snapshot;
    tx.commit().await.map_err(|e| Storage(e.to_string()))?;
    Ok(CommitResult { snapshot, sealed })
}
