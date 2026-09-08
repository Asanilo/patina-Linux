use crate::data::repositories;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::backup::{
    BackupIconCache, BackupImportBatch, BackupImportExactSession, BackupImportTimeBucket,
    BackupMeta, BackupPayload, BackupPreview, BackupSession, BackupSetting, BackupTitleSample,
    BackupWebActivitySegment, CURRENT_BACKUP_SCHEMA_VERSION, CURRENT_BACKUP_VERSION,
};
use crate::platform::storage_paths;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Pool, Sqlite};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const BACKUP_FILE_EXT: &str = "zip";
const BACKUP_FORMAT: &str = "PatinaBackup";
const BACKUP_MANIFEST_ENTRY_NAME: &str = "manifest.json";
const BACKUP_CHECKSUMS_ENTRY_NAME: &str = "checksums.json";
const BACKUP_SESSIONS_ENTRY_NAME: &str = "data/sessions.json";
const BACKUP_TITLE_SAMPLES_ENTRY_NAME: &str = "data/session_title_samples.json";
const BACKUP_SETTINGS_ENTRY_NAME: &str = "data/settings.json";
const BACKUP_ICON_CACHE_ENTRY_NAME: &str = "data/icon_cache.json";
const BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME: &str = "data/web_activity_segments.json";
const BACKUP_TOOL_REMINDERS_ENTRY_NAME: &str = "data/tool_reminders.json";
const BACKUP_TOOL_TIMERS_ENTRY_NAME: &str = "data/tool_timers.json";
const BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME: &str = "data/tool_timer_laps.json";
const BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME: &str = "data/tool_pomodoro_runs.json";
const BACKUP_TOOL_DAILY_STATS_ENTRY_NAME: &str = "data/tool_daily_stats.json";
const BACKUP_IMPORT_ACTIVITY_ENTRY_NAME: &str = "data/import_activity.json";
const MAX_BACKUP_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BACKUP_ARCHIVE_ENTRIES: usize = 64;
const MAX_BACKUP_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BACKUP_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
struct BackupImportActivity {
    batches: Vec<BackupImportBatch>,
    exact_sessions: Vec<BackupImportExactSession>,
    time_buckets: Vec<BackupImportTimeBucket>,
}

#[derive(Debug)]
pub enum CreateNewBackupError {
    AlreadyExists,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupArchiveManifest {
    format: String,
    backup_version: u32,
    exported_at_ms: u64,
    schema_version: u32,
    app_version: String,
    files: BackupArchiveFiles,
    counts: BackupArchiveCounts,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupArchiveFiles {
    sessions: String,
    #[serde(default)]
    title_samples: String,
    settings: String,
    icon_cache: String,
    #[serde(default)]
    web_activity_segments: String,
    #[serde(default)]
    tool_reminders: String,
    #[serde(default)]
    tool_timers: String,
    #[serde(default)]
    tool_timer_laps: String,
    #[serde(default)]
    tool_pomodoro_runs: String,
    #[serde(default)]
    tool_daily_stats: String,
    #[serde(default)]
    import_activity: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupArchiveCounts {
    sessions: usize,
    #[serde(default)]
    title_samples: usize,
    settings: usize,
    icon_cache: usize,
    #[serde(default)]
    web_activity_segments: usize,
    #[serde(default)]
    tool_reminders: usize,
    #[serde(default)]
    tool_timers: usize,
    #[serde(default)]
    tool_timer_laps: usize,
    #[serde(default)]
    tool_pomodoro_runs: usize,
    #[serde(default)]
    tool_daily_stats: usize,
    #[serde(default)]
    import_batches: usize,
    #[serde(default)]
    import_exact_sessions: usize,
    #[serde(default)]
    import_time_buckets: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupArchiveChecksums {
    algorithm: String,
    files: BTreeMap<String, String>,
}

fn default_backup_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let backup_dir = storage_paths::resolve_storage_paths(app)?.backup_dir;
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("failed to create backup dir: {error}"))?;

    Ok(backup_dir.join(backup_file_name()))
}

fn backup_file_name() -> String {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    backup_file_name_for_timestamp(&timestamp)
}

fn backup_file_name_for_timestamp(timestamp: &str) -> String {
    format!("Patina-backup-{timestamp}.{BACKUP_FILE_EXT}")
}

fn resolve_backup_path<R: Runtime>(
    app: &AppHandle<R>,
    raw_path: Option<String>,
) -> Result<PathBuf, String> {
    let Some(raw_path) = raw_path.map(|value| value.trim().to_string()) else {
        return default_backup_path(app);
    };

    if raw_path.is_empty() {
        return default_backup_path(app);
    }

    let mut path = PathBuf::from(&raw_path);
    let ends_with_separator = raw_path.ends_with('\\') || raw_path.ends_with('/');
    if path.is_dir() || ends_with_separator {
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create backup target dir: {error}"))?;
        path = path.join(backup_file_name());
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create backup parent dir: {error}"))?;
        }
    }

    Ok(path)
}

async fn load_backup_payload<R: Runtime>(app: &AppHandle<R>) -> Result<BackupPayload, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    load_backup_payload_from_pool(&pool).await
}

