//! Snapshot-consistent export to a private staged file; never buffers a whole table or ZIP.
use super::*;
use futures_util::TryStreamExt;
use sqlx::{Column, Row, TypeInfo, ValueRef};
use std::io::{BufWriter, Seek};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

static EXPORT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) async fn export(
    pool: &Pool<Sqlite>,
    target: &Path,
    create_new: bool,
) -> Result<(), CreateNewBackupError> {
    let permit = EXPORT
        .acquire()
        .await
        .map_err(|_| CreateNewBackupError::Failed("backup export is unavailable".into()))?;
    let cancel = CancelOnDrop(Arc::new(AtomicBool::new(false)));
    let cancelled = cancel.0.clone();
    let pool = pool.clone();
    let target = target.to_path_buf();
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        runtime.block_on(export_snapshot(&pool, &target, create_new, &cancelled))
    })
    .await
    .map_err(|error| CreateNewBackupError::Failed(format!("backup worker failed: {error}")))?
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        Err("backup export cancelled".into())
    } else {
        Ok(())
    }
}

// All entry bytes pass through the same limits and incremental checksum.
struct Entry<'a, W: Write + Seek> {
    archive: &'a mut ZipWriter<W>,
    total: &'a mut u64,
    bytes: u64,
    hash: Hasher,
    cancelled: &'a AtomicBool,
}
impl<W: Write + Seek> Write for Entry<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        check_cancel(self.cancelled).map_err(std::io::Error::other)?;
        if self.bytes.saturating_add(bytes.len() as u64) > MAX_BACKUP_ENTRY_BYTES
            || self.total.saturating_add(bytes.len() as u64) > MAX_BACKUP_UNCOMPRESSED_BYTES
        {
            return Err(std::io::Error::other("backup data exceeds size limit"));
        }
        let written = self.archive.write(bytes)?;
        self.hash.update(&bytes[..written]);
        self.bytes += written as u64;
        *self.total += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.archive.flush()
    }
}

fn start_entry<'a, W: Write + Seek>(
    archive: &'a mut ZipWriter<W>,
    total: &'a mut u64,
    name: &str,
    cancelled: &'a AtomicBool,
) -> Result<Entry<'a, W>, String> {
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .map_err(|error| error.to_string())?;
    Ok(Entry {
        archive,
        total,
        bytes: 0,
        hash: Hasher::new(),
        cancelled,
    })
}

async fn array<W: Write + Seek>(
    connection: &mut sqlx::SqliteConnection,
    writer: &mut Entry<'_, W>,
    query: &str,
) -> Result<usize, String> {
    writer.write_all(b"[").map_err(|error| error.to_string())?;
    let mut rows = sqlx::query(query).fetch(connection);
    let mut count = 0;
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        check_cancel(writer.cancelled)?;
        let mut object = serde_json::Map::new();
        for column in row.columns() {
            let raw = row
                .try_get_raw(column.ordinal())
                .map_err(|error| error.to_string())?;
            let value = if raw.is_null() {
                serde_json::Value::Null
            } else {
                match raw.type_info().name() {
                    "INTEGER" => serde_json::Value::from(
                        row.try_get::<i64, _>(column.ordinal())
                            .map_err(|error| error.to_string())?,
                    ),
                    "TEXT" => serde_json::Value::from(
                        row.try_get::<String, _>(column.ordinal())
                            .map_err(|error| error.to_string())?,
                    ),
                    _ => {
                        return Err(format!(
                            "unsupported backup field type for {}",
                            column.name()
                        ))
                    }
                }
            };
            object.insert(column.name().to_string(), value);
        }
        if count > 0 {
            writer.write_all(b",").map_err(|error| error.to_string())?;
        }
        serde_json::to_writer(&mut *writer, &object).map_err(|error| error.to_string())?;
        count += 1;
    }
    writer.write_all(b"]").map_err(|error| error.to_string())?;
    Ok(count)
}

