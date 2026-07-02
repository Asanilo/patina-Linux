use super::plan::{self, TargetKind};
use crate::data::sqlite_pool;
use crate::platform::storage_anchor::{PendingStorageMigration, STORAGE_MIGRATION_PENDING_FORMAT};
use crate::platform::storage_paths::StoragePaths;
use crate::platform::{storage_usage, webview_cache};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const STAGING_MARKER_FILE: &str = ".patina-storage-migration";
const STAGING_MARKER_FORMAT: &str = "patina.storage-staging.v1";
const DATA_ENTRIES: &[&str] = &["patina.db", "patina.db-wal", "patina.db-shm", "backups"];
const MINIMUM_FREE_SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

struct Promotion {
    migration_id: String,
    target_root: PathBuf,
    promoted_entries: Vec<String>,
    quarantine_root: Option<PathBuf>,
    target_created: bool,
}

pub(crate) async fn validate_migrated_database(
    source_path: &Path,
    staged_path: &Path,
) -> Result<(), String> {
    sqlite_pool::validate_migrated_database_copy(source_path, staged_path).await
}

pub(crate) async fn execute_pending_with_deps<SetDataAnchor, SetWebviewAnchor>(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    mut set_data_anchor: SetDataAnchor,
    mut set_webview_anchor: SetWebviewAnchor,
) -> Result<(), String>
where
    SetDataAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    SetWebviewAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
{
    validate_pending(pending, current)?;
    let data_changed = !same_path(&pending.source_data_root, &pending.target_data_root);
    let webview_changed = !same_path(&pending.source_webview_root, &pending.target_webview_root);

    let mut data_promotion = if data_changed {
        Some(prepare_and_promote_data(pending, current).await?)
    } else {
        None
    };
    let mut webview_promotion = if webview_changed {
        match prepare_and_promote_webview(pending, current) {
            Ok(promotion) => Some(promotion),
            Err(error) => {
                if let Some(promotion) = data_promotion.as_mut() {
                    rollback_promotion(promotion)?;
                }
                return Err(error);
            }
        }
    } else {
        None
    };

    let source_data_anchor =
        anchor_value(&pending.source_data_root, &current.stable_product_data_root);
    let source_webview_anchor = anchor_value(
        &pending.source_webview_root,
        &current.stable_product_data_root,
    );
    let target_data_anchor =
        anchor_value(&pending.target_data_root, &current.stable_product_data_root);
    let target_webview_anchor = anchor_value(
        &pending.target_webview_root,
        &current.stable_product_data_root,
    );

    if data_changed {
        if let Err(error) = set_data_anchor(target_data_anchor) {
            let anchor_rollback = set_data_anchor(source_data_anchor.clone());
            rollback_promotions(&mut data_promotion, &mut webview_promotion)?;
            anchor_rollback.map_err(|rollback_error| {
                format!(
                    "failed to activate data anchor: {error}; data anchor rollback also failed: {rollback_error}"
                )
            })?;
            return Err(format!("failed to activate migrated data anchor: {error}"));
        }
    }
    if webview_changed {
        if let Err(error) = set_webview_anchor(target_webview_anchor) {
            let webview_anchor_rollback = set_webview_anchor(source_webview_anchor);
            let anchor_rollback = if data_changed {
                set_data_anchor(source_data_anchor)
            } else {
                Ok(())
            };
            rollback_promotions(&mut data_promotion, &mut webview_promotion)?;
            webview_anchor_rollback.map_err(|rollback_error| {
                format!(
                    "failed to activate WebView anchor: {error}; WebView anchor rollback also failed: {rollback_error}"
                )
            })?;
            anchor_rollback.map_err(|rollback_error| {
                format!(
                    "failed to activate WebView anchor: {error}; data anchor rollback also failed: {rollback_error}"
                )
            })?;
            return Err(format!(
                "failed to activate migrated WebView anchor: {error}"
            ));
        }
    }

    Ok(())
}

