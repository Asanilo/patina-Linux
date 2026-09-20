use crate::data::schema;
use crate::platform::storage_paths;
use futures_util::future::BoxFuture;
use sqlx::error::BoxDynError;
use sqlx::migrate::{Migration as SqlxMigration, MigrationSource, MigrationType, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_sql::{DbInstances, DbPool, MigrationKind};
use tokio::time::{sleep, Duration};

pub const SQLITE_DB_NAME: &str = "sqlite:patina.db";

#[derive(Debug)]
struct InlineMigrationList(Vec<tauri_plugin_sql::Migration>);

impl MigrationSource<'static> for InlineMigrationList {
    fn resolve(self) -> BoxFuture<'static, Result<Vec<SqlxMigration>, BoxDynError>> {
        Box::pin(async move {
            let mut migrations = Vec::new();
            for migration in self.0 {
                if matches!(migration.kind, MigrationKind::Up) {
                    migrations.push(SqlxMigration::new(
                        migration.version,
                        migration.description.into(),
                        MigrationType::ReversibleUp,
                        migration.sql.into(),
                        false,
                    ));
                }
            }
            Ok(migrations)
        })
    }
}

fn expected_migration_metadata() -> Vec<(i64, &'static str, Vec<u8>)> {
    schema::tracker_migrations()
        .into_iter()
        .map(|migration| {
            let sqlx_migration = SqlxMigration::new(
                migration.version,
                migration.description.into(),
                MigrationType::ReversibleUp,
                migration.sql.into(),
                false,
            );
            (
                migration.version,
                migration.description,
                sqlx_migration.checksum.into_owned(),
            )
        })
        .collect()
}

fn resolve_product_db_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let paths = storage_paths::resolve_storage_paths(app)?;
    if paths.database_creation_allowed {
        create_dir_all(&paths.data_root).map_err(|error| {
            format!(
                "failed to create app data dir `{}`: {error}",
                paths.data_root.display()
            )
        })?;
    }
    Ok(paths.db_path)
}

async fn open_single_connection_sqlite_pool(
    db_path: &Path,
    create_if_missing: bool,
) -> Result<Pool<Sqlite>, String> {
    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(create_if_missing)
        .pragma("busy_timeout", "5000")
        .pragma("foreign_keys", "ON");

    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options)
        .await
        .map_err(|error| format!("failed to open sqlite db `{}`: {error}", db_path.display()))
}

pub async fn open_prepared_sqlite_pool_at_path(
    db_path: &Path,
    create_if_missing: bool,
) -> Result<Pool<Sqlite>, String> {
    if create_if_missing {
        let parent = db_path.parent().ok_or_else(|| {
            format!(
                "sqlite database path `{}` has no parent directory",
                db_path.display()
            )
        })?;
        create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create sqlite database directory `{}`: {error}",
                parent.display()
            )
        })?;
    }

    let pool = open_single_connection_sqlite_pool(db_path, create_if_missing).await?;
    prepare_current_schema_for_pool(&pool)
        .await
        .map_err(|error| format!("{error} (`{}`)", db_path.display()))?;
    crate::data::repositories::app_settings::ensure_background_tracking_login_preference(&pool)
        .await
        .map_err(|error| format!("{error} (`{}`)", db_path.display()))?;
    Ok(pool)
}

pub fn is_recoverable_sqlite_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("database is locked")
        || normalized.contains("database is busy")
        || normalized.contains("sqlite_busy")
        || normalized.contains("sqlite_locked")
        || normalized.contains("pool closed")
        || normalized.contains("pooltimedout")
}

pub async fn reopen_sqlite_pool<R: Runtime>(app: &AppHandle<R>) -> Result<Pool<Sqlite>, String> {
    let db_path = resolve_product_db_path(app)?;
    let next_pool = open_prepared_sqlite_pool_at_path(&db_path, true).await?;

    register_sqlite_pool(app, next_pool.clone()).await?;

    Ok(next_pool)
}

async fn open_existing_sqlite_pool_at_path(db_path: &Path) -> Result<Pool<Sqlite>, String> {
    let pool = open_single_connection_sqlite_pool(db_path, false).await?;
    let validation = has_current_baseline_schema(&pool).await;
    match validation {
        Ok(true) => Ok(pool),
        result => {
            pool.close().await;
            match result {
                Err(error) => Err(error),
                _ => Err(format!(
                    "existing sqlite database `{}` is not ready for daemon client mode",
                    db_path.display()
                )),
            }
        }
    }
}

pub async fn reopen_existing_sqlite_pool<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Pool<Sqlite>, String> {
    // The daemon owns database creation, schema maintenance, and setting backfills.
    let db_path = storage_paths::resolve_storage_paths(app)?.db_path;
    let next_pool = open_existing_sqlite_pool_at_path(&db_path).await?;
    register_sqlite_pool(app, next_pool.clone()).await?;
    Ok(next_pool)
}

async fn register_sqlite_pool<R: Runtime>(
    app: &AppHandle<R>,
    next_pool: Pool<Sqlite>,
) -> Result<(), String> {
    let instances = app
        .try_state::<DbInstances>()
        .ok_or_else(|| "sqlite db instances state is not available".to_string())?;

    let previous_pool = {
        let mut instances = instances.0.write().await;
        match instances.insert(
            SQLITE_DB_NAME.to_string(),
            DbPool::Sqlite(next_pool.clone()),
        ) {
            Some(DbPool::Sqlite(pool)) => Some(pool),
            _ => None,
        }
    };

    if let Some(pool) = previous_pool {
        pool.close().await;
    }

    Ok(())
}

pub async fn initialize_app_sqlite<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let db_path = resolve_product_db_path(app)?;
    let pool = open_prepared_sqlite_pool_at_path(&db_path, true).await?;

    register_sqlite_pool(app, pool).await?;

    Ok(())
}

pub async fn initialize_existing_app_sqlite<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    reopen_existing_sqlite_pool(app).await.map(|_| ())
}

async fn prepare_current_schema_for_pool(pool: &Pool<Sqlite>) -> Result<(), String> {
    if repair_legacy_schema_before_baseline_normalization(pool).await? {
        eprintln!("[sql] repaired legacy sqlite schema before baseline normalization");
    }

    if normalize_current_baseline_migration_history_for_pool(pool).await? {
        eprintln!("[sql] normalized sqlite migration history to the current baseline");
    }

    run_current_migrations(pool).await?;

    if normalize_current_baseline_migration_history_for_pool(pool).await? {
        eprintln!("[sql] normalized sqlite migration history to the current baseline");
    }

    if !has_current_schema(pool).await? {
        return Err("sqlite schema validation failed for prepared database".to_string());
    }
    Ok(())
}

async fn prepare_staged_schema_for_pool(pool: &Pool<Sqlite>) -> Result<(), String> {
    if !has_current_baseline_schema(pool).await?
        && repair_legacy_schema_before_baseline_normalization(pool).await?
    {
        eprintln!("[sql] repaired legacy sqlite schema in staged migration database");
    }
    normalize_current_baseline_migration_history_for_pool(pool).await?;
    run_current_migrations(pool).await?;
    normalize_current_baseline_migration_history_for_pool(pool).await?;
    if !has_current_schema(pool).await? {
        return Err("sqlite schema validation failed for staged migration database".to_string());
    }
    Ok(())
}