async fn load_backup_payload_from_pool(pool: &Pool<Sqlite>) -> Result<BackupPayload, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start backup snapshot transaction: {error}"))?;
    let sessions = repositories::sessions::fetch_all_for_backup(&mut *tx).await?;
    let title_samples = repositories::session_title_samples::fetch_all_for_backup(&mut *tx).await?;
    let settings = repositories::settings::fetch_all_for_backup(&mut *tx).await?;
    let icon_cache = repositories::icon_cache::fetch_all_for_backup(&mut *tx).await?;
    let web_activity_segments = repositories::web_activity::fetch_all_for_backup(&mut *tx).await?;
    let tool_reminders = repositories::tools::fetch_all_reminders_for_backup(&mut *tx).await?;
    let tool_timers = repositories::tools::fetch_all_timers_for_backup(&mut *tx).await?;
    let tool_timer_laps = repositories::tools::fetch_all_timer_laps_for_backup(&mut *tx).await?;
    let tool_pomodoro_runs =
        repositories::tools::fetch_all_pomodoro_runs_for_backup(&mut *tx).await?;
    let tool_daily_stats = repositories::tools::fetch_all_daily_stats_for_backup(&mut *tx).await?;
    let (import_batches, import_exact_sessions, import_time_buckets) =
        repositories::activity_import::fetch_all_for_backup(&mut tx).await?;
    tx.commit()
        .await
        .map_err(|error| format!("failed to finish backup snapshot transaction: {error}"))?;

    Ok(BackupPayload {
        version: CURRENT_BACKUP_VERSION,
        meta: BackupMeta {
            exported_at_ms: now_ms(),
            schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        sessions,
        title_samples,
        settings,
        icon_cache,
        web_activity_segments,
        tool_reminders,
        tool_timers,
        tool_timer_laps,
        tool_pomodoro_runs,
        tool_daily_stats,
        import_batches,
        import_exact_sessions,
        import_time_buckets,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn resolve_dialog_directory(initial_path: Option<String>) -> Option<PathBuf> {
    let raw = initial_path?.trim().to_string();
    if raw.is_empty() {
        return None;
    }

    let path = PathBuf::from(raw);
    if path.is_dir() {
        return Some(path);
    }

    path.parent().and_then(|parent| {
        if parent.as_os_str().is_empty() {
            None
        } else {
            Some(parent.to_path_buf())
        }
    })
}

pub fn pick_backup_save_file(initial_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().add_filter("Patina backup", &["zip"]);
    if let Some(dir) = resolve_dialog_directory(initial_path) {
        dialog = dialog.set_directory(dir);
    }
    dialog = dialog.set_file_name(backup_file_name());

    dialog
        .save_file()
        .map(|path| path.to_string_lossy().to_string())
}

pub fn pick_backup_file(initial_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().add_filter("Patina backup", &["zip"]);
    if let Some(dir) = resolve_dialog_directory(initial_path) {
        dialog = dialog.set_directory(dir);
    }

    dialog
        .pick_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RestoreStrategy {
    #[default]
    Replace,
    Merge,
}

fn build_backup_manifest(payload: &BackupPayload) -> BackupArchiveManifest {
    BackupArchiveManifest {
        format: BACKUP_FORMAT.to_string(),
        backup_version: payload.version,
        exported_at_ms: payload.meta.exported_at_ms,
        schema_version: payload.meta.schema_version,
        app_version: payload.meta.app_version.clone(),
        files: BackupArchiveFiles {
            sessions: BACKUP_SESSIONS_ENTRY_NAME.to_string(),
            title_samples: BACKUP_TITLE_SAMPLES_ENTRY_NAME.to_string(),
            settings: BACKUP_SETTINGS_ENTRY_NAME.to_string(),
            icon_cache: BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
            web_activity_segments: BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME.to_string(),
            tool_reminders: BACKUP_TOOL_REMINDERS_ENTRY_NAME.to_string(),
            tool_timers: BACKUP_TOOL_TIMERS_ENTRY_NAME.to_string(),
            tool_timer_laps: BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME.to_string(),
            tool_pomodoro_runs: BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME.to_string(),
            tool_daily_stats: BACKUP_TOOL_DAILY_STATS_ENTRY_NAME.to_string(),
            import_activity: BACKUP_IMPORT_ACTIVITY_ENTRY_NAME.to_string(),
        },
        counts: BackupArchiveCounts {
            sessions: payload.sessions.len(),
            title_samples: payload.title_samples.len(),
            settings: payload.settings.len(),
            icon_cache: payload.icon_cache.len(),
            web_activity_segments: payload.web_activity_segments.len(),
            tool_reminders: payload.tool_reminders.len(),
            tool_timers: payload.tool_timers.len(),
            tool_timer_laps: payload.tool_timer_laps.len(),
            tool_pomodoro_runs: payload.tool_pomodoro_runs.len(),
            tool_daily_stats: payload.tool_daily_stats.len(),
            import_batches: payload.import_batches.len(),
            import_exact_sessions: payload.import_exact_sessions.len(),
            import_time_buckets: payload.import_time_buckets.len(),
        },
    }
}

fn serialize_pretty<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize backup {label}: {error}"))
}

fn checksum(value: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(value.as_bytes());
    format!("{:08x}", hasher.finalize())
}

fn zip_start_file(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    archive
        .start_file(name, options)
        .map_err(|error| format!("failed to start backup archive entry `{name}`: {error}"))
}

fn zip_write_file(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    content: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    zip_start_file(archive, name, options)?;
    archive
        .write_all(content.as_bytes())
        .map_err(|error| format!("failed to write backup archive entry `{name}`: {error}"))
}

fn encode_backup_archive(payload: &BackupPayload) -> Result<Vec<u8>, String> {
    let manifest = build_backup_manifest(payload);
    let sessions = serialize_pretty(&payload.sessions, "sessions")?;
    let title_samples = serialize_pretty(&payload.title_samples, "title samples")?;
    let settings = serialize_pretty(&payload.settings, "settings")?;
    let icon_cache = serialize_pretty(&payload.icon_cache, "icon cache")?;
    let web_activity_segments =
        serialize_pretty(&payload.web_activity_segments, "web activity segments")?;
    let tool_reminders = serialize_pretty(&payload.tool_reminders, "tool reminders")?;
    let tool_timers = serialize_pretty(&payload.tool_timers, "tool timers")?;
    let tool_timer_laps = serialize_pretty(&payload.tool_timer_laps, "tool timer laps")?;
    let tool_pomodoro_runs = serialize_pretty(&payload.tool_pomodoro_runs, "tool pomodoro runs")?;
    let tool_daily_stats = serialize_pretty(&payload.tool_daily_stats, "tool daily stats")?;
    let import_activity = serialize_pretty(
        &BackupImportActivity {
            batches: payload.import_batches.clone(),
            exact_sessions: payload.import_exact_sessions.clone(),
            time_buckets: payload.import_time_buckets.clone(),
        },
        "imported activity",
    )?;
    let manifest_json = serialize_pretty(&manifest, "manifest")?;

    let mut checksum_files = BTreeMap::new();
    checksum_files.insert(
        BACKUP_MANIFEST_ENTRY_NAME.to_string(),
        checksum(&manifest_json),
    );
    checksum_files.insert(BACKUP_SESSIONS_ENTRY_NAME.to_string(), checksum(&sessions));
    checksum_files.insert(
        BACKUP_TITLE_SAMPLES_ENTRY_NAME.to_string(),
        checksum(&title_samples),
    );
    checksum_files.insert(BACKUP_SETTINGS_ENTRY_NAME.to_string(), checksum(&settings));
    checksum_files.insert(
        BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
        checksum(&icon_cache),
    );
    checksum_files.insert(
        BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME.to_string(),
        checksum(&web_activity_segments),
    );
    checksum_files.insert(
        BACKUP_TOOL_REMINDERS_ENTRY_NAME.to_string(),
        checksum(&tool_reminders),
    );
    checksum_files.insert(
        BACKUP_TOOL_TIMERS_ENTRY_NAME.to_string(),
        checksum(&tool_timers),
    );
    checksum_files.insert(
        BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME.to_string(),
        checksum(&tool_timer_laps),
    );
    checksum_files.insert(
        BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME.to_string(),
        checksum(&tool_pomodoro_runs),
    );
    checksum_files.insert(
        BACKUP_TOOL_DAILY_STATS_ENTRY_NAME.to_string(),
        checksum(&tool_daily_stats),
    );
    checksum_files.insert(
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME.to_string(),
        checksum(&import_activity),
    );
    let checksums = BackupArchiveChecksums {
        algorithm: "crc32".to_string(),
        files: checksum_files,
    };
    let checksums_json = serialize_pretty(&checksums, "checksums")?;

    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip_write_file(
        &mut archive,
        BACKUP_MANIFEST_ENTRY_NAME,
        &manifest_json,
        options,
    )?;
    zip_write_file(&mut archive, BACKUP_SESSIONS_ENTRY_NAME, &sessions, options)?;
    zip_write_file(
        &mut archive,
        BACKUP_TITLE_SAMPLES_ENTRY_NAME,
        &title_samples,
        options,
    )?;
    zip_write_file(&mut archive, BACKUP_SETTINGS_ENTRY_NAME, &settings, options)?;
    zip_write_file(
        &mut archive,
        BACKUP_ICON_CACHE_ENTRY_NAME,
        &icon_cache,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME,
        &web_activity_segments,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_TOOL_REMINDERS_ENTRY_NAME,
        &tool_reminders,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_TOOL_TIMERS_ENTRY_NAME,
        &tool_timers,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME,
        &tool_timer_laps,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME,
        &tool_pomodoro_runs,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_TOOL_DAILY_STATS_ENTRY_NAME,
        &tool_daily_stats,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME,
        &import_activity,
        options,
    )?;
    zip_write_file(
        &mut archive,
        BACKUP_CHECKSUMS_ENTRY_NAME,
        &checksums_json,
        options,
    )?;

    let buffer = archive
        .finish()
        .map_err(|error| format!("failed to finish backup archive: {error}"))?
        .into_inner();
    Ok(buffer)
}

fn read_zip_entry(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    entry_name: &str,
    backup_path: &Path,
) -> Result<String, String> {
    let mut entry = archive.by_name(entry_name).map_err(|error| {
        format!(
            "backup archive `{}` does not contain {entry_name}: {error}",
            backup_path.display()
        )
    })?;
    if entry.size() > MAX_BACKUP_ENTRY_BYTES {
        return Err(format!(
            "backup archive entry `{entry_name}` exceeds the size limit"
        ));
    }
    let mut content = String::new();
    entry.read_to_string(&mut content).map_err(|error| {
        format!(
            "failed to read backup archive entry `{entry_name}` from `{}`: {error}",
            backup_path.display()
        )
    })?;
    Ok(content)
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    raw_json: &str,
    source_path: &Path,
    label: &str,
) -> Result<T, String> {
    serde_json::from_str::<T>(raw_json).map_err(|error| {
        format!(
            "failed to parse backup {label} from `{}`: {error}",
            source_path.display()
        )
    })
}

fn verify_backup_checksums(
    checksums: &BackupArchiveChecksums,
    entries: &[(&str, &str)],
    backup_path: &Path,
) -> Result<(), String> {
    if checksums.algorithm != "crc32" {
        return Err(format!(
            "backup archive `{}` uses unsupported checksum algorithm `{}`",
            backup_path.display(),
            checksums.algorithm
        ));
    }

    for (entry_name, content) in entries {
        let Some(expected) = checksums.files.get(*entry_name) else {
            return Err(format!(
                "backup archive `{}` is missing checksum for {entry_name}",
                backup_path.display()
            ));
        };
        let actual = checksum(content);
        if expected != &actual {
            return Err(format!(
                "backup archive `{}` checksum mismatch for {entry_name}",
                backup_path.display()
            ));
        }
    }

    Ok(())
}

fn backup_archive_declares_title_samples(
    manifest: &BackupArchiveManifest,
    checksums: &BackupArchiveChecksums,
) -> bool {
    !manifest.files.title_samples.trim().is_empty()
        || checksums
            .files
            .contains_key(BACKUP_TITLE_SAMPLES_ENTRY_NAME)
}

fn backup_archive_declares_entry(
    manifest_path: &str,
    checksums: &BackupArchiveChecksums,
    entry_name: &str,
) -> bool {
    !manifest_path.trim().is_empty() || checksums.files.contains_key(entry_name)
}

fn read_optional_declared_zip_entry(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    manifest_path: &str,
    checksums: &BackupArchiveChecksums,
    entry_name: &str,
    backup_path: &Path,
) -> Result<Option<String>, String> {
    if backup_archive_declares_entry(manifest_path, checksums, entry_name) {
        return read_zip_entry(archive, entry_name, backup_path).map(Some);
    }

    Ok(None)
}

fn decode_structured_backup_archive(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    backup_path: &Path,
) -> Result<BackupPayload, String> {
    let manifest_json = read_zip_entry(archive, BACKUP_MANIFEST_ENTRY_NAME, backup_path)?;
    let sessions_json = read_zip_entry(archive, BACKUP_SESSIONS_ENTRY_NAME, backup_path)?;
    let settings_json = read_zip_entry(archive, BACKUP_SETTINGS_ENTRY_NAME, backup_path)?;
    let icon_cache_json = read_zip_entry(archive, BACKUP_ICON_CACHE_ENTRY_NAME, backup_path)?;
    let checksums_json = read_zip_entry(archive, BACKUP_CHECKSUMS_ENTRY_NAME, backup_path)?;

    let checksums =
        parse_json::<BackupArchiveChecksums>(&checksums_json, backup_path, "checksums")?;
    let manifest = parse_json::<BackupArchiveManifest>(&manifest_json, backup_path, "manifest")?;
    if manifest.format != BACKUP_FORMAT {
        return Err(format!(
            "backup archive `{}` has unsupported format `{}`",
            backup_path.display(),
            manifest.format
        ));
    }
    let title_samples_json = if backup_archive_declares_title_samples(&manifest, &checksums) {
        Some(read_zip_entry(
            archive,
            BACKUP_TITLE_SAMPLES_ENTRY_NAME,
            backup_path,
        )?)
    } else {
        None
    };
    let tool_reminders_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.tool_reminders,
        &checksums,
        BACKUP_TOOL_REMINDERS_ENTRY_NAME,
        backup_path,
    )?;
    let tool_timers_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.tool_timers,
        &checksums,
        BACKUP_TOOL_TIMERS_ENTRY_NAME,
        backup_path,
    )?;
    let tool_timer_laps_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.tool_timer_laps,
        &checksums,
        BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME,
        backup_path,
    )?;
    let tool_pomodoro_runs_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.tool_pomodoro_runs,
        &checksums,
        BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME,
        backup_path,
    )?;
    let tool_daily_stats_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.tool_daily_stats,
        &checksums,
        BACKUP_TOOL_DAILY_STATS_ENTRY_NAME,
        backup_path,
    )?;
    let web_activity_segments_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.web_activity_segments,
        &checksums,
        BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME,
        backup_path,
    )?;
    let import_activity_json = read_optional_declared_zip_entry(
        archive,
        &manifest.files.import_activity,
        &checksums,
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME,
        backup_path,
    )?;
    let mut checksum_entries = vec![
        (BACKUP_MANIFEST_ENTRY_NAME, manifest_json.as_str()),
        (BACKUP_SESSIONS_ENTRY_NAME, sessions_json.as_str()),
        (BACKUP_SETTINGS_ENTRY_NAME, settings_json.as_str()),
        (BACKUP_ICON_CACHE_ENTRY_NAME, icon_cache_json.as_str()),
    ];
    if let Some(web_activity_segments_json) = web_activity_segments_json.as_deref() {
        checksum_entries.push((
            BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME,
            web_activity_segments_json,
        ));
    }
    if let Some(title_samples_json) = title_samples_json.as_deref() {
        checksum_entries.push((BACKUP_TITLE_SAMPLES_ENTRY_NAME, title_samples_json));
    }
    if let Some(tool_reminders_json) = tool_reminders_json.as_deref() {
        checksum_entries.push((BACKUP_TOOL_REMINDERS_ENTRY_NAME, tool_reminders_json));
    }
    if let Some(tool_timers_json) = tool_timers_json.as_deref() {
        checksum_entries.push((BACKUP_TOOL_TIMERS_ENTRY_NAME, tool_timers_json));
    }
    if let Some(tool_timer_laps_json) = tool_timer_laps_json.as_deref() {
        checksum_entries.push((BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME, tool_timer_laps_json));
    }
    if let Some(tool_pomodoro_runs_json) = tool_pomodoro_runs_json.as_deref() {
        checksum_entries.push((
            BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME,
            tool_pomodoro_runs_json,
        ));
    }
    if let Some(tool_daily_stats_json) = tool_daily_stats_json.as_deref() {
        checksum_entries.push((BACKUP_TOOL_DAILY_STATS_ENTRY_NAME, tool_daily_stats_json));
    }
    if let Some(import_activity_json) = import_activity_json.as_deref() {
        checksum_entries.push((BACKUP_IMPORT_ACTIVITY_ENTRY_NAME, import_activity_json));
    }
    verify_backup_checksums(&checksums, &checksum_entries, backup_path)?;

    let sessions = parse_json::<Vec<BackupSession>>(&sessions_json, backup_path, "sessions")?;
    let title_samples = title_samples_json
        .map(|json| parse_json::<Vec<BackupTitleSample>>(&json, backup_path, "title samples"))
        .transpose()?
        .unwrap_or_default();
    let settings = parse_json::<Vec<BackupSetting>>(&settings_json, backup_path, "settings")?;
    let icon_cache =
        parse_json::<Vec<BackupIconCache>>(&icon_cache_json, backup_path, "icon cache")?;
    let web_activity_segments = web_activity_segments_json
        .map(|json| {
            parse_json::<Vec<BackupWebActivitySegment>>(&json, backup_path, "web activity segments")
        })
        .transpose()?
        .unwrap_or_default();
    let tool_reminders = tool_reminders_json
        .map(|json| parse_json(&json, backup_path, "tool reminders"))
        .transpose()?
        .unwrap_or_default();
    let tool_timers = tool_timers_json
        .map(|json| parse_json(&json, backup_path, "tool timers"))
        .transpose()?
        .unwrap_or_default();
    let tool_timer_laps = tool_timer_laps_json
        .map(|json| parse_json(&json, backup_path, "tool timer laps"))
        .transpose()?
        .unwrap_or_default();
    let tool_pomodoro_runs = tool_pomodoro_runs_json
        .map(|json| parse_json(&json, backup_path, "tool pomodoro runs"))
        .transpose()?
        .unwrap_or_default();
    let tool_daily_stats = tool_daily_stats_json
        .map(|json| parse_json(&json, backup_path, "tool daily stats"))
        .transpose()?
        .unwrap_or_default();
    let import_activity = import_activity_json
        .map(|json| parse_json::<BackupImportActivity>(&json, backup_path, "imported activity"))
        .transpose()?
        .unwrap_or_default();

    Ok(BackupPayload {
        version: manifest.backup_version,
        meta: BackupMeta {
            exported_at_ms: manifest.exported_at_ms,
            schema_version: manifest.schema_version,
            app_version: manifest.app_version,
        },
        sessions,
        title_samples,
        settings,
        icon_cache,
        web_activity_segments,
        tool_reminders,
        tool_timers,
        tool_timer_laps,
        tool_pomodoro_runs,
        tool_daily_stats,
        import_batches: import_activity.batches,
        import_exact_sessions: import_activity.exact_sessions,
        import_time_buckets: import_activity.time_buckets,
    })
}

