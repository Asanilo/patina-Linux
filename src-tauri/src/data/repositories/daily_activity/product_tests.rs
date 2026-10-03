use super::*;
use sqlx::Executor;

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
