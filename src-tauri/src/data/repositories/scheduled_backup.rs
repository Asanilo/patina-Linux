use crate::domain::backup_schedule::{
    ScheduledBackupCadence, ScheduledBackupConfig, ScheduledBackupRun, SCHEDULED_BACKUP_KEEP_COUNT,
};
use sqlx::{Pool, Row, Sqlite};

pub async fn load_config(pool: &Pool<Sqlite>) -> Result<Option<ScheduledBackupConfig>, String> {
    let row = sqlx::query(
        "SELECT enabled, cadence, weekday, local_time_minutes, target_dir,
                target_generation, schedule_anchor_at_ms, updated_at_ms
         FROM scheduled_backup_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load scheduled backup configuration: {error}"))?;
    row.map(config_from_row).transpose()
}

pub async fn save_config(
    pool: &Pool<Sqlite>,
    config: &ScheduledBackupConfig,
) -> Result<(), String> {
    config.validate()?;
    sqlx::query(
        "INSERT INTO scheduled_backup_config (
            id, enabled, cadence, weekday, local_time_minutes, target_dir, retention_count,
            target_generation, schedule_anchor_at_ms, updated_at_ms
         ) VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            enabled = excluded.enabled,
            cadence = excluded.cadence,
            weekday = excluded.weekday,
            local_time_minutes = excluded.local_time_minutes,
            target_dir = excluded.target_dir,
            retention_count = excluded.retention_count,
            target_generation = excluded.target_generation,
            schedule_anchor_at_ms = excluded.schedule_anchor_at_ms,
            updated_at_ms = excluded.updated_at_ms",
    )
    .bind(i64::from(config.enabled))
    .bind(config.cadence.as_str())
    .bind(config.weekday.map(i64::from))
    .bind(i64::from(config.local_time_minutes))
    .bind(&config.target_dir)
    .bind(i64::from(SCHEDULED_BACKUP_KEEP_COUNT))
    .bind(&config.target_generation)
    .bind(config.schedule_anchor_at_ms)
    .bind(config.updated_at_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to save scheduled backup configuration: {error}"))?;
    Ok(())
}

pub async fn claim_run(pool: &Pool<Sqlite>, run: &ScheduledBackupRun) -> Result<bool, String> {
    let affected = sqlx::query(
        "INSERT INTO scheduled_backup_runs (
            run_key, target_generation, logical_date, logical_time_minutes, target_path,
            status, file_state, attempt_count, retry_at_ms, started_at_ms, completed_at_ms,
            archive_sha256, size_bytes, error_code, error_message, cleanup_warning, updated_at_ms
         ) VALUES (?, ?, ?, ?, ?, 'running', 'absent', 1, NULL, ?, NULL, NULL, NULL, NULL, NULL, NULL, ?)
         ON CONFLICT DO NOTHING",
    )
    .bind(&run.run_key)
    .bind(&run.target_generation)
    .bind(&run.logical_date)
    .bind(i64::from(run.logical_time_minutes))
    .bind(&run.target_path)
    .bind(run.started_at_ms)
    .bind(run.updated_at_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to claim scheduled backup run: {error}"))?
    .rows_affected();
    Ok(affected == 1)
}

pub async fn load_run(
    pool: &Pool<Sqlite>,
    run_key: &str,
) -> Result<Option<ScheduledBackupRun>, String> {
    let row = sqlx::query(&format!("{} WHERE run_key = ? LIMIT 1", run_select_sql()))
        .bind(run_key)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load scheduled backup run: {error}"))?;
    row.map(run_from_row).transpose()
}

pub async fn load_active(pool: &Pool<Sqlite>) -> Result<Option<ScheduledBackupRun>, String> {
    let row = sqlx::query(&format!(
        "{} WHERE status = 'running' ORDER BY updated_at_ms ASC LIMIT 1",
        run_select_sql()
    ))
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load active scheduled backup: {error}"))?;
    row.map(run_from_row).transpose()
}

pub async fn load_due_retry(
    pool: &Pool<Sqlite>,
    generation: &str,
    now_ms: i64,
) -> Result<Option<ScheduledBackupRun>, String> {
    let row = sqlx::query(&format!(
        "{} WHERE target_generation = ? AND status = 'retry_wait' AND retry_at_ms <= ?
         ORDER BY retry_at_ms ASC LIMIT 1",
        run_select_sql()
    ))
    .bind(generation)
    .bind(now_ms)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load scheduled backup retry: {error}"))?;
    row.map(run_from_row).transpose()
}

pub async fn start_retry(pool: &Pool<Sqlite>, run_key: &str, now_ms: i64) -> Result<bool, String> {
    let affected = sqlx::query(
        "UPDATE scheduled_backup_runs
         SET status = 'running', attempt_count = attempt_count + 1,
             retry_at_ms = NULL, started_at_ms = ?, completed_at_ms = NULL,
             error_code = NULL, error_message = NULL, updated_at_ms = ?
         WHERE run_key = ? AND status = 'retry_wait' AND attempt_count < 3",
    )
    .bind(now_ms)
    .bind(now_ms)
    .bind(run_key)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to start scheduled backup retry: {error}"))?
    .rows_affected();
    Ok(affected == 1)
}

pub async fn update_target_path(
    pool: &Pool<Sqlite>,
    run_key: &str,
    target_path: &str,
    now_ms: i64,
) -> Result<(), String> {
    transition(
        sqlx::query(
            "UPDATE scheduled_backup_runs SET target_path = ?, updated_at_ms = ?
             WHERE run_key = ? AND status = 'running'",
        )
        .bind(target_path)
        .bind(now_ms)
        .bind(run_key)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to reserve scheduled backup path: {error}"))?
        .rows_affected(),
        "reserve scheduled backup path",
    )
}

pub async fn mark_succeeded(
    pool: &Pool<Sqlite>,
    run_key: &str,
    archive_sha256: &str,
    size_bytes: u64,
    now_ms: i64,
) -> Result<(), String> {
    let size_bytes = i64::try_from(size_bytes)
        .map_err(|_| "scheduled backup file is too large to record".to_string())?;
    transition(
        sqlx::query(
            "UPDATE scheduled_backup_runs
             SET status = 'succeeded', file_state = 'present', retry_at_ms = NULL,
                 completed_at_ms = ?, archive_sha256 = ?, size_bytes = ?,
                 error_code = NULL, error_message = NULL, updated_at_ms = ?
             WHERE run_key = ? AND status = 'running'",
        )
        .bind(now_ms)
        .bind(archive_sha256)
        .bind(size_bytes)
        .bind(now_ms)
        .bind(run_key)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to record scheduled backup success: {error}"))?
        .rows_affected(),
        "record scheduled backup success",
    )
}

pub async fn mark_failed_or_retry(
    pool: &Pool<Sqlite>,
    run_key: &str,
    attempt_count: u8,
    error_code: &str,
    error_message: &str,
    now_ms: i64,
) -> Result<(), String> {
    let retry_at_ms = match attempt_count {
        1 => Some(now_ms.saturating_add(5 * 60 * 1000)),
        2 => Some(now_ms.saturating_add(30 * 60 * 1000)),
        _ => None,
    };
    let status = if retry_at_ms.is_some() {
        "retry_wait"
    } else {
        "failed"
    };
    transition(
        sqlx::query(
            "UPDATE scheduled_backup_runs
             SET status = ?, retry_at_ms = ?, completed_at_ms = ?,
                 error_code = ?, error_message = ?, updated_at_ms = ?
             WHERE run_key = ? AND status = 'running'",
        )
        .bind(status)
        .bind(retry_at_ms)
        .bind(now_ms)
        .bind(error_code)
        .bind(error_message)
        .bind(now_ms)
        .bind(run_key)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to record scheduled backup failure: {error}"))?
        .rows_affected(),
        "record scheduled backup failure",
    )
}

pub async fn load_recent_by_status(
    pool: &Pool<Sqlite>,
    generation: &str,
    status: &str,
) -> Result<Option<ScheduledBackupRun>, String> {
    let row = sqlx::query(&format!(
        "{} WHERE target_generation = ? AND status = ?
         ORDER BY updated_at_ms DESC LIMIT 1",
        run_select_sql()
    ))
    .bind(generation)
    .bind(status)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load recent scheduled backup run: {error}"))?;
    row.map(run_from_row).transpose()
}

pub async fn list_retention_candidates(
    pool: &Pool<Sqlite>,
    generation: &str,
) -> Result<Vec<ScheduledBackupRun>, String> {
    let rows = sqlx::query(&format!(
        "{} WHERE target_generation = ? AND status = 'succeeded' AND file_state = 'present'
         ORDER BY logical_date DESC, logical_time_minutes DESC",
        run_select_sql()
    ))
    .bind(generation)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load scheduled backup retention candidates: {error}"))?;
    rows.into_iter().map(run_from_row).collect()
}

pub async fn mark_file_state(
    pool: &Pool<Sqlite>,
    run_key: &str,
    file_state: &str,
    warning: Option<&str>,
    now_ms: i64,
) -> Result<(), String> {
    transition(
        sqlx::query(
            "UPDATE scheduled_backup_runs SET file_state = ?, cleanup_warning = ?, updated_at_ms = ?
             WHERE run_key = ? AND status = 'succeeded' AND file_state = 'present'",
        )
        .bind(file_state)
        .bind(warning)
        .bind(now_ms)
        .bind(run_key)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to update scheduled backup file state: {error}"))?
        .rows_affected(),
        "update scheduled backup file state",
    )
}

pub async fn disable_and_reset(
    pool: &Pool<Sqlite>,
    generation: &str,
    now_ms: i64,
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start scheduled backup reset: {error}"))?;
    sqlx::query(
        "UPDATE scheduled_backup_config
         SET enabled = 0, target_generation = ?, schedule_anchor_at_ms = ?, updated_at_ms = ?
         WHERE id = 1",
    )
    .bind(generation)
    .bind(now_ms)
    .bind(now_ms)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to reset scheduled backup configuration: {error}"))?;
    sqlx::query(
        "UPDATE scheduled_backup_runs
         SET status = 'failed', completed_at_ms = ?, retry_at_ms = NULL,
             error_code = 'restore_reset', error_message = 'Schedule reset after replace restore',
             updated_at_ms = ?
         WHERE status IN ('running', 'retry_wait')",
    )
    .bind(now_ms)
    .bind(now_ms)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to stop scheduled backup runs during restore: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("failed to commit scheduled backup reset: {error}"))
}

