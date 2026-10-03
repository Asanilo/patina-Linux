use super::*;
use sqlx::Executor;

async fn database() -> SqlitePool {
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
    pool
}

#[tokio::test]
async fn history_input_and_sample_counts_are_bounded() {
    let pool = database().await;
    pool.execute("WITH RECURSIVE n(v) AS(SELECT 1 UNION ALL SELECT v+1 FROM n WHERE v<20001) INSERT INTO sessions(app_name,exe_name,start_time,end_time) SELECT 'Editor','editor',0,100 FROM n").await.unwrap();
    assert!(read_snapshot(&pool, 0, 100, 1000, "en-US")
        .await
        .unwrap_err()
        .contains("input fact budget"));
    pool.execute("DELETE FROM sessions").await.unwrap();
    pool.execute("INSERT INTO sessions(id,app_name,exe_name,start_time,end_time) VALUES(1,'Editor','editor',0,100)").await.unwrap();
    pool.execute("WITH RECURSIVE n(v) AS(SELECT 1 UNION ALL SELECT v+1 FROM n WHERE v<50001) INSERT INTO session_title_samples(session_id,title,start_time,end_time) SELECT 1,'Title',10,20 FROM n").await.unwrap();
    assert!(read_snapshot(&pool, 0, 100, 1000, "en-US")
        .await
        .unwrap_err()
        .contains("title sample budget"));
    pool.close().await;
}

#[tokio::test]
async fn exact_history_preserves_precedence_and_clips_records_and_title_samples() {
    let pool = database().await;
    pool.execute("INSERT INTO sessions(id,app_name,exe_name,window_title,start_time,end_time,duration,continuity_group_start_time) VALUES(1,'Editor','editor','Record caption',20,40,20,10),(2,'Hidden','hidden','hidden caption',60,80,20,60)").await.unwrap();
    pool.execute("INSERT INTO settings(key,value) VALUES('__app_override::hidden','{\"track\":false}'),('__app_override::editor','{\"category\":\"development\",\"displayName\":\"Work\"}'),('local_api_token','private-token')").await.unwrap();
    pool.execute("INSERT INTO session_title_samples(session_id,title,start_time,end_time) VALUES(1,'outside',0,10),(1,'Exact sample',15,NULL),(2,'hidden sample',60,80)").await.unwrap();
    sqlx::query("INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES('fixture',0,'fixture','patina-csv',?,1,1)").bind("a".repeat(64)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO import_exact_sessions(id,batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration) VALUES(1,'fixture',?,'Imported','imported','Import caption',0,100,100)").bind("b".repeat(64)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES('fixture',?,'Bucket','must-not-appear',0,1000)").bind("c".repeat(64)).execute(&pool).await.unwrap();
    let result = read_snapshot(&pool, 10, 90, 1000, "en-US").await.unwrap();
    assert_eq!(
        result
            .records
            .iter()
            .map(|r| (r.origin, r.start_ms, r.end_ms))
            .collect::<Vec<_>>(),
        vec![
            (ExactActivityOrigin::ImportExact, 10, 20),
            (ExactActivityOrigin::Native, 20, 40),
            (ExactActivityOrigin::ImportExact, 40, 60),
            (ExactActivityOrigin::ImportExact, 80, 90)
        ]
    );
    let native = &result.records[1];
    assert_eq!(native.record_id, 1);
    assert_eq!(native.category, "development");
    assert_eq!(native.display_name_override.as_deref(), Some("Work"));
    assert_eq!(native.continuity_start_ms, 10);
    assert_eq!(
        native.title_samples,
        vec![ExactTitleSample {
            title: "Exact sample".into(),
            start_ms: 20,
            end_ms: 40
        }]
    );
    assert!(result
        .records
        .iter()
        .filter(|r| r.origin == ExactActivityOrigin::ImportExact)
        .all(|r| r.title_samples.is_empty()));
    let encoded = serde_json::to_string(&result).unwrap();
    for private in [
        "private-token",
        "hidden sample",
        "must-not-appear",
        "outside",
    ] {
        assert!(!encoded.contains(private));
    }
    pool.close().await;
}

#[tokio::test]
async fn open_history_uses_owner_cutoff_and_is_read_only() {
    let pool = database().await;
    pool.execute(
        "INSERT INTO sessions(app_name,exe_name,start_time) VALUES('Editor','editor',1000)",
    )
    .await
    .unwrap();
    pool.execute("INSERT INTO settings(key,value) VALUES('__tracker_last_heartbeat_ms','2000')")
        .await
        .unwrap();
    let first = read_snapshot(&pool, 0, 30000, 20000, "en-US")
        .await
        .unwrap();
    let later = read_snapshot(&pool, 0, 30000, 25000, "en-US")
        .await
        .unwrap();
    assert_eq!(first.records, later.records);
    assert_eq!(first.records[0].end_ms, 2000);
    assert!(first.records[0].is_open);
    pool.execute("UPDATE settings SET value='24999' WHERE key='__tracker_last_heartbeat_ms'")
        .await
        .unwrap();
    assert_eq!(
        read_snapshot(&pool, 0, 30000, 25000, "en-US")
            .await
            .unwrap()
            .records[0]
            .end_ms,
        25000
    );
    pool.execute("DELETE FROM settings").await.unwrap();
    assert!(read_snapshot(&pool, 0, 30000, 25000, "en-US")
        .await
        .unwrap()
        .records
        .is_empty());
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE end_time IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(open, 1);
    pool.close().await;
}

#[tokio::test]
async fn history_rejects_oversized_title_and_escaped_response_without_partial_success() {
    let pool = database().await;
    sqlx::query("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time) VALUES('Editor','editor',?,0,100)").bind("中".repeat(6000)).execute(&pool).await.unwrap();
    assert!(read_snapshot(&pool, 0, 100, 1000, "en-US")
        .await
        .unwrap_err()
        .contains("field budget"));
    pool.execute("DELETE FROM sessions").await.unwrap();
    for i in 0..100 {
        sqlx::query("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time) VALUES('Editor','editor',?,?,?)").bind("\0".repeat(16000)).bind(i*100).bind(i*100+100).execute(&pool).await.unwrap();
    }
    assert!(read_snapshot(&pool, 0, 10000, 10000, "en-US")
        .await
        .unwrap_err()
        .contains("response budget"));
    assert!(!valid_range(0, MAX_HISTORY_RANGE_MS + 1));
    pool.close().await;
}
