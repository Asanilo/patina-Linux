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
        crate::data::schema::WEB_ACTIVITY_SCHEMA_SQL,
        crate::data::schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    pool
}
async fn insert(
    pool: &SqlitePool,
    id: i64,
    client: &str,
    domain: &str,
    start: i64,
    end: Option<i64>,
    observed: i64,
) {
    sqlx::query("INSERT INTO web_activity_segments(id,browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,url,title,start_time,end_time,created_at,updated_at) VALUES(?,?,'firefox','firefox',?,?,'https://example.com/private?token=secret#fragment','Stored title',?,?,?,?)")
        .bind(id).bind(client).bind(domain).bind(domain).bind(start).bind(end).bind(start).bind(observed).execute(pool).await.unwrap();
}
#[tokio::test]
async fn precise_web_projection_has_shared_metadata_scope_and_source_precedence() {
    let pool = database().await;
    insert(&pool, 1, "one", "example.com", 10, Some(40), 40).await;
    insert(&pool, 2, "one", "example.com", 20, Some(50), 50).await;
    insert(&pool, 3, "two", "example.com", 30, Some(50), 50).await;
    pool.execute(r#"INSERT INTO settings(key,value) VALUES('__web_domain_override::example.com','{"category":"development","displayName":"Docs","color":"aabbcc","enabled":false}'),('local_api_token','must-not-leak'),('web_activity_url_privacy','strip_query')"#).await.unwrap();
    let read = read_snapshot(&pool, 15, 45, 100, "en-US").await.unwrap();
    assert_eq!(
        read.records
            .iter()
            .map(|r| (r.record_id, r.start_ms, r.end_ms))
            .collect::<Vec<_>>(),
        vec![(1, 15, 40), (3, 30, 45), (2, 40, 45)]
    );
    assert!(read.records.iter().all(|r| r.category == "development"
        && r.display_name_override.as_deref() == Some("Docs")
        && !r.recording_enabled));
    assert_eq!(read.records[0].color_override.as_deref(), Some("#AABBCC"));
    assert_eq!(
        read.records[0].url.as_deref(),
        Some("https://example.com/private")
    );
    assert!(!serde_json::to_string(&read)
        .unwrap()
        .contains("must-not-leak"));
    pool.execute("UPDATE settings SET value='domain_only' WHERE key='web_activity_url_privacy'")
        .await
        .unwrap();
    sqlx::query("UPDATE web_activity_segments SET url=?")
        .bind("x".repeat(MAX_WEB_HISTORY_URL_BYTES + 1))
        .execute(&pool)
        .await
        .unwrap();
    let private = read_snapshot(&pool, 15, 45, 100, "en-US").await.unwrap();
    assert!(private.records.iter().all(|r| r.url.is_none()));
    assert_eq!(
        private.classification_revision,
        read.classification_revision
    );
    pool.close().await;
}
#[tokio::test]
async fn web_open_facts_freeze_without_browser_or_owner_evidence_and_reads_never_repair() {
    let pool = database().await;
    pool.execute(
        "INSERT INTO sessions(id,app_name,exe_name,start_time) VALUES(1,'Firefox','firefox',1000)",
    )
    .await
    .unwrap();
    // Use the actual owner key rather than assuming a client setting spelling.
    sqlx::query("INSERT OR REPLACE INTO settings(key,value) VALUES(?, '90000')")
        .bind(super::super::tracker_settings::TRACKER_LAST_HEARTBEAT_KEY)
        .execute(&pool)
        .await
        .unwrap();
    insert(&pool, 1, "one", "example.com", 1000, None, 20000).await;
    pool.execute("INSERT INTO web_activity_native_sessions(segment_id,session_id) VALUES(1,1)")
        .await
        .unwrap();
    let live = read_snapshot(&pool, 0, 200000, 90000, "en-US")
        .await
        .unwrap();
    assert_eq!(live.records[0].end_ms, 90000);
    assert!(live.records[0].is_live);
    let stale = read_snapshot(&pool, 0, 200000, 100000, "en-US")
        .await
        .unwrap();
    assert_eq!(stale.records[0].end_ms, 20000);
    assert!(!stale.records[0].is_live);
    let repeat = read_snapshot(&pool, 0, 200000, 150000, "en-US")
        .await
        .unwrap();
    assert_eq!(repeat.records, stale.records);
    let open: Option<i64> =
        sqlx::query_scalar("SELECT end_time FROM web_activity_segments WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(open.is_none());
    pool.execute("UPDATE web_activity_segments SET updated_at=100000 WHERE id=1")
        .await
        .unwrap();
    sqlx::query("UPDATE settings SET value='100000' WHERE key=?")
        .bind(super::super::tracker_settings::TRACKER_LAST_HEARTBEAT_KEY)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        read_snapshot(&pool, 0, 200000, 100000, "en-US")
            .await
            .unwrap()
            .records[0]
            .end_ms,
        100000
    );
    sqlx::query("DELETE FROM settings WHERE key=?")
        .bind(super::super::tracker_settings::TRACKER_LAST_HEARTBEAT_KEY)
        .execute(&pool)
        .await
        .unwrap();
    assert!(read_snapshot(&pool, 0, 200000, 110000, "en-US")
        .await
        .unwrap()
        .records
        .is_empty());
    pool.close().await;
}
#[tokio::test]
async fn web_projection_rejects_input_text_and_encoded_response_overflow() {
    let pool = database().await;
    insert(&pool, 1, "one", "example.com", 0, Some(10), 10).await;
    sqlx::query("UPDATE web_activity_segments SET title=?")
        .bind("中".repeat(6000))
        .execute(&pool)
        .await
        .unwrap();
    assert!(read_snapshot(&pool, 0, 20, 100, "en-US")
        .await
        .unwrap_err()
        .contains("metadata budget"));
    pool.execute("DELETE FROM web_activity_segments")
        .await
        .unwrap();
    pool.execute("WITH RECURSIVE n(v) AS(SELECT 1 UNION ALL SELECT v+1 FROM n WHERE v<20001) INSERT INTO web_activity_segments(browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,start_time,end_time,created_at,updated_at) SELECT 'one','firefox','firefox','example.com','example.com',0,10,0,10 FROM n").await.unwrap();
    assert!(read_snapshot(&pool, 0, 20, 100, "en-US")
        .await
        .unwrap_err()
        .contains("input fact budget"));
    pool.execute("DELETE FROM web_activity_segments")
        .await
        .unwrap();
    for id in 1..=100 {
        insert(&pool, id, &id.to_string(), "example.com", 0, Some(10), 10).await;
    }
    sqlx::query("UPDATE web_activity_segments SET title=?,url=?")
        .bind("\u{0001}".repeat(16000))
        .bind("x".repeat(30000))
        .execute(&pool)
        .await
        .unwrap();
    assert!(read_snapshot(&pool, 0, 20, 100, "en-US")
        .await
        .unwrap_err()
        .contains("response budget"));
    pool.close().await;
}

#[tokio::test]
async fn browser_source_identity_uses_a_tuple_not_a_delimited_string() {
    let pool = database().await;
    insert(&pool, 1, "a\0b", "example.com", 0, Some(20), 20).await;
    insert(&pool, 2, "a", "example.com", 0, Some(20), 20).await;
    sqlx::query("UPDATE web_activity_segments SET browser_kind=? WHERE id=1")
        .bind("c")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE web_activity_segments SET browser_kind=? WHERE id=2")
        .bind("b\0c")
        .execute(&pool)
        .await
        .unwrap();
    let read = read_snapshot(&pool, 0, 30, 100, "en-US").await.unwrap();
    assert_eq!(read.records.len(), 2);
    assert_eq!(
        read.records
            .iter()
            .map(|r| r.end_ms - r.start_ms)
            .sum::<i64>(),
        40
    );
    pool.close().await;
}