fn transition(affected: u64, action: &str) -> Result<(), String> {
    if affected == 1 {
        Ok(())
    } else {
        Err(format!("could not {action}; persisted state changed"))
    }
}

fn run_select_sql() -> &'static str {
    "SELECT run_key, target_generation, logical_date, logical_time_minutes, target_path,
            status, file_state, attempt_count, retry_at_ms, started_at_ms, completed_at_ms,
            archive_sha256, size_bytes, error_code, error_message, cleanup_warning, updated_at_ms
     FROM scheduled_backup_runs"
}

fn config_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ScheduledBackupConfig, String> {
    let weekday = row
        .try_get::<Option<i64>, _>("weekday")
        .map_err(|error| format!("failed to decode scheduled backup weekday: {error}"))?
        .map(|value| {
            u8::try_from(value).map_err(|_| "invalid scheduled backup weekday".to_string())
        })
        .transpose()?;
    let local_time_minutes = u16::try_from(
        row.try_get::<i64, _>("local_time_minutes")
            .map_err(|error| format!("failed to decode scheduled backup time: {error}"))?,
    )
    .map_err(|_| "invalid scheduled backup time".to_string())?;
    let config = ScheduledBackupConfig {
        enabled: row.try_get::<i64, _>("enabled").unwrap_or_default() != 0,
        cadence: ScheduledBackupCadence::parse(
            &row.try_get::<String, _>("cadence")
                .map_err(|error| format!("failed to decode scheduled backup cadence: {error}"))?,
        )?,
        weekday,
        local_time_minutes,
        target_dir: row
            .try_get("target_dir")
            .map_err(|error| format!("failed to decode scheduled backup directory: {error}"))?,
        target_generation: row.try_get("target_generation").map_err(|error| {
            format!("failed to decode scheduled backup target generation: {error}")
        })?,
        schedule_anchor_at_ms: row.try_get("schedule_anchor_at_ms").map_err(|error| {
            format!("failed to decode scheduled backup schedule anchor: {error}")
        })?,
        updated_at_ms: row
            .try_get("updated_at_ms")
            .map_err(|error| format!("failed to decode scheduled backup update time: {error}"))?,
    };
    config.validate()?;
    Ok(config)
}

