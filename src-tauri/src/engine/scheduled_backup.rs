use crate::data::backup::{self, CreateNewBackupError};
use crate::data::repositories::scheduled_backup as repository;
use crate::domain::backup_schedule::{
    latest_due_slot, next_slot_after, LogicalBackupSlot, ScheduledBackupCadence,
    ScheduledBackupConfig, ScheduledBackupConfigInput, ScheduledBackupRun, ScheduledBackupSnapshot,
    DEFAULT_LOCAL_TIME_MINUTES, SCHEDULED_BACKUP_KEEP_COUNT,
};
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use sqlx::{Pool, Sqlite};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_NAME_CANDIDATES: u8 = 99;

pub async fn get_snapshot(
    pool: &Pool<Sqlite>,
    default_backup_dir: &Path,
) -> Result<ScheduledBackupSnapshot, String> {
    let config = load_or_create_config(pool, default_backup_dir, now_ms()).await?;
    snapshot_from_config(pool, config).await
}

pub async fn save_config(
    pool: &Pool<Sqlite>,
    default_backup_dir: &Path,
    input: ScheduledBackupConfigInput,
) -> Result<ScheduledBackupSnapshot, String> {
    input.validate()?;
    let now = now_ms();
    let current = load_or_create_config(pool, default_backup_dir, now).await?;
    let normalized_dir = normalize_target_directory(&input.target_dir)?;
    let normalized_dir = normalized_dir.to_string_lossy().to_string();
    let target_changed = current.target_dir != normalized_dir;
    let schedule_changed = current.schedule_changed_from(&input);
    let config = ScheduledBackupConfig {
        enabled: input.enabled,
        cadence: input.cadence,
        weekday: input.weekday,
        local_time_minutes: input.local_time_minutes,
        target_dir: normalized_dir,
        target_generation: if target_changed || schedule_changed {
            new_generation()?
        } else {
            current.target_generation
        },
        schedule_anchor_at_ms: if target_changed || schedule_changed {
            now
        } else {
            current.schedule_anchor_at_ms
        },
        updated_at_ms: now,
    };
    repository::save_config(pool, &config).await?;
    snapshot_from_config(pool, config).await
}

pub async fn tick(pool: &Pool<Sqlite>, default_backup_dir: &Path) -> Result<bool, String> {
    let now = now_ms();
    let config = load_or_create_config(pool, default_backup_dir, now).await?;

    if let Some(active) = repository::load_active(pool).await? {
        reconcile_running_run(pool, &config, &active, now).await?;
        return Ok(true);
    }

    if config.enabled {
        if let Some(retry) =
            repository::load_due_retry(pool, &config.target_generation, now).await?
        {
            if repository::start_retry(pool, &retry.run_key, now).await? {
                let run = repository::load_run(pool, &retry.run_key)
                    .await?
                    .ok_or_else(|| "scheduled backup retry disappeared".to_string())?;
                execute_claimed(pool, &config, run, now).await?;
                return Ok(true);
            }
        }
    }

    let now_local = Local::now().naive_local();
    let Some(anchor_local) = local_datetime_from_ms(config.schedule_anchor_at_ms) else {
        return Err("scheduled backup anchor is outside the local time range".to_string());
    };
    let Some(slot) = latest_due_slot(now_local, anchor_local, &config) else {
        return Ok(false);
    };
    let run = new_run(&config, slot, now);
    if !repository::claim_run(pool, &run).await? {
        return Ok(false);
    }
    execute_claimed(pool, &config, run, now).await?;
    Ok(true)
}

pub async fn reset_after_replace_restore(pool: &Pool<Sqlite>) -> Result<(), String> {
    repository::disable_and_reset(pool, &new_generation()?, now_ms()).await
}