fn read_backup_payload(backup_path: &Path) -> Result<BackupPayload, String> {
    let metadata = fs::symlink_metadata(backup_path).map_err(|error| {
        format!(
            "failed to inspect backup file `{}`: {error}",
            backup_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "backup file `{}` must not be a symbolic link",
            backup_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "backup path `{}` is not a regular file",
            backup_path.display()
        ));
    }
    if metadata.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(format!(
            "backup file `{}` exceeds the archive size limit",
            backup_path.display()
        ));
    }
    let raw_bytes = fs::read(backup_path).map_err(|error| {
        format!(
            "failed to read backup file `{}`: {error}",
            backup_path.display()
        )
    })?;

    if raw_bytes.starts_with(b"PK") {
        let mut archive = ZipArchive::new(Cursor::new(raw_bytes)).map_err(|error| {
            format!(
                "failed to read backup archive `{}`: {error}",
                backup_path.display()
            )
        })?;
        validate_backup_archive_limits(&mut archive, backup_path)?;

        if archive.by_name(BACKUP_MANIFEST_ENTRY_NAME).is_ok() {
            return decode_structured_backup_archive(&mut archive, backup_path);
        }

        return Err(format!(
            "backup archive `{}` is not a supported structured Patina backup",
            backup_path.display()
        ));
    }

    Err(format!(
        "backup file `{}` is not a supported structured Patina backup",
        backup_path.display()
    ))
}

