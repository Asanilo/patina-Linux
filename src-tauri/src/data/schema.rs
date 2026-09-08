use tauri_plugin_sql::{Migration, MigrationKind};

pub const CURRENT_BASELINE_MIGRATION_VERSION: i64 = 1;
pub const CURRENT_BASELINE_MIGRATION_DESCRIPTION: &str = "create_current_baseline_schema";
pub const TOOLS_TABLES_MIGRATION_VERSION: i64 = 2;
pub const TOOLS_TABLES_MIGRATION_DESCRIPTION: &str = "create_tools_tables";
pub const SOFTWARE_REMINDER_RULES_MIGRATION_VERSION: i64 = 3;
pub const SOFTWARE_REMINDER_RULES_MIGRATION_DESCRIPTION: &str = "create_software_reminder_rules";
pub const WEB_ACTIVITY_MIGRATION_VERSION: i64 = 4;
pub const WEB_ACTIVITY_MIGRATION_DESCRIPTION: &str = "create_web_activity_segments";
pub const SCHEDULED_BACKUP_MIGRATION_VERSION: i64 = 5;
pub const SCHEDULED_BACKUP_MIGRATION_DESCRIPTION: &str = "create_scheduled_backup_tables";
pub const ACTIVITY_IMPORT_MIGRATION_VERSION: i64 = 6;
pub const ACTIVITY_IMPORT_MIGRATION_DESCRIPTION: &str = "create_activity_import_tables";
pub const WEB_ACTIVITY_SESSION_MIGRATION_VERSION: i64 = 7;
pub const WEB_ACTIVITY_SESSION_MIGRATION_DESCRIPTION: &str = "bind_web_activity_to_native_sessions";

pub const CURRENT_BASELINE_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        app_name TEXT NOT NULL,
        exe_name TEXT NOT NULL,
        window_title TEXT,
        start_time INTEGER NOT NULL,
        end_time INTEGER,
        duration INTEGER,
        continuity_group_start_time INTEGER
    );

    CREATE TABLE IF NOT EXISTS session_title_samples (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id INTEGER NOT NULL,
        title TEXT NOT NULL,
        start_time INTEGER NOT NULL,
        end_time INTEGER,
        FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
    );

    UPDATE sessions
    SET end_time = start_time,
        duration = 0
    WHERE end_time IS NULL
      AND id NOT IN (
        SELECT id
        FROM sessions
        WHERE end_time IS NULL
        ORDER BY start_time DESC, id DESC
        LIMIT 1
      );

    UPDATE sessions
    SET continuity_group_start_time = start_time
    WHERE continuity_group_start_time IS NULL;

    CREATE INDEX IF NOT EXISTS idx_sessions_date ON sessions(start_time);

    CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_single_active
    ON sessions((1))
    WHERE end_time IS NULL;

    CREATE INDEX IF NOT EXISTS idx_session_title_samples_session_time
    ON session_title_samples(session_id, start_time);

    CREATE INDEX IF NOT EXISTS idx_session_title_samples_time
    ON session_title_samples(start_time, end_time);

    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS icon_cache (
        exe_name TEXT PRIMARY KEY,
        icon_base64 TEXT NOT NULL,
        last_updated INTEGER
    );
";