fn run_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ScheduledBackupRun, String> {
    let size_bytes = row
        .try_get::<Option<i64>, _>("size_bytes")
        .map_err(|error| format!("failed to decode scheduled backup size: {error}"))?
        .map(|value| u64::try_from(value).map_err(|_| "invalid scheduled backup size".to_string()))
        .transpose()?;
    Ok(ScheduledBackupRun {
        run_key: row.try_get("run_key").map_err(decode_run_error)?,
        target_generation: row.try_get("target_generation").map_err(decode_run_error)?,
        logical_date: row.try_get("logical_date").map_err(decode_run_error)?,
        logical_time_minutes: u16::try_from(
            row.try_get::<i64, _>("logical_time_minutes")
                .map_err(decode_run_error)?,
        )
        .map_err(|_| "invalid scheduled backup logical time".to_string())?,
        target_path: row.try_get("target_path").map_err(decode_run_error)?,
        status: row.try_get("status").map_err(decode_run_error)?,
        file_state: row.try_get("file_state").map_err(decode_run_error)?,
        attempt_count: u8::try_from(
            row.try_get::<i64, _>("attempt_count")
                .map_err(decode_run_error)?,
        )
        .map_err(|_| "invalid scheduled backup attempt count".to_string())?,
        retry_at_ms: row.try_get("retry_at_ms").map_err(decode_run_error)?,
        started_at_ms: row.try_get("started_at_ms").map_err(decode_run_error)?,
        completed_at_ms: row.try_get("completed_at_ms").map_err(decode_run_error)?,
        archive_sha256: row.try_get("archive_sha256").map_err(decode_run_error)?,
        size_bytes,
        error_code: row.try_get("error_code").map_err(decode_run_error)?,
        error_message: row.try_get("error_message").map_err(decode_run_error)?,
        cleanup_warning: row.try_get("cleanup_warning").map_err(decode_run_error)?,
        updated_at_ms: row.try_get("updated_at_ms").map_err(decode_run_error)?,
    })
}