fn validate_pending(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
) -> Result<(), String> {
    if pending.format != STORAGE_MIGRATION_PENDING_FORMAT {
        return Err(format!(
            "unsupported storage migration format `{}`",
            pending.format
        ));
    }
    if pending.state != "pending-restart" {
        return Err(format!(
            "unsupported pending storage migration state `{}`",
            pending.state
        ));
    }
    if !same_path(&pending.source_data_root, &current.data_root)
        || !same_path(&pending.source_webview_root, &current.webview_root)
    {
        return Err(
            "pending storage migration source no longer matches active storage".to_string(),
        );
    }
    if !same_path(&pending.source_data_root, &pending.target_data_root) {
        plan::validate_target_relationships(current, &pending.target_data_root, TargetKind::Data)?;
    }
    if !same_path(&pending.source_webview_root, &pending.target_webview_root) {
        plan::validate_target_relationships(
            current,
            &pending.target_webview_root,
            TargetKind::Webview,
        )?;
    }
    if same_path(&pending.target_data_root, &pending.target_webview_root)
        && !same_path(&pending.target_data_root, &current.stable_product_data_root)
    {
        return Err("custom data and WebView targets must not be the same directory".to_string());
    }
    Ok(())
}

async fn prepare_and_promote_data(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
) -> Result<Promotion, String> {
    let source_db = pending.source_data_root.join("patina.db");
    require_regular_file(&source_db, "source Patina database")?;
    ensure_target_parent_is_safe(&pending.target_data_root)?;
    ensure_capacity(
        &pending.source_data_root,
        &pending.target_data_root,
        DATA_ENTRIES,
    )?;

    let staging = staging_path(&pending.target_data_root, &pending.id, "data")?;
    create_owned_staging(&staging, &pending.id)?;
    let operation = async {
        for name in DATA_ENTRIES {
            copy_optional_entry(&pending.source_data_root.join(name), &staging.join(name))?;
        }
        validate_migrated_database(&source_db, &staging.join("patina.db")).await?;
        promote_entries(
            &staging,
            &pending.target_data_root,
            DATA_ENTRIES,
            &pending.id,
            same_path(&pending.target_data_root, &current.stable_product_data_root),
            "data",
        )
    }
    .await;

    remove_owned_staging(&staging, &pending.id)?;
    operation
}

fn prepare_and_promote_webview(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
) -> Result<Promotion, String> {
    ensure_target_parent_is_safe(&pending.target_webview_root)?;
    let payload = webview_cache::persistent_profile_size(&pending.source_webview_root)?;
    ensure_payload_capacity(payload, &pending.target_webview_root)?;
    let staging = staging_path(&pending.target_webview_root, &pending.id, "webview")?;
    create_owned_staging(&staging, &pending.id)?;
    let operation = (|| {
        let report =
            webview_cache::copy_persistent_profile(&pending.source_webview_root, &staging)?;
        let entries = report.copied.iter().map(String::as_str).collect::<Vec<_>>();
        promote_entries(
            &staging,
            &pending.target_webview_root,
            &entries,
            &pending.id,
            same_path(
                &pending.target_webview_root,
                &current.stable_product_data_root,
            ),
            "webview",
        )
    })();
    remove_owned_staging(&staging, &pending.id)?;
    operation
}

fn promote_entries(
    staging: &Path,
    target: &Path,
    entries: &[&str],
    migration_id: &str,
    quarantine_conflicts: bool,
    label: &str,
) -> Result<Promotion, String> {
    let target_created = !target.exists();
    create_private_dir(target)?;
    let conflicts = entries
        .iter()
        .filter(|name| target.join(name).exists())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let quarantine_root = if conflicts.is_empty() {
        None
    } else if quarantine_conflicts {
        let quarantine = quarantine_path(target, migration_id, label)?;
        create_owned_staging(&quarantine, migration_id)?;
        Some(quarantine)
    } else {
        return Err(format!(
            "storage target `{}` contains existing managed entries: {}",
            target.display(),
            conflicts.join(", ")
        ));
    };

    let mut promotion = Promotion {
        migration_id: migration_id.to_string(),
        target_root: target.to_path_buf(),
        promoted_entries: Vec::new(),
        quarantine_root,
        target_created,
    };
    if let Some(quarantine) = promotion.quarantine_root.as_ref() {
        for name in &conflicts {
            if let Err(error) = fs::rename(target.join(name), quarantine.join(name)) {
                rollback_promotion(&mut promotion)?;
                return Err(format!(
                    "failed to quarantine existing storage entry `{}`: {error}",
                    target.join(name).display()
                ));
            }
        }
    }
    for name in entries {
        let source = staging.join(name);
        if !source.exists() {
            continue;
        }
        if let Err(error) = fs::rename(&source, target.join(name)) {
            rollback_promotion(&mut promotion)?;
            return Err(format!(
                "failed to promote migrated storage entry `{}`: {error}",
                source.display()
            ));
        }
        promotion.promoted_entries.push((*name).to_string());
    }
    sync_directory(target)?;
    Ok(promotion)
}

