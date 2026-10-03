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
    pool.execute(crate::data::schema::WEB_ACTIVITY_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn read_is_pure_bounded_and_legacy_credential_rotation_invalidates_revision() {
    let pool = database().await;
    let initial = load_snapshot(&pool, 1).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let no_op = commit(&pool, &initial.revision, None, None, 2)
        .await
        .unwrap();
    assert_eq!(no_op.snapshot.revision, initial.revision);
    let mut browser = WebActivityBridgeSettings {
        enabled: true,
        port: 12345,
        token: "first-sensitive-token".into(),
    };
    super::super::app_settings::save_web_activity_runtime_settings(
        &pool,
        &browser,
        settings::WebActivityUrlPrivacyMode::Full,
    )
    .await
    .unwrap();
    let before_rotation = load_snapshot(&pool, 3).await.unwrap();
    browser.token = "second-sensitive-token".into();
    super::super::app_settings::save_web_activity_runtime_settings(
        &pool,
        &browser,
        settings::WebActivityUrlPrivacyMode::Full,
    )
    .await
    .unwrap();
    let after_rotation = load_snapshot(&pool, 4).await.unwrap();
    assert_eq!(
        before_rotation.browser_activity,
        after_rotation.browser_activity
    );
    assert_ne!(before_rotation.revision, after_rotation.revision);
    assert!(matches!(
        commit(&pool, &before_rotation.revision, Some(false), None, 5).await,
        Err(CommitError::Conflict)
    ));
    let json = serde_json::to_string(&after_rotation).unwrap();
    assert!(!json.contains("sensitive-token"));
    assert!(!json.contains(GENERATION_KEY));
    assert!(json.len() < patina_protocol::resource_settings::MAX_RESOURCE_SETTINGS_RESPONSE_BYTES);
    // Restoring an older archive keeps this host's current credential AND generation.
    let archive = [
        ("web_activity_enabled", "1"),
        ("web_activity_port", "12345"),
        ("web_activity_token", "archived-sensitive-token"),
        (GENERATION_KEY, "1"),
    ]
    .map(|(key, value)| crate::domain::backup::BackupSetting {
        key: key.into(),
        value: value.into(),
    });
    let mut tx = pool.begin().await.unwrap();
    super::super::settings::replace_for_restore_preserving_host_integrations(&mut tx, &archive)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(load_snapshot(&pool, 4).await.unwrap(), after_rotation);
    assert_eq!(
        load_state(&pool, 4).await.unwrap().browser.token,
        "second-sensitive-token"
    );
    assert!(matches!(
        commit(&pool, &before_rotation.revision, Some(false), None, 5).await,
        Err(CommitError::Conflict)
    ));
    sqlx::query("UPDATE settings SET value=? WHERE key='web_activity_token'")
        .bind("x".repeat(4097))
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_snapshot(&pool, 6)
        .await
        .unwrap_err()
        .contains("value budget"));
    pool.close().await;
}

#[tokio::test]
async fn seal_failure_rolls_back_audio_browser_and_generation() {
    let pool = database().await;
    let browser = WebActivityBridgeSettings {
        enabled: true,
        port: 12345,
        token: "test-secret".into(),
    };
    super::super::app_settings::save_web_activity_runtime_settings(
        &pool,
        &browser,
        settings::WebActivityUrlPrivacyMode::Full,
    )
    .await
    .unwrap();
    pool.execute("INSERT INTO web_activity_segments
        (browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,start_time,source,created_at,updated_at)
        VALUES ('test','chrome','chrome','example.org','example.org',1000,'browser-extension',1000,1000)").await.unwrap();
    pool.execute("CREATE TRIGGER reject_seal BEFORE UPDATE OF end_time ON web_activity_segments BEGIN SELECT RAISE(ABORT,'injected'); END").await.unwrap();
    let before = load_snapshot(&pool, 2000).await.unwrap();
    let disabled = WebActivityBridgeSettings {
        enabled: false,
        ..browser
    };
    assert!(matches!(
        commit(
            &pool,
            &before.revision,
            Some(false),
            Some((&disabled, settings::WebActivityUrlPrivacyMode::DomainOnly)),
            2000
        )
        .await,
        Err(CommitError::Storage(_))
    ));
    assert_eq!(load_snapshot(&pool, 2000).await.unwrap(), before);
    pool.execute("DROP TRIGGER reject_seal").await.unwrap();
    let result = commit(
        &pool,
        &before.revision,
        Some(false),
        Some((&disabled, settings::WebActivityUrlPrivacyMode::DomainOnly)),
        2000,
    )
    .await
    .unwrap();
    assert!(result.sealed);
    assert!(!result.snapshot.audio_participation_enabled);
    assert!(!result.snapshot.browser_activity.enabled);
    let row: (i64, i64) = sqlx::query_as("SELECT end_time,duration FROM web_activity_segments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row, (2000, 1000));
    pool.close().await;
}

#[tokio::test]
async fn separate_sqlite_writers_only_accept_one_revision() {
    let path = std::env::temp_dir().join(format!(
        "patina-resource-cas-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(3));
    let first = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    first
        .execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    let second = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let revision = load_snapshot(&first, 1).await.unwrap().revision;
    let (a, b) = tokio::join!(
        commit(&first, &revision, Some(false), None, 2),
        commit(&second, &revision, Some(true), None, 2)
    );
    let winner = match (a, b) {
        (Ok(result), Err(CommitError::Conflict)) | (Err(CommitError::Conflict), Ok(result)) => {
            result.snapshot
        }
        _ => panic!("expected one committed resource writer and one stale revision"),
    };
    assert_eq!(load_snapshot(&second, 2).await.unwrap(), winner);
    first.close().await;
    second.close().await;
    std::fs::remove_file(path).unwrap();
}
