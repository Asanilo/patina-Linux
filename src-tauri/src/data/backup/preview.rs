//! Validating preview: keep one decoded record, not the full restore payload.
use super::reader::checked;
use super::*;
use crate::domain::backup::*;
use serde::de::{DeserializeOwned, SeqAccess, Visitor};
use std::marker::PhantomData;

struct Count<T>(usize, PhantomData<T>);
impl<T> Default for Count<T> {
    fn default() -> Self {
        Self(0, PhantomData)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Count<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Counter<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Counter<T> {
            type Value = Count<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of backup records")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut count = 0usize;
                while let Some(record) = seq.next_element::<T>()? {
                    drop(record);
                    count = count
                        .checked_add(1)
                        .ok_or_else(|| serde::de::Error::custom("record count overflow"))?;
                }
                Ok(Count(count, PhantomData))
            }
        }
        deserializer.deserialize_seq(Counter(PhantomData))
    }
}

#[derive(Default, Deserialize)]
struct ImportCounts {
    batches: Count<BackupImportBatch>,
    exact_sessions: Count<BackupImportExactSession>,
    time_buckets: Count<BackupImportTimeBucket>,
}

fn optional<R: Read + Seek, T: DeserializeOwned + Default>(
    archive: &mut ZipArchive<R>,
    checksums: &BackupArchiveChecksums,
    declared: &str,
    name: &str,
    path: &Path,
) -> Result<T, String> {
    if !declared.trim().is_empty() || checksums.files.contains_key(name) {
        checked(archive, checksums, name, path)
    } else {
        Ok(T::default())
    }
}