async fn run_current_migrations(pool: &Pool<Sqlite>) -> Result<(), String> {
    let migrator = Migrator::new(InlineMigrationList(schema::tracker_migrations()))
        .await
        .map_err(|error| format!("failed to prepare sqlite migrations: {error}"))?;
    migrator
        .run(pool)
        .await
        .map_err(|error| format!("failed to run sqlite migrations: {error}"))
}

async fn table_exists(pool: &Pool<Sqlite>, table_name: &str) -> Result<bool, String> {
    sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1")
        .bind(table_name)
        .fetch_optional(pool)
        .await
        .map(|row| row.is_some())
        .map_err(|error| format!("failed to inspect sqlite table `{table_name}`: {error}"))
}

async fn table_has_columns(
    pool: &Pool<Sqlite>,
    table_name: &str,
    required_columns: &[&str],
) -> Result<bool, String> {
    let pragma = match table_name {
        "sessions" => "PRAGMA table_info(sessions)",
        "session_title_samples" => "PRAGMA table_info(session_title_samples)",
        "settings" => "PRAGMA table_info(settings)",
        "icon_cache" => "PRAGMA table_info(icon_cache)",
        "tool_reminders" => "PRAGMA table_info(tool_reminders)",
        "tool_timers" => "PRAGMA table_info(tool_timers)",
        "tool_timer_laps" => "PRAGMA table_info(tool_timer_laps)",
        "tool_pomodoro_runs" => "PRAGMA table_info(tool_pomodoro_runs)",
        "tool_daily_stats" => "PRAGMA table_info(tool_daily_stats)",
        "tool_software_reminder_rules" => "PRAGMA table_info(tool_software_reminder_rules)",
        "web_activity_segments" => "PRAGMA table_info(web_activity_segments)",
        "web_activity_native_sessions" => "PRAGMA table_info(web_activity_native_sessions)",
        "backup_restore_receipts" => "PRAGMA table_info(backup_restore_receipts)",
        "scheduled_backup_config" => "PRAGMA table_info(scheduled_backup_config)",
        "scheduled_backup_runs" => "PRAGMA table_info(scheduled_backup_runs)",
        "import_batches" => "PRAGMA table_info(import_batches)",
        "import_exact_sessions" => "PRAGMA table_info(import_exact_sessions)",
        "import_time_buckets" => "PRAGMA table_info(import_time_buckets)",
        _ => {
            return Err(format!(
                "unsupported schema inspection table `{table_name}`"
            ))
        }
    };

    let rows = sqlx::query(pragma).fetch_all(pool).await.map_err(|error| {
        format!("failed to inspect sqlite table `{table_name}` columns: {error}")
    })?;
    let columns = rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    Ok(required_columns
        .iter()
        .all(|required| columns.iter().any(|column| column == required)))
}

async fn sessions_has_column(pool: &Pool<Sqlite>, column_name: &str) -> Result<bool, String> {
    table_has_columns(pool, "sessions", &[column_name]).await
}

async fn sessions_has_index(pool: &Pool<Sqlite>, index_name: &str) -> Result<bool, String> {
    table_has_index(pool, "sessions", index_name).await
}

async fn table_has_index(
    pool: &Pool<Sqlite>,
    table_name: &str,
    index_name: &str,
) -> Result<bool, String> {
    let pragma = match table_name {
        "sessions" => "PRAGMA index_list(sessions)",
        "session_title_samples" => "PRAGMA index_list(session_title_samples)",
        "tool_reminders" => "PRAGMA index_list(tool_reminders)",
        "tool_timers" => "PRAGMA index_list(tool_timers)",
        "tool_timer_laps" => "PRAGMA index_list(tool_timer_laps)",
        "tool_pomodoro_runs" => "PRAGMA index_list(tool_pomodoro_runs)",
        "tool_daily_stats" => "PRAGMA index_list(tool_daily_stats)",
        "tool_software_reminder_rules" => "PRAGMA index_list(tool_software_reminder_rules)",
        "web_activity_segments" => "PRAGMA index_list(web_activity_segments)",
        "web_activity_native_sessions" => "PRAGMA index_list(web_activity_native_sessions)",
        "scheduled_backup_runs" => "PRAGMA index_list(scheduled_backup_runs)",
        "import_batches" => "PRAGMA index_list(import_batches)",
        "import_exact_sessions" => "PRAGMA index_list(import_exact_sessions)",
        "import_time_buckets" => "PRAGMA index_list(import_time_buckets)",
        _ => return Err(format!("unsupported index inspection table `{table_name}`")),
    };

    let rows = sqlx::query(pragma)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to inspect {table_name} indexes: {error}"))?;

    Ok(rows
        .iter()
        .any(|row| row.get::<String, _>("name") == index_name))
}

async fn ensure_sessions_continuity_group_start_time(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if sessions_has_column(pool, "continuity_group_start_time").await? {
        return Ok(false);
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start sqlite legacy schema repair: {error}"))?;

    sqlx::query("ALTER TABLE sessions ADD COLUMN continuity_group_start_time INTEGER")
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            format!(
                "failed to add sessions.continuity_group_start_time during schema repair: {error}"
            )
        })?;
    sqlx::query(
        "UPDATE sessions
         SET continuity_group_start_time = start_time
         WHERE continuity_group_start_time IS NULL",
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!(
            "failed to backfill sessions.continuity_group_start_time during schema repair: {error}"
        )
    })?;

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit sqlite legacy schema repair: {error}"))?;

    Ok(true)
}

async fn ensure_current_indexes(pool: &Pool<Sqlite>) -> Result<bool, String> {
    let mut changed = false;

    if !sessions_has_index(pool, "idx_sessions_date").await? {
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_date ON sessions(start_time)")
            .execute(pool)
            .await
            .map_err(|error| format!("failed to create sessions date index: {error}"))?;
        changed = true;
    }

    if !sessions_has_index(pool, "idx_sessions_single_active").await? {
        sqlx::query(
            "UPDATE sessions
             SET end_time = start_time,
                 duration = 0
             WHERE end_time IS NULL
               AND id NOT IN (
                 SELECT id
                 FROM sessions
                 WHERE end_time IS NULL
                 ORDER BY start_time DESC, id DESC
                 LIMIT 1
               )",
        )
        .execute(pool)
        .await
        .map_err(|error| {
            format!("failed to seal duplicate active sessions before index repair: {error}")
        })?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_single_active
             ON sessions((1))
             WHERE end_time IS NULL",
        )
        .execute(pool)
        .await
        .map_err(|error| format!("failed to create single active session index: {error}"))?;
        changed = true;
    }

    Ok(changed)
}