async fn export_snapshot(
    pool: &Pool<Sqlite>,
    target: &Path,
    create_new: bool,
    cancelled: &AtomicBool,
) -> Result<(), CreateNewBackupError> {
    let fail = CreateNewBackupError::Failed;
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| fail("backup target requires a parent directory".into()))?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|error| fail(error.to_string()))?;
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let path = parent.join(format!(".patina-backup-{suffix}.tmp"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|error| fail(error.to_string()))?;
    let stage = Stage(path);
    let mut archive = ZipWriter::new(BufWriter::with_capacity(64 * 1024, file));
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| fail(error.to_string()))?;
    let mut total = 0;
    let mut checksums = BTreeMap::new();
    let mut counts = BackupArchiveCounts::default();
    // Explicit field lists preserve the public backup schema when tables gain private columns.
    let tables = [
        (BACKUP_SESSIONS_ENTRY_NAME, "SELECT id,app_name,exe_name,window_title,start_time,end_time,duration,continuity_group_start_time FROM sessions ORDER BY id", &mut counts.sessions),
        (BACKUP_TITLE_SAMPLES_ENTRY_NAME, "SELECT id,session_id,title,start_time,end_time FROM session_title_samples ORDER BY id", &mut counts.title_samples),
        (BACKUP_SETTINGS_ENTRY_NAME, "SELECT key,value FROM settings ORDER BY key", &mut counts.settings),
        (BACKUP_ICON_CACHE_ENTRY_NAME, "SELECT exe_name,icon_base64,last_updated FROM icon_cache ORDER BY exe_name", &mut counts.icon_cache),
        (BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME, "SELECT w.id,browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,url,title,favicon_url,start_time,end_time,duration,source,created_at,updated_at,l.session_id AS native_session_id FROM web_activity_segments w LEFT JOIN web_activity_native_sessions l ON l.segment_id=w.id ORDER BY w.id", &mut counts.web_activity_segments),
        (BACKUP_TOOL_REMINDERS_ENTRY_NAME, "SELECT id,label,scheduled_at,created_at,status,fired_at,cancelled_at FROM tool_reminders ORDER BY id", &mut counts.tool_reminders),
        (BACKUP_TOOL_TIMERS_ENTRY_NAME, "SELECT id,mode,label,duration_ms,accumulated_ms,started_at,paused_at,completed_at,status,created_at,updated_at FROM tool_timers ORDER BY id", &mut counts.tool_timers),
        (BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME, "SELECT id,timer_id,lap_index,started_at,ended_at,duration_ms FROM tool_timer_laps ORDER BY id", &mut counts.tool_timer_laps),
        (BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME, "SELECT id,phase,status,cycle_index,focus_ms,short_break_ms,long_break_ms,long_break_every,phase_started_at,phase_paused_at,phase_remaining_ms,completed_focus_count,created_at,updated_at FROM tool_pomodoro_runs ORDER BY id", &mut counts.tool_pomodoro_runs),
        (BACKUP_TOOL_DAILY_STATS_ENTRY_NAME, "SELECT date_key,completed_pomodoros,updated_at FROM tool_daily_stats ORDER BY date_key", &mut counts.tool_daily_stats),
    ];
    for (name, query, count) in tables {
        let mut entry = start_entry(&mut archive, &mut total, name, cancelled).map_err(fail)?;
        *count = array(&mut tx, &mut entry, query).await.map_err(fail)?;
        checksums.insert(name.to_string(), format!("{:08x}", entry.hash.finalize()));
    }
    let mut entry = start_entry(
        &mut archive,
        &mut total,
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME,
        cancelled,
    )
    .map_err(fail)?;
    entry
        .write_all(b"{\"batches\":")
        .map_err(|error| fail(error.to_string()))?;
    counts.import_batches = array(&mut tx, &mut entry,"SELECT id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count FROM import_batches ORDER BY id").await.map_err(fail)?;
    entry
        .write_all(b",\"exact_sessions\":")
        .map_err(|error| fail(error.to_string()))?;
    counts.import_exact_sessions = array(&mut tx, &mut entry,"SELECT id,batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration,source_category FROM import_exact_sessions ORDER BY id").await.map_err(fail)?;
    entry
        .write_all(b",\"time_buckets\":")
        .map_err(|error| fail(error.to_string()))?;
    counts.import_time_buckets = array(&mut tx, &mut entry,"SELECT id,batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration,source_category FROM import_time_buckets ORDER BY id").await.map_err(fail)?;
    entry
        .write_all(b"}")
        .map_err(|error| fail(error.to_string()))?;
    checksums.insert(
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME.to_string(),
        format!("{:08x}", entry.hash.finalize()),
    );
    tx.commit().await.map_err(|error| fail(error.to_string()))?;
    let empty = BackupPayload {
        version: CURRENT_BACKUP_VERSION,
        meta: BackupMeta {
            exported_at_ms: now_ms(),
            schema_version: CURRENT_BACKUP_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
        },
        sessions: vec![],
        title_samples: vec![],
        settings: vec![],
        icon_cache: vec![],
        web_activity_segments: vec![],
        tool_reminders: vec![],
        tool_timers: vec![],
        tool_timer_laps: vec![],
        tool_pomodoro_runs: vec![],
        tool_daily_stats: vec![],
        import_batches: vec![],
        import_exact_sessions: vec![],
        import_time_buckets: vec![],
    };
    let mut manifest = build_backup_manifest(&empty);
    manifest.counts = counts;
    let mut entry = start_entry(
        &mut archive,
        &mut total,
        BACKUP_MANIFEST_ENTRY_NAME,
        cancelled,
    )
    .map_err(fail)?;
    serde_json::to_writer(&mut entry, &manifest).map_err(|error| fail(error.to_string()))?;
    checksums.insert(
        BACKUP_MANIFEST_ENTRY_NAME.to_string(),
        format!("{:08x}", entry.hash.finalize()),
    );
    let mut entry = start_entry(
        &mut archive,
        &mut total,
        BACKUP_CHECKSUMS_ENTRY_NAME,
        cancelled,
    )
    .map_err(fail)?;
    serde_json::to_writer(
        &mut entry,
        &BackupArchiveChecksums {
            algorithm: "crc32".into(),
            files: checksums,
        },
    )
    .map_err(|error| fail(error.to_string()))?;
    let mut writer = archive.finish().map_err(|error| fail(error.to_string()))?;
    writer.flush().map_err(|error| fail(error.to_string()))?;
    if writer
        .get_ref()
        .metadata()
        .map_err(|error| fail(error.to_string()))?
        .len()
        > MAX_BACKUP_ARCHIVE_BYTES
    {
        return Err(fail("backup archive exceeds size limit".into()));
    }
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| fail(error.to_string()))?;
    drop(writer);
    check_cancel(cancelled).map_err(fail)?;
    if create_new {
        fs::hard_link(&stage.0, target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CreateNewBackupError::AlreadyExists
            } else {
                fail(error.to_string())
            }
        })?;
    } else {
        fs::rename(&stage.0, target).map_err(|error| fail(error.to_string()))?;
    }
    drop(stage);
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| fail(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_cancellation_and_limits_fail_before_writing() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let mut total = MAX_BACKUP_UNCOMPRESSED_BYTES;
        let cancelled = AtomicBool::new(false);
        let mut entry = start_entry(&mut archive, &mut total, "test.json", &cancelled).unwrap();
        assert!(entry
            .write_all(b"x")
            .unwrap_err()
            .to_string()
            .contains("size limit"));
        assert_eq!(entry.bytes, 0);
        cancelled.store(true, Ordering::Release);
        assert!(entry
            .write_all(b"x")
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert_eq!(entry.bytes, 0);
    }

    #[test]
    fn cancelled_snapshot_keeps_target_and_cleans_stage() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "patina-cancel-export-{}-{}",
                std::process::id(),
                now_ms()
            ));
            fs::create_dir(&root).unwrap();
            let target = root.join("existing.zip");
            fs::write(&target, b"existing").unwrap();
            let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
            let result = export_snapshot(&pool, &target, false, &AtomicBool::new(true)).await;
            assert!(
                matches!(result, Err(CreateNewBackupError::Failed(message)) if message.contains("cancelled"))
            );
            assert_eq!(fs::read(&target).unwrap(), b"existing");
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
            pool.close().await;
            fs::remove_dir_all(root).unwrap();
        });
    }
}