pub(super) fn decode<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &Path,
) -> Result<BackupPreview, String> {
    let checksums: BackupArchiveChecksums = parse_json(
        &read_zip_entry(archive, BACKUP_CHECKSUMS_ENTRY_NAME, path)?,
        path,
        "checksums",
    )?;
    let manifest: BackupArchiveManifest =
        checked(archive, &checksums, BACKUP_MANIFEST_ENTRY_NAME, path)?;
    if manifest.format != BACKUP_FORMAT {
        return Err(format!("unsupported backup format {}", manifest.format));
    }
    let safety = restore_safety(manifest.backup_version, manifest.schema_version);
    let session_count: Count<BackupSession> =
        checked(archive, &checksums, BACKUP_SESSIONS_ENTRY_NAME, path)?;
    let setting_count: Count<BackupSetting> =
        checked(archive, &checksums, BACKUP_SETTINGS_ENTRY_NAME, path)?;
    let icon_cache_count: Count<BackupIconCache> =
        checked(archive, &checksums, BACKUP_ICON_CACHE_ENTRY_NAME, path)?;
    let title_sample_count: Count<BackupTitleSample> = optional(
        archive,
        &checksums,
        &manifest.files.title_samples,
        BACKUP_TITLE_SAMPLES_ENTRY_NAME,
        path,
    )?;
    let web_activity_segment_count: Count<BackupWebActivitySegment> = optional(
        archive,
        &checksums,
        &manifest.files.web_activity_segments,
        BACKUP_WEB_ACTIVITY_SEGMENTS_ENTRY_NAME,
        path,
    )?;
    let tool_reminder_count: Count<BackupToolReminder> = optional(
        archive,
        &checksums,
        &manifest.files.tool_reminders,
        BACKUP_TOOL_REMINDERS_ENTRY_NAME,
        path,
    )?;
    let tool_timer_count: Count<BackupToolTimer> = optional(
        archive,
        &checksums,
        &manifest.files.tool_timers,
        BACKUP_TOOL_TIMERS_ENTRY_NAME,
        path,
    )?;
    let tool_timer_lap_count: Count<BackupToolTimerLap> = optional(
        archive,
        &checksums,
        &manifest.files.tool_timer_laps,
        BACKUP_TOOL_TIMER_LAPS_ENTRY_NAME,
        path,
    )?;
    let tool_pomodoro_run_count: Count<BackupToolPomodoroRun> = optional(
        archive,
        &checksums,
        &manifest.files.tool_pomodoro_runs,
        BACKUP_TOOL_POMODORO_RUNS_ENTRY_NAME,
        path,
    )?;
    let tool_daily_stats_count: Count<BackupToolDailyStats> = optional(
        archive,
        &checksums,
        &manifest.files.tool_daily_stats,
        BACKUP_TOOL_DAILY_STATS_ENTRY_NAME,
        path,
    )?;
    let imports: ImportCounts = optional(
        archive,
        &checksums,
        &manifest.files.import_activity,
        BACKUP_IMPORT_ACTIVITY_ENTRY_NAME,
        path,
    )?;
    Ok(BackupPreview {
        version: manifest.backup_version,
        exported_at_ms: manifest.exported_at_ms,
        schema_version: manifest.schema_version,
        app_version: manifest.app_version,
        restore_supported: safety.supported,
        restore_message_key: safety.message_key.into(),
        restore_message_args: safety.message_args,
        restore_message: safety.message,
        session_count: session_count.0,
        setting_count: setting_count.0,
        icon_cache_count: icon_cache_count.0,
        title_sample_count: title_sample_count.0,
        web_activity_segment_count: web_activity_segment_count.0,
        tool_reminder_count: tool_reminder_count.0,
        tool_timer_count: tool_timer_count.0,
        tool_timer_lap_count: tool_timer_lap_count.0,
        tool_pomodoro_run_count: tool_pomodoro_run_count.0,
        tool_daily_stats_count: tool_daily_stats_count.0,
        import_batch_count: imports.batches.0,
        import_exact_session_count: imports.exact_sessions.0,
        import_time_bucket_count: imports.time_buckets.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_keeps_only_one_typed_record_alive_and_rejects_bad_rows() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LIVE: AtomicUsize = AtomicUsize::new(0);
        struct Record;
        impl<'de> Deserialize<'de> for Record {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                u64::deserialize(d)?;
                assert_eq!(LIVE.fetch_add(1, Ordering::SeqCst), 0);
                Ok(Self)
            }
        }
        impl Drop for Record {
            fn drop(&mut self) {
                LIVE.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let json = format!("[{}]", vec!["1"; 10000].join(","));
        assert_eq!(
            serde_json::from_str::<Count<Record>>(&json).unwrap().0,
            10000
        );
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
        assert!(serde_json::from_str::<Count<Record>>("[1,\"wrong\"]").is_err());
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn preview_matches_payload_and_uses_records_not_manifest_counts() {
        let mut payload = super::super::tests::payload_with_bound_web_activity();
        for version in [0, CURRENT_BACKUP_VERSION, CURRENT_BACKUP_VERSION + 1] {
            payload.version = version;
            let bytes = encode_backup_archive(&payload).unwrap();
            let mut source = ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut entries = BTreeMap::new();
            for i in 0..source.len() {
                let mut entry = source.by_index(i).unwrap();
                let mut json = String::new();
                entry.read_to_string(&mut json).unwrap();
                entries.insert(entry.name().to_string(), json);
            }
            let mut manifest: BackupArchiveManifest =
                serde_json::from_str(&entries[BACKUP_MANIFEST_ENTRY_NAME]).unwrap();
            manifest.counts.sessions = 999999;
            let json = serde_json::to_string(&manifest).unwrap();
            let mut checksums: BackupArchiveChecksums =
                serde_json::from_str(&entries[BACKUP_CHECKSUMS_ENTRY_NAME]).unwrap();
            checksums
                .files
                .insert(BACKUP_MANIFEST_ENTRY_NAME.into(), checksum(&json));
            entries.insert(BACKUP_MANIFEST_ENTRY_NAME.into(), json);
            entries.insert(
                BACKUP_CHECKSUMS_ENTRY_NAME.into(),
                serde_json::to_string(&checksums).unwrap(),
            );
            let mut target = ZipWriter::new(Cursor::new(Vec::new()));
            for (name, json) in entries {
                target
                    .start_file(name, SimpleFileOptions::default())
                    .unwrap();
                target.write_all(json.as_bytes()).unwrap();
            }
            let mut archive = ZipArchive::new(target.finish().unwrap()).unwrap();
            let actual = decode(&mut archive, Path::new("synthetic.zip")).unwrap();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(payload.preview()).unwrap()
            );
        }
    }
}
