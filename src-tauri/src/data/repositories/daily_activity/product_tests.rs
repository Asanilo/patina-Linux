use super::*;
use sqlx::Executor;

#[tokio::test]
async fn dashboard_hourly_quantities_match_daily_totals_and_do_not_expose_titles() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for schema in [
        crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL,
        crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL,
        crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    let days = crate::domain::activity_calendar::dashboard_boundaries("2026-03-08").unwrap();
    let start = days[1];
    let bucket_start = (start.div_euclid(HOUR_MS) + 2) * HOUR_MS;
    sqlx::query("INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES('fixture',0,'fixture','patina-csv',?,0,1)").bind("a".repeat(64)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES('fixture',?,'Imported','imported',?,1)").bind("b".repeat(64)).bind(bucket_start).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES('Editor','editor','private title',?,?,1000)").bind(start+1000).bind(start+2000).execute(&pool).await.unwrap();
    let result = load_snapshot_with_apps(&pool, &days, days[2], ReadMode::DashboardEnglish)
        .await
        .unwrap();
    assert_eq!(result.app_days[1].active_ms, 1001);
    assert_eq!(
        result.hours.iter().map(|hour| hour.active_ms).sum::<i64>(),
        1001
    );
    assert_eq!(
        result
            .hours
            .iter()
            .flat_map(|hour| &hour.categories)
            .map(|category| category.active_ms)
            .sum::<i64>(),
        1001
    );
    assert!(!serde_json::to_string(&result.hours)
        .unwrap()
        .contains("private title"));
    let daily = load_snapshot_with_apps(&pool, &days, days[2], ReadMode::ProductEnglish)
        .await
        .unwrap();
    assert_eq!(result.app_days, daily.app_days);
    pool.close().await;
}

#[tokio::test]
async fn stale_open_time_stops_at_owner_heartbeat_across_days_and_recovers() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for schema in [
        crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL,
        crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL,
        crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    let day = 24 * HOUR_MS;
    sqlx::query("INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) VALUES('Editor','editor',?,NULL,NULL),('Closed','closed',100,200,100)")
        .bind(day - 2000).execute(&pool).await.unwrap();
    for (heartbeat, now, expected, status) in [
        (
            Some((day + 9000).to_string()),
            day + 10000,
            vec![2100, 10000],
            patina_protocol::activity::ActivityReadStatus::Healthy,
        ),
        (
            Some((day + 9000).to_string()),
            day + 20000,
            vec![2100, 9000],
            patina_protocol::activity::ActivityReadStatus::Stale,
        ),
        (
            Some((day + 9000).to_string()),
            day + 30000,
            vec![2100, 9000],
            patina_protocol::activity::ActivityReadStatus::Stale,
        ),
        (
            Some((day + 29000).to_string()),
            day + 30000,
            vec![2100, 30000],
            patina_protocol::activity::ActivityReadStatus::Healthy,
        ),
        (
            None,
            day + 30000,
            vec![100, 0],
            patina_protocol::activity::ActivityReadStatus::Unavailable,
        ),
        (
            Some("invalid".into()),
            day + 30000,
            vec![100, 0],
            patina_protocol::activity::ActivityReadStatus::Unavailable,
        ),
        (
            Some(i64::MAX.to_string()),
            day + 30000,
            vec![100, 0],
            patina_protocol::activity::ActivityReadStatus::Unavailable,
        ),
    ] {
        pool.execute("DELETE FROM settings WHERE key='__tracker_last_heartbeat_ms'")
            .await
            .unwrap();
        if let Some(value) = heartbeat {
            sqlx::query("INSERT INTO settings(key,value) VALUES('__tracker_last_heartbeat_ms',?)")
                .bind(value)
                .execute(&pool)
                .await
                .unwrap();
        }
        let result =
            load_snapshot_with_apps(&pool, &[0, day, day * 2], now, ReadMode::ProductEnglish)
                .await
                .unwrap();
        assert_eq!(
            result
                .app_days
                .iter()
                .map(|day| day.active_ms)
                .collect::<Vec<_>>(),
            expected
        );
        let health = result.tracking_health.unwrap();
        assert_eq!(health.status, status);
        assert!(health.is_valid_at(now));
    }
    pool.close().await;
}

#[tokio::test]
async fn product_snapshot_uses_one_classification_policy_and_preserves_occupied_time() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute("INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) VALUES('Editor','editor',100,200,100),('Hidden','hidden',200,300,100)").await.unwrap();
    sqlx::query("INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES('fixture',0,'fixture','patina-csv',?,1,0)").bind("a".repeat(64)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO import_exact_sessions(batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration,source_category) VALUES('fixture',?,'Import','imported','private title',0,400,400,'Imported category')").bind("b".repeat(64)).execute(&pool).await.unwrap();
    for (key, value) in [
        (
            "__app_override::editor",
            r#"{"category":"development","displayName":"Work"}"#,
        ),
        ("__deleted_category::development", "1"),
        ("__app_override::hidden", r#"{"track":false}"#),
        ("local_api_token", "private-token"),
    ] {
        sqlx::query("INSERT INTO settings(key,value) VALUES(?,?)")
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
    }
    let snapshot = load_snapshot_with_apps(&pool, &[0, 1000], 1000, ReadMode::ProductEnglish)
        .await
        .unwrap();
    let (revision, policy) = snapshot.product.unwrap();
    assert_eq!(snapshot.app_days[0].active_ms, 300); // Excluded native time still suppresses import overlap.
    assert_eq!(snapshot.app_days[0].apps.len(), 2);
    assert_eq!(policy.category("editor"), "ai"); // Deleted-category fallback order.
    assert_eq!(policy.category("imported"), "other"); // Import metadata is not manual classification.
    assert_eq!(policy.display_name_override("editor"), Some("Work"));
    assert_eq!(
        revision,
        super::super::classification_settings::load_classification_snapshot(&pool, 2000)
            .await
            .unwrap()
            .revision
    );
    let metadata = serde_json::to_string(&snapshot.applications.unwrap()).unwrap();
    assert!(!metadata.contains("private"));
    // Legacy endpoint retains its own compatibility projection and no product policy.
    let legacy = load_snapshot_with_apps(&pool, &[0, 1000], 1000, ReadMode::ApplicationsNamed)
        .await
        .unwrap();
    assert!(legacy.product.is_none());
    assert_eq!(legacy.app_days[0].active_ms, 300);
    pool.close().await;
}