fn rollback_promotions(
    data: &mut Option<Promotion>,
    webview: &mut Option<Promotion>,
) -> Result<(), String> {
    if let Some(promotion) = webview.as_mut() {
        rollback_promotion(promotion)?;
    }
    if let Some(promotion) = data.as_mut() {
        rollback_promotion(promotion)?;
    }
    Ok(())
}

fn rollback_promotion(promotion: &mut Promotion) -> Result<(), String> {
    for name in promotion.promoted_entries.iter().rev() {
        remove_path_without_following_links(&promotion.target_root.join(name))?;
    }
    promotion.promoted_entries.clear();

    if let Some(quarantine) = promotion.quarantine_root.take() {
        verify_staging_marker(&quarantine, &promotion.migration_id)?;
        for entry in fs::read_dir(&quarantine).map_err(|error| {
            format!(
                "failed to read storage quarantine `{}`: {error}",
                quarantine.display()
            )
        })? {
            let entry =
                entry.map_err(|error| format!("failed to read quarantine entry: {error}"))?;
            if entry.file_name() == STAGING_MARKER_FILE {
                continue;
            }
            fs::rename(entry.path(), promotion.target_root.join(entry.file_name()))
                .map_err(|error| format!("failed to restore quarantined storage entry: {error}"))?;
        }
        fs::remove_file(quarantine.join(STAGING_MARKER_FILE))
            .map_err(|error| format!("failed to remove quarantine marker: {error}"))?;
        fs::remove_dir(&quarantine)
            .map_err(|error| format!("failed to remove empty quarantine: {error}"))?;
    }
    if promotion.target_created {
        match fs::remove_dir(&promotion.target_root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => {
                return Err(format!(
                    "failed to remove empty migration target `{}`: {error}",
                    promotion.target_root.display()
                ))
            }
        }
    }
    Ok(())
}

fn ensure_capacity(source: &Path, target: &Path, entries: &[&str]) -> Result<(), String> {
    let mut payload = 0_u64;
    for name in entries {
        payload = payload.saturating_add(storage_usage::path_size(&source.join(name))?);
    }
    ensure_payload_capacity(payload, target)
}

fn ensure_payload_capacity(payload: u64, target: &Path) -> Result<(), String> {
    let required = payload.saturating_add((payload / 10).max(MINIMUM_FREE_SPACE_MARGIN_BYTES));
    let available = storage_usage::available_space_for(target)?;
    if available < required {
        return Err(format!(
            "storage target has {available} bytes available but {required} bytes are required"
        ));
    }
    Ok(())
}

