use super::*;
use sqlx::{Executor, SqlitePool};

async fn database() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    pool
}
async fn put(pool: &SqlitePool, key: &str, value: &str) {
    sqlx::query("INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn snapshot_is_bounded_read_only_and_never_includes_credentials() {
    let pool = database().await;
    put(&pool, "local_api_token", "secret").await;
    put(&pool, "web_activity_token", "secret").await;
    put(&pool, "remote_status_bridge_token", "secret").await;
    put(&pool, "XXappXoverride::not-a-classification-key", "secret").await;
    put(
        &pool,
        "__app_override::editor",
        r#"{"displayName":"Editor"}"#,
    )
    .await;
    put(
        &pool,
        "__category_label_override::custom:work",
        "Work\0With exact bytes",
    )
    .await;
    put(&pool, "__deleted_category::", "legacy empty suffix").await;
    put(
        &pool,
        &format!("__deleted_category::{}", "中".repeat(100)),
        "legacy long key",
    )
    .await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    let snapshot = load_classification_snapshot(&pool, 1).await.unwrap();
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(snapshot.entries[1].value, "Work\0With exact bytes");
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("secret"));
    let later = load_classification_snapshot(&pool, 2).await.unwrap();
    assert_eq!(snapshot.revision, later.revision);
    assert_ne!(snapshot.sampled_at_ms, later.sampled_at_ms);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn conditional_writes_reject_stale_revisions_without_partial_changes() {
    let pool = database().await;
    let baseline = load_classification_snapshot(&pool, 1).await.unwrap();
    let edit = ClassificationSettingMutation {
        key: "__category_label_override::custom:work".into(),
        value: Some("Work".into()),
    };
    let committed = commit_classification_if_revision(&pool, &[edit], &baseline.revision, 2)
        .await
        .unwrap();
    assert_ne!(committed.revision.as_ref().unwrap(), &baseline.revision);
    let stale = [ClassificationSettingMutation {
        key: "__custom_category::custom:lost".into(),
        value: Some("1".into()),
    }];
    assert_eq!(
        commit_classification_if_revision(&pool, &stale, &baseline.revision, 3)
            .await
            .unwrap_err(),
        ConditionalCommitError::Conflict
    );
    let current = load_classification_snapshot(&pool, 4).await.unwrap();
    assert_eq!(current.entries.len(), 1);
    assert_eq!(Some(current.revision), committed.revision);
    assert!(matches!(
        commit_classification_if_revision(&pool, &[], "bad", 5).await,
        Err(ConditionalCommitError::InvalidInput(_))
    ));
}

#[tokio::test]
async fn concurrent_clients_cannot_both_commit_the_same_revision() {
    let root = std::env::temp_dir().join(format!(
        "patina-classification-cas-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("fixture.db"))
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(3)),
        )
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    let base = load_classification_snapshot(&pool, 1).await.unwrap();
    let a = [ClassificationSettingMutation {
        key: "__custom_category::custom:a".into(),
        value: Some("1".into()),
    }];
    let b = [ClassificationSettingMutation {
        key: "__custom_category::custom:b".into(),
        value: Some("1".into()),
    }];
    let (left, right) = tokio::join!(
        commit_classification_if_revision(&pool, &a, &base.revision, 2),
        commit_classification_if_revision(&pool, &b, &base.revision, 2)
    );
    assert_eq!([&left, &right].into_iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        [left, right]
            .into_iter()
            .filter(|r| *r == Err(ConditionalCommitError::Conflict))
            .count(),
        1
    );
    assert_eq!(
        load_classification_snapshot(&pool, 3)
            .await
            .unwrap()
            .entries
            .len(),
        1
    );
    pool.close().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn snapshot_rejects_oversized_values_entries_and_encoded_responses() {
    let pool = database().await;
    put(
        &pool,
        "__category_label_override::custom:huge",
        &"x".repeat(4097),
    )
    .await;
    assert!(load_classification_snapshot(&pool, 1).await.is_err());
    pool.execute("DELETE FROM settings").await.unwrap();
    pool.execute("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<20000) INSERT INTO settings(key,value) SELECT '__custom_category::custom:'||i,'1' FROM n").await.unwrap();
    assert!(load_classification_snapshot(&pool, 1)
        .await
        .unwrap_err()
        .contains("entry budget"));
    pool.execute("DELETE FROM settings").await.unwrap();
    for i in 0..180 {
        put(
            &pool,
            &format!("__category_label_override::custom:{i}"),
            &"\0".repeat(4000),
        )
        .await;
    }
    assert!(load_classification_snapshot(&pool, 1)
        .await
        .unwrap_err()
        .contains("response budget"));
}

#[tokio::test]
async fn conditional_commit_rolls_back_when_the_result_exceeds_the_budget() {
    let pool = database().await;
    // Raw strings fit the stored-value budget, but JSON escaping expands them.
    for i in 0..173 {
        put(
            &pool,
            &format!("__category_label_override::custom:{i}"),
            &"\0".repeat(4000),
        )
        .await;
    }
    let before = load_classification_snapshot(&pool, 1).await.unwrap();
    let mutations = (173..180)
        .map(|i| ClassificationSettingMutation {
            key: format!("__category_label_override::custom:{i}"),
            value: Some("\0".repeat(4000)),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        commit_classification_if_revision(&pool, &mutations, &before.revision, 2).await,
        Err(ConditionalCommitError::Storage(_))
    ));
    assert_eq!(
        load_classification_snapshot(&pool, 3)
            .await
            .unwrap()
            .revision,
        before.revision
    );
}
