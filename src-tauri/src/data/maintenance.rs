use crate::domain::data_maintenance::{TrackingDataCleanupResult, WindowTitleCleanupResult};
use sqlx::{Pool, Sqlite};

pub async fn delete_tracking_data_before(
    pool: &Pool<Sqlite>,
    cutoff_time_ms: i64,
) -> Result<TrackingDataCleanupResult, String> {
    if cutoff_time_ms < 0 {
        return Err("tracking data cleanup cutoff must not be negative".to_string());
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start tracking data cleanup: {error}"))?;
    let title_samples_deleted = sqlx::query(
        "DELETE FROM session_title_samples
         WHERE session_id IN (SELECT id FROM sessions WHERE start_time < ?)",
    )
    .bind(cutoff_time_ms)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to delete old session title samples: {error}"))?
    .rows_affected();
    let sessions_deleted = sqlx::query("DELETE FROM sessions WHERE start_time < ?")
        .bind(cutoff_time_ms)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to delete old sessions: {error}"))?
        .rows_affected();
    let web_activity_segments_deleted =
        sqlx::query("DELETE FROM web_activity_segments WHERE start_time < ?")
            .bind(cutoff_time_ms)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to delete old web activity: {error}"))?
            .rows_affected();
    let imported_exact_sessions_deleted =
        sqlx::query("DELETE FROM import_exact_sessions WHERE start_time < ?")
            .bind(cutoff_time_ms)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to delete old imported sessions: {error}"))?
            .rows_affected();
    let imported_time_buckets_deleted =
        sqlx::query("DELETE FROM import_time_buckets WHERE bucket_start_time < ?")
            .bind(cutoff_time_ms)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to delete old imported time buckets: {error}"))?
            .rows_affected();
    sqlx::query(
        "UPDATE import_batches
         SET exact_session_count = (
               SELECT COUNT(*) FROM import_exact_sessions
               WHERE batch_id = import_batches.id
             ),
             hour_bucket_count = (
               SELECT COUNT(*) FROM import_time_buckets
               WHERE batch_id = import_batches.id
             )",
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to update imported activity batch counts: {error}"))?;
    let import_batches_deleted = sqlx::query(
        "DELETE FROM import_batches
         WHERE exact_session_count = 0 AND hour_bucket_count = 0",
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to delete empty imported activity batches: {error}"))?
    .rows_affected();
    tx.commit()
        .await
        .map_err(|error| format!("failed to commit tracking data cleanup: {error}"))?;

    Ok(TrackingDataCleanupResult {
        title_samples_deleted,
        sessions_deleted,
        web_activity_segments_deleted,
        imported_exact_sessions_deleted,
        imported_time_buckets_deleted,
        import_batches_deleted,
    })
}

pub async fn clear_all_window_titles(
    pool: &Pool<Sqlite>,
) -> Result<WindowTitleCleanupResult, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start window title cleanup: {error}"))?;
    let title_samples_deleted = sqlx::query("DELETE FROM session_title_samples")
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to delete session title samples: {error}"))?
        .rows_affected();
    let sessions_redacted =
        sqlx::query("UPDATE sessions SET window_title = '' WHERE COALESCE(window_title, '') <> ''")
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to clear session window titles: {error}"))?
            .rows_affected();
    let imported_exact_sessions_redacted =
        sqlx::query("UPDATE import_exact_sessions SET window_title = '' WHERE window_title <> ''")
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to clear imported session window titles: {error}"))?
            .rows_affected();
    tx.commit()
        .await
        .map_err(|error| format!("failed to commit window title cleanup: {error}"))?;

    Ok(WindowTitleCleanupResult {
        title_samples_deleted,
        sessions_redacted,
        imported_exact_sessions_redacted,
    })
}

pub async fn delete_app_tracking_data(
    pool: &Pool<Sqlite>,
    exe_names: &[String],
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
) -> Result<(), String> {
    if exe_names.is_empty() || exe_names.len() > 512 {
        return Err("application cleanup requires between 1 and 512 executable names".to_string());
    }
    let exe_names = exe_names
        .iter()
        .map(|value| value.trim())
        .collect::<Vec<_>>();
    if exe_names
        .iter()
        .any(|value| value.is_empty() || value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err("application cleanup contains an invalid executable name".to_string());
    }
    match (start_time_ms, end_time_ms) {
        (None, None) => {}
        (Some(start), Some(end)) if start >= 0 && end > start => {}
        _ => return Err("application cleanup requires a valid complete time range".to_string()),
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start application data cleanup: {error}"))?;
    delete_app_rows(
        &mut tx,
        "sessions",
        "start_time",
        &exe_names,
        start_time_ms,
        end_time_ms,
    )
    .await?;
    delete_app_rows(
        &mut tx,
        "import_exact_sessions",
        "start_time",
        &exe_names,
        start_time_ms,
        end_time_ms,
    )
    .await?;
    delete_app_rows(
        &mut tx,
        "import_time_buckets",
        "bucket_start_time",
        &exe_names,
        start_time_ms,
        end_time_ms,
    )
    .await?;
    sqlx::query(
        "UPDATE import_batches
         SET exact_session_count = (
               SELECT COUNT(*) FROM import_exact_sessions
               WHERE batch_id = import_batches.id
             ),
             hour_bucket_count = (
               SELECT COUNT(*) FROM import_time_buckets
               WHERE batch_id = import_batches.id
             )",
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to update imported activity batch counts: {error}"))?;
    sqlx::query(
        "DELETE FROM import_batches
         WHERE exact_session_count = 0 AND hour_bucket_count = 0",
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to delete empty imported activity batches: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("failed to commit application data cleanup: {error}"))
}

async fn delete_app_rows(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    table: &'static str,
    time_column: &'static str,
    exe_names: &[&str],
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
) -> Result<(), String> {
    let mut query =
        sqlx::QueryBuilder::<Sqlite>::new(format!("DELETE FROM {table} WHERE exe_name IN ("));
    {
        let mut separated = query.separated(", ");
        for exe_name in exe_names {
            separated.push_bind(*exe_name);
        }
        separated.push_unseparated(")");
    }
    if let (Some(start), Some(end)) = (start_time_ms, end_time_ms) {
        query
            .push(format!(" AND {time_column} >= "))
            .push_bind(start)
            .push(format!(" AND {time_column} < "))
            .push_bind(end);
    }
    query
        .build()
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to delete application data from {table}: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    async fn test_pool() -> Pool<Sqlite> {
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
        pool.execute(crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    async fn seed(pool: &Pool<Sqlite>) {
        sqlx::query(
            "INSERT INTO sessions (id, app_name, exe_name, window_title, start_time, end_time, duration)
             VALUES (1, 'Old', 'old', 'Old title', 1000, 2000, 1000),
                    (2, 'New', 'new', 'New title', 3000, 4000, 1000)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO import_batches (
                 id, imported_at, source_name, source_kind, source_fingerprint,
                 exact_session_count, hour_bucket_count
             ) VALUES ('old-batch', 1000, 'old.csv', 'patina-csv', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 1, 1),
                      ('new-batch', 3000, 'new.csv', 'patina-csv', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO import_exact_sessions (
                 batch_id, fingerprint, app_name, exe_name, window_title,
                 start_time, end_time, duration
             ) VALUES ('old-batch', 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'Old import', 'old-import', 'Old imported title', 1000, 2000, 1000),
                      ('new-batch', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 'New import', 'new-import', 'New imported title', 3000, 4000, 1000)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO import_time_buckets (
                 batch_id, fingerprint, app_name, exe_name, bucket_start_time, duration
             ) VALUES ('old-batch', 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee', 'Old bucket', 'old-bucket', 1000, 1000),
                      ('new-batch', 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff', 'New bucket', 'new-bucket', 3000, 1000)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO session_title_samples (session_id, title, start_time, end_time)
             VALUES (1, 'Old title', 1000, 2000), (2, 'New title', 3000, 4000)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO web_activity_segments (
                 browser_client_id, browser_kind, browser_exe_name, domain, normalized_domain,
                 start_time, end_time, duration, created_at, updated_at, source
             ) VALUES
                 ('old', 'firefox', 'zen', 'old.example', 'old.example', 1000, 2000, 1000, 1000, 2000, 'extension'),
                 ('new', 'firefox', 'zen', 'new.example', 'new.example', 3000, 4000, 1000, 3000, 4000, 'extension')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cleanup_deletes_related_rows_by_session_start_in_one_operation() {
        let pool = test_pool().await;
        seed(&pool).await;

        let result = delete_tracking_data_before(&pool, 2_500).await.unwrap();

        assert_eq!(result.sessions_deleted, 1);
        assert_eq!(result.title_samples_deleted, 1);
        assert_eq!(result.web_activity_segments_deleted, 1);
        assert_eq!(result.imported_exact_sessions_deleted, 1);
        assert_eq!(result.imported_time_buckets_deleted, 1);
        assert_eq!(result.import_batches_deleted, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_title_samples")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM web_activity_segments")
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
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_time_buckets")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_batches")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn title_cleanup_redacts_sessions_and_removes_samples() {
        let pool = test_pool().await;
        seed(&pool).await;

        let result = clear_all_window_titles(&pool).await.unwrap();

        assert_eq!(result.sessions_redacted, 2);
        assert_eq!(result.title_samples_deleted, 2);
        assert_eq!(result.imported_exact_sessions_redacted, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE COALESCE(window_title, '') <> ''",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM import_exact_sessions WHERE window_title <> ''",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_title_samples")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn cleanup_rejects_negative_cutoff_before_writing() {
        let pool = test_pool().await;
        seed(&pool).await;

        assert!(delete_tracking_data_before(&pool, -1).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn app_cleanup_removes_native_and_imported_rows_in_one_transaction() {
        let pool = test_pool().await;
        seed(&pool).await;

        delete_app_tracking_data(
            &pool,
            &[
                "old".to_string(),
                "old-import".to_string(),
                "old-bucket".to_string(),
            ],
            None,
            None,
        )
        .await
        .unwrap();

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
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_time_buckets")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM import_batches")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn app_cleanup_rejects_partial_or_reversed_ranges() {
        let pool = test_pool().await;
        seed(&pool).await;
        let apps = ["old".to_string()];

        assert!(delete_app_tracking_data(&pool, &apps, Some(1_000), None)
            .await
            .is_err());
        assert!(
            delete_app_tracking_data(&pool, &apps, Some(2_000), Some(1_000))
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
        pool.close().await;
    }
}