async fn ensure_session_title_samples_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    let mut changed = false;

    if !table_exists(pool, "session_title_samples").await? {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS session_title_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )",
        )
        .execute(pool)
        .await
        .map_err(|error| format!("failed to create session_title_samples table: {error}"))?;
        changed = true;
    }

    if !table_has_index(
        pool,
        "session_title_samples",
        "idx_session_title_samples_session_time",
    )
    .await?
    {
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_title_samples_session_time
             ON session_title_samples(session_id, start_time)",
        )
        .execute(pool)
        .await
        .map_err(|error| {
            format!("failed to create session_title_samples session/time index: {error}")
        })?;
        changed = true;
    }

    if !table_has_index(
        pool,
        "session_title_samples",
        "idx_session_title_samples_time",
    )
    .await?
    {
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_session_title_samples_time
             ON session_title_samples(start_time, end_time)",
        )
        .execute(pool)
        .await
        .map_err(|error| format!("failed to create session_title_samples time index: {error}"))?;
        changed = true;
    }

    let inserted = sqlx::query(
        "INSERT INTO session_title_samples (session_id, title, start_time, end_time)
         SELECT id, TRIM(window_title), start_time, end_time
         FROM sessions
         WHERE TRIM(COALESCE(window_title, '')) <> ''
           AND NOT EXISTS (
             SELECT 1
             FROM session_title_samples
             WHERE session_title_samples.session_id = sessions.id
           )",
    )
    .execute(pool)
    .await
    .map_err(|error| format!("failed to backfill legacy title samples: {error}"))?
    .rows_affected();

    Ok(changed || inserted > 0)
}

async fn repair_legacy_schema_before_baseline_normalization(
    pool: &Pool<Sqlite>,
) -> Result<bool, String> {
    if !table_exists(pool, "sessions").await? {
        return Ok(false);
    }

    let mut changed = ensure_sessions_continuity_group_start_time(pool).await?;
    changed = ensure_session_title_samples_schema(pool).await? || changed;

    if has_current_baseline_schema(pool).await? {
        return Ok(changed);
    }

    let sessions_base_ready = table_has_columns(
        pool,
        "sessions",
        &[
            "id",
            "app_name",
            "exe_name",
            "window_title",
            "start_time",
            "end_time",
            "duration",
            "continuity_group_start_time",
        ],
    )
    .await?;

    if sessions_base_ready {
        changed = ensure_current_indexes(pool).await? || changed;
    }

    Ok(changed)
}

async fn has_current_baseline_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "sessions").await?
        || !table_exists(pool, "settings").await?
        || !table_exists(pool, "icon_cache").await?
    {
        return Ok(false);
    }

    let sessions_ready = table_has_columns(
        pool,
        "sessions",
        &[
            "id",
            "app_name",
            "exe_name",
            "window_title",
            "start_time",
            "end_time",
            "duration",
            "continuity_group_start_time",
        ],
    )
    .await?;
    let title_samples_ready = table_has_columns(
        pool,
        "session_title_samples",
        &["id", "session_id", "title", "start_time", "end_time"],
    )
    .await?;
    let settings_ready = table_has_columns(pool, "settings", &["key", "value"]).await?;
    let icon_cache_ready = table_has_columns(
        pool,
        "icon_cache",
        &["exe_name", "icon_base64", "last_updated"],
    )
    .await?;
    let date_index_ready = sessions_has_index(pool, "idx_sessions_date").await?;
    let active_index_ready = sessions_has_index(pool, "idx_sessions_single_active").await?;
    let title_sample_session_index_ready = table_has_index(
        pool,
        "session_title_samples",
        "idx_session_title_samples_session_time",
    )
    .await?;
    let title_sample_time_index_ready = table_has_index(
        pool,
        "session_title_samples",
        "idx_session_title_samples_time",
    )
    .await?;

    Ok(sessions_ready
        && title_samples_ready
        && settings_ready
        && icon_cache_ready
        && date_index_ready
        && active_index_ready
        && title_sample_session_index_ready
        && title_sample_time_index_ready)
}

async fn has_base_tools_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "tool_reminders").await?
        || !table_exists(pool, "tool_timers").await?
        || !table_exists(pool, "tool_timer_laps").await?
        || !table_exists(pool, "tool_pomodoro_runs").await?
        || !table_exists(pool, "tool_daily_stats").await?
    {
        return Ok(false);
    }

    let reminders_ready = table_has_columns(
        pool,
        "tool_reminders",
        &[
            "id",
            "label",
            "scheduled_at",
            "created_at",
            "status",
            "fired_at",
            "cancelled_at",
        ],
    )
    .await?;
    let timers_ready = table_has_columns(
        pool,
        "tool_timers",
        &[
            "id",
            "mode",
            "label",
            "duration_ms",
            "accumulated_ms",
            "started_at",
            "paused_at",
            "completed_at",
            "status",
            "created_at",
            "updated_at",
        ],
    )
    .await?;
    let laps_ready = table_has_columns(
        pool,
        "tool_timer_laps",
        &[
            "id",
            "timer_id",
            "lap_index",
            "started_at",
            "ended_at",
            "duration_ms",
        ],
    )
    .await?;
    let pomodoros_ready = table_has_columns(
        pool,
        "tool_pomodoro_runs",
        &[
            "id",
            "phase",
            "status",
            "cycle_index",
            "focus_ms",
            "short_break_ms",
            "long_break_ms",
            "long_break_every",
            "phase_started_at",
            "phase_paused_at",
            "phase_remaining_ms",
            "completed_focus_count",
            "created_at",
            "updated_at",
        ],
    )
    .await?;
    let daily_ready = table_has_columns(
        pool,
        "tool_daily_stats",
        &["date_key", "completed_pomodoros", "updated_at"],
    )
    .await?;
    let reminder_index_ready =
        table_has_index(pool, "tool_reminders", "idx_tool_reminders_schedule_status").await?;
    let timer_index_ready =
        table_has_index(pool, "tool_timers", "idx_tool_timers_status_updated").await?;
    let lap_index_ready =
        table_has_index(pool, "tool_timer_laps", "idx_tool_timer_laps_timer_id").await?;
    let pomodoro_index_ready = table_has_index(
        pool,
        "tool_pomodoro_runs",
        "idx_tool_pomodoro_runs_status_updated",
    )
    .await?;
    let daily_index_ready =
        table_has_index(pool, "tool_daily_stats", "idx_tool_daily_stats_updated").await?;

    Ok(reminders_ready
        && timers_ready
        && laps_ready
        && pomodoros_ready
        && daily_ready
        && reminder_index_ready
        && timer_index_ready
        && lap_index_ready
        && pomodoro_index_ready
        && daily_index_ready)
}

async fn has_software_reminder_rules_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "tool_software_reminder_rules").await? {
        return Ok(false);
    }

    let software_rules_ready = table_has_columns(
        pool,
        "tool_software_reminder_rules",
        &[
            "id",
            "app_name",
            "exe_name",
            "limit_ms",
            "message",
            "created_at",
            "updated_at",
            "disabled_at",
            "last_fired_date_key",
        ],
    )
    .await?;
    let software_rules_index_ready = table_has_index(
        pool,
        "tool_software_reminder_rules",
        "idx_tool_software_reminder_rules_active",
    )
    .await?;
    let sessions_app_usage_index_ready =
        table_has_index(pool, "sessions", "idx_sessions_app_usage_time").await?;
    let sessions_exe_usage_index_ready =
        table_has_index(pool, "sessions", "idx_sessions_exe_usage_time").await?;

    Ok(software_rules_ready
        && software_rules_index_ready
        && sessions_app_usage_index_ready
        && sessions_exe_usage_index_ready)
}