fn copy_optional_entry(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect `{}`: {error}", source.display())),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to copy symbolic link `{}`",
            source.display()
        ));
    }
    if metadata.is_file() {
        fs::copy(source, target).map_err(|error| {
            format!(
                "failed to copy storage file `{}` to `{}`: {error}",
                source.display(),
                target.display()
            )
        })?;
        restrict_file(target)?;
        fs::File::open(target)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                format!("failed to sync copied file `{}`: {error}", target.display())
            })?;
        return Ok(());
    }
    if metadata.is_dir() {
        create_private_dir(target)?;
        for entry in fs::read_dir(source)
            .map_err(|error| format!("failed to read `{}`: {error}", source.display()))?
        {
            let entry = entry.map_err(|error| format!("failed to read storage entry: {error}"))?;
            copy_optional_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn create_owned_staging(path: &Path, migration_id: &str) -> Result<(), String> {
    if path.exists() {
        remove_owned_staging(path, migration_id)?;
    }
    create_private_dir(path)?;
    let marker = path.join(STAGING_MARKER_FILE);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|error| {
            format!(
                "failed to create staging marker `{}`: {error}",
                marker.display()
            )
        })?;
    file.write_all(marker_contents(migration_id).as_bytes())
        .map_err(|error| format!("failed to write staging marker: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync staging marker: {error}"))?;
    restrict_file(&marker)
}

fn remove_owned_staging(path: &Path, migration_id: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    verify_staging_marker(path, migration_id)?;
    fs::remove_dir_all(path).map_err(|error| {
        format!(
            "failed to remove owned migration staging `{}`: {error}",
            path.display()
        )
    })
}

fn verify_staging_marker(path: &Path, migration_id: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect staging `{}`: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("migration staging `{}` is unsafe", path.display()));
    }
    let actual = fs::read_to_string(path.join(STAGING_MARKER_FILE)).map_err(|error| {
        format!(
            "failed to read migration staging marker `{}`: {error}",
            path.display()
        )
    })?;
    if actual != marker_contents(migration_id) {
        return Err(format!(
            "migration staging marker does not match `{}`",
            path.display()
        ));
    }
    Ok(())
}

fn marker_contents(migration_id: &str) -> String {
    format!("{STAGING_MARKER_FORMAT}\n{migration_id}\n")
}

fn staging_path(target: &Path, migration_id: &str, label: &str) -> Result<PathBuf, String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("storage target `{}` has no parent", target.display()))?;
    Ok(parent.join(format!(".patina-storage-{migration_id}-{label}-staging")))
}

fn quarantine_path(target: &Path, migration_id: &str, label: &str) -> Result<PathBuf, String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("storage target `{}` has no parent", target.display()))?;
    Ok(parent.join(format!(".patina-storage-{migration_id}-{label}-quarantine")))
}

fn ensure_target_parent_is_safe(target: &Path) -> Result<(), String> {
    if !target.is_absolute() {
        return Err(format!(
            "storage target `{}` must be absolute",
            target.display()
        ));
    }
    let mut current = PathBuf::new();
    for component in target.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "storage path component `{}` must not be a symbolic link",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "failed to inspect storage path component `{}`: {error}",
                    current.display()
                ));
            }
        }
    }
    if let Some(parent) = target.parent() {
        create_private_dir(parent)?;
    }
    Ok(())
}

fn require_regular_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label} `{}` is unavailable: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} `{}` is not a regular file",
            path.display()
        ));
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    let mut candidate = path;
    loop {
        match fs::symlink_metadata(candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "directory path `{}` is not a real directory",
                        candidate.display()
                    ));
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(candidate.to_path_buf());
                candidate = candidate.parent().ok_or_else(|| {
                    format!("directory `{}` has no existing ancestor", path.display())
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "failed to inspect directory `{}`: {error}",
                    candidate.display()
                ));
            }
        }
    }

    for directory in missing.iter().rev() {
        fs::create_dir(directory).map_err(|error| {
            format!(
                "failed to create directory `{}`: {error}",
                directory.display()
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|error| {
                format!(
                    "failed to restrict directory permissions `{}`: {error}",
                    directory.display()
                )
            })?;
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync directory `{}`: {error}", path.display()))
}

fn restrict_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
            format!(
                "failed to restrict file permissions `{}`: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn remove_path_without_following_links(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect `{}`: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to remove file `{}`: {error}", path.display()))
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?
        {
            let entry = entry.map_err(|error| format!("failed to read entry: {error}"))?;
            remove_path_without_following_links(&entry.path())?;
        }
        fs::remove_dir(path)
            .map_err(|error| format!("failed to remove directory `{}`: {error}", path.display()))
    } else {
        Ok(())
    }
}

