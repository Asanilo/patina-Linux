//! Synthetic data for the opt-in real frontend acceptance. Never opens a product profile.
use crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) async fn seed(db: &Path, port: &str) -> Value {
    assert!(!db.exists());
    let pool = open_prepared_sqlite_pool_at_path(db, true).await.unwrap();
    let today = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    let start = today - 348 * 86_400_000;
    sqlx::query("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i < 49999) INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) SELECT 'Fixture App', 'fixture-app', ?, ? + i * 600000, ? + i * 600000 + 60000, 60000 FROM n")
        .bind("Synthetic fixture title ".repeat(45)).bind(start).bind(start).execute(&pool).await.unwrap();
    for (key, value) in [
        ("tracking_paused", "true".to_string()),
        ("audio_participation_enabled", "false".into()),
        ("web_activity_enabled", "false".into()),
        ("remote_status_bridge_enabled", "false".into()),
        ("launch_at_login", "false".into()),
        ("background_tracking_at_login", "false".into()),
        ("start_minimized", "false".into()),
        ("background_optimization", "true".into()),
        ("close_behavior", "tray".into()),
        ("minimize_behavior", "taskbar".into()),
        (
            "__classification_manual_confirmation_migration::v1",
            "1".into(),
        ),
        (
            "__update_last_auto_check_day",
            chrono::Utc::now().format("%Y-%m-%d").to_string(),
        ),
        ("local_api_port", port.to_string()),
    ] {
        sqlx::query("INSERT OR REPLACE INTO settings(key,value) VALUES (?,?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
    json!({"rows":50000,"start_ms":start,"expected_ms":3000000000_i64})
}

pub(crate) async fn verify(db: &Path) -> Value {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(db)
                .read_only(true),
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let total: i64 = sqlx::query_scalar("SELECT SUM(end_time-start_time) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 50000);
    assert_eq!(total, 3000000000);
    assert_eq!(integrity, "ok");
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap()
        .is_empty());
    pool.close().await;
    json!({"count":count,"total_ms":total,"quick_check":integrity})
}

#[test]
fn synthetic_fixture_roundtrip_preserves_counts_and_disabled_sources() {
    let root = std::env::temp_dir().join(format!(
        "patina-heatmap-fixture-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::create_dir(&root).unwrap();
    let db = root.join("fixture.db");
    tauri::async_runtime::block_on(async {
        let seeded = seed(&db, "34567").await;
        assert_eq!(verify(&db).await["total_ms"], seeded["expected_ms"]);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&db)
                    .read_only(true),
            )
            .await
            .unwrap();
        for (key, expected) in [
            ("tracking_paused", "true"),
            ("audio_participation_enabled", "false"),
            ("web_activity_enabled", "false"),
            ("launch_at_login", "false"),
            ("local_api_port", "34567"),
        ] {
            let value: String = sqlx::query_scalar("SELECT value FROM settings WHERE key=?")
                .bind(key)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(value, expected);
        }
        pool.close().await;
    });
    std::fs::remove_file(db).unwrap();
    std::fs::remove_dir(root).unwrap();
}