async fn load_or_create_config(
    pool: &Pool<Sqlite>,
    default_backup_dir: &Path,
    now_ms: i64,
) -> Result<ScheduledBackupConfig, String> {
    if let Some(config) = repository::load_config(pool).await? {
        return Ok(config);
    }
    let target_dir = default_backup_dir.to_path_buf();
    fs::create_dir_all(&target_dir).map_err(|error| {
        format!(
            "failed to create default scheduled backup directory `{}`: {error}",
            target_dir.display()
        )
    })?;
    let config = ScheduledBackupConfig {
        enabled: false,
        cadence: ScheduledBackupCadence::Weekly,
        weekday: Some(5),
        local_time_minutes: DEFAULT_LOCAL_TIME_MINUTES,
        target_dir: target_dir
            .canonicalize()
            .map_err(|error| format!("failed to resolve default backup directory: {error}"))?
            .to_string_lossy()
            .to_string(),
        target_generation: new_generation()?,
        schedule_anchor_at_ms: now_ms,
        updated_at_ms: now_ms,
    };
    repository::save_config(pool, &config).await?;
    Ok(config)
}

async fn snapshot_from_config(
    pool: &Pool<Sqlite>,
    config: ScheduledBackupConfig,
) -> Result<ScheduledBackupSnapshot, String> {
    let next_execution_at_ms = next_slot_after(Local::now().naive_local(), &config)
        .and_then(|slot| local_datetime_ms(slot.local_datetime()));
    let recent_success =
        repository::load_recent_by_status(pool, &config.target_generation, "succeeded").await?;
    let recent_failure =
        repository::load_recent_by_status(pool, &config.target_generation, "failed").await?;
    let active_run = repository::load_active(pool).await?;
    Ok(ScheduledBackupSnapshot {
        config,
        next_execution_at_ms,
        recent_success,
        recent_failure,
        active_run,
    })
}

fn new_run(
    config: &ScheduledBackupConfig,
    slot: LogicalBackupSlot,
    now_ms: i64,
) -> ScheduledBackupRun {
    ScheduledBackupRun {
        run_key: slot.run_key(&config.target_generation),
        target_generation: config.target_generation.clone(),
        logical_date: slot.date_key(),
        logical_time_minutes: slot.local_time_minutes,
        target_path: config.target_dir.clone(),
        status: "running".to_string(),
        file_state: "absent".to_string(),
        attempt_count: 1,
        retry_at_ms: None,
        started_at_ms: now_ms,
        completed_at_ms: None,
        archive_sha256: None,
        size_bytes: None,
        error_code: None,
        error_message: None,
        cleanup_warning: None,
        updated_at_ms: now_ms,
    }
}

async fn execute_claimed(
    pool: &Pool<Sqlite>,
    config: &ScheduledBackupConfig,
    run: ScheduledBackupRun,
    now_ms: i64,
) -> Result<(), String> {
    let slot = slot_from_run(&run)?;
    let target_dir = normalize_target_directory(&config.target_dir)?;
    let mut last_error = None;
    for candidate in candidate_paths(&target_dir, slot) {
        match backup::export_scheduled_backup_create_new(pool, &candidate).await {
            Ok(()) => {
                repository::update_target_path(
                    pool,
                    &run.run_key,
                    &candidate.to_string_lossy(),
                    now_ms,
                )
                .await?;
                match backup::validate_scheduled_snapshot(&candidate) {
                    Ok((hash, size)) => {
                        repository::mark_succeeded(pool, &run.run_key, &hash, size, now_ms).await?;
                        apply_retention(pool, config, now_ms).await;
                        return Ok(());
                    }
                    Err(error) => {
                        last_error = Some(("validation_failed", safe_error_message(&error)));
                        break;
                    }
                }
            }
            Err(CreateNewBackupError::AlreadyExists) => continue,
            Err(CreateNewBackupError::Failed(error)) => {
                last_error = Some((classify_error(&error), safe_error_message(&error)));
                break;
            }
        }
    }

    let (code, message) = last_error.unwrap_or((
        "name_collision",
        "No unused scheduled backup file name was available".to_string(),
    ));
    repository::mark_failed_or_retry(
        pool,
        &run.run_key,
        run.attempt_count,
        code,
        &message,
        now_ms,
    )
    .await
}

