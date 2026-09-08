use crate::domain::activity_import::{
    record_fingerprint, CanonicalImportRecord, ImportBatchDto, ImportCommitReportDto,
    ImportDeleteReportDto, ImportRecordType,
};
use crate::domain::backup::{BackupImportBatch, BackupImportExactSession, BackupImportTimeBucket};
use sha2::{Digest, Sha256};
use sqlx::{Pool, Row, Sqlite, SqliteConnection, Transaction};
use std::collections::{HashMap, HashSet};

pub async fn fetch_all_for_backup(
    connection: &mut SqliteConnection,
) -> Result<
    (
        Vec<BackupImportBatch>,
        Vec<BackupImportExactSession>,
        Vec<BackupImportTimeBucket>,
    ),
    String,
> {
    let batches = sqlx::query(
        "SELECT id, imported_at, source_name, source_kind, source_fingerprint,
                exact_session_count, hour_bucket_count
         FROM import_batches ORDER BY imported_at, id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to load import batches for backup: {error}"))?
    .into_iter()
    .map(|row| BackupImportBatch {
        id: row.get("id"),
        imported_at: row.get("imported_at"),
        source_name: row.get("source_name"),
        source_kind: row.get("source_kind"),
        source_fingerprint: row.get("source_fingerprint"),
        exact_session_count: row.get("exact_session_count"),
        hour_bucket_count: row.get("hour_bucket_count"),
    })
    .collect();
    let exact = sqlx::query(
        "SELECT id, batch_id, fingerprint, app_name, exe_name, window_title,
                start_time, end_time, duration, source_category
         FROM import_exact_sessions ORDER BY id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to load imported sessions for backup: {error}"))?
    .into_iter()
    .map(|row| BackupImportExactSession {
        id: row.get("id"),
        batch_id: row.get("batch_id"),
        fingerprint: row.get("fingerprint"),
        app_name: row.get("app_name"),
        exe_name: row.get("exe_name"),
        window_title: row.get("window_title"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        duration: row.get("duration"),
        source_category: row.get("source_category"),
    })
    .collect();
    let buckets = sqlx::query(
        "SELECT id, batch_id, fingerprint, app_name, exe_name, bucket_start_time,
                duration, source_category
         FROM import_time_buckets ORDER BY id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to load imported buckets for backup: {error}"))?
    .into_iter()
    .map(|row| BackupImportTimeBucket {
        id: row.get("id"),
        batch_id: row.get("batch_id"),
        fingerprint: row.get("fingerprint"),
        app_name: row.get("app_name"),
        exe_name: row.get("exe_name"),
        bucket_start_time: row.get("bucket_start_time"),
        duration: row.get("duration"),
        source_category: row.get("source_category"),
    })
    .collect();
    Ok((batches, exact, buckets))
}

pub async fn clear_for_restore(tx: &mut Transaction<'_, Sqlite>) -> Result<(), String> {
    sqlx::query("DELETE FROM import_batches")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear imported activity for restore: {error}"))?;
    Ok(())
}

pub async fn insert_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    batches: &[BackupImportBatch],
    exact: &[BackupImportExactSession],
    buckets: &[BackupImportTimeBucket],
) -> Result<(), String> {
    insert_restore_records(tx, batches, exact, buckets, false).await
}

pub async fn insert_missing_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    batches: &[BackupImportBatch],
    exact: &[BackupImportExactSession],
    buckets: &[BackupImportTimeBucket],
) -> Result<(), String> {
    insert_restore_records(tx, batches, exact, buckets, true).await
}

async fn insert_restore_records(
    tx: &mut Transaction<'_, Sqlite>,
    batches: &[BackupImportBatch],
    exact: &[BackupImportExactSession],
    buckets: &[BackupImportTimeBucket],
    merge: bool,
) -> Result<(), String> {
    let mut batch_ids = HashMap::new();
    for batch in batches {
        let mut target_id = batch.id.clone();
        if merge {
            let existing: Option<String> =
                sqlx::query_scalar("SELECT source_fingerprint FROM import_batches WHERE id = ?")
                    .bind(&target_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(|error| format!("failed to inspect restored import batch: {error}"))?;
            if existing
                .as_deref()
                .is_some_and(|value| value != batch.source_fingerprint)
            {
                target_id = restored_batch_id(batch);
            }
        }
        let verb = if merge { "INSERT OR IGNORE" } else { "INSERT" };
        let sql = format!(
            "{verb} INTO import_batches (
                id, imported_at, source_name, source_kind, source_fingerprint,
                exact_session_count, hour_bucket_count
             ) VALUES (?, ?, ?, ?, ?, 0, 0)"
        );
        sqlx::query(&sql)
            .bind(&target_id)
            .bind(batch.imported_at)
            .bind(&batch.source_name)
            .bind(&batch.source_kind)
            .bind(&batch.source_fingerprint)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("failed to restore import batch: {error}"))?;
        batch_ids.insert(batch.id.clone(), target_id);
    }

    for record in exact {
        let batch_id = batch_ids
            .get(&record.batch_id)
            .ok_or_else(|| "backup imported session references a missing batch".to_string())?;
        let verb = if merge { "INSERT OR IGNORE" } else { "INSERT" };
        let sql = format!(
            "{verb} INTO import_exact_sessions (
                batch_id, fingerprint, app_name, exe_name, window_title,
                start_time, end_time, duration, source_category
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        );
        sqlx::query(&sql)
            .bind(batch_id)
            .bind(&record.fingerprint)
            .bind(&record.app_name)
            .bind(&record.exe_name)
            .bind(&record.window_title)
            .bind(record.start_time)
            .bind(record.end_time)
            .bind(record.duration)
            .bind(&record.source_category)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("failed to restore imported session: {error}"))?;
    }
    for record in buckets {
        let batch_id = batch_ids
            .get(&record.batch_id)
            .ok_or_else(|| "backup imported bucket references a missing batch".to_string())?;
        let verb = if merge { "INSERT OR IGNORE" } else { "INSERT" };
        let sql = format!(
            "{verb} INTO import_time_buckets (
                batch_id, fingerprint, app_name, exe_name, bucket_start_time,
                duration, source_category
             ) VALUES (?, ?, ?, ?, ?, ?, ?)"
        );
        sqlx::query(&sql)
            .bind(batch_id)
            .bind(&record.fingerprint)
            .bind(&record.app_name)
            .bind(&record.exe_name)
            .bind(record.bucket_start_time)
            .bind(record.duration)
            .bind(&record.source_category)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("failed to restore imported bucket: {error}"))?;
    }
    sqlx::query(
        "UPDATE import_batches
         SET exact_session_count = (SELECT COUNT(*) FROM import_exact_sessions WHERE batch_id = import_batches.id),
             hour_bucket_count = (SELECT COUNT(*) FROM import_time_buckets WHERE batch_id = import_batches.id)",
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to reconcile restored import batches: {error}"))?;
    sqlx::query(
        "DELETE FROM import_batches WHERE exact_session_count = 0 AND hour_bucket_count = 0",
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to remove empty restored import batches: {error}"))?;
    Ok(())
}

fn restored_batch_id(batch: &BackupImportBatch) -> String {
    let mut digest = Sha256::new();
    digest.update(batch.id.as_bytes());
    digest.update(batch.source_fingerprint.as_bytes());
    format!("restore-{:x}", digest.finalize())
}

pub async fn list(pool: &Pool<Sqlite>) -> Result<Vec<ImportBatchDto>, String> {
    let rows = sqlx::query(
        "SELECT b.id, b.imported_at, b.source_name, b.source_kind,
                (SELECT COUNT(*) FROM import_exact_sessions e WHERE e.batch_id = b.id)
                    AS exact_sessions,
                (SELECT COUNT(*) FROM import_time_buckets h WHERE h.batch_id = b.id)
                    AS hour_buckets
         FROM import_batches b
         ORDER BY b.imported_at DESC, b.id DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to list import batches: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let exact_sessions = row.get::<i64, _>("exact_sessions");
            let hour_buckets = row.get::<i64, _>("hour_buckets");
            ImportBatchDto {
                id: row.get("id"),
                imported_at: row.get("imported_at"),
                source_name: row.get("source_name"),
                source_kind: row.get("source_kind"),
                exact_sessions,
                hour_buckets,
                total_records: exact_sessions + hour_buckets,
            }
        })
        .collect())
}