async fn has_web_activity_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "web_activity_segments").await? {
        return Ok(false);
    }

    let segments_ready = table_has_columns(
        pool,
        "web_activity_segments",
        &[
            "id",
            "browser_client_id",
            "browser_kind",
            "browser_exe_name",
            "domain",
            "normalized_domain",
            "url",
            "title",
            "favicon_url",
            "start_time",
            "end_time",
            "duration",
            "source",
            "created_at",
            "updated_at",
        ],
    )
    .await?;
    let time_index_ready = table_has_index(
        pool,
        "web_activity_segments",
        "idx_web_activity_segments_time",
    )
    .await?;
    let domain_time_index_ready = table_has_index(
        pool,
        "web_activity_segments",
        "idx_web_activity_segments_domain_time",
    )
    .await?;
    let single_active_index_ready = table_has_index(
        pool,
        "web_activity_segments",
        "idx_web_activity_segments_single_active",
    )
    .await?;

    Ok(segments_ready && time_index_ready && domain_time_index_ready && single_active_index_ready)
}

async fn has_web_activity_session_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "web_activity_native_sessions").await? {
        return Ok(false);
    }
    let columns_ready = table_has_columns(
        pool,
        "web_activity_native_sessions",
        &["segment_id", "session_id"],
    )
    .await?;
    let index_ready = table_has_index(
        pool,
        "web_activity_native_sessions",
        "idx_web_activity_native_session",
    )
    .await?;
    let trigger_ready = sqlx::query(
        "SELECT 1 FROM sqlite_master
         WHERE type = 'trigger' AND name = 'trg_native_session_web_boundary'
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to inspect native web boundary trigger: {error}"))?
    .is_some();

    Ok(columns_ready && index_ready && trigger_ready)
}

async fn has_backup_restore_receipt_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    Ok(table_exists(pool, "backup_restore_receipts").await?
        && table_has_columns(
            pool,
            "backup_restore_receipts",
            &[
                "request_id",
                "archive_sha256",
                "strategy",
                "completed_at_ms",
            ],
        )
        .await?)
}

async fn has_scheduled_backup_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "scheduled_backup_config").await?
        || !table_exists(pool, "scheduled_backup_runs").await?
    {
        return Ok(false);
    }

    let config_ready = table_has_columns(
        pool,
        "scheduled_backup_config",
        &[
            "id",
            "enabled",
            "cadence",
            "weekday",
            "local_time_minutes",
            "target_dir",
            "retention_count",
            "target_generation",
            "schedule_anchor_at_ms",
            "updated_at_ms",
        ],
    )
    .await?;
    let runs_ready = table_has_columns(
        pool,
        "scheduled_backup_runs",
        &[
            "run_key",
            "target_generation",
            "logical_date",
            "logical_time_minutes",
            "target_path",
            "status",
            "file_state",
            "attempt_count",
            "retry_at_ms",
            "started_at_ms",
            "completed_at_ms",
            "archive_sha256",
            "size_bytes",
            "error_code",
            "error_message",
            "cleanup_warning",
            "updated_at_ms",
        ],
    )
    .await?;
    let retention_index_ready = table_has_index(
        pool,
        "scheduled_backup_runs",
        "idx_scheduled_backup_runs_retention",
    )
    .await?;
    let retry_index_ready = table_has_index(
        pool,
        "scheduled_backup_runs",
        "idx_scheduled_backup_runs_status_retry",
    )
    .await?;

    Ok(config_ready && runs_ready && retention_index_ready && retry_index_ready)
}

async fn has_activity_import_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    if !table_exists(pool, "import_batches").await?
        || !table_exists(pool, "import_exact_sessions").await?
        || !table_exists(pool, "import_time_buckets").await?
    {
        return Ok(false);
    }

    let batches_ready = table_has_columns(
        pool,
        "import_batches",
        &[
            "id",
            "imported_at",
            "source_name",
            "source_kind",
            "source_fingerprint",
            "exact_session_count",
            "hour_bucket_count",
        ],
    )
    .await?;
    let exact_ready = table_has_columns(
        pool,
        "import_exact_sessions",
        &[
            "id",
            "batch_id",
            "fingerprint",
            "app_name",
            "exe_name",
            "window_title",
            "start_time",
            "end_time",
            "duration",
            "source_category",
        ],
    )
    .await?;
    let buckets_ready = table_has_columns(
        pool,
        "import_time_buckets",
        &[
            "id",
            "batch_id",
            "fingerprint",
            "app_name",
            "exe_name",
            "bucket_start_time",
            "duration",
            "source_category",
        ],
    )
    .await?;

    Ok(batches_ready
        && exact_ready
        && buckets_ready
        && table_has_index(pool, "import_batches", "idx_import_batches_imported_at").await?
        && table_has_index(
            pool,
            "import_exact_sessions",
            "idx_import_exact_sessions_time",
        )
        .await?
        && table_has_index(
            pool,
            "import_exact_sessions",
            "idx_import_exact_sessions_exe_time",
        )
        .await?
        && table_has_index(
            pool,
            "import_exact_sessions",
            "idx_import_exact_sessions_batch",
        )
        .await?
        && table_has_index(pool, "import_time_buckets", "idx_import_time_buckets_time").await?
        && table_has_index(
            pool,
            "import_time_buckets",
            "idx_import_time_buckets_exe_time",
        )
        .await?
        && table_has_index(pool, "import_time_buckets", "idx_import_time_buckets_batch").await?)
}

async fn has_current_schema(pool: &Pool<Sqlite>) -> Result<bool, String> {
    Ok(has_current_baseline_schema(pool).await?
        && has_base_tools_schema(pool).await?
        && has_software_reminder_rules_schema(pool).await?
        && has_web_activity_schema(pool).await?
        && has_scheduled_backup_schema(pool).await?
        && has_activity_import_schema(pool).await?
        && has_web_activity_session_schema(pool).await?
        && has_backup_restore_receipt_schema(pool).await?)
}

async fn normalize_current_baseline_migration_history_for_pool(
    pool: &Pool<Sqlite>,
) -> Result<bool, String> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(false);
    }

    if !has_current_baseline_schema(pool).await? {
        return Ok(false);
    }

    let mut expected = expected_migration_metadata();
    if !has_base_tools_schema(pool).await? {
        expected.truncate(1);
    } else if !has_software_reminder_rules_schema(pool).await? {
        expected.truncate(2);
    } else if !has_web_activity_schema(pool).await? {
        expected.truncate(3);
    } else if !has_scheduled_backup_schema(pool).await? {
        expected.truncate(4);
    } else if !has_activity_import_schema(pool).await? {
        expected.truncate(5);
    } else if !has_web_activity_session_schema(pool).await? {
        expected.truncate(6);
    } else if !has_backup_restore_receipt_schema(pool).await? {
        expected.truncate(7);
    }
    if expected.is_empty() {
        return Ok(false);
    }

    let applied_rows = sqlx::query("SELECT version, description, checksum FROM _sqlx_migrations")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load applied sqlite migrations: {error}"))?;

    let already_normalized = applied_rows.len() == expected.len()
        && expected.iter().all(|(version, description, checksum)| {
            applied_rows.iter().any(|row| {
                row.get::<i64, _>("version") == *version
                    && row.get::<String, _>("description") == *description
                    && row.get::<Vec<u8>, _>("checksum") == *checksum
            })
        });

    if already_normalized {
        return Ok(false);
    }

    let mut tx = pool.begin().await.map_err(|error| {
        format!("failed to start sqlite migration history normalization: {error}")
    })?;
    sqlx::query("DELETE FROM _sqlx_migrations")
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to clear sqlite migration history: {error}"))?;
    for (version, description, checksum) in expected {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, ?, 1, ?, 0)",
        )
        .bind(version)
        .bind(description)
        .bind(checksum)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to write sqlite current migration history: {error}"))?;
    }
    tx.commit().await.map_err(|error| {
        format!("failed to commit sqlite migration history normalization: {error}")
    })?;

    Ok(true)
}

