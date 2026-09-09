use crate::domain::backup::RestoreStrategy;
use sqlx::{Pool, Sqlite, Transaction};

pub async fn record_receipt(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    archive_sha256: &str,
    strategy: RestoreStrategy,
    completed_at_ms: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO backup_restore_receipts (
            request_id, archive_sha256, strategy, completed_at_ms
         ) VALUES (?, ?, ?, ?)",
    )
    .bind(request_id)
    .bind(archive_sha256)
    .bind(match strategy {
        RestoreStrategy::Replace => "replace",
        RestoreStrategy::Merge => "merge",
    })
    .bind(completed_at_ms)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to record backup restore receipt: {error}"))?;
    Ok(())
}

pub async fn receipt_exists(
    pool: &Pool<Sqlite>,
    request_id: &str,
    archive_sha256: &str,
) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM backup_restore_receipts
         WHERE request_id = ? AND archive_sha256 = ?
         LIMIT 1",
    )
    .bind(request_id)
    .bind(archive_sha256)
    .fetch_optional(pool)
    .await
    .map(|value| value.is_some())
    .map_err(|error| format!("failed to inspect backup restore receipt: {error}"))
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub async fn seed_session(pool: &Pool<Sqlite>, id: i64, exe: &str, start: i64) {
        sqlx::query("INSERT INTO sessions (id,app_name,exe_name,start_time,end_time,duration) VALUES (?,?,?,?,?,1000)")
            .bind(id).bind(exe).bind(exe).bind(start).bind(start + 1000)
            .execute(pool).await.unwrap();
    }

    pub async fn session_names(pool: &Pool<Sqlite>) -> Vec<String> {
        sqlx::query_scalar("SELECT exe_name FROM sessions ORDER BY exe_name")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    pub async fn receipt_count(pool: &Pool<Sqlite>) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM backup_restore_receipts")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    pub async fn inject_source_insert_failure(pool: &Pool<Sqlite>) {
        sqlx::query("CREATE TRIGGER injected_restore_failure BEFORE INSERT ON sessions WHEN NEW.exe_name='source' BEGIN SELECT RAISE(ABORT,'isolated restore failure'); END")
            .execute(pool).await.unwrap();
    }

    pub async fn assert_integrity(pool: &Pool<Sqlite>) {
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA quick_check")
                .fetch_one(pool)
                .await
                .unwrap(),
            "ok"
        );
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(pool)
            .await
            .unwrap()
            .is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[test]
    fn receipt_is_committed_with_the_restore_transaction() {
        tauri::async_runtime::block_on(async {
            let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(crate::data::schema::BACKUP_RESTORE_RECEIPT_SCHEMA_SQL)
                .await
                .unwrap();
            let mut tx = pool.begin().await.unwrap();
            record_receipt(
                &mut tx,
                "restore_1",
                &"a".repeat(64),
                RestoreStrategy::Merge,
                10,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();

            assert!(receipt_exists(&pool, "restore_1", &"a".repeat(64))
                .await
                .unwrap());
        });
    }
}