async fn reconcile_running_run(
    pool: &Pool<Sqlite>,
    config: &ScheduledBackupConfig,
    run: &ScheduledBackupRun,
    now_ms: i64,
) -> Result<(), String> {
    let path = PathBuf::from(&run.target_path);
    if path.is_file() {
        match backup::validate_scheduled_snapshot(&path) {
            Ok((hash, size)) => {
                repository::mark_succeeded(pool, &run.run_key, &hash, size, now_ms).await?;
                apply_retention(pool, config, now_ms).await;
            }
            Err(error) => {
                repository::mark_failed_or_retry(
                    pool,
                    &run.run_key,
                    run.attempt_count,
                    "validation_conflict",
                    &safe_error_message(&error),
                    now_ms,
                )
                .await?;
            }
        }
    } else {
        repository::mark_failed_or_retry(
            pool,
            &run.run_key,
            run.attempt_count,
            "interrupted",
            "Scheduled backup was interrupted before publication",
            now_ms,
        )
        .await?;
    }
    Ok(())
}

async fn apply_retention(pool: &Pool<Sqlite>, config: &ScheduledBackupConfig, now_ms: i64) {
    let Ok(target_dir) = normalize_target_directory(&config.target_dir) else {
        return;
    };
    let Ok(candidates) =
        repository::list_retention_candidates(pool, &config.target_generation).await
    else {
        return;
    };
    for candidate in candidates
        .into_iter()
        .skip(usize::from(SCHEDULED_BACKUP_KEEP_COUNT))
    {
        let path = PathBuf::from(&candidate.target_path);
        match prune_owned_candidate(&target_dir, &candidate) {
            Ok(PruneOutcome::Pruned) => {
                let _ =
                    repository::mark_file_state(pool, &candidate.run_key, "pruned", None, now_ms)
                        .await;
            }
            Ok(PruneOutcome::Missing) => {
                let _ = repository::mark_file_state(
                    pool,
                    &candidate.run_key,
                    "missing",
                    Some("Owned backup file was already missing; no other file was touched"),
                    now_ms,
                )
                .await;
            }
            Err(error) => {
                eprintln!(
                    "[scheduled-backup] retained unverified candidate `{}`: {}",
                    path.display(),
                    safe_error_message(&error)
                );
            }
        }
    }
}

enum PruneOutcome {
    Pruned,
    Missing,
}

fn prune_owned_candidate(
    target_dir: &Path,
    candidate: &ScheduledBackupRun,
) -> Result<PruneOutcome, String> {
    let path = PathBuf::from(&candidate.target_path);
    if !path.exists() {
        return Ok(PruneOutcome::Missing);
    }
    let parent = path
        .parent()
        .ok_or_else(|| "scheduled backup candidate has no parent directory".to_string())?
        .canonicalize()
        .map_err(|error| format!("failed to resolve backup candidate directory: {error}"))?;
    if parent != target_dir {
        return Err("scheduled backup candidate is outside its owned directory".to_string());
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("failed to inspect backup candidate: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("scheduled backup candidate is not a regular owned file".to_string());
    }
    let (hash, size) = backup::validate_scheduled_snapshot(&path)?;
    if candidate.archive_sha256.as_deref() != Some(hash.as_str())
        || candidate.size_bytes != Some(size)
        || metadata.len() != size
    {
        return Err(
            "scheduled backup candidate no longer matches its ownership record".to_string(),
        );
    }
    let final_metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("failed to recheck backup candidate: {error}"))?;
    if final_metadata.file_type().is_symlink()
        || !final_metadata.is_file()
        || final_metadata.len() != metadata.len()
        || final_metadata.modified().ok() != metadata.modified().ok()
    {
        return Err("scheduled backup candidate changed during cleanup".to_string());
    }
    fs::remove_file(&path).map_err(|error| format!("failed to prune owned backup: {error}"))?;
    Ok(PruneOutcome::Pruned)
}