pub async fn delete(pool: &Pool<Sqlite>, batch_id: &str) -> Result<ImportDeleteReportDto, String> {
    if batch_id.is_empty() {
        return Err("import batch id cannot be empty".to_string());
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to begin import batch deletion: {error}"))?;
    let counts = sqlx::query(
        "SELECT
            (SELECT COUNT(*) FROM import_exact_sessions WHERE batch_id = ?) AS exact_sessions,
            (SELECT COUNT(*) FROM import_time_buckets WHERE batch_id = ?) AS hour_buckets",
    )
    .bind(batch_id)
    .bind(batch_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| format!("failed to inspect import batch: {error}"))?;
    let deleted_exact_sessions = counts.get::<i64, _>("exact_sessions");
    let deleted_hour_buckets = counts.get::<i64, _>("hour_buckets");
    let deleted = sqlx::query("DELETE FROM import_batches WHERE id = ?")
        .bind(batch_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to delete import batch: {error}"))?;
    if deleted.rows_affected() != 1 {
        tx.rollback().await.map_err(|error| {
            format!("failed to close missing import batch transaction: {error}")
        })?;
        return Err("import batch no longer exists".to_string());
    }
    tx.commit()
        .await
        .map_err(|error| format!("failed to commit import batch deletion: {error}"))?;
    Ok(ImportDeleteReportDto {
        deleted_exact_sessions,
        deleted_hour_buckets,
    })
}

pub async fn load_fingerprints(pool: &Pool<Sqlite>) -> Result<HashSet<String>, String> {
    sqlx::query(
        "SELECT fingerprint FROM import_exact_sessions
         UNION ALL
         SELECT fingerprint FROM import_time_buckets",
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row.get("fingerprint")).collect())
    .map_err(|error| format!("failed to load imported record identities: {error}"))
}

pub async fn commit_records(
    pool: &Pool<Sqlite>,
    source_name: &str,
    source_fingerprint: &str,
    records: &[CanonicalImportRecord],
    error_records: usize,
) -> Result<ImportCommitReportDto, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to begin canonical import: {error}"))?;
    let mut known_fingerprints = load_fingerprints_in_tx(&mut tx).await?;
    let mut duplicate_records = 0usize;
    let mut new_records = Vec::with_capacity(records.len());
    for record in records {
        if !known_fingerprints.insert(record_fingerprint(record)) {
            duplicate_records += 1;
        } else {
            new_records.push(record);
        }
    }

    if new_records.is_empty() {
        tx.rollback()
            .await
            .map_err(|error| format!("failed to close empty import transaction: {error}"))?;
        return Ok(ImportCommitReportDto {
            batch_id: None,
            imported_records: 0,
            duplicate_records,
            error_records,
            exact_sessions: 0,
            hour_buckets: 0,
        });
    }

    let imported_at = now_ms();
    let batch_id = build_batch_id(source_fingerprint, imported_at);
    let exact_sessions = new_records
        .iter()
        .filter(|record| record.record_type == ImportRecordType::ExactSession)
        .count();
    let hour_buckets = new_records.len() - exact_sessions;
    sqlx::query(
        "INSERT INTO import_batches (
            id, imported_at, source_name, source_kind, source_fingerprint,
            exact_session_count, hour_bucket_count
         ) VALUES (?, ?, ?, 'patina-csv', ?, ?, ?)",
    )
    .bind(&batch_id)
    .bind(imported_at)
    .bind(source_name)
    .bind(source_fingerprint)
    .bind(exact_sessions as i64)
    .bind(hour_buckets as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to create import batch: {error}"))?;

    for record in new_records {
        match record.record_type {
            ImportRecordType::ExactSession => {
                insert_exact_record(&mut tx, &batch_id, record).await?
            }
            ImportRecordType::HourBucket => insert_hour_bucket(&mut tx, &batch_id, record).await?,
        }
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit canonical import: {error}"))?;
    Ok(ImportCommitReportDto {
        batch_id: Some(batch_id),
        imported_records: exact_sessions + hour_buckets,
        duplicate_records,
        error_records,
        exact_sessions,
        hour_buckets,
    })
}

async fn load_fingerprints_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<HashSet<String>, String> {
    sqlx::query(
        "SELECT fingerprint FROM import_exact_sessions
         UNION ALL
         SELECT fingerprint FROM import_time_buckets",
    )
    .fetch_all(&mut **tx)
    .await
    .map(|rows| rows.into_iter().map(|row| row.get("fingerprint")).collect())
    .map_err(|error| format!("failed to load imported record identities: {error}"))
}

async fn insert_exact_record(
    tx: &mut Transaction<'_, Sqlite>,
    batch_id: &str,
    record: &CanonicalImportRecord,
) -> Result<(), String> {
    let end_time = record
        .end_time_ms
        .ok_or_else(|| "exact import record is missing end_time".to_string())?;
    sqlx::query(
        "INSERT INTO import_exact_sessions (
            batch_id, fingerprint, app_name, exe_name, window_title,
            start_time, end_time, duration, source_category
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(batch_id)
    .bind(record_fingerprint(record))
    .bind(record.app_name.as_deref().unwrap_or(&record.exe_name))
    .bind(&record.exe_name)
    .bind(record.title.as_deref().unwrap_or(""))
    .bind(record.start_time_ms)
    .bind(end_time)
    .bind(record.duration_ms)
    .bind(&record.category)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to insert exact imported session: {error}"))?;
    Ok(())
}

async fn insert_hour_bucket(
    tx: &mut Transaction<'_, Sqlite>,
    batch_id: &str,
    record: &CanonicalImportRecord,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO import_time_buckets (
            batch_id, fingerprint, app_name, exe_name, bucket_start_time,
            duration, source_category
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(batch_id)
    .bind(record_fingerprint(record))
    .bind(record.app_name.as_deref().unwrap_or(&record.exe_name))
    .bind(&record.exe_name)
    .bind(record.start_time_ms)
    .bind(record.duration_ms)
    .bind(&record.category)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to insert imported hour bucket: {error}"))?;
    Ok(())
}

fn build_batch_id(source_fingerprint: &str, imported_at: i64) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let mut digest = Sha256::new();
    digest.update(source_fingerprint.as_bytes());
    digest.update(imported_at.to_le_bytes());
    digest.update(nonce.to_le_bytes());
    format!("import-{:x}", digest.finalize())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema::{ACTIVITY_IMPORT_SCHEMA_SQL, CURRENT_BASELINE_SCHEMA_SQL};
    use sqlx::Executor;

    async fn setup_pool() -> Pool<Sqlite> {
        let pool = Pool::<Sqlite>::connect("sqlite::memory:").await.unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        pool.execute(CURRENT_BASELINE_SCHEMA_SQL).await.unwrap();
        pool.execute(ACTIVITY_IMPORT_SCHEMA_SQL).await.unwrap();
        pool
    }

    fn exact(start: i64) -> CanonicalImportRecord {
        CanonicalImportRecord {
            source_line: 2,
            record_type: ImportRecordType::ExactSession,
            start_time_ms: start,
            end_time_ms: Some(start + 1_000),
            duration_ms: 1_000,
            exe_name: "org.example.App".to_string(),
            app_name: Some("Example".to_string()),
            title: Some("Work".to_string()),
            category: None,
        }
    }

    #[tokio::test]
    async fn commit_is_deduplicated_and_does_not_write_native_sessions() {
        let pool = setup_pool().await;
        let records = vec![exact(1_000), exact(1_000)];
        let report = commit_records(&pool, "activity.csv", &"a".repeat(64), &records, 0)
            .await
            .unwrap();
        assert_eq!(report.imported_records, 1);
        assert_eq!(report.duplicate_records, 1);
        let native_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(native_count, 0);
    }

    #[tokio::test]
    async fn deleting_one_batch_cascades_only_that_batch() {
        let pool = setup_pool().await;
        let first = commit_records(&pool, "one.csv", &"a".repeat(64), &[exact(1_000)], 0)
            .await
            .unwrap();
        let second = commit_records(&pool, "two.csv", &"b".repeat(64), &[exact(3_000)], 0)
            .await
            .unwrap();
        delete(&pool, first.batch_id.as_deref().unwrap())
            .await
            .unwrap();
        let starts: Vec<i64> =
            sqlx::query_scalar("SELECT start_time FROM import_exact_sessions ORDER BY start_time")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(starts, vec![3_000]);
        assert!(second.batch_id.is_some());
    }
}