fn validate_backup_archive_limits(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    backup_path: &Path,
) -> Result<(), String> {
    if archive.len() > MAX_BACKUP_ARCHIVE_ENTRIES {
        return Err(format!(
            "backup archive `{}` contains too many entries",
            backup_path.display()
        ));
    }

    let mut names = HashSet::with_capacity(archive.len());
    let mut total_uncompressed_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            format!(
                "failed to inspect backup archive `{}` entry {index}: {error}",
                backup_path.display()
            )
        })?;
        if entry.size() > MAX_BACKUP_ENTRY_BYTES {
            return Err(format!(
                "backup archive `{}` entry `{}` exceeds the size limit",
                backup_path.display(),
                entry.name()
            ));
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "backup archive uncompressed size overflowed".to_string())?;
        if total_uncompressed_bytes > MAX_BACKUP_UNCOMPRESSED_BYTES {
            return Err(format!(
                "backup archive `{}` exceeds the uncompressed size limit",
                backup_path.display()
            ));
        }
        if !names.insert(entry.name().to_string()) {
            return Err(format!(
                "backup archive `{}` contains duplicate entry `{}`",
                backup_path.display(),
                entry.name()
            ));
        }
    }
    Ok(())
}

fn write_backup_archive_atomic(target_path: &Path, archive: &[u8]) -> Result<(), String> {
    let parent = target_path.parent().ok_or_else(|| {
        format!(
            "backup target `{}` has no parent directory",
            target_path.display()
        )
    })?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| format!("failed to create backup temporary file name: {error}"))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Patina-backup.zip");
    let temp_path = parent.join(format!(".{file_name}.{suffix}.tmp"));
    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp_path).map_err(|error| {
            format!(
                "failed to create backup temporary file `{}`: {error}",
                temp_path.display()
            )
        })?;
        file.write_all(archive)
            .map_err(|error| format!("failed to write backup temporary file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync backup temporary file: {error}"))?;
        drop(file);
        fs::rename(&temp_path, target_path)
            .map_err(|error| format!("failed to atomically replace backup file: {error}"))?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("failed to sync backup directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

pub async fn export_backup(backup_path: Option<String>, app: AppHandle) -> Result<String, String> {
    let payload = load_backup_payload(&app).await?;
    let target_path = resolve_backup_path(&app, backup_path)?;

    let archive = encode_backup_archive(&payload)?;
    if archive.len() as u64 > MAX_BACKUP_ARCHIVE_BYTES {
        return Err("generated backup exceeds the archive size limit".to_string());
    }
    write_backup_archive_atomic(&target_path, &archive)?;

    Ok(target_path.to_string_lossy().to_string())
}

pub async fn export_scheduled_backup_create_new(
    pool: &Pool<Sqlite>,
    target_path: &Path,
) -> Result<(), CreateNewBackupError> {
    let payload = load_backup_payload_from_pool(pool)
        .await
        .map_err(CreateNewBackupError::Failed)?;
    let archive = encode_backup_archive(&payload).map_err(CreateNewBackupError::Failed)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CreateNewBackupError::AlreadyExists
            } else {
                CreateNewBackupError::Failed(format!(
                    "failed to create scheduled backup `{}`: {error}",
                    target_path.display()
                ))
            }
        })?;

    if let Err(error) = file.write_all(&archive).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(target_path);
        return Err(CreateNewBackupError::Failed(format!(
            "failed to publish scheduled backup `{}`: {error}",
            target_path.display()
        )));
    }
    Ok(())
}

pub fn validate_scheduled_snapshot(target_path: &Path) -> Result<(String, u64), String> {
    read_backup_payload(target_path)?;
    let bytes = fs::read(target_path).map_err(|error| {
        format!(
            "failed to read scheduled backup `{}` for validation: {error}",
            target_path.display()
        )
    })?;
    let size = u64::try_from(bytes.len())
        .map_err(|_| "scheduled backup is too large to validate".to_string())?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    Ok((hash, size))
}

pub async fn restore_backup(
    backup_path: String,
    app: AppHandle,
    strategy: RestoreStrategy,
) -> Result<(), String> {
    let backup_path = PathBuf::from(backup_path.trim());
    if backup_path.as_os_str().is_empty() {
        return Err("backup path cannot be empty".to_string());
    }

    let payload = read_backup_payload(&backup_path)?;
    let restore_safety = payload.restore_safety();
    if !restore_safety.supported {
        return Err(restore_safety.message);
    }

    let pool = wait_for_sqlite_pool(&app).await?;
    restore_backup_payload(&pool, &payload, strategy).await?;
    Ok(())
}