pub fn normalize_target_directory(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err("scheduled backup directory must be an absolute path".to_string());
    }
    fs::create_dir_all(&path).map_err(|error| {
        format!(
            "failed to create scheduled backup directory `{}`: {error}",
            path.display()
        )
    })?;
    path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve scheduled backup directory `{}`: {error}",
            path.display()
        )
    })
}

fn candidate_paths(
    target_dir: &Path,
    slot: LogicalBackupSlot,
) -> impl Iterator<Item = PathBuf> + '_ {
    let stamp = format!(
        "{}-{:02}{:02}00",
        slot.date.format("%Y%m%d"),
        slot.local_time_minutes / 60,
        slot.local_time_minutes % 60
    );
    (1..=MAX_NAME_CANDIDATES).map(move |index| {
        let suffix = if index == 1 {
            String::new()
        } else {
            format!("-{index:02}")
        };
        target_dir.join(format!("Patina-scheduled-backup-{stamp}{suffix}.zip"))
    })
}

fn slot_from_run(run: &ScheduledBackupRun) -> Result<LogicalBackupSlot, String> {
    let date = NaiveDate::parse_from_str(&run.logical_date, "%Y-%m-%d")
        .map_err(|error| format!("invalid scheduled backup logical date: {error}"))?;
    Ok(LogicalBackupSlot {
        date,
        local_time_minutes: run.logical_time_minutes,
    })
}

fn local_datetime_from_ms(value: i64) -> Option<NaiveDateTime> {
    Local
        .timestamp_millis_opt(value)
        .single()
        .map(|value| value.naive_local())
}

fn local_datetime_ms(value: NaiveDateTime) -> Option<i64> {
    Local
        .from_local_datetime(&value)
        .earliest()
        .map(|value| value.timestamp_millis())
}

fn classify_error(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("permission denied") {
        "permission_denied"
    } else if normalized.contains("no space") {
        "disk_full"
    } else if normalized.contains("validation") || normalized.contains("backup archive") {
        "validation_failed"
    } else {
        "io_error"
    }
}

fn safe_error_message(error: &str) -> String {
    error.chars().take(500).collect()
}

fn new_generation() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate scheduled backup identity: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_names_are_stable_and_never_overwrite_by_design() {
        let dir = Path::new("/tmp/backups");
        let slot = LogicalBackupSlot {
            date: NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
            local_time_minutes: 21 * 60 + 5,
        };
        let paths = candidate_paths(dir, slot).take(2).collect::<Vec<_>>();
        assert_eq!(
            paths[0],
            dir.join("Patina-scheduled-backup-20260830-210500.zip")
        );
        assert_eq!(
            paths[1],
            dir.join("Patina-scheduled-backup-20260830-210500-02.zip")
        );
    }

    #[test]
    fn target_directory_must_be_absolute() {
        assert!(normalize_target_directory("relative/backups").is_err());
    }

    #[test]
    fn retention_never_deletes_an_unverified_file() {
        let root = std::env::temp_dir().join(format!(
            "patina-scheduled-backup-test-{}",
            new_generation().unwrap()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Patina-scheduled-backup-invalid.zip");
        fs::write(&path, b"not a Patina backup").unwrap();
        let candidate = ScheduledBackupRun {
            run_key: "run".to_string(),
            target_generation: "generation".to_string(),
            logical_date: "2026-08-30".to_string(),
            logical_time_minutes: 0,
            target_path: path.to_string_lossy().to_string(),
            status: "succeeded".to_string(),
            file_state: "present".to_string(),
            attempt_count: 1,
            retry_at_ms: None,
            started_at_ms: 1,
            completed_at_ms: Some(2),
            archive_sha256: Some("untrusted".to_string()),
            size_bytes: Some(19),
            error_code: None,
            error_message: None,
            cleanup_warning: None,
            updated_at_ms: 2,
        };

        assert!(prune_owned_candidate(&root.canonicalize().unwrap(), &candidate).is_err());
        assert!(path.is_file());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