pub async fn wait_for_sqlite_pool<R: Runtime>(app: &AppHandle<R>) -> Result<Pool<Sqlite>, String> {
    let mut wait_cycles: u64 = 0;

    loop {
        if let Some(instances) = app.try_state::<DbInstances>() {
            let instances = instances.0.read().await;
            if let Some(DbPool::Sqlite(pool)) = instances.get(SQLITE_DB_NAME) {
                return Ok(pool.clone());
            }
        }

        wait_cycles += 1;
        if wait_cycles > 300 {
            return Err("sqlite pool not available in time".to_string());
        }

        sleep(Duration::from_millis(100)).await;
    }
}

pub async fn checkpoint_current_database<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    checkpoint_sqlite_pool(&pool).await
}

async fn checkpoint_sqlite_pool(pool: &Pool<Sqlite>) -> Result<(), String> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to checkpoint the Patina database: {error}"))?;
    Ok(())
}

pub(crate) async fn validate_migrated_database_copy(
    source_path: &Path,
    staged_path: &Path,
) -> Result<(), String> {
    let source = open_single_connection_sqlite_pool(source_path, false).await?;
    let staged = open_single_connection_sqlite_pool(staged_path, false).await?;

    let result = async {
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&staged)
            .await
            .map_err(|error| {
                format!(
                    "failed to run integrity check on staged database `{}`: {error}",
                    staged_path.display()
                )
            })?;
        if integrity != "ok" {
            return Err(format!(
                "staged database `{}` failed integrity check: {integrity}",
                staged_path.display()
            ));
        }

        prepare_staged_schema_for_pool(&staged).await?;
        for table in [
            "sessions",
            "session_title_samples",
            "settings",
            "icon_cache",
            "web_activity_segments",
            "web_activity_native_sessions",
            "backup_restore_receipts",
            "tool_reminders",
            "tool_timers",
            "tool_timer_laps",
            "tool_pomodoro_runs",
            "tool_daily_stats",
            "tool_software_reminder_rules",
            "import_batches",
            "import_exact_sessions",
            "import_time_buckets",
        ] {
            let source_count = table_row_count_if_present(&source, table).await?;
            let staged_count = table_row_count_if_present(&staged, table).await?;
            if source_count != staged_count {
                return Err(format!(
                    "{table} row count changed during storage migration: source={source_count}, staged={staged_count}"
                ));
            }
        }
        Ok(())
    }
    .await;

    source.close().await;
    staged.close().await;
    result
}

