use crate::domain::settings;
use patina_protocol::product_settings::{ProductSettings, ProductSettingsSnapshot};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::{collections::BTreeMap, time::Duration};

pub mod conditional;
#[cfg(test)]
mod conditional_tests;

// Exact keys and bounded values prevent unrelated settings/secrets from entering
// the snapshot. A single SELECT provides the same read point for policy/health.
const KEYS: &[&str] = &[
    "idle_timeout_secs",
    "timeline_merge_gap_secs",
    "min_session_secs",
    "tracking_paused",
    "audio_participation_enabled",
    "web_activity_enabled",
    "web_activity_port",
    "web_activity_url_privacy",
    "__tracker_last_heartbeat_ms",
    "__tracker_last_successful_sample_ms",
];
const MAX_VALUE_BYTES: usize = 4096;

pub async fn load_snapshot(
    pool: &SqlitePool,
    sampled_at_ms: i64,
) -> Result<ProductSettingsSnapshot, String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;
        let snapshot = read_snapshot(&mut transaction, sampled_at_ms).await?;
        transaction.commit().await.map_err(|e| e.to_string())?;
        Ok(snapshot)
    })
    .await
    .map_err(|_| "product settings snapshot exceeded its time budget".to_string())?
}

pub(crate) async fn read_snapshot(
    connection: &mut sqlx::SqliteConnection,
    sampled_at_ms: i64,
) -> Result<ProductSettingsSnapshot, String> {
    let sql = format!(
            "SELECT key, length(CAST(value AS BLOB)) AS value_bytes, CASE WHEN length(CAST(value AS BLOB)) <= {} THEN value ELSE '' END AS value FROM settings WHERE key IN ({})",
            MAX_VALUE_BYTES, vec!["?"; KEYS.len()].join(",")
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
        let key: String = row.try_get("key").map_err(|e| e.to_string())?;
        if row
            .try_get::<i64, _>("value_bytes")
            .map_err(|e| e.to_string())?
            > MAX_VALUE_BYTES as i64
        {
            return Err("product setting exceeds its value budget".into());
        }
        let value: String = row.try_get("value").map_err(|e| e.to_string())?;
        values.insert(key, value);
    }
    // No credential bytes cross the repository boundary or enter its revision.
    // Bound before trimming, with Rust's whitespace semantics matching the runtime.
    let token_row: Option<(i64, String)> = sqlx::query_as(
            "SELECT length(CAST(value AS BLOB)), CASE WHEN length(CAST(value AS BLOB)) <= 4096 THEN value ELSE '' END FROM settings WHERE key='web_activity_token'"
        ).fetch_optional(&mut *connection).await.map_err(|e| e.to_string())?;
    if token_row
        .as_ref()
        .is_some_and(|(bytes, _)| *bytes > MAX_VALUE_BYTES as i64)
    {
        return Err("browser credential exceeds its value budget".into());
    }
    let token = token_row.map(|(_, value)| value);
    let raw = |key: &str| values.get(key).map(String::as_str);
    let number = |key: &str, fallback: u64| {
        raw(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(fallback)
    };
    let token_present = token.as_ref().is_some_and(|v| !v.trim().is_empty());
    let preferences = ProductSettings {
        idle_timeout_secs: number("idle_timeout_secs", settings::DEFAULT_IDLE_TIMEOUT_SECS),
        timeline_merge_gap_secs: number(
            "timeline_merge_gap_secs",
            settings::DEFAULT_TIMELINE_MERGE_GAP_SECS,
        ),
        min_session_secs: settings::parse_min_session_secs(raw("min_session_secs")),
        tracking_paused: raw("tracking_paused")
            .is_some_and(|v| settings::parse_boolean_setting(v, false)),
        audio_participation_enabled: settings::parse_audio_participation_enabled(raw(
            "audio_participation_enabled",
        )),
        web_activity_enabled: raw("web_activity_enabled").is_some_and(|v| {
            settings::parse_boolean_setting(v, settings::DEFAULT_WEB_ACTIVITY_ENABLED)
        }) && token_present,
        web_activity_port: raw("web_activity_port")
            .and_then(settings::parse_web_activity_port)
            .unwrap_or(settings::DEFAULT_WEB_ACTIVITY_PORT),
        web_activity_token_present: token_present,
        web_activity_url_privacy: settings::parse_web_activity_url_privacy(raw(
            "web_activity_url_privacy",
        )),
    };
    let timestamp = |key: &str| {
        raw(key)
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v >= 0)
    };
    let mut hash = Sha256::new();
    hash.update(b"patina.product-settings.v1\0");
    hash.update(serde_json::to_vec(&preferences).map_err(|e| e.to_string())?);
    let snapshot = ProductSettingsSnapshot {
        revision: format!("{:x}", hash.finalize()),
        sampled_at_ms,
        settings: preferences,
        last_heartbeat_ms: timestamp("__tracker_last_heartbeat_ms"),
        last_successful_sample_ms: timestamp("__tracker_last_successful_sample_ms"),
    };
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }
    async fn set(pool: &SqlitePool, key: &str, value: &str) {
        sqlx::query("INSERT OR REPLACE INTO settings VALUES(?,?)")
            .bind(key)
            .bind(value)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn snapshot_has_owner_defaults_and_does_not_repair_missing_settings() {
        let pool = database().await;
        let snapshot = load_snapshot(&pool, 50).await.unwrap();
        assert_eq!(snapshot.settings.idle_timeout_secs, 900);
        assert_eq!(snapshot.settings.timeline_merge_gap_secs, 180);
        assert_eq!(snapshot.settings.min_session_secs, 300);
        assert!(!snapshot.settings.web_activity_enabled);
        assert_eq!(snapshot.last_heartbeat_ms, None);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM settings")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn revision_excludes_health_local_preferences_and_credentials() {
        let pool = database().await;
        set(&pool, "web_activity_enabled", "true").await;
        set(&pool, "web_activity_token", "super-secret").await;
        set(&pool, "local_api_token", &"unrelated-secret".repeat(10000)).await;
        let first = load_snapshot(&pool, 50).await.unwrap();
        assert!(first.settings.web_activity_enabled);
        let json = serde_json::to_string(&first).unwrap();
        assert!(!json.contains("super-secret"));
        assert!(!json.contains("local_api_token"));
        assert!(!json.contains("unrelated-secret"));
        set(&pool, "theme_mode", "dark").await;
        set(&pool, "web_activity_token", "rotated-secret").await;
        set(&pool, "__tracker_last_heartbeat_ms", "60").await;
        set(&pool, "__tracker_last_successful_sample_ms", "59").await;
        let next = load_snapshot(&pool, 70).await.unwrap();
        assert_eq!(first.revision, next.revision);
        assert_eq!(next.last_heartbeat_ms, Some(60));
        assert_eq!(next.last_successful_sample_ms, Some(59));
        set(&pool, "tracking_paused", "1").await;
        assert_ne!(
            next.revision,
            load_snapshot(&pool, 70).await.unwrap().revision
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn effective_values_follow_runtime_and_preserve_external_thresholds() {
        let pool = database().await;
        set(&pool, "idle_timeout_secs", "60").await;
        set(&pool, "timeline_merge_gap_secs", "30").await;
        set(&pool, "min_session_secs", "330").await;
        set(&pool, "audio_participation_enabled", "off").await;
        set(&pool, "web_activity_enabled", "1").await;
        set(&pool, "web_activity_token", "\u{2003} ").await;
        set(&pool, "web_activity_port", "1").await;
        set(&pool, "web_activity_url_privacy", "domain_only").await;
        set(&pool, "__tracker_last_heartbeat_ms", "-1").await;
        let snapshot = load_snapshot(&pool, 100).await.unwrap();
        assert_eq!(
            snapshot.settings.idle_timeout_secs,
            super::super::tracker_settings::load_idle_timeout_secs(&pool, 900)
                .await
                .unwrap()
        );
        assert_eq!(snapshot.settings.timeline_merge_gap_secs, 30);
        assert_eq!(snapshot.settings.min_session_secs, 360);
        assert!(!snapshot.settings.audio_participation_enabled);
        assert!(!snapshot.settings.web_activity_enabled);
        assert!(!snapshot.settings.web_activity_token_present);
        assert_eq!(snapshot.settings.web_activity_port, 12345);
        assert_eq!(snapshot.last_heartbeat_ms, None);
        pool.close().await;
    }

    #[tokio::test]
    async fn oversized_values_fail_without_partial_defaults_or_writes() {
        let pool = database().await;
        set(&pool, "idle_timeout_secs", &"9".repeat(4097)).await;
        assert!(load_snapshot(&pool, 1)
            .await
            .unwrap_err()
            .contains("budget"));
        set(&pool, "idle_timeout_secs", "900").await;
        set(&pool, "web_activity_token", &"密".repeat(1366)).await;
        let error = load_snapshot(&pool, 1).await.unwrap_err();
        assert!(error.contains("budget"));
        assert!(!error.contains("密"));
        pool.close().await;
    }
}