pub const TOOLS_TABLES_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS tool_reminders (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        label TEXT NOT NULL,
        scheduled_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        status TEXT NOT NULL,
        fired_at INTEGER,
        cancelled_at INTEGER
    );

    CREATE INDEX IF NOT EXISTS idx_tool_reminders_schedule_status
    ON tool_reminders(status, scheduled_at);

    CREATE TABLE IF NOT EXISTS tool_timers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        mode TEXT NOT NULL,
        label TEXT,
        duration_ms INTEGER,
        accumulated_ms INTEGER NOT NULL DEFAULT 0,
        started_at INTEGER,
        paused_at INTEGER,
        completed_at INTEGER,
        status TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_tool_timers_status_updated
    ON tool_timers(status, updated_at);

    CREATE TABLE IF NOT EXISTS tool_timer_laps (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timer_id INTEGER NOT NULL,
        lap_index INTEGER NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL,
        FOREIGN KEY(timer_id) REFERENCES tool_timers(id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_tool_timer_laps_timer_id
    ON tool_timer_laps(timer_id, lap_index);

    CREATE TABLE IF NOT EXISTS tool_pomodoro_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        phase TEXT NOT NULL,
        status TEXT NOT NULL,
        cycle_index INTEGER NOT NULL,
        focus_ms INTEGER NOT NULL,
        short_break_ms INTEGER NOT NULL,
        long_break_ms INTEGER NOT NULL,
        long_break_every INTEGER NOT NULL,
        phase_started_at INTEGER,
        phase_paused_at INTEGER,
        phase_remaining_ms INTEGER,
        completed_focus_count INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_tool_pomodoro_runs_status_updated
    ON tool_pomodoro_runs(status, updated_at);

    CREATE TABLE IF NOT EXISTS tool_daily_stats (
        date_key TEXT PRIMARY KEY,
        completed_pomodoros INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_tool_daily_stats_updated
    ON tool_daily_stats(updated_at);
";

pub const SOFTWARE_REMINDER_RULES_SCHEMA_SQL: &str = "
    CREATE INDEX IF NOT EXISTS idx_sessions_app_usage_time
    ON sessions(app_name COLLATE NOCASE, start_time, end_time);

    CREATE INDEX IF NOT EXISTS idx_sessions_exe_usage_time
    ON sessions(exe_name COLLATE NOCASE, start_time, end_time);

    CREATE TABLE IF NOT EXISTS tool_software_reminder_rules (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        app_name TEXT NOT NULL,
        exe_name TEXT,
        limit_ms INTEGER NOT NULL,
        message TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        disabled_at INTEGER,
        last_fired_date_key TEXT
    );

    CREATE INDEX IF NOT EXISTS idx_tool_software_reminder_rules_active
    ON tool_software_reminder_rules(disabled_at, app_name, exe_name);
";

pub const WEB_ACTIVITY_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS web_activity_segments (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        browser_client_id TEXT NOT NULL,
        browser_kind TEXT NOT NULL,
        browser_exe_name TEXT NOT NULL,
        domain TEXT NOT NULL,
        normalized_domain TEXT NOT NULL,
        url TEXT,
        title TEXT,
        favicon_url TEXT,
        start_time INTEGER NOT NULL,
        end_time INTEGER,
        duration INTEGER,
        source TEXT NOT NULL DEFAULT 'browser-extension',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_web_activity_segments_time
    ON web_activity_segments(start_time, end_time);

    CREATE INDEX IF NOT EXISTS idx_web_activity_segments_domain_time
    ON web_activity_segments(normalized_domain, start_time, end_time);

    CREATE UNIQUE INDEX IF NOT EXISTS idx_web_activity_segments_single_active
    ON web_activity_segments((1))
    WHERE end_time IS NULL;
";

pub const WEB_ACTIVITY_SESSION_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS web_activity_native_sessions (
        segment_id INTEGER PRIMARY KEY REFERENCES web_activity_segments(id) ON DELETE CASCADE,
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_web_activity_native_session
    ON web_activity_native_sessions(session_id);

    DROP TRIGGER IF EXISTS trg_native_session_web_boundary;
    CREATE TRIGGER trg_native_session_web_boundary
    AFTER UPDATE OF end_time ON sessions
    WHEN NEW.end_time IS NOT NULL
    BEGIN
        UPDATE web_activity_segments
        SET end_time = MAX(start_time, MIN(COALESCE(end_time, NEW.end_time), NEW.end_time)),
            duration = MAX(0, MIN(COALESCE(end_time, NEW.end_time), NEW.end_time) - start_time),
            updated_at = MAX(start_time, MIN(COALESCE(end_time, NEW.end_time), NEW.end_time))
        WHERE id IN (
            SELECT segment_id
            FROM web_activity_native_sessions
            WHERE session_id = NEW.id
        )
          AND (end_time IS NULL OR end_time > MAX(start_time, NEW.end_time));
    END;
";

pub const SCHEDULED_BACKUP_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS scheduled_backup_config (
        id INTEGER PRIMARY KEY CHECK(id = 1),
        enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
        cadence TEXT NOT NULL CHECK(cadence IN ('daily', 'weekly')),
        weekday INTEGER CHECK(weekday BETWEEN 1 AND 7),
        local_time_minutes INTEGER NOT NULL CHECK(local_time_minutes BETWEEN 0 AND 1439),
        target_dir TEXT NOT NULL CHECK(TRIM(target_dir) <> ''),
        retention_count INTEGER NOT NULL CHECK(retention_count = 3),
        target_generation TEXT NOT NULL CHECK(TRIM(target_generation) <> ''),
        schedule_anchor_at_ms INTEGER NOT NULL CHECK(schedule_anchor_at_ms >= 0),
        updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
        CHECK(
            (cadence = 'daily' AND weekday IS NULL)
            OR (cadence = 'weekly' AND weekday IS NOT NULL)
        )
    );

    CREATE TABLE IF NOT EXISTS scheduled_backup_runs (
        run_key TEXT PRIMARY KEY CHECK(TRIM(run_key) <> ''),
        target_generation TEXT NOT NULL CHECK(TRIM(target_generation) <> ''),
        logical_date TEXT NOT NULL CHECK(length(logical_date) = 10),
        logical_time_minutes INTEGER NOT NULL CHECK(logical_time_minutes BETWEEN 0 AND 1439),
        target_path TEXT NOT NULL CHECK(TRIM(target_path) <> ''),
        status TEXT NOT NULL CHECK(status IN ('running', 'retry_wait', 'succeeded', 'failed')),
        file_state TEXT NOT NULL CHECK(file_state IN ('absent', 'present', 'pruned', 'missing', 'conflict')),
        attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 1 AND 3),
        retry_at_ms INTEGER CHECK(retry_at_ms IS NULL OR retry_at_ms >= 0),
        started_at_ms INTEGER NOT NULL CHECK(started_at_ms >= 0),
        completed_at_ms INTEGER CHECK(completed_at_ms IS NULL OR completed_at_ms >= 0),
        archive_sha256 TEXT,
        size_bytes INTEGER CHECK(size_bytes IS NULL OR size_bytes >= 0),
        error_code TEXT,
        error_message TEXT,
        cleanup_warning TEXT,
        updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
        UNIQUE(target_generation, logical_date, logical_time_minutes)
    );

    CREATE INDEX IF NOT EXISTS idx_scheduled_backup_runs_retention
    ON scheduled_backup_runs(
        target_generation, status, file_state,
        logical_date DESC, logical_time_minutes DESC
    );

    CREATE INDEX IF NOT EXISTS idx_scheduled_backup_runs_status_retry
    ON scheduled_backup_runs(status, retry_at_ms, updated_at_ms);
";

pub const ACTIVITY_IMPORT_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS import_batches (
        id TEXT PRIMARY KEY CHECK(TRIM(id) <> ''),
        imported_at INTEGER NOT NULL CHECK(imported_at >= 0),
        source_name TEXT NOT NULL CHECK(TRIM(source_name) <> ''),
        source_kind TEXT NOT NULL CHECK(source_kind = 'patina-csv'),
        source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint) = 64),
        exact_session_count INTEGER NOT NULL DEFAULT 0 CHECK(exact_session_count >= 0),
        hour_bucket_count INTEGER NOT NULL DEFAULT 0 CHECK(hour_bucket_count >= 0)
    );

    CREATE INDEX IF NOT EXISTS idx_import_batches_imported_at
    ON import_batches(imported_at, id);

    CREATE TABLE IF NOT EXISTS import_exact_sessions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        batch_id TEXT NOT NULL,
        fingerprint TEXT NOT NULL UNIQUE CHECK(length(fingerprint) = 64),
        app_name TEXT NOT NULL CHECK(TRIM(app_name) <> ''),
        exe_name TEXT NOT NULL CHECK(TRIM(exe_name) <> ''),
        window_title TEXT NOT NULL DEFAULT '',
        start_time INTEGER NOT NULL,
        end_time INTEGER NOT NULL,
        duration INTEGER NOT NULL CHECK(
            duration > 0
            AND end_time > start_time
            AND ABS((end_time - start_time) - duration) <= 1000
        ),
        source_category TEXT,
        FOREIGN KEY(batch_id) REFERENCES import_batches(id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_import_exact_sessions_time
    ON import_exact_sessions(start_time, end_time);

    CREATE INDEX IF NOT EXISTS idx_import_exact_sessions_exe_time
    ON import_exact_sessions(exe_name COLLATE NOCASE, start_time, end_time);

    CREATE INDEX IF NOT EXISTS idx_import_exact_sessions_batch
    ON import_exact_sessions(batch_id, id);

    CREATE TABLE IF NOT EXISTS import_time_buckets (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        batch_id TEXT NOT NULL,
        fingerprint TEXT NOT NULL UNIQUE CHECK(length(fingerprint) = 64),
        app_name TEXT NOT NULL CHECK(TRIM(app_name) <> ''),
        exe_name TEXT NOT NULL CHECK(TRIM(exe_name) <> ''),
        bucket_start_time INTEGER NOT NULL,
        duration INTEGER NOT NULL CHECK(duration > 0 AND duration <= 3600000),
        source_category TEXT,
        FOREIGN KEY(batch_id) REFERENCES import_batches(id) ON DELETE CASCADE
    );

    CREATE INDEX IF NOT EXISTS idx_import_time_buckets_time
    ON import_time_buckets(bucket_start_time, duration);

    CREATE INDEX IF NOT EXISTS idx_import_time_buckets_exe_time
    ON import_time_buckets(exe_name COLLATE NOCASE, bucket_start_time);

    CREATE INDEX IF NOT EXISTS idx_import_time_buckets_batch
    ON import_time_buckets(batch_id, id);
";

pub fn tracker_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: CURRENT_BASELINE_MIGRATION_VERSION,
            description: CURRENT_BASELINE_MIGRATION_DESCRIPTION,
            sql: CURRENT_BASELINE_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: TOOLS_TABLES_MIGRATION_VERSION,
            description: TOOLS_TABLES_MIGRATION_DESCRIPTION,
            sql: TOOLS_TABLES_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: SOFTWARE_REMINDER_RULES_MIGRATION_VERSION,
            description: SOFTWARE_REMINDER_RULES_MIGRATION_DESCRIPTION,
            sql: SOFTWARE_REMINDER_RULES_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: WEB_ACTIVITY_MIGRATION_VERSION,
            description: WEB_ACTIVITY_MIGRATION_DESCRIPTION,
            sql: WEB_ACTIVITY_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: SCHEDULED_BACKUP_MIGRATION_VERSION,
            description: SCHEDULED_BACKUP_MIGRATION_DESCRIPTION,
            sql: SCHEDULED_BACKUP_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: ACTIVITY_IMPORT_MIGRATION_VERSION,
            description: ACTIVITY_IMPORT_MIGRATION_DESCRIPTION,
            sql: ACTIVITY_IMPORT_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
        Migration {
            version: WEB_ACTIVITY_SESSION_MIGRATION_VERSION,
            description: WEB_ACTIVITY_SESSION_MIGRATION_DESCRIPTION,
            sql: WEB_ACTIVITY_SESSION_SCHEMA_SQL,
            kind: MigrationKind::Up,
        },
    ]
}