async fn table_row_count_if_present(pool: &Pool<Sqlite>, table_name: &str) -> Result<i64, String> {
    if !table_exists(pool, table_name).await? {
        return Ok(0);
    }
    let query = format!("SELECT COUNT(*) FROM {table_name}");
    sqlx::query_scalar(&query)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("failed to count sqlite table `{table_name}`: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Executor, SqlitePool};

    fn existing_pool_test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-existing-pool-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn existing_only_pool_does_not_create_a_missing_database() {
        tauri::async_runtime::block_on(async {
            let root = existing_pool_test_root("missing");
            let db_path = root.join("Patina").join("patina.db");

            assert!(open_existing_sqlite_pool_at_path(&db_path).await.is_err());
            assert!(!root.exists());

            std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
            assert!(open_existing_sqlite_pool_at_path(&db_path).await.is_err());
            assert!(!db_path.exists());
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn existing_only_pool_rejects_incompatible_schema_without_repair() {
        tauri::async_runtime::block_on(async {
            let root = existing_pool_test_root("incompatible");
            std::fs::create_dir_all(&root).unwrap();
            let db_path = root.join("patina.db");
            let pool = open_single_connection_sqlite_pool(&db_path, true)
                .await
                .unwrap();
            pool.execute("CREATE TABLE legacy_marker (value TEXT NOT NULL)")
                .await
                .unwrap();
            pool.execute("INSERT INTO legacy_marker (value) VALUES ('preserved')")
                .await
                .unwrap();
            pool.close().await;

            let error = open_existing_sqlite_pool_at_path(&db_path)
                .await
                .unwrap_err();
            assert!(error.contains("not ready for daemon client mode"));

            let pool = open_single_connection_sqlite_pool(&db_path, false)
                .await
                .unwrap();
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(tables, vec!["legacy_marker"]);
            let value: String = sqlx::query_scalar("SELECT value FROM legacy_marker")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(value, "preserved");
            pool.close().await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn existing_only_pool_does_not_run_migrations_or_backfill_settings() {
        tauri::async_runtime::block_on(async {
            let root = existing_pool_test_root("no-maintenance");
            let db_path = root.join("patina.db");
            let pool = open_prepared_sqlite_pool_at_path(&db_path, true)
                .await
                .unwrap();
            pool.execute("DROP TABLE _sqlx_migrations").await.unwrap();
            pool.execute("DELETE FROM settings WHERE key = 'background_tracking_at_login'")
                .await
                .unwrap();
            pool.close().await;

            let pool = open_existing_sqlite_pool_at_path(&db_path).await.unwrap();
            assert!(has_current_baseline_schema(&pool).await.unwrap());
            assert!(!table_exists(&pool, "_sqlx_migrations").await.unwrap());
            let login_settings: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM settings WHERE key = 'background_tracking_at_login'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(login_settings, 0);
            pool.close().await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    async fn create_sqlx_migrations_table(pool: &SqlitePool) {
        pool.execute(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .await
        .unwrap();
    }

    #[test]
    fn explicit_path_pool_prepares_sessions_schema() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "patina-explicit-pool-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let db_path = root.join("Patina").join("patina.db");

            let pool = open_prepared_sqlite_pool_at_path(&db_path, true)
                .await
                .unwrap();
            let sessions_table_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sessions'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_eq!(sessions_table_count, 1);

            pool.close().await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn current_baseline_migration_creates_complete_schema() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_current_baseline_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn tools_schema_creates_complete_tool_tables() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();

            assert!(has_base_tools_schema(&pool).await.unwrap());
            assert!(!has_software_reminder_rules_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn software_reminder_schema_creates_rule_table() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_software_reminder_rules_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn web_activity_schema_creates_complete_table() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::WEB_ACTIVITY_SCHEMA_SQL).await.unwrap();

            assert!(has_web_activity_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn web_activity_session_schema_creates_relation_and_boundary_trigger() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::WEB_ACTIVITY_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_web_activity_session_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn backup_restore_receipt_schema_creates_completion_table() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::BACKUP_RESTORE_RECEIPT_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_backup_restore_receipt_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn scheduled_backup_schema_creates_complete_tables() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::SCHEDULED_BACKUP_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_scheduled_backup_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn activity_import_schema_creates_complete_tables() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::ACTIVITY_IMPORT_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(has_activity_import_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn current_schema_history_does_not_mark_missing_activity_import_schema_as_applied() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::WEB_ACTIVITY_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SCHEDULED_BACKUP_SCHEMA_SQL)
                .await
                .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0),
                        (3, 'old_v3', 1, x'03', 0),
                        (4, 'old_v4', 1, x'04', 0),
                        (5, 'old_v5', 1, x'05', 0),
                        (6, 'old_v6_without_tables', 1, x'06', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            assert!(!has_activity_import_schema(&pool).await.unwrap());
            let versions: Vec<i64> =
                sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(versions, vec![1, 2, 3, 4, 5]);

            run_current_migrations(&pool).await.unwrap();
            assert!(has_activity_import_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn current_schema_history_does_not_mark_missing_web_session_binding_as_applied() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::WEB_ACTIVITY_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SCHEDULED_BACKUP_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::ACTIVITY_IMPORT_SCHEMA_SQL)
                .await
                .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0),
                        (3, 'old_v3', 1, x'03', 0),
                        (4, 'old_v4', 1, x'04', 0),
                        (5, 'old_v5', 1, x'05', 0),
                        (6, 'old_v6', 1, x'06', 0),
                        (7, 'old_v7_without_tables', 1, x'07', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            assert!(!has_web_activity_session_schema(&pool).await.unwrap());
            let versions: Vec<i64> =
                sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(versions, vec![1, 2, 3, 4, 5, 6]);

            run_current_migrations(&pool).await.unwrap();
            assert!(has_web_activity_session_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn published_linux_main_a13a64a_upgrades_to_daemon_without_losing_existing_data() {
        tauri::async_runtime::block_on(async {
            // Fixture provenance: Linux main a13a64a669849234df5575994673cb7a90cc3003
            // (1.8.4), src-tauri/src/data/schema.rs. Its six SQL migrations are
            // byte-identical to the first six here. Pin their actual SQLx SHA384
            // checksums and descriptions instead of copying the migration system.
            // A future edit must not silently redefine this historical fixture.
            let main_metadata = [
                (1, "create_current_baseline_schema", "814c93ce744d7cd7d412b0f6a333fa694912cbfeb50631c3fe2e272b4d48901ec36aa920ca7f2ca2f9c9f65d1181b3b5"),
                (2, "create_tools_tables", "86f667e46ec43873b4427f49ae6ef6ff4a4651f13a51b89af48d4e808a2a68254407867d4438a4b65479ba749c0b54ee"),
                (3, "create_software_reminder_rules", "c3b882b9d9002c1ac77a84cfb84825480add3048b508aae0e4a3e6987a506b56d5e9bef88be8cd3236794559b1f700c0"),
                (4, "create_web_activity_segments", "08919d97b0cc1098695fb1bf6880e87a56e31672a7cefe47dc056f3005ed9cd845af56fd1478d9587af5c442b830c791"),
                (5, "create_scheduled_backup_tables", "651789cdf631343313427566e3c24b0d28940f1cff1e33e2b6cccfc8d0342fbde81f13eb640972d9d8ee7cf2314c42e5"),
                (6, "create_activity_import_tables", "7143cd4a81f7523ff67af96c187db75ebb507fbc3ac290c3abb8575dc3d31ad31db297654bcf9f5218b065442bbcec99"),
            ];
            let main_migrations = schema::tracker_migrations().into_iter().take(6).collect();
            let migrator = Migrator::new(InlineMigrationList(main_migrations))
                .await
                .unwrap();
            assert_eq!(migrator.iter().count(), main_metadata.len());
            for (migration, (version, description, checksum)) in migrator.iter().zip(main_metadata)
            {
                let actual_checksum: String = migration
                    .checksum
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                assert_eq!(migration.version, version);
                assert_eq!(migration.description, description);
                assert_eq!(
                    actual_checksum, checksum,
                    "published main migration {version} changed"
                );
            }

            let root = existing_pool_test_root("published-main-upgrade");
            std::fs::create_dir_all(&root).unwrap();
            let db_path = root.join("patina.db");
            let pool = open_single_connection_sqlite_pool(&db_path, true)
                .await
                .unwrap();
            migrator.run(&pool).await.unwrap();
            assert!(!has_web_activity_session_schema(&pool).await.unwrap());
            assert!(!has_backup_restore_receipt_schema(&pool).await.unwrap());
            // Entirely synthetic rows, including nullable live records, Unicode,
            // relationship keys and each of main's sixteen existing data tables.
            pool.execute(r#"
                INSERT INTO sessions VALUES
                    (41, 'Terminal', 'gnome-terminal', '文档', 1000, 5000, 4000, 1000),
                    (42, 'Firefox', 'firefox', 'Active tab', 6000, NULL, NULL, 6000);
                INSERT INTO session_title_samples VALUES
                    (1, 41, '文档', 1000, 5000), (2, 42, 'Active tab', 6000, NULL);
                INSERT INTO settings VALUES
                    ('launch_at_login', '1'), ('language', 'en-US'),
                    ('__app_override::firefox', '{"category":"development","displayName":"工作浏览器","enabled":true}'),
                    ('__custom_category::main-fixture', '1234'),
                    ('__web_domain_override::example.org', '{"category":"development"}');
                INSERT INTO icon_cache VALUES ('firefox', 'synthetic-icon', 1000);
                INSERT INTO tool_reminders VALUES (21, 'Break', 50000, 1000, 'pending', NULL, NULL);
                INSERT INTO tool_timers VALUES
                    (31, 'countdown', 'Tea', 60000, 2000, NULL, 3000, NULL, 'paused', 1000, 3000);
                INSERT INTO tool_timer_laps VALUES (32, 31, 1, 1000, 3000, 2000);
                INSERT INTO tool_pomodoro_runs VALUES
                    (51, 'focus', 'paused', 1, 1500000, 300000, 900000, 4, NULL, 4000, 1000000, 2, 1000, 4000);
                INSERT INTO tool_daily_stats VALUES ('2026-09-20', 2, 4000);
                INSERT INTO tool_software_reminder_rules VALUES
                    (61, 'Firefox', 'firefox', 300000, '休息', 1000, 4000, NULL, '2026-09-20');
                INSERT INTO web_activity_segments VALUES
                    (71, 'synthetic-client', 'firefox', 'firefox', 'Example.org', 'example.org',
                     'https://example.org/docs', '文档', NULL, 2000, 4000, 2000, 'browser-extension', 2000, 4000),
                    (72, 'synthetic-client', 'firefox', 'firefox', 'Example.org', 'example.org',
                     'https://example.org/live', 'Active tab', NULL, 6000, NULL, NULL, 'browser-extension', 6000, 6000);
                INSERT INTO scheduled_backup_config VALUES
                    (1, 1, 'daily', NULL, 540, '/synthetic/backups', 3, 'main-fixture', 1, 2);
                INSERT INTO scheduled_backup_runs VALUES
                    ('main-run', 'main-fixture', '2026-09-19', 540, '/synthetic/backups/old.patina',
                     'succeeded', 'present', 1, NULL, 1, 2, printf('%064d', 3), 123, NULL, NULL, NULL, 2);
                INSERT INTO import_batches VALUES
                    ('main-import', 7000, 'Synthetic CSV', 'patina-csv', printf('%064d', 0), 1, 1);
                INSERT INTO import_exact_sessions VALUES
                    (81, 'main-import', printf('%064d', 1), 'Imported App', 'imported', 'Old title',
                     10000, 12000, 2000, 'other');
                INSERT INTO import_time_buckets VALUES
                    (82, 'main-import', printf('%064d', 2), 'Imported App', 'imported', 0, 1000, 'other');
            "#).await.unwrap();

            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'table'
                 AND name NOT IN ('_sqlx_migrations', 'sqlite_sequence') ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(tables.len(), 16);
            let mut snapshots = Vec::new();
            for table in tables {
                let columns: Vec<String> = sqlx::query(&format!("PRAGMA table_info({table})"))
                    .fetch_all(&pool)
                    .await
                    .unwrap()
                    .iter()
                    .map(|row| format!("\"{}\"", row.get::<String, _>("name")))
                    .collect();
                let filter = if table == "settings" {
                    " WHERE key <> 'background_tracking_at_login'"
                } else {
                    ""
                };
                let query = format!(
                    "SELECT json_array({}) FROM {table}{filter} ORDER BY 1",
                    columns.join(", ")
                );
                let rows: Vec<String> = sqlx::query_scalar(&query).fetch_all(&pool).await.unwrap();
                assert!(!rows.is_empty(), "fixture must exercise {table}");
                snapshots.push((table, query, rows));
            }
            let original_indexes: Vec<(String, String)> = sqlx::query_as(
                "SELECT name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
            ).fetch_all(&pool).await.unwrap();
            let original_migrations: Vec<(i64, String, Vec<u8>)> = sqlx::query_as(
                "SELECT version, description, checksum FROM _sqlx_migrations ORDER BY version",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            pool.close().await;

            // Exercise the actual daemon preparation path, then an idempotent reopen.
            for _ in 0..2 {
                let upgraded = open_prepared_sqlite_pool_at_path(&db_path, false)
                    .await
                    .unwrap();
                assert!(has_current_schema(&upgraded).await.unwrap());
                for (table, query, expected) in &snapshots {
                    let actual: Vec<String> = sqlx::query_scalar(query)
                        .fetch_all(&upgraded)
                        .await
                        .unwrap();
                    assert_eq!(&actual, expected, "main data changed in {table}");
                }
                for (name, expected_sql) in &original_indexes {
                    let actual: String = sqlx::query_scalar(
                        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?",
                    )
                    .bind(name)
                    .fetch_one(&upgraded)
                    .await
                    .unwrap();
                    assert_eq!(&actual, expected_sql);
                }
                let preserved_migrations: Vec<(i64, String, Vec<u8>)> = sqlx::query_as(
                    "SELECT version, description, checksum FROM _sqlx_migrations WHERE version <= 6 ORDER BY version",
                ).fetch_all(&upgraded).await.unwrap();
                assert_eq!(preserved_migrations, original_migrations);
                let versions: Vec<i64> = sqlx::query_scalar(
                    "SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
                )
                .fetch_all(&upgraded)
                .await
                .unwrap();
                let expected_versions: Vec<i64> = schema::tracker_migrations()
                    .iter()
                    .map(|migration| migration.version)
                    .collect();
                assert_eq!(versions, expected_versions);
                let background_login: String = sqlx::query_scalar(
                    "SELECT value FROM settings WHERE key = 'background_tracking_at_login'",
                )
                .fetch_one(&upgraded)
                .await
                .unwrap();
                assert_eq!(background_login, "1");
                assert!(has_web_activity_session_schema(&upgraded).await.unwrap());
                assert!(has_backup_restore_receipt_schema(&upgraded).await.unwrap());
                let linked_rows: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM web_activity_native_sessions")
                        .fetch_one(&upgraded)
                        .await
                        .unwrap();
                assert_eq!(
                    linked_rows, 0,
                    "upgrade must not invent historical browser bindings"
                );
                let foreign_key_errors = sqlx::query("PRAGMA foreign_key_check")
                    .fetch_all(&upgraded)
                    .await
                    .unwrap();
                assert!(foreign_key_errors.is_empty());
                let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
                    .fetch_one(&upgraded)
                    .await
                    .unwrap();
                assert_eq!(integrity, "ok");
                upgraded.close().await;
            }

            // New v7/v8 structures must work, not merely have matching names.
            let upgraded = open_prepared_sqlite_pool_at_path(&db_path, false)
                .await
                .unwrap();
            upgraded.execute(
                "INSERT INTO web_activity_native_sessions VALUES (72, 42);
                 UPDATE sessions SET end_time = 9000, duration = 3000 WHERE id = 42;
                 INSERT INTO backup_restore_receipts VALUES ('synthetic-restore', 'synthetic-sha', 'merge', 9000);",
            ).await.unwrap();
            let closed_web: (i64, i64) = sqlx::query_as(
                "SELECT end_time, duration FROM web_activity_segments WHERE id = 72",
            )
            .fetch_one(&upgraded)
            .await
            .unwrap();
            assert_eq!(closed_web, (9000, 3000));
            let receipt: (String, i64) = sqlx::query_as("SELECT strategy, completed_at_ms FROM backup_restore_receipts WHERE request_id = 'synthetic-restore'")
                .fetch_one(&upgraded).await.unwrap();
            assert_eq!(receipt, ("merge".to_string(), 9000));
            upgraded.close().await;
            std::fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn current_schema_history_is_normalized_to_single_baseline_row() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0),
                        (7, 'old_v7', 1, x'07', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            let rows = sqlx::query("SELECT version, description, checksum FROM _sqlx_migrations")
                .fetch_all(&pool)
                .await
                .unwrap();
            let expected = expected_migration_metadata();

            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get::<i64, _>("version"), expected[0].0);
            assert_eq!(rows[0].get::<String, _>("description"), expected[0].1);
            assert_eq!(rows[0].get::<Vec<u8>, _>("checksum"), expected[0].2);
        });
    }

    #[test]
    fn current_schema_history_preserves_tools_schema_row() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            let rows = sqlx::query("SELECT version, description, checksum FROM _sqlx_migrations")
                .fetch_all(&pool)
                .await
                .unwrap();
            let mut expected = expected_migration_metadata();
            expected.truncate(2);

            assert_eq!(rows.len(), expected.len());
            for (version, description, checksum) in expected {
                assert!(rows.iter().any(|row| {
                    row.get::<i64, _>("version") == version
                        && row.get::<String, _>("description") == description
                        && row.get::<Vec<u8>, _>("checksum") == checksum
                }));
            }
        });
    }

    #[test]
    fn current_schema_history_preserves_software_reminder_schema_row() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
                .await
                .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0),
                        (3, 'old_v3', 1, x'03', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            let rows = sqlx::query("SELECT version, description, checksum FROM _sqlx_migrations")
                .fetch_all(&pool)
                .await
                .unwrap();
            let mut expected = expected_migration_metadata();
            expected.truncate(3);

            assert_eq!(rows.len(), expected.len());
            for (version, description, checksum) in expected {
                assert!(rows.iter().any(|row| {
                    row.get::<i64, _>("version") == version
                        && row.get::<String, _>("description") == description
                        && row.get::<Vec<u8>, _>("checksum") == checksum
                }));
            }
        });
    }

    #[test]
    fn current_schema_history_does_not_mark_missing_web_activity_schema_as_applied() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();
            pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
            pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
                .await
                .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0),
                        (2, 'old_v2', 1, x'02', 0),
                        (3, 'old_v3', 1, x'03', 0),
                        (4, 'old_v4_without_table', 1, x'04', 0)",
            )
            .await
            .unwrap();

            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            assert!(!has_web_activity_schema(&pool).await.unwrap());

            let rows = sqlx::query("SELECT version, description, checksum FROM _sqlx_migrations")
                .fetch_all(&pool)
                .await
                .unwrap();
            let mut expected = expected_migration_metadata();
            expected.truncate(3);

            assert_eq!(rows.len(), expected.len());
            for (version, description, checksum) in expected {
                assert!(rows.iter().any(|row| {
                    row.get::<i64, _>("version") == version
                        && row.get::<String, _>("description") == description
                        && row.get::<Vec<u8>, _>("checksum") == checksum
                }));
            }

            run_current_migrations(&pool).await.unwrap();
            assert!(has_web_activity_schema(&pool).await.unwrap());
        });
    }

    async fn create_legacy_schema_without_continuity_column(pool: &SqlitePool) {
        pool.execute(
            "CREATE TABLE sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                app_name TEXT NOT NULL,
                exe_name TEXT NOT NULL,
                window_title TEXT,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                duration INTEGER
            );
            CREATE INDEX idx_sessions_date ON sessions(start_time);
            CREATE UNIQUE INDEX idx_sessions_single_active ON sessions((1)) WHERE end_time IS NULL;
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE icon_cache (
                exe_name TEXT PRIMARY KEY,
                icon_base64 TEXT NOT NULL,
                last_updated INTEGER
            );",
        )
        .await
        .unwrap();
    }

    #[test]
    fn legacy_schema_without_continuity_column_is_repaired() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            create_legacy_schema_without_continuity_column(&pool).await;

            let repaired = repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();

            assert!(repaired);
            assert!(sessions_has_column(&pool, "continuity_group_start_time")
                .await
                .unwrap());
            assert!(has_current_baseline_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn legacy_schema_repair_preserves_existing_sessions() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            create_legacy_schema_without_continuity_column(&pool).await;
            pool.execute(
                "INSERT INTO sessions (app_name, exe_name, window_title, start_time, end_time, duration)
                 VALUES ('Editor', 'editor.exe', 'Doc', 100, 150, 50),
                        ('Browser', 'browser.exe', 'Page', 200, NULL, NULL)",
            )
            .await
            .unwrap();

            repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();

            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 2);
        });
    }

    #[test]
    fn legacy_schema_repair_backfills_continuity_group_start_time() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            create_legacy_schema_without_continuity_column(&pool).await;
            pool.execute(
                "INSERT INTO sessions (app_name, exe_name, window_title, start_time, end_time, duration)
                 VALUES ('Editor', 'editor.exe', 'Doc', 321, 654, 333)",
            )
            .await
            .unwrap();

            repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();

            let continuity_group_start_time: i64 =
                sqlx::query_scalar("SELECT continuity_group_start_time FROM sessions")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(continuity_group_start_time, 321);
        });
    }

    #[test]
    fn legacy_schema_repair_then_normalizes_migration_history() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            create_legacy_schema_without_continuity_column(&pool).await;
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0)",
            )
                .await
                .unwrap();

            repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();
            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(normalized);
            let description: String =
                sqlx::query_scalar("SELECT description FROM _sqlx_migrations WHERE version = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(description, schema::CURRENT_BASELINE_MIGRATION_DESCRIPTION);
        });
    }

    #[test]
    fn legacy_schema_repair_dedupes_active_sessions() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(
                "CREATE TABLE sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    app_name TEXT NOT NULL,
                    exe_name TEXT NOT NULL,
                    window_title TEXT,
                    start_time INTEGER NOT NULL,
                    end_time INTEGER,
                    duration INTEGER
                );
                CREATE INDEX idx_sessions_date ON sessions(start_time);
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE icon_cache (
                    exe_name TEXT PRIMARY KEY,
                    icon_base64 TEXT NOT NULL,
                    last_updated INTEGER
                );
                INSERT INTO sessions (app_name, exe_name, window_title, start_time, end_time, duration)
                VALUES ('A', 'a.exe', 'A', 100, NULL, NULL),
                       ('B', 'b.exe', 'B', 200, NULL, NULL);",
            )
            .await
            .unwrap();

            repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();

            let active_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE end_time IS NULL")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let sealed_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE duration = 0")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(active_count, 1);
            assert_eq!(sealed_count, 1);
            assert!(sessions_has_index(&pool, "idx_sessions_single_active")
                .await
                .unwrap());
        });
    }

    #[test]
    fn current_baseline_includes_title_samples_table_and_indexes() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(table_exists(&pool, "session_title_samples").await.unwrap());
            assert!(table_has_index(
                &pool,
                "session_title_samples",
                "idx_session_title_samples_session_time",
            )
            .await
            .unwrap());
            assert!(table_has_index(
                &pool,
                "session_title_samples",
                "idx_session_title_samples_time",
            )
            .await
            .unwrap());
            assert!(has_current_baseline_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn legacy_schema_repair_creates_title_samples_and_backfills_once() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            create_legacy_schema_without_continuity_column(&pool).await;
            pool.execute(
                "INSERT INTO sessions (id, app_name, exe_name, window_title, start_time, end_time, duration)
                 VALUES (1, 'Editor', 'editor.exe', 'Doc', 100, 150, 50),
                        (2, 'Browser', 'browser.exe', '', 200, 250, 50)",
            )
            .await
            .unwrap();

            assert!(repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap());
            assert!(!repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap());

            let samples: Vec<(i64, String, i64, Option<i64>)> = sqlx::query_as(
                "SELECT session_id, title, start_time, end_time
                 FROM session_title_samples
                 ORDER BY id ASC",
            )
            .fetch_all(&pool)
            .await
            .unwrap();

            assert_eq!(samples, vec![(1, "Doc".to_string(), 100, Some(150))]);
            assert!(has_current_baseline_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn current_schema_repair_is_idempotent() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
                .await
                .unwrap();

            assert!(!repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap());
            assert!(has_current_baseline_schema(&pool).await.unwrap());
        });
    }

    #[test]
    fn incomplete_schema_is_not_marked_as_current_baseline() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
            pool.execute(
                "CREATE TABLE sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    app_name TEXT NOT NULL,
                    exe_name TEXT NOT NULL,
                    window_title TEXT,
                    start_time INTEGER NOT NULL,
                    end_time INTEGER,
                    duration INTEGER
                );",
            )
            .await
            .unwrap();
            create_sqlx_migrations_table(&pool).await;
            pool.execute(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
                 VALUES (1, 'old_v1', 1, x'01', 0)",
            )
            .await
            .unwrap();

            repair_legacy_schema_before_baseline_normalization(&pool)
                .await
                .unwrap();
            let normalized = normalize_current_baseline_migration_history_for_pool(&pool)
                .await
                .unwrap();

            assert!(!normalized);
            let description: String =
                sqlx::query_scalar("SELECT description FROM _sqlx_migrations WHERE version = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(description, "old_v1");
        });
    }

    #[test]
    fn checkpoint_helper_flushes_the_current_pool() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

            checkpoint_sqlite_pool(&pool).await.unwrap();
        });
    }
}