fn decode_run_error(error: sqlx::Error) -> String {
    format!("failed to decode scheduled backup run: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema;
    use sqlx::{Executor, SqlitePool};

    fn config() -> ScheduledBackupConfig {
        ScheduledBackupConfig {
            enabled: true,
            cadence: ScheduledBackupCadence::Weekly,
            weekday: Some(5),
            local_time_minutes: 21 * 60,
            target_dir: "/tmp/patina-backups".to_string(),
            target_generation: "generation".to_string(),
            schedule_anchor_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn run() -> ScheduledBackupRun {
        ScheduledBackupRun {
            run_key: "scheduled-backup:generation:2026-08-30:2100".to_string(),
            target_generation: "generation".to_string(),
            logical_date: "2026-08-30".to_string(),
            logical_time_minutes: 21 * 60,
            target_path: "/tmp/patina-backups".to_string(),
            status: "running".to_string(),
            file_state: "absent".to_string(),
            attempt_count: 1,
            retry_at_ms: None,
            started_at_ms: 1,
            completed_at_ms: None,
            archive_sha256: None,
            size_bytes: None,
            error_code: None,
            error_message: None,
            cleanup_warning: None,
            updated_at_ms: 1,
        }
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::SCHEDULED_BACKUP_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    #[test]
    fn configuration_round_trips_through_repository() {
        tauri::async_runtime::block_on(async {
            let pool = pool().await;
            save_config(&pool, &config()).await.unwrap();
            assert_eq!(load_config(&pool).await.unwrap(), Some(config()));
        });
    }

    #[test]
    fn run_claim_is_idempotent_and_failure_has_bounded_retry() {
        tauri::async_runtime::block_on(async {
            let pool = pool().await;
            let run = run();
            assert!(claim_run(&pool, &run).await.unwrap());
            assert!(!claim_run(&pool, &run).await.unwrap());

            mark_failed_or_retry(&pool, &run.run_key, 1, "io_error", "failed", 10)
                .await
                .unwrap();
            let retry = load_run(&pool, &run.run_key).await.unwrap().unwrap();
            assert_eq!(retry.status, "retry_wait");
            assert_eq!(retry.retry_at_ms, Some(300_010));

            assert!(load_due_retry(&pool, "other-generation", 300_010)
                .await
                .unwrap()
                .is_none());
            assert!(load_due_retry(&pool, "generation", 300_010)
                .await
                .unwrap()
                .is_some());

            assert!(start_retry(&pool, &run.run_key, 300_010).await.unwrap());
            let active = load_active(&pool).await.unwrap().unwrap();
            assert_eq!(active.attempt_count, 2);
        });
    }

    #[test]
    fn replace_restore_reset_disables_schedule_and_cancels_active_run() {
        tauri::async_runtime::block_on(async {
            let pool = pool().await;
            save_config(&pool, &config()).await.unwrap();
            let run = run();
            assert!(claim_run(&pool, &run).await.unwrap());

            disable_and_reset(&pool, "restored-generation", 20)
                .await
                .unwrap();

            let restored = load_config(&pool).await.unwrap().unwrap();
            assert!(!restored.enabled);
            assert_eq!(restored.target_generation, "restored-generation");
            assert!(load_active(&pool).await.unwrap().is_none());
            let cancelled = load_run(&pool, &run.run_key).await.unwrap().unwrap();
            assert_eq!(cancelled.status, "failed");
            assert_eq!(cancelled.error_code.as_deref(), Some("restore_reset"));
        });
    }
}