async fn restore_backup_payload(
    pool: &Pool<Sqlite>,
    payload: &BackupPayload,
    strategy: RestoreStrategy,
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start restore transaction: {error}"))?;
    match strategy {
        RestoreStrategy::Replace => {
            repositories::session_title_samples::clear_for_restore(&mut tx).await?;
            repositories::sessions::clear_for_restore(&mut tx).await?;
            repositories::settings::clear_for_restore(&mut tx).await?;
            repositories::icon_cache::clear_for_restore(&mut tx).await?;
            repositories::web_activity::clear_for_restore(&mut tx).await?;
            repositories::tools::clear_for_restore(&mut tx).await?;
            repositories::activity_import::clear_for_restore(&mut tx).await?;

            repositories::sessions::insert_for_restore(&mut tx, &payload.sessions).await?;
            let session_id_map =
                repositories::sessions::resolve_restore_session_id_map(&mut tx, &payload.sessions)
                    .await?;
            repositories::session_title_samples::insert_for_restore(
                &mut tx,
                &payload.title_samples,
                &session_id_map,
            )
            .await?;
            repositories::settings::insert_for_restore(&mut tx, &payload.settings).await?;
            repositories::icon_cache::insert_for_restore(&mut tx, &payload.icon_cache).await?;
            repositories::web_activity::insert_for_restore(&mut tx, &payload.web_activity_segments)
                .await?;
            repositories::tools::insert_for_restore(
                &mut tx,
                &payload.tool_reminders,
                &payload.tool_timers,
                &payload.tool_timer_laps,
                &payload.tool_pomodoro_runs,
                &payload.tool_daily_stats,
            )
            .await?;
            repositories::activity_import::insert_for_restore(
                &mut tx,
                &payload.import_batches,
                &payload.import_exact_sessions,
                &payload.import_time_buckets,
            )
            .await?;
        }
        RestoreStrategy::Merge => {
            repositories::sessions::insert_missing_for_restore(&mut tx, &payload.sessions).await?;
            let session_id_map =
                repositories::sessions::resolve_restore_session_id_map(&mut tx, &payload.sessions)
                    .await?;
            repositories::session_title_samples::insert_missing_for_restore(
                &mut tx,
                &payload.title_samples,
                &session_id_map,
            )
            .await?;
            repositories::settings::insert_missing_for_restore(&mut tx, &payload.settings).await?;
            repositories::icon_cache::insert_missing_for_restore(&mut tx, &payload.icon_cache)
                .await?;
            repositories::web_activity::insert_missing_for_restore(
                &mut tx,
                &payload.web_activity_segments,
            )
            .await?;
            repositories::tools::insert_missing_for_restore(
                &mut tx,
                &payload.tool_reminders,
                &payload.tool_timers,
                &payload.tool_timer_laps,
                &payload.tool_pomodoro_runs,
                &payload.tool_daily_stats,
            )
            .await?;
            repositories::activity_import::insert_missing_for_restore(
                &mut tx,
                &payload.import_batches,
                &payload.import_exact_sessions,
                &payload.import_time_buckets,
            )
            .await?;
        }
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit restore transaction: {error}"))?;
    Ok(())
}

pub async fn preview_backup(backup_path: String) -> Result<BackupPreview, String> {
    let backup_path = PathBuf::from(backup_path.trim());
    if backup_path.as_os_str().is_empty() {
        return Err("backup path cannot be empty".to_string());
    }

    let payload = read_backup_payload(&backup_path)?;

    Ok(payload.preview())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema as db_schema;
    use crate::domain::backup::{BackupIconCache, BackupSession, BackupSetting, BackupTitleSample};
    use sqlx::{Executor, SqlitePool};

    fn temp_backup_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-backup-{label}-{}-{}.zip",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    #[test]
    fn backup_file_name_uses_timestamp_zip_format() {
        assert_eq!(
            backup_file_name_for_timestamp("20260515-213045"),
            "Patina-backup-20260515-213045.zip"
        );
    }

    #[test]
    fn backup_archive_write_is_atomic_and_owner_only() {
        let path = temp_backup_path("atomic");
        fs::write(&path, b"old").unwrap();

        write_backup_archive_atomic(&path, b"new archive").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new archive");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn backup_reader_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let target = temp_backup_path("symlink-target");
        let link = temp_backup_path("symlink-link");
        fs::write(&target, b"not a backup").unwrap();
        symlink(&target, &link).unwrap();

        let error = read_backup_payload(&link).unwrap_err();

        assert!(error.contains("must not be a symbolic link"));
        fs::remove_file(link).unwrap();
        fs::remove_file(target).unwrap();
    }

    #[test]
    fn backup_archive_uses_manifest_data_and_checksums_layout() {
        let payload = BackupPayload {
            version: CURRENT_BACKUP_VERSION,
            meta: BackupMeta {
                exported_at_ms: 1_714_000_000_000,
                schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                app_version: "test".to_string(),
            },
            sessions: vec![BackupSession {
                id: 1,
                app_name: "App".to_string(),
                exe_name: "app.exe".to_string(),
                window_title: Some("Window".to_string()),
                start_time: 10,
                end_time: Some(20),
                duration: Some(10),
                continuity_group_start_time: Some(10),
            }],
            title_samples: vec![BackupTitleSample {
                id: 1,
                session_id: 1,
                title: "Window".to_string(),
                start_time: 10,
                end_time: Some(20),
            }],
            settings: vec![BackupSetting {
                key: "language".to_string(),
                value: "zh-CN".to_string(),
            }],
            icon_cache: vec![BackupIconCache {
                exe_name: "app.exe".to_string(),
                icon_base64: "aWNvbg==".to_string(),
                last_updated: Some(30),
            }],
            web_activity_segments: Vec::new(),
            tool_reminders: Vec::new(),
            tool_timers: Vec::new(),
            tool_timer_laps: Vec::new(),
            tool_pomodoro_runs: Vec::new(),
            tool_daily_stats: Vec::new(),
            import_batches: Vec::new(),
            import_exact_sessions: Vec::new(),
            import_time_buckets: Vec::new(),
        };

        let archive = encode_backup_archive(&payload).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
        assert!(zip.by_name(BACKUP_MANIFEST_ENTRY_NAME).is_ok());
        let mut manifest_json = String::new();
        zip.by_name(BACKUP_MANIFEST_ENTRY_NAME)
            .unwrap()
            .read_to_string(&mut manifest_json)
            .unwrap();
        let manifest: BackupArchiveManifest =
            serde_json::from_str(&manifest_json).expect("manifest json");
        assert_eq!(manifest.format, BACKUP_FORMAT);
        assert!(zip.by_name(BACKUP_SESSIONS_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_TITLE_SAMPLES_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_SETTINGS_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_ICON_CACHE_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_IMPORT_ACTIVITY_ENTRY_NAME).is_ok());
        assert!(zip.by_name(BACKUP_CHECKSUMS_ENTRY_NAME).is_ok());
    }

    #[test]
    fn structured_backup_archive_round_trips_payload() {
        let payload = BackupPayload {
            version: CURRENT_BACKUP_VERSION,
            meta: BackupMeta {
                exported_at_ms: 1_714_000_000_000,
                schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                app_version: "test".to_string(),
            },
            sessions: vec![BackupSession {
                id: 1,
                app_name: "App".to_string(),
                exe_name: "app.exe".to_string(),
                window_title: Some("Window".to_string()),
                start_time: 10,
                end_time: Some(20),
                duration: Some(10),
                continuity_group_start_time: Some(10),
            }],
            title_samples: vec![BackupTitleSample {
                id: 1,
                session_id: 1,
                title: "Window".to_string(),
                start_time: 10,
                end_time: Some(20),
            }],
            settings: vec![BackupSetting {
                key: "language".to_string(),
                value: "zh-CN".to_string(),
            }],
            icon_cache: vec![BackupIconCache {
                exe_name: "app.exe".to_string(),
                icon_base64: "aWNvbg==".to_string(),
                last_updated: Some(30),
            }],
            web_activity_segments: Vec::new(),
            tool_reminders: Vec::new(),
            tool_timers: Vec::new(),
            tool_timer_laps: Vec::new(),
            tool_pomodoro_runs: Vec::new(),
            tool_daily_stats: Vec::new(),
            import_batches: vec![BackupImportBatch {
                id: "import-test".to_string(),
                imported_at: 40,
                source_name: "activity.csv".to_string(),
                source_kind: "patina-csv".to_string(),
                source_fingerprint: "a".repeat(64),
                exact_session_count: 1,
                hour_bucket_count: 1,
            }],
            import_exact_sessions: vec![BackupImportExactSession {
                id: 1,
                batch_id: "import-test".to_string(),
                fingerprint: "b".repeat(64),
                app_name: "Editor".to_string(),
                exe_name: "org.example.Editor".to_string(),
                window_title: "Document".to_string(),
                start_time: 100,
                end_time: 200,
                duration: 100,
                source_category: Some("Development".to_string()),
            }],
            import_time_buckets: vec![BackupImportTimeBucket {
                id: 1,
                batch_id: "import-test".to_string(),
                fingerprint: "c".repeat(64),
                app_name: "Editor".to_string(),
                exe_name: "org.example.Editor".to_string(),
                bucket_start_time: 3_600_000,
                duration: 60_000,
                source_category: Some("Development".to_string()),
            }],
        };

        let archive = encode_backup_archive(&payload).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
        let decoded = decode_structured_backup_archive(&mut zip, Path::new("backup.zip")).unwrap();
        assert_eq!(decoded.version, payload.version);
        assert_eq!(decoded.meta.exported_at_ms, payload.meta.exported_at_ms);
        assert_eq!(decoded.sessions.len(), 1);
        assert_eq!(decoded.title_samples.len(), 1);
        assert_eq!(decoded.settings.len(), 1);
        assert_eq!(decoded.icon_cache.len(), 1);
        assert_eq!(decoded.import_batches.len(), 1);
        assert_eq!(decoded.import_exact_sessions.len(), 1);
        assert_eq!(decoded.import_time_buckets.len(), 1);
    }

    #[test]
    fn structured_backup_archive_without_title_samples_still_decodes() {
        let manifest = BackupArchiveManifest {
            format: BACKUP_FORMAT.to_string(),
            backup_version: CURRENT_BACKUP_VERSION,
            exported_at_ms: 1_714_000_000_000,
            schema_version: CURRENT_BACKUP_SCHEMA_VERSION - 1,
            app_version: "test".to_string(),
            files: BackupArchiveFiles {
                sessions: BACKUP_SESSIONS_ENTRY_NAME.to_string(),
                title_samples: String::new(),
                settings: BACKUP_SETTINGS_ENTRY_NAME.to_string(),
                icon_cache: BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                web_activity_segments: String::new(),
                tool_reminders: String::new(),
                tool_timers: String::new(),
                tool_timer_laps: String::new(),
                tool_pomodoro_runs: String::new(),
                tool_daily_stats: String::new(),
                import_activity: String::new(),
            },
            counts: BackupArchiveCounts {
                sessions: 0,
                title_samples: 0,
                settings: 0,
                icon_cache: 0,
                web_activity_segments: 0,
                tool_reminders: 0,
                tool_timers: 0,
                tool_timer_laps: 0,
                tool_pomodoro_runs: 0,
                tool_daily_stats: 0,
                import_batches: 0,
                import_exact_sessions: 0,
                import_time_buckets: 0,
            },
        };
        let manifest_json = serialize_pretty(&manifest, "manifest").unwrap();
        let sessions = "[]";
        let settings = "[]";
        let icon_cache = "[]";
        let checksums = BackupArchiveChecksums {
            algorithm: "crc32".to_string(),
            files: BTreeMap::from([
                (
                    BACKUP_MANIFEST_ENTRY_NAME.to_string(),
                    checksum(&manifest_json),
                ),
                (BACKUP_SESSIONS_ENTRY_NAME.to_string(), checksum(sessions)),
                (BACKUP_SETTINGS_ENTRY_NAME.to_string(), checksum(settings)),
                (
                    BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                    checksum(icon_cache),
                ),
            ]),
        };
        let checksums_json = serialize_pretty(&checksums, "checksums").unwrap();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_write_file(
            &mut archive,
            BACKUP_MANIFEST_ENTRY_NAME,
            &manifest_json,
            options,
        )
        .unwrap();
        zip_write_file(&mut archive, BACKUP_SESSIONS_ENTRY_NAME, sessions, options).unwrap();
        zip_write_file(&mut archive, BACKUP_SETTINGS_ENTRY_NAME, settings, options).unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_ICON_CACHE_ENTRY_NAME,
            icon_cache,
            options,
        )
        .unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_CHECKSUMS_ENTRY_NAME,
            &checksums_json,
            options,
        )
        .unwrap();

        let archive_bytes = archive.finish().unwrap().into_inner();
        let mut zip = ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
        let decoded = decode_structured_backup_archive(&mut zip, Path::new("backup.zip")).unwrap();

        assert!(decoded.title_samples.is_empty());
    }

    #[test]
    fn structured_backup_archive_rejects_time_tracker_format() {
        let manifest = BackupArchiveManifest {
            format: "TimeTrackerBackup".to_string(),
            backup_version: CURRENT_BACKUP_VERSION,
            exported_at_ms: 1_714_000_000_000,
            schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
            app_version: "test".to_string(),
            files: BackupArchiveFiles {
                sessions: BACKUP_SESSIONS_ENTRY_NAME.to_string(),
                title_samples: String::new(),
                settings: BACKUP_SETTINGS_ENTRY_NAME.to_string(),
                icon_cache: BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                web_activity_segments: String::new(),
                tool_reminders: String::new(),
                tool_timers: String::new(),
                tool_timer_laps: String::new(),
                tool_pomodoro_runs: String::new(),
                tool_daily_stats: String::new(),
                import_activity: String::new(),
            },
            counts: BackupArchiveCounts {
                sessions: 0,
                title_samples: 0,
                settings: 0,
                icon_cache: 0,
                web_activity_segments: 0,
                tool_reminders: 0,
                tool_timers: 0,
                tool_timer_laps: 0,
                tool_pomodoro_runs: 0,
                tool_daily_stats: 0,
                import_batches: 0,
                import_exact_sessions: 0,
                import_time_buckets: 0,
            },
        };
        let manifest_json = serialize_pretty(&manifest, "manifest").unwrap();
        let sessions = "[]";
        let settings = "[]";
        let icon_cache = "[]";
        let checksums = BackupArchiveChecksums {
            algorithm: "crc32".to_string(),
            files: BTreeMap::from([
                (
                    BACKUP_MANIFEST_ENTRY_NAME.to_string(),
                    checksum(&manifest_json),
                ),
                (BACKUP_SESSIONS_ENTRY_NAME.to_string(), checksum(sessions)),
                (BACKUP_SETTINGS_ENTRY_NAME.to_string(), checksum(settings)),
                (
                    BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                    checksum(icon_cache),
                ),
            ]),
        };
        let checksums_json = serialize_pretty(&checksums, "checksums").unwrap();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_write_file(
            &mut archive,
            BACKUP_MANIFEST_ENTRY_NAME,
            &manifest_json,
            options,
        )
        .unwrap();
        zip_write_file(&mut archive, BACKUP_SESSIONS_ENTRY_NAME, sessions, options).unwrap();
        zip_write_file(&mut archive, BACKUP_SETTINGS_ENTRY_NAME, settings, options).unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_ICON_CACHE_ENTRY_NAME,
            icon_cache,
            options,
        )
        .unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_CHECKSUMS_ENTRY_NAME,
            &checksums_json,
            options,
        )
        .unwrap();

        let archive_bytes = archive.finish().unwrap().into_inner();
        let mut zip = ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
        let error =
            decode_structured_backup_archive(&mut zip, Path::new("backup.zip")).unwrap_err();

        assert!(error.contains("unsupported format `TimeTrackerBackup`"));
    }

    #[test]
    fn structured_backup_archive_declaring_title_samples_requires_the_file() {
        let manifest = BackupArchiveManifest {
            format: BACKUP_FORMAT.to_string(),
            backup_version: CURRENT_BACKUP_VERSION,
            exported_at_ms: 1_714_000_000_000,
            schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
            app_version: "test".to_string(),
            files: BackupArchiveFiles {
                sessions: BACKUP_SESSIONS_ENTRY_NAME.to_string(),
                title_samples: BACKUP_TITLE_SAMPLES_ENTRY_NAME.to_string(),
                settings: BACKUP_SETTINGS_ENTRY_NAME.to_string(),
                icon_cache: BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                web_activity_segments: String::new(),
                tool_reminders: String::new(),
                tool_timers: String::new(),
                tool_timer_laps: String::new(),
                tool_pomodoro_runs: String::new(),
                tool_daily_stats: String::new(),
                import_activity: String::new(),
            },
            counts: BackupArchiveCounts {
                sessions: 0,
                title_samples: 1,
                settings: 0,
                icon_cache: 0,
                web_activity_segments: 0,
                tool_reminders: 0,
                tool_timers: 0,
                tool_timer_laps: 0,
                tool_pomodoro_runs: 0,
                tool_daily_stats: 0,
                import_batches: 0,
                import_exact_sessions: 0,
                import_time_buckets: 0,
            },
        };
        let manifest_json = serialize_pretty(&manifest, "manifest").unwrap();
        let sessions = "[]";
        let settings = "[]";
        let icon_cache = "[]";
        let checksums = BackupArchiveChecksums {
            algorithm: "crc32".to_string(),
            files: BTreeMap::from([
                (
                    BACKUP_MANIFEST_ENTRY_NAME.to_string(),
                    checksum(&manifest_json),
                ),
                (BACKUP_SESSIONS_ENTRY_NAME.to_string(), checksum(sessions)),
                (BACKUP_TITLE_SAMPLES_ENTRY_NAME.to_string(), checksum("[]")),
                (BACKUP_SETTINGS_ENTRY_NAME.to_string(), checksum(settings)),
                (
                    BACKUP_ICON_CACHE_ENTRY_NAME.to_string(),
                    checksum(icon_cache),
                ),
            ]),
        };
        let checksums_json = serialize_pretty(&checksums, "checksums").unwrap();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_write_file(
            &mut archive,
            BACKUP_MANIFEST_ENTRY_NAME,
            &manifest_json,
            options,
        )
        .unwrap();
        zip_write_file(&mut archive, BACKUP_SESSIONS_ENTRY_NAME, sessions, options).unwrap();
        zip_write_file(&mut archive, BACKUP_SETTINGS_ENTRY_NAME, settings, options).unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_ICON_CACHE_ENTRY_NAME,
            icon_cache,
            options,
        )
        .unwrap();
        zip_write_file(
            &mut archive,
            BACKUP_CHECKSUMS_ENTRY_NAME,
            &checksums_json,
            options,
        )
        .unwrap();

        let archive_bytes = archive.finish().unwrap().into_inner();
        let mut zip = ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
        let error =
            decode_structured_backup_archive(&mut zip, Path::new("backup.zip")).unwrap_err();

        assert!(error.contains(BACKUP_TITLE_SAMPLES_ENTRY_NAME));
    }

    #[test]
    fn plain_json_backup_payload_is_not_supported() {
        let backup_path = std::env::temp_dir().join(format!(
            "timetracker-legacy-json-{}.json",
            std::process::id()
        ));
        fs::write(&backup_path, r#"{"version":1}"#).unwrap();

        let error = read_backup_payload(&backup_path).unwrap_err();
        let _ = fs::remove_file(&backup_path);
        assert!(error.contains("not a supported structured Patina backup"));
    }

    #[test]
    fn legacy_zip_backup_json_payload_is_not_supported() {
        let backup_path =
            std::env::temp_dir().join(format!("timetracker-legacy-zip-{}.zip", std::process::id()));
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip_write_file(&mut archive, "backup.json", r#"{"version":1}"#, options).unwrap();
        let archive_bytes = archive.finish().unwrap().into_inner();
        fs::write(&backup_path, archive_bytes).unwrap();

        let error = read_backup_payload(&backup_path).unwrap_err();
        let _ = fs::remove_file(&backup_path);
        assert!(error.contains("not a supported structured Patina backup"));
    }

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::TOOLS_TABLES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::WEB_ACTIVITY_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    #[test]
    fn restore_backup_payload_rolls_back_when_insert_fails() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;

            sqlx::query(
                "INSERT INTO sessions (app_name, exe_name, window_title, start_time, end_time, duration)\n                 VALUES ('Baseline App', 'baseline.exe', 'Baseline Window', 1000, 2000, 1000)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO settings (key, value) VALUES ('baseline_key', 'baseline_value')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO icon_cache (exe_name, icon_base64, last_updated)\n                 VALUES ('baseline.exe', 'aWNvbg==', 1234)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let bad_payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: vec![BackupSession {
                    id: 100,
                    app_name: "New App".to_string(),
                    exe_name: "new.exe".to_string(),
                    window_title: Some("New Window".to_string()),
                    start_time: 3000,
                    end_time: Some(4000),
                    duration: Some(1000),
                    continuity_group_start_time: Some(3000),
                }],
                title_samples: vec![BackupTitleSample {
                    id: 100,
                    session_id: 100,
                    title: "New Window".to_string(),
                    start_time: 3000,
                    end_time: Some(4000),
                }],
                settings: vec![
                    BackupSetting {
                        key: "dup_key".to_string(),
                        value: "v1".to_string(),
                    },
                    BackupSetting {
                        key: "dup_key".to_string(),
                        value: "v2".to_string(),
                    },
                ],
                icon_cache: vec![BackupIconCache {
                    exe_name: "new.exe".to_string(),
                    icon_base64: "bmV3aWNvbg==".to_string(),
                    last_updated: Some(5678),
                }],
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: Vec::new(),
                import_exact_sessions: Vec::new(),
                import_time_buckets: Vec::new(),
            };

            let result =
                restore_backup_payload(&pool, &bad_payload, RestoreStrategy::Replace).await;
            assert!(result.is_err());
            assert!(
                result.unwrap_err().contains("failed to restore settings"),
                "restore should fail in settings stage"
            );

            let session_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE exe_name = 'baseline.exe'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let setting_value: Option<String> =
                sqlx::query_scalar("SELECT value FROM settings WHERE key = 'baseline_key' LIMIT 1")
                    .fetch_optional(&pool)
                    .await
                    .unwrap();
            let icon_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM icon_cache WHERE exe_name = 'baseline.exe'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_eq!(session_count, 1, "original session should be preserved");
            assert_eq!(
                setting_value.as_deref(),
                Some("baseline_value"),
                "original setting should be preserved"
            );
            assert_eq!(icon_count, 1, "original icon cache should be preserved");
        });
    }

    #[test]
    fn replace_restore_restores_imported_activity() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: Vec::new(),
                title_samples: Vec::new(),
                settings: Vec::new(),
                icon_cache: Vec::new(),
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: vec![BackupImportBatch {
                    id: "import-restore".to_string(),
                    imported_at: 1_000,
                    source_name: "activity.csv".to_string(),
                    source_kind: "patina-csv".to_string(),
                    source_fingerprint: "a".repeat(64),
                    exact_session_count: 1,
                    hour_bucket_count: 1,
                }],
                import_exact_sessions: vec![BackupImportExactSession {
                    id: 7,
                    batch_id: "import-restore".to_string(),
                    fingerprint: "b".repeat(64),
                    app_name: "Editor".to_string(),
                    exe_name: "org.example.Editor".to_string(),
                    window_title: "Document".to_string(),
                    start_time: 2_000,
                    end_time: 3_000,
                    duration: 1_000,
                    source_category: None,
                }],
                import_time_buckets: vec![BackupImportTimeBucket {
                    id: 9,
                    batch_id: "import-restore".to_string(),
                    fingerprint: "c".repeat(64),
                    app_name: "Editor".to_string(),
                    exe_name: "org.example.Editor".to_string(),
                    bucket_start_time: 3_600_000,
                    duration: 60_000,
                    source_category: None,
                }],
            };

            restore_backup_payload(&pool, &payload, RestoreStrategy::Replace)
                .await
                .unwrap();

            let batch_counts: (i64, i64) = sqlx::query_as(
                "SELECT exact_session_count, hour_bucket_count
                 FROM import_batches WHERE id = 'import-restore'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let exact_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM import_exact_sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
            let bucket_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM import_time_buckets")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(batch_counts, (1, 1));
            assert_eq!(exact_count, 1);
            assert_eq!(bucket_count, 1);
        });
    }

    #[test]
    fn replace_restore_restores_title_samples() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: vec![BackupSession {
                    id: 10,
                    app_name: "Editor".to_string(),
                    exe_name: "editor.exe".to_string(),
                    window_title: Some("Doc B".to_string()),
                    start_time: 1000,
                    end_time: Some(3000),
                    duration: Some(2000),
                    continuity_group_start_time: Some(1000),
                }],
                title_samples: vec![
                    BackupTitleSample {
                        id: 1,
                        session_id: 10,
                        title: "Doc A".to_string(),
                        start_time: 1000,
                        end_time: Some(2000),
                    },
                    BackupTitleSample {
                        id: 2,
                        session_id: 10,
                        title: "Doc B".to_string(),
                        start_time: 2000,
                        end_time: Some(3000),
                    },
                ],
                settings: Vec::new(),
                icon_cache: Vec::new(),
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: Vec::new(),
                import_exact_sessions: Vec::new(),
                import_time_buckets: Vec::new(),
            };

            restore_backup_payload(&pool, &payload, RestoreStrategy::Replace)
                .await
                .unwrap();

            let samples: Vec<(i64, String, i64, Option<i64>)> = sqlx::query_as(
                "SELECT session_id, title, start_time, end_time
                 FROM session_title_samples
                 ORDER BY id ASC",
            )
            .fetch_all(&pool)
            .await
            .unwrap();

            assert_eq!(
                samples,
                vec![
                    (10, "Doc A".to_string(), 1000, Some(2000)),
                    (10, "Doc B".to_string(), 2000, Some(3000)),
                ]
            );
        });
    }

    #[test]
    fn replace_restore_skips_orphan_title_samples() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: vec![BackupSession {
                    id: 10,
                    app_name: "Editor".to_string(),
                    exe_name: "editor.exe".to_string(),
                    window_title: Some("Doc".to_string()),
                    start_time: 1000,
                    end_time: Some(2000),
                    duration: Some(1000),
                    continuity_group_start_time: Some(1000),
                }],
                title_samples: vec![
                    BackupTitleSample {
                        id: 1,
                        session_id: 10,
                        title: "Doc".to_string(),
                        start_time: 1000,
                        end_time: Some(2000),
                    },
                    BackupTitleSample {
                        id: 2,
                        session_id: 99,
                        title: "Orphan".to_string(),
                        start_time: 1000,
                        end_time: Some(2000),
                    },
                ],
                settings: Vec::new(),
                icon_cache: Vec::new(),
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: Vec::new(),
                import_exact_sessions: Vec::new(),
                import_time_buckets: Vec::new(),
            };

            restore_backup_payload(&pool, &payload, RestoreStrategy::Replace)
                .await
                .unwrap();

            let samples: Vec<(i64, String)> = sqlx::query_as(
                "SELECT session_id, title
                 FROM session_title_samples
                 ORDER BY id ASC",
            )
            .fetch_all(&pool)
            .await
            .unwrap();

            assert_eq!(samples, vec![(10, "Doc".to_string())]);
        });
    }

    #[test]
    fn merge_restore_payload_preserves_existing_data_and_imports_missing_data() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;

            sqlx::query(
                "INSERT INTO sessions (app_name, exe_name, window_title, start_time, end_time, duration, continuity_group_start_time)
                 VALUES ('Existing App', 'existing.exe', 'Existing Window', 1000, 2000, 1000, 1000)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO settings (key, value) VALUES ('language', 'zh-CN')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO icon_cache (exe_name, icon_base64, last_updated)
                 VALUES ('existing.exe', 'old', 1)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: vec![
                    BackupSession {
                        id: 1,
                        app_name: "Existing App".to_string(),
                        exe_name: "existing.exe".to_string(),
                        window_title: Some("Existing Window".to_string()),
                        start_time: 1000,
                        end_time: Some(2000),
                        duration: Some(1000),
                        continuity_group_start_time: Some(1000),
                    },
                    BackupSession {
                        id: 2,
                        app_name: "Imported App".to_string(),
                        exe_name: "imported.exe".to_string(),
                        window_title: Some("Imported Window".to_string()),
                        start_time: 3000,
                        end_time: Some(4000),
                        duration: Some(1000),
                        continuity_group_start_time: Some(3000),
                    },
                ],
                title_samples: vec![BackupTitleSample {
                    id: 1,
                    session_id: 2,
                    title: "Imported Window".to_string(),
                    start_time: 3000,
                    end_time: Some(4000),
                }],
                settings: vec![
                    BackupSetting {
                        key: "language".to_string(),
                        value: "en-US".to_string(),
                    },
                    BackupSetting {
                        key: "theme_mode".to_string(),
                        value: "dark".to_string(),
                    },
                ],
                icon_cache: vec![
                    BackupIconCache {
                        exe_name: "existing.exe".to_string(),
                        icon_base64: "new".to_string(),
                        last_updated: Some(2),
                    },
                    BackupIconCache {
                        exe_name: "imported.exe".to_string(),
                        icon_base64: "imported".to_string(),
                        last_updated: Some(3),
                    },
                ],
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: Vec::new(),
                import_exact_sessions: Vec::new(),
                import_time_buckets: Vec::new(),
            };

            restore_backup_payload(&pool, &payload, RestoreStrategy::Merge)
                .await
                .unwrap();

            let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
            let language: String =
                sqlx::query_scalar("SELECT value FROM settings WHERE key = 'language'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let theme_mode: String =
                sqlx::query_scalar("SELECT value FROM settings WHERE key = 'theme_mode'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let existing_icon: String = sqlx::query_scalar(
                "SELECT icon_base64 FROM icon_cache WHERE exe_name = 'existing.exe'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let imported_icon: String = sqlx::query_scalar(
                "SELECT icon_base64 FROM icon_cache WHERE exe_name = 'imported.exe'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_eq!(session_count, 2);
            assert_eq!(language, "zh-CN");
            assert_eq!(theme_mode, "dark");
            assert_eq!(existing_icon, "old");
            assert_eq!(imported_icon, "imported");

            let title_sample_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM session_title_samples")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(title_sample_count, 1);

            restore_backup_payload(&pool, &payload, RestoreStrategy::Merge)
                .await
                .unwrap();
            let title_sample_count_after_second_restore: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM session_title_samples")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(title_sample_count_after_second_restore, 1);
        });
    }

    #[test]
    fn merge_restore_maps_title_samples_to_inserted_session_ids() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;

            sqlx::query(
                "INSERT INTO sessions (id, app_name, exe_name, window_title, start_time, end_time, duration, continuity_group_start_time)
                 VALUES (2, 'Different App', 'different.exe', 'Different Window', 10, 20, 10, 10)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let payload = BackupPayload {
                version: CURRENT_BACKUP_VERSION,
                meta: BackupMeta {
                    exported_at_ms: 1,
                    schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
                    app_version: "test".to_string(),
                },
                sessions: vec![BackupSession {
                    id: 2,
                    app_name: "Imported App".to_string(),
                    exe_name: "imported.exe".to_string(),
                    window_title: Some("Imported Window".to_string()),
                    start_time: 3000,
                    end_time: Some(4000),
                    duration: Some(1000),
                    continuity_group_start_time: Some(3000),
                }],
                title_samples: vec![BackupTitleSample {
                    id: 8,
                    session_id: 2,
                    title: "Imported Window".to_string(),
                    start_time: 3000,
                    end_time: Some(4000),
                }],
                settings: Vec::new(),
                icon_cache: Vec::new(),
                web_activity_segments: Vec::new(),
                tool_reminders: Vec::new(),
                tool_timers: Vec::new(),
                tool_timer_laps: Vec::new(),
                tool_pomodoro_runs: Vec::new(),
                tool_daily_stats: Vec::new(),
                import_batches: Vec::new(),
                import_exact_sessions: Vec::new(),
                import_time_buckets: Vec::new(),
            };

            restore_backup_payload(&pool, &payload, RestoreStrategy::Merge)
                .await
                .unwrap();

            let restored: (i64, String) = sqlx::query_as(
                "SELECT samples.session_id, sessions.exe_name
                 FROM session_title_samples samples
                 JOIN sessions ON sessions.id = samples.session_id
                 WHERE samples.title = 'Imported Window'
                 LIMIT 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();

            assert_ne!(restored.0, 2);
            assert_eq!(restored.1, "imported.exe");
        });
    }
}