fn anchor_value(path: &Path, default_root: &Path) -> Option<PathBuf> {
    (!same_path(path, default_root)).then(|| path.to_path_buf())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.components().eq(right.components())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema;
    use crate::platform::storage_anchor::{
        PendingStorageMigration, STORAGE_MIGRATION_PENDING_FORMAT,
    };
    use crate::platform::storage_paths::StoragePaths;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Executor;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "patina-storage-executor-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    async fn create_current_database(path: &Path, session_count: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let options = SqliteConnectOptions::from_str(path.to_str().unwrap())
            .unwrap()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::TOOLS_TABLES_SCHEMA_SQL).await.unwrap();
        pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::WEB_ACTIVITY_SCHEMA_SQL).await.unwrap();
        for index in 0..session_count {
            sqlx::query(
                "INSERT INTO sessions
                 (app_name, exe_name, window_title, start_time, end_time, duration, continuity_group_start_time)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind("Editor")
            .bind("editor")
            .bind(format!("Document {index}"))
            .bind(index as i64)
            .bind(index as i64 + 1)
            .bind(1_i64)
            .bind(index as i64)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;
    }

    fn paths(root: &Path, source: PathBuf) -> StoragePaths {
        StoragePaths::from_roots(
            root.join("config/Patina"),
            root.join("default/Patina"),
            source.clone(),
            source,
            true,
            true,
        )
    }

    fn pending(source: &Path, target: &Path) -> PendingStorageMigration {
        PendingStorageMigration {
            format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
            id: "migration-test".to_string(),
            profile: "production".to_string(),
            source_data_root: source.to_path_buf(),
            target_data_root: target.to_path_buf(),
            source_webview_root: source.to_path_buf(),
            target_webview_root: source.to_path_buf(),
            created_at_ms: 1,
            state: "pending-restart".to_string(),
        }
    }

    #[test]
    fn successful_custom_migration_keeps_source_and_activates_target() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("success");
            let source = root.join("source/Patina");
            let target = root.join("target/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            fs::create_dir_all(source.join("backups")).unwrap();
            fs::write(source.join("backups/before.zip"), b"backup").unwrap();
            let activated = Arc::new(Mutex::new(Vec::new()));
            let activated_data = activated.clone();

            execute_pending_with_deps(
                &pending(&source, &target),
                &paths(&root, source.clone()),
                move |next| {
                    activated_data.lock().unwrap().push(next);
                    Ok(())
                },
                |_| Ok(()),
            )
            .await
            .unwrap();

            assert!(source.join("patina.db").exists());
            assert!(target.join("patina.db").exists());
            assert!(target.join("backups/before.zip").exists());
            assert_eq!(activated.lock().unwrap().as_slice(), [Some(target.clone())]);
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn corrupt_staged_database_is_rejected() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("corrupt");
            let source = root.join("source.db");
            let staged = root.join("staged.db");
            create_current_database(&source, 1).await;
            fs::write(&staged, b"not sqlite").unwrap();

            let error = validate_migrated_database(&source, &staged)
                .await
                .unwrap_err();

            assert!(error.contains("staged") || error.contains("database"));
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn critical_row_count_mismatch_is_rejected() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("counts");
            let source = root.join("source.db");
            let staged = root.join("staged.db");
            create_current_database(&source, 2).await;
            create_current_database(&staged, 1).await;

            let error = validate_migrated_database(&source, &staged)
                .await
                .unwrap_err();

            assert!(error.contains("sessions row count"));
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn missing_source_is_rejected_without_creating_target() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("missing-source");
            let source = root.join("missing/Patina");
            let target = root.join("target/Patina");

            let error = execute_pending_with_deps(
                &pending(&source, &target),
                &paths(&root, source.clone()),
                |_| Ok(()),
                |_| Ok(()),
            )
            .await
            .unwrap_err();

            assert!(error.contains("source Patina database"));
            assert!(!target.exists());
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn anchor_failure_restores_existing_default_database() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("anchor-rollback");
            let source = root.join("custom/Patina");
            let default_target = root.join("default/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            create_current_database(&default_target.join("patina.db"), 1).await;
            let old_default = fs::read(default_target.join("patina.db")).unwrap();

            let error = execute_pending_with_deps(
                &pending(&source, &default_target),
                &paths(&root, source.clone()),
                |_| Err("simulated anchor failure".to_string()),
                |_| Ok(()),
            )
            .await
            .unwrap_err();

            assert!(error.contains("anchor failure"));
            assert!(source.join("patina.db").exists());
            assert_eq!(
                fs::read(default_target.join("patina.db")).unwrap(),
                old_default
            );
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn webview_anchor_failure_restores_both_previous_anchors() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("webview-anchor-rollback");
            let source = root.join("source/Patina");
            let data_target = root.join("data-target/Patina");
            let webview_target = root.join("webview-target/Patina/webview");
            create_current_database(&source.join("patina.db"), 1).await;
            fs::create_dir_all(source.join("localstorage")).unwrap();
            fs::write(source.join("localstorage/settings"), b"state").unwrap();
            let mut migration = pending(&source, &data_target);
            migration.target_webview_root = webview_target.clone();

            let data_anchor = Arc::new(Mutex::new(Some(source.clone())));
            let webview_anchor = Arc::new(Mutex::new(Some(source.clone())));
            let data_state = data_anchor.clone();
            let webview_state = webview_anchor.clone();
            let webview_calls = Arc::new(Mutex::new(0_u8));
            let calls = webview_calls.clone();

            let error = execute_pending_with_deps(
                &migration,
                &paths(&root, source.clone()),
                move |next| {
                    *data_state.lock().unwrap() = next;
                    Ok(())
                },
                move |next| {
                    *webview_state.lock().unwrap() = next;
                    let mut count = calls.lock().unwrap();
                    *count += 1;
                    if *count == 1 {
                        Err("simulated WebView anchor sync failure".to_string())
                    } else {
                        Ok(())
                    }
                },
            )
            .await
            .unwrap_err();

            assert!(error.contains("WebView anchor"));
            assert_eq!(*data_anchor.lock().unwrap(), Some(source.clone()));
            assert_eq!(*webview_anchor.lock().unwrap(), Some(source));
            assert!(!data_target.join("patina.db").exists());
            assert!(!webview_target.join("localstorage/settings").exists());
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn existing_custom_target_database_is_never_overwritten() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("existing-custom");
            let source = root.join("source/Patina");
            let target = root.join("target/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            create_current_database(&target.join("patina.db"), 1).await;
            let existing = fs::read(target.join("patina.db")).unwrap();

            let error = execute_pending_with_deps(
                &pending(&source, &target),
                &paths(&root, source),
                |_| Ok(()),
                |_| Ok(()),
            )
            .await
            .unwrap_err();

            assert!(error.contains("existing managed entries"));
            assert_eq!(fs::read(target.join("patina.db")).unwrap(), existing);
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn mismatched_staging_marker_blocks_cleanup() {
        let root = temp_dir("marker-mismatch");
        let staging = root.join(".patina-storage-staging");
        fs::create_dir_all(&staging).unwrap();
        fs::write(
            staging.join(STAGING_MARKER_FILE),
            marker_contents("another-migration"),
        )
        .unwrap();
        fs::write(staging.join("keep"), b"data").unwrap();

        let error = remove_owned_staging(&staging, "expected-migration").unwrap_err();

        assert!(error.contains("does not match"));
        assert!(staging.join("keep").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_selected_parent_permissions_are_not_changed() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_dir("parent-mode");
        let selected_parent = root.join("selected");
        fs::create_dir_all(&selected_parent).unwrap();
        fs::set_permissions(&selected_parent, fs::Permissions::from_mode(0o755)).unwrap();

        ensure_target_parent_is_safe(&selected_parent.join("Patina")).unwrap();

        let mode = fs::metadata(&selected_parent).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn webview_migration_copies_persistent_state_without_cache() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("webview-copy");
            let source = root.join("source/Patina");
            let target = root.join("target/Patina/webview");
            fs::create_dir_all(source.join("localstorage")).unwrap();
            fs::create_dir_all(source.join("WebKitCache/cache")).unwrap();
            fs::write(source.join("localstorage/settings"), b"state").unwrap();
            fs::write(source.join("WebKitCache/cache/blob"), b"cache").unwrap();
            let mut migration = pending(&source, &source);
            migration.target_webview_root = target.clone();

            execute_pending_with_deps(&migration, &paths(&root, source), |_| Ok(()), |_| Ok(()))
                .await
                .unwrap();

            assert!(target.join("localstorage/settings").exists());
            assert!(!target.join("WebKitCache").exists());
            fs::remove_dir_all(root).unwrap();
        });
    }
}
