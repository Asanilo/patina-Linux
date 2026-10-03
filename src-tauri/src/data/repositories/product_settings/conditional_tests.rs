use super::{
    conditional::{commit, CommitError},
    load_snapshot,
};
use patina_protocol::product_settings::{ProductSettingsCommitRequest, ProductSettingsPatch};
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

#[tokio::test]
async fn pause_samples_owner_clock_after_waiting_for_the_transition_lock() {
    use std::sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    };
    struct Clock(Arc<AtomicI64>);
    impl crate::engine::runtime_context::RuntimeClock for Clock {
        fn now_ms(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }
    let pool = database().await;
    pool.execute(
        "INSERT INTO sessions(app_name,exe_name,start_time) VALUES('Fixture','fixture',1000)",
    )
    .await
    .unwrap();
    let request = ProductSettingsCommitRequest {
        expected_revision: load_snapshot(&pool, 1).await.unwrap().revision,
        patch: ProductSettingsPatch {
            tracking_paused: Some(true),
            ..Default::default()
        },
    };
    let now = Arc::new(AtomicI64::new(1200));
    let context = crate::engine::runtime_context::RuntimeContext::new(
        pool.clone(),
        Arc::new(Clock(now.clone())),
    );
    let state = crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default();
    let guard = state.lock_transition().await;
    let next_state = state.clone();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        entered.send(()).unwrap();
        crate::engine::tracking::runtime_settings::commit_product_settings(
            &context,
            Some(&next_state),
            &request,
        )
        .await
    });
    waiting.await.unwrap();
    now.store(2200, Ordering::SeqCst);
    drop(guard);
    assert_eq!(task.await.unwrap().unwrap().sampled_at_ms, 2200);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT end_time FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2200
    );
    pool.close().await;
}

#[tokio::test]
async fn rejected_revision_and_invalid_policy_never_mutate_settings() {
    let pool = database().await;
    let before = load_snapshot(&pool, 1).await.unwrap();
    let mut request = ProductSettingsCommitRequest {
        expected_revision: "f".repeat(64),
        patch: ProductSettingsPatch {
            min_session_secs: Some(360),
            ..Default::default()
        },
    };
    assert_eq!(
        commit(&pool, &request, 2, 2).await.unwrap_err(),
        CommitError::Conflict
    );
    request.expected_revision = before.revision.clone();
    request.patch.min_session_secs = Some(361);
    assert!(matches!(
        commit(&pool, &request, 2, 2).await,
        Err(CommitError::InvalidInput(_))
    ));
    assert_eq!(
        load_snapshot(&pool, 3).await.unwrap().revision,
        before.revision
    );
    for patch in [
        serde_json::json!({"theme_mode":"dark"}),
        serde_json::json!({"web_activity_token":"secret"}),
        serde_json::json!({"local_api_port":14850}),
    ] {
        assert!(serde_json::from_value::<ProductSettingsCommitRequest>(
            serde_json::json!({"expected_revision":before.revision,"patch":patch})
        )
        .is_err());
    }
    pool.close().await;
}

#[tokio::test]
async fn pause_uses_pending_probe_boundary_and_sql_failure_preserves_policy_and_pending_seal() {
    let pool = database().await;
    pool.execute(
        "INSERT INTO sessions(app_name,exe_name,start_time) VALUES('Fixture','fixture',1000)",
    )
    .await
    .unwrap();
    let state = crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default();
    state.note_probe_interruption(1500);
    let request = ProductSettingsCommitRequest {
        expected_revision: load_snapshot(&pool, 1).await.unwrap().revision,
        patch: ProductSettingsPatch {
            min_session_secs: Some(360),
            tracking_paused: Some(true),
            ..Default::default()
        },
    };
    pool.execute("CREATE TRIGGER fail_seal BEFORE UPDATE ON sessions BEGIN SELECT RAISE(ABORT,'synthetic seal failure'); END;").await.unwrap();
    struct FixedClock;
    impl crate::engine::runtime_context::RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            10_000
        }
    }
    let context = crate::engine::runtime_context::RuntimeContext::new(
        pool.clone(),
        std::sync::Arc::new(FixedClock),
    );
    let owner = crate::engine::tracking::runtime_settings::commit_product_settings;
    assert!(matches!(
        owner(&context, Some(&state), &request).await,
        Err(CommitError::Storage(_))
    ));
    assert_eq!(state.pending_probe_seal(), Some(1500));
    assert_eq!(
        load_snapshot(&pool, 2).await.unwrap().revision,
        request.expected_revision
    );
    assert_eq!(
        sqlx::query_scalar::<_, Option<i64>>("SELECT end_time FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        None
    );
    pool.execute("DROP TRIGGER fail_seal").await.unwrap();
    let snapshot = owner(&context, Some(&state), &request).await.unwrap();
    assert_eq!(snapshot.settings.min_session_secs, 360);
    assert!(snapshot.settings.tracking_paused);
    assert_eq!(state.pending_probe_seal(), None);
    assert_eq!(
        sqlx::query_as::<_, (i64, i64)>("SELECT end_time,duration FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        (1500, 500)
    );
    pool.close().await;
}

#[tokio::test]
async fn two_independent_sqlite_writers_cannot_accept_the_same_revision() {
    let path = std::env::temp_dir().join(format!(
        "patina-product-cas-{}-{}.db",
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
    let left = ProductSettingsCommitRequest {
        expected_revision: revision.clone(),
        patch: ProductSettingsPatch {
            min_session_secs: Some(360),
            ..Default::default()
        },
    };
    let right = ProductSettingsCommitRequest {
        expected_revision: revision,
        patch: ProductSettingsPatch {
            min_session_secs: Some(420),
            ..Default::default()
        },
    };
    let (a, b) = tokio::join!(commit(&first, &left, 2, 2), commit(&second, &right, 2, 2));
    let winner = match (a, b) {
        (Ok(snapshot), Err(CommitError::Conflict)) | (Err(CommitError::Conflict), Ok(snapshot)) => {
            snapshot
        }
        other => panic!("expected one committed writer and one conflict: {other:?}"),
    };
    assert_eq!(
        winner.settings,
        load_snapshot(&second, 3).await.unwrap().settings
    );
    first.close().await;
    second.close().await;
    std::fs::remove_file(path).unwrap();
}
