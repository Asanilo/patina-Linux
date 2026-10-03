use super::*;
use sqlx::Executor;
async fn database() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for schema in [
        crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL,
        crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL,
        crate::data::schema::WEB_ACTIVITY_SCHEMA_SQL,
        crate::data::schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    pool
}
fn request(scope: AppCleanupScope) -> CanonicalAppCleanupRequest {
    CanonicalAppCleanupRequest {
        app_key: "Steam.exe".into(),
        scope,
        confirmed: true,
    }
}
async fn batch(pool: &SqlitePool, id: &str) {
    sqlx::query("INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES(?,0,'fixture','patina-csv',?,0,0)")
        .bind(id).bind(format!("{:0<64}",id)).execute(pool).await.unwrap();
}
async fn seed(pool: &SqlitePool) {
    pool.execute("INSERT INTO sessions(id,app_name,exe_name,start_time,end_time) VALUES(1,'Steam','Steam.exe',100,200),(2,'Helper',' SteamWebHelper.exe ',100,200),(3,'Other','not-steam.exe',100,200); INSERT INTO session_title_samples(session_id,title,start_time,end_time) VALUES(1,'Title',100,200)").await.unwrap();
    batch(pool, "aaa").await;
    batch(pool, "bbb").await;
    batch(pool, "ccc").await;
    sqlx::query("INSERT INTO import_exact_sessions(batch_id,fingerprint,app_name,exe_name,start_time,end_time,duration) VALUES('aaa',?,'Steam','steam.exe',100,200,100)").bind("a".repeat(64)).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES('bbb',?,'Steam','steamwebhelper.exe',100,100)").bind("b".repeat(64)).execute(pool).await.unwrap();
    pool.execute("INSERT INTO web_activity_segments(id,browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,start_time,end_time,created_at,updated_at) VALUES(1,'client','browser','Steam.exe','example.com','example.com',100,200,100,200);INSERT INTO web_activity_native_sessions(segment_id,session_id) VALUES(1,1)").await.unwrap();
}
#[tokio::test]
async fn canonical_cleanup_preserves_unrelated_facts_settings_and_empty_batches() {
    let pool = database().await;
    seed(&pool).await;
    pool.execute("INSERT INTO settings(key,value) VALUES('__app_override::steam.exe','{}')")
        .await
        .unwrap();
    let result = delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
        .await
        .unwrap();
    assert_eq!(result.app_key, "steam.exe");
    assert_eq!(result.matched_executables, 4);
    assert_eq!(
        result.deleted,
        AppTrackingDataCleanupResult {
            sessions_deleted: 2,
            imported_exact_sessions_deleted: 1,
            imported_time_buckets_deleted: 1,
            import_batches_deleted: 2
        }
    );
    for (query, expected) in [
        ("SELECT COUNT(*) FROM sessions", 1),
        ("SELECT COUNT(*) FROM session_title_samples", 0),
        ("SELECT COUNT(*) FROM web_activity_native_sessions", 0),
        ("SELECT COUNT(*) FROM web_activity_segments", 1),
        ("SELECT COUNT(*) FROM settings", 1),
        ("SELECT COUNT(*) FROM import_batches WHERE id='ccc'", 1),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(query)
                .fetch_one(&pool)
                .await
                .unwrap(),
            expected,
            "{query}"
        );
    }
    let again = delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
        .await
        .unwrap();
    assert_eq!(again.matched_executables, 0);
    assert_eq!(again.deleted, AppTrackingDataCleanupResult::default());
    pool.close().await;
}
#[tokio::test]
async fn canonical_cleanup_rejects_unconfirmed_and_rolls_back_partial_deletion() {
    let pool = database().await;
    seed(&pool).await;
    let mut unconfirmed = request(AppCleanupScope::All);
    unconfirmed.confirmed = false;
    assert!(delete_canonical_app(&pool, &unconfirmed, 1000)
        .await
        .unwrap_err()
        .contains("confirmed"));
    pool.execute("CREATE TRIGGER refuse_import_delete BEFORE DELETE ON import_exact_sessions BEGIN SELECT RAISE(ABORT,'synthetic failure'); END").await.unwrap();
    assert!(
        delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
            .await
            .unwrap_err()
            .contains("synthetic failure")
    );
    for (query, expected) in [
        ("SELECT COUNT(*) FROM sessions", 3),
        ("SELECT COUNT(*) FROM session_title_samples", 1),
        ("SELECT COUNT(*) FROM web_activity_native_sessions", 1),
        ("SELECT COUNT(*) FROM import_exact_sessions", 1),
        ("SELECT COUNT(*) FROM import_time_buckets", 1),
        ("SELECT COUNT(*) FROM import_batches", 3),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(query)
                .fetch_one(&pool)
                .await
                .unwrap(),
            expected,
            "{query}"
        );
    }
    pool.close().await;
}
#[tokio::test]
async fn canonical_cleanup_fails_closed_when_name_or_batch_budgets_are_exceeded() {
    let pool = database().await;
    pool.execute("WITH RECURSIVE n(v) AS(SELECT 1 UNION ALL SELECT v+1 FROM n WHERE v<4097) INSERT INTO sessions(app_name,exe_name,start_time,end_time) SELECT 'App','app-'||v,0,1 FROM n").await.unwrap();
    assert!(
        delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
            .await
            .unwrap_err()
            .contains("name budget")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        4097
    );
    pool.execute("DELETE FROM sessions").await.unwrap();
    sqlx::query("INSERT INTO sessions(app_name,exe_name,start_time,end_time) VALUES('Long',?,0,1)")
        .bind("中".repeat(400))
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
            .await
            .unwrap_err()
            .contains("name budget")
    );
    pool.execute("DELETE FROM sessions; INSERT INTO sessions(app_name,exe_name,start_time,end_time) VALUES('Steam','Steam.exe',0,1)").await.unwrap();
    pool.execute("WITH RECURSIVE n(v) AS(SELECT 1 UNION ALL SELECT v+1 FROM n WHERE v<10001) INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) SELECT 'batch-'||v,0,'fixture','patina-csv',printf('%064x',v),1,0 FROM n").await.unwrap();
    pool.execute("INSERT INTO import_exact_sessions(batch_id,fingerprint,app_name,exe_name,start_time,end_time,duration) SELECT id,source_fingerprint,'Steam','steam.exe',0,1,1 FROM import_batches").await.unwrap();
    assert!(
        delete_canonical_app(&pool, &request(AppCleanupScope::All), 1000)
            .await
            .unwrap_err()
            .contains("batch budget")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_exact_sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        10001
    );
    pool.close().await;
}
#[tokio::test]
async fn canonical_today_uses_owner_calendar_and_preserves_cross_day_starts() {
    use chrono::TimeZone;
    let pool = database().await;
    let date = if std::env::var("TZ").unwrap_or_default() == "Australia/Lord_Howe" {
        "2026-04-05"
    } else {
        "2026-11-01"
    };
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
    let now = chrono::Local
        .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
        .single()
        .unwrap()
        .timestamp_millis();
    let next = day.succ_opt().unwrap().format("%Y-%m-%d").to_string();
    let range = crate::domain::daily_activity::local_day_boundaries(date, &next).unwrap();
    match std::env::var("TZ").unwrap_or_default().as_str() {
        "America/New_York" => assert_eq!(range[1] - range[0], 25 * 3_600_000),
        "Australia/Lord_Howe" => assert_eq!(range[1] - range[0], 49 * 1_800_000),
        _ => {}
    }
    for start in [range[0] - 1, range[0], range[1] - 1, range[1]] {
        sqlx::query("INSERT INTO sessions(app_name,exe_name,start_time,end_time) VALUES('Steam','Steam.exe',?,?)").bind(start).bind(start+1000).execute(&pool).await.unwrap();
    }
    let result = delete_canonical_app(&pool, &request(AppCleanupScope::Today), now)
        .await
        .unwrap();
    assert_eq!(result.deleted.sessions_deleted, 2);
    let remaining: Vec<i64> =
        sqlx::query_scalar("SELECT start_time FROM sessions ORDER BY start_time")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, vec![range[0] - 1, range[1]]);
    pool.close().await;
}
