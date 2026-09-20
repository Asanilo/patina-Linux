use super::journal::{self, Journal, Promotion, PromotionKind, PromotionPhase, State};
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

pub(crate) async fn validate_migrated_database(
    source_path: &Path,
    staged_path: &Path,
) -> Result<(), String> {
    sqlite_pool::validate_migrated_database_copy(source_path, staged_path).await
}

pub(crate) async fn execute_pending_with_deps<SetDataAnchor, SetWebviewAnchor>(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    set_data_anchor: SetDataAnchor,
    set_webview_anchor: SetWebviewAnchor,
) -> Result<(), String>
where
    SetDataAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    SetWebviewAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
{
    execute_pending_with_hook(
        pending,
        current,
        set_data_anchor,
        set_webview_anchor,
        |_| {},
    )
    .await
}

async fn execute_pending_with_hook<SetDataAnchor, SetWebviewAnchor, Hook>(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    mut set_data_anchor: SetDataAnchor,
    mut set_webview_anchor: SetWebviewAnchor,
    mut hook: Hook,
) -> Result<(), String>
where
    SetDataAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    SetWebviewAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    Hook: FnMut(&str),
{
    let source_paths = source_paths(pending, current);
    if let Some(mut previous) = journal::read(&current.control_root)? {
        validate_journal(&previous, pending, current)?;
        if previous.state == State::Committed {
            ensure_active_roots(pending, current, true)?;
            return Ok(());
        }
        if previous.state == State::Active {
            rollback_execution(
                &mut previous,
                current,
                &mut set_data_anchor,
                &mut set_webview_anchor,
                &mut hook,
            )?;
        } else {
            ensure_active_roots(pending, current, false)?;
        }
        cleanup_terminal_journal(&previous, &current.control_root)?;
    } else {
        ensure_active_roots(pending, current, false)?;
    }
    validate_pending(pending, &source_paths)?;
    let mut receipt = Journal::new(pending);
    journal::write(&current.control_root, &receipt)?;
    let result = async {
        if pending.source_data_root != pending.target_data_root {
            prepare_and_promote_data(pending, &source_paths, &mut receipt, &mut hook).await?;
        }
        if pending.source_webview_root != pending.target_webview_root {
            prepare_and_promote_webview(pending, &source_paths, &mut receipt, &mut hook)?;
        }
        if pending.source_data_root != pending.target_data_root {
            set_data_anchor(anchor_value(
                &pending.target_data_root,
                &current.stable_product_data_root,
            ))
            .map_err(|error| format!("failed to activate migrated data anchor: {error}"))?;
            hook("data-anchor");
        }
        if pending.source_webview_root != pending.target_webview_root {
            set_webview_anchor(anchor_value(
                &pending.target_webview_root,
                &current.stable_product_data_root,
            ))
            .map_err(|error| format!("failed to activate migrated WebView anchor: {error}"))?;
            hook("webview-anchor");
        }
        receipt.state = State::Committed;
        journal::write(&current.control_root, &receipt)?;
        hook("committed");
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        rollback_execution(
            &mut receipt,
            current,
            &mut set_data_anchor,
            &mut set_webview_anchor,
            &mut hook,
        )
        .map_err(|rollback| format!("{error}; migration recovery is incomplete: {rollback}"))?;
        return Err(error);
    }
    Ok(())
}

fn source_paths(pending: &PendingStorageMigration, current: &StoragePaths) -> StoragePaths {
    StoragePaths::from_roots(
        current.control_root.clone(),
        current.stable_product_data_root.clone(),
        pending.source_data_root.clone(),
        pending.source_webview_root.clone(),
        pending.source_data_root != current.stable_product_data_root,
        pending.source_webview_root != current.stable_product_data_root,
    )
}

fn ensure_active_roots(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    committed: bool,
) -> Result<(), String> {
    let (data, webview) = if committed {
        (&pending.target_data_root, &pending.target_webview_root)
    } else {
        (&pending.source_data_root, &pending.source_webview_root)
    };
    if &current.data_root != data || &current.webview_root != webview {
        return Err("storage migration receipt does not match active storage anchors".to_string());
    }
    if pending.source_data_root != pending.target_data_root {
        require_regular_file(
            &data.join("patina.db"),
            if committed {
                "migrated Patina database"
            } else {
                "source Patina database"
            },
        )?;
    }
    if pending.source_webview_root != pending.target_webview_root {
        let metadata = fs::symlink_metadata(webview)
            .map_err(|error| format!("storage migration WebView root is unavailable: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("storage migration WebView root is not a real directory".to_string());
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
    if pending.id.is_empty()
        || pending.id.len() > 128
        || !pending
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("storage migration id is invalid".to_string());
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

async fn prepare_and_promote_data<Hook: FnMut(&str)>(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    receipt: &mut Journal,
    hook: &mut Hook,
) -> Result<(), String> {
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
    for name in DATA_ENTRIES {
        copy_optional_entry(&pending.source_data_root.join(name), &staging.join(name))?;
    }
    validate_migrated_database(&source_db, &staging.join("patina.db")).await?;
    promote_entries(
        PromotionKind::Data,
        &staging,
        &pending.target_data_root,
        DATA_ENTRIES,
        current,
        receipt,
        hook,
    )
}

fn prepare_and_promote_webview<Hook: FnMut(&str)>(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
    receipt: &mut Journal,
    hook: &mut Hook,
) -> Result<(), String> {
    ensure_target_parent_is_safe(&pending.target_webview_root)?;
    let payload = webview_cache::persistent_profile_size(&pending.source_webview_root)?;
    ensure_payload_capacity(payload, &pending.target_webview_root)?;
    let staging = staging_path(&pending.target_webview_root, &pending.id, "webview")?;
    create_owned_staging(&staging, &pending.id)?;
    let report = webview_cache::copy_persistent_profile(&pending.source_webview_root, &staging)?;
    let entries = report.copied.iter().map(String::as_str).collect::<Vec<_>>();
    promote_entries(
        PromotionKind::Webview,
        &staging,
        &pending.target_webview_root,
        &entries,
        current,
        receipt,
        hook,
    )
}

fn promote_entries<Hook: FnMut(&str)>(
    kind: PromotionKind,
    staging: &Path,
    target: &Path,
    entries: &[&str],
    current: &StoragePaths,
    receipt: &mut Journal,
    hook: &mut Hook,
) -> Result<(), String> {
    sync_tree(staging)?;
    let migration_id = &receipt.pending.id;
    // Both kinds can restore into the same default root in one appointment.
    // Ownership, allowlists and recovery paths must not be inferred from that root.
    let label = kind.label();
    let target_created = !target.exists();
    let managed_entries = match kind {
        PromotionKind::Data => DATA_ENTRIES,
        PromotionKind::Webview => webview_cache::PERSISTENT_WEBVIEW_ENTRIES,
    };
    let entries = entries
        .iter()
        .filter(|name| staging.join(name).exists())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let conflicts = managed_entries
        .iter()
        .filter(|name| target.join(name).exists())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let quarantine_root = if conflicts.is_empty() {
        None
    } else if target == current.stable_product_data_root {
        Some(quarantine_path(target, migration_id, label)?)
    } else {
        return Err(format!(
            "storage target `{}` contains existing managed entries: {}",
            target.display(),
            conflicts.join(", ")
        ));
    };

    receipt.promotions.push(Promotion {
        kind,
        target_root: target.to_path_buf(),
        staging_root: staging.to_path_buf(),
        quarantine_root,
        target_created,
        entries,
        original_entries: conflicts.clone(),
        phase: PromotionPhase::Quarantining,
    });
    let index = receipt.promotions.len() - 1;
    journal::write(&current.control_root, receipt)?;
    create_private_dir(target)?;
    if let Some(quarantine) = receipt.promotions[index].quarantine_root.as_ref() {
        create_owned_staging(quarantine, &receipt.pending.id)?;
        for name in &conflicts {
            fs::rename(target.join(name), quarantine.join(name)).map_err(|error| {
                format!(
                    "failed to quarantine existing storage entry `{}`: {error}",
                    target.join(name).display()
                )
            })?;
            sync_directory(target)?;
            sync_directory(quarantine)?;
            hook("quarantined-entry");
        }
        sync_tree(quarantine)?;
    }
    receipt.promotions[index].phase = PromotionPhase::Promoting;
    journal::write(&current.control_root, receipt)?;
    for name in &receipt.promotions[index].entries {
        let source = staging.join(name);
        fs::rename(&source, target.join(name)).map_err(|error| {
            format!(
                "failed to promote migrated storage entry `{}`: {error}",
                source.display()
            )
        })?;
        sync_directory(staging)?;
        sync_directory(target)?;
        hook("promoted-entry");
    }
    receipt.promotions[index].phase = PromotionPhase::Promoted;
    journal::write(&current.control_root, receipt)
}

fn rollback_execution<SetDataAnchor, SetWebviewAnchor, Hook>(
    receipt: &mut Journal,
    current: &StoragePaths,
    set_data_anchor: &mut SetDataAnchor,
    set_webview_anchor: &mut SetWebviewAnchor,
    hook: &mut Hook,
) -> Result<(), String>
where
    SetDataAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    SetWebviewAnchor: FnMut(Option<PathBuf>) -> Result<(), String>,
    Hook: FnMut(&str),
{
    receipt.state = State::Active;
    journal::write(&current.control_root, receipt)?;
    if receipt.pending.source_data_root != receipt.pending.target_data_root {
        set_data_anchor(anchor_value(
            &receipt.pending.source_data_root,
            &current.stable_product_data_root,
        ))?;
    }
    if receipt.pending.source_webview_root != receipt.pending.target_webview_root {
        set_webview_anchor(anchor_value(
            &receipt.pending.source_webview_root,
            &current.stable_product_data_root,
        ))?;
    }
    for promotion in receipt.promotions.iter().rev() {
        rollback_promotion(promotion, &receipt.pending.id, hook)?;
    }
    receipt.state = State::RolledBack;
    journal::write(&current.control_root, receipt)
}

fn rollback_promotion<Hook: FnMut(&str)>(
    promotion: &Promotion,
    migration_id: &str,
    hook: &mut Hook,
) -> Result<(), String> {
    if let Some(quarantine) = &promotion.quarantine_root {
        if quarantine.exists() {
            verify_staging_marker(quarantine, migration_id)?;
        } else if promotion.phase != PromotionPhase::Quarantining {
            return Err("storage recovery is missing original target files".to_string());
        }
        for name in &promotion.original_entries {
            if !quarantine.join(name).exists()
                && (promotion.phase != PromotionPhase::Quarantining
                    || !promotion.target_root.join(name).exists())
            {
                return Err(format!(
                    "storage recovery is missing original entry `{name}`"
                ));
            }
        }
    }
    if promotion.phase != PromotionPhase::Quarantining {
        for name in &promotion.entries {
            remove_path_without_following_links(&promotion.target_root.join(name))?;
            hook("rollback-entry-removed");
        }
    }
    if let Some(quarantine) = &promotion.quarantine_root {
        for name in &promotion.original_entries {
            if quarantine.join(name).exists() {
                // Keep the original intact until the rollback receipt is durable. A second
                // interruption during this copy can then safely repeat the same recovery.
                copy_optional_entry(&quarantine.join(name), &promotion.target_root.join(name))?;
                hook("rollback-restored-entry");
            }
        }
    }
    if promotion.target_root.exists() {
        sync_directory(&promotion.target_root)?;
    }
    if promotion.target_created {
        match fs::remove_dir(&promotion.target_root) {
            Ok(()) => {
                if let Some(parent) = promotion.target_root.parent() {
                    sync_directory(parent)?;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                ) => {}
            Err(error) => return Err(format!("failed to remove empty migration target: {error}")),
        }
    }
    Ok(())
}

fn validate_journal(
    receipt: &Journal,
    pending: &PendingStorageMigration,
    current: &StoragePaths,
) -> Result<(), String> {
    if &receipt.pending != pending || receipt.promotions.len() > 2 {
        return Err("storage migration journal does not match the pending request".to_string());
    }
    validate_pending(pending, &source_paths(pending, current))?;
    if ![&pending.source_data_root, &pending.target_data_root].contains(&&current.data_root)
        || ![&pending.source_webview_root, &pending.target_webview_root]
            .contains(&&current.webview_root)
    {
        return Err("storage migration journal does not match active storage".to_string());
    }
    let mut kinds = std::collections::HashSet::new();
    for promotion in &receipt.promotions {
        let (source, target, allowed) = match promotion.kind {
            PromotionKind::Data => (
                &pending.source_data_root,
                &pending.target_data_root,
                DATA_ENTRIES,
            ),
            PromotionKind::Webview => (
                &pending.source_webview_root,
                &pending.target_webview_root,
                webview_cache::PERSISTENT_WEBVIEW_ENTRIES,
            ),
        };
        if source == target || &promotion.target_root != target {
            return Err("storage journal promotion has an unrelated target".to_string());
        }
        let label = promotion.kind.label();
        if !kinds.insert(promotion.kind)
            || promotion.staging_root != staging_path(&promotion.target_root, &pending.id, label)?
            || promotion.quarantine_root.as_ref().is_some_and(|path| {
                quarantine_path(&promotion.target_root, &pending.id, label).as_ref() != Ok(path)
            })
            || promotion.original_entries.is_empty() != promotion.quarantine_root.is_none()
        {
            return Err("storage journal promotion paths are invalid".to_string());
        }
        for names in [&promotion.entries, &promotion.original_entries] {
            let mut entries = std::collections::HashSet::new();
            for name in names {
                if !allowed.contains(&name.as_str()) || !entries.insert(name) {
                    return Err("storage journal contains an invalid managed entry".to_string());
                }
            }
        }
    }
    Ok(())
}

fn cleanup_terminal_journal(receipt: &Journal, control_root: &Path) -> Result<(), String> {
    if receipt.state == State::Active {
        return Err(
            "storage migration recovery must finish before clearing its journal".to_string(),
        );
    }
    for (target, label) in [
        (&receipt.pending.target_data_root, "data"),
        (&receipt.pending.target_webview_root, "webview"),
    ] {
        remove_owned_staging(
            &staging_path(target, &receipt.pending.id, label)?,
            &receipt.pending.id,
        )?;
    }
    if receipt.state == State::RolledBack {
        for promotion in &receipt.promotions {
            if let Some(quarantine) = &promotion.quarantine_root {
                remove_owned_staging(quarantine, &receipt.pending.id)?;
            }
        }
    }
    // Committed default-target quarantines remain as retained original files.
    journal::remove(control_root)
}

pub(super) fn failed_execution_can_finish(
    pending: &PendingStorageMigration,
    current: &StoragePaths,
) -> Result<bool, String> {
    let Some(receipt) = journal::read(&current.control_root)? else {
        return Ok(false);
    };
    validate_journal(&receipt, pending, current)?;
    Ok(receipt.state == State::RolledBack && ensure_active_roots(pending, current, false).is_ok())
}

pub(super) fn finish_without_pending(current: &StoragePaths) -> Result<(), String> {
    let Some(receipt) = journal::read(&current.control_root)? else {
        return Ok(());
    };
    validate_journal(&receipt, &receipt.pending, current)?;
    if receipt.state == State::Active {
        return Err(
            "active storage migration journal has no pending request; recovery is required"
                .to_string(),
        );
    }
    ensure_active_roots(&receipt.pending, current, receipt.state == State::Committed)?;
    cleanup_terminal_journal(&receipt, &current.control_root)
}

fn ensure_capacity(source: &Path, target: &Path, entries: &[&str]) -> Result<(), String> {
    let mut payload = 0_u64;
    for name in entries {
        payload = payload.saturating_add(storage_usage::path_size(&source.join(name))?);
    }
    ensure_payload_capacity(payload, target)
}

fn sync_tree(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect copied storage entry: {error}"))?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err(format!(
            "copied storage entry `{}` is unsafe",
            path.display()
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|error| format!("failed to read copied storage directory: {error}"))?
        {
            let entry = entry
                .map_err(|error| format!("failed to inspect copied storage entry: {error}"))?;
            sync_tree(&entry.path())?;
        }
    }
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            format!(
                "failed to sync copied storage entry `{}`: {error}",
                path.display()
            )
        })
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
        sync_directory(target)?;
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
    restrict_file(&marker)?;
    sync_directory(path)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
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
    use futures_util::FutureExt;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Executor;
    use std::fs;
    use std::panic::AssertUnwindSafe;
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
            let mut calls = 0;

            let error = execute_pending_with_deps(
                &pending(&source, &default_target),
                &paths(&root, source.clone()),
                |_| {
                    calls += 1;
                    if calls == 1 {
                        Err("simulated anchor failure".to_string())
                    } else {
                        Ok(())
                    }
                },
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

    #[test]
    fn interrupted_migration_recovers_each_durable_transition_without_losing_originals() {
        tauri::async_runtime::block_on(async {
            for fault in [
                "quarantined-entry",
                "promoted-entry",
                "data-anchor",
                "committed",
            ] {
                let root = temp_dir(fault);
                let source = root.join("source/Patina");
                let target = root.join("default/Patina");
                let webview_target = root.join("webview/Patina");
                create_current_database(&source.join("patina.db"), 2).await;
                create_current_database(&target.join("patina.db"), 1).await;
                fs::create_dir_all(source.join("localstorage")).unwrap();
                fs::write(source.join("localstorage/preferences"), b"webview-state").unwrap();
                let original_source = fs::read(source.join("patina.db")).unwrap();
                let original_target = fs::read(target.join("patina.db")).unwrap();
                let mut request = pending(&source, &target);
                request.target_webview_root = webview_target.clone();
                let current = paths(&root, source.clone());
                let mut active_data = source.clone();
                let mut active_webview = source.clone();
                let stopped = AssertUnwindSafe(execute_pending_with_hook(
                    &request,
                    &current,
                    |next| {
                        active_data = next.unwrap_or_else(|| target.clone());
                        Ok(())
                    },
                    |next| {
                        active_webview = next.unwrap_or_else(|| target.clone());
                        Ok(())
                    },
                    |step| {
                        if step == fault {
                            panic!("simulated process interruption at {step}");
                        }
                    },
                ))
                .catch_unwind()
                .await;
                assert!(stopped.is_err(), "fault point {fault} must be reached");
                assert!(journal::read(&current.control_root).unwrap().is_some());
                let resumed_paths = StoragePaths::from_roots(
                    current.control_root.clone(),
                    current.stable_product_data_root.clone(),
                    active_data.clone(),
                    active_webview.clone(),
                    true,
                    true,
                );
                execute_pending_with_deps(
                    &request,
                    &resumed_paths,
                    |next| {
                        active_data = next.unwrap_or_else(|| target.clone());
                        Ok(())
                    },
                    |next| {
                        active_webview = next.unwrap_or_else(|| target.clone());
                        Ok(())
                    },
                )
                .await
                .unwrap();
                assert_eq!(active_data, target);
                assert_eq!(active_webview, webview_target);
                assert_eq!(fs::read(source.join("patina.db")).unwrap(), original_source);
                assert_eq!(
                    fs::read(webview_target.join("localstorage/preferences")).unwrap(),
                    b"webview-state"
                );
                let retained = quarantine_path(&target, &request.id, "data").unwrap();
                assert_eq!(
                    fs::read(retained.join("patina.db")).unwrap(),
                    original_target
                );
                let finished_paths = StoragePaths::from_roots(
                    current.control_root.clone(),
                    current.stable_product_data_root.clone(),
                    active_data,
                    active_webview,
                    false,
                    true,
                );
                finish_without_pending(&finished_paths).unwrap();
                assert!(journal::read(&current.control_root).unwrap().is_none());
                fs::remove_dir_all(root).unwrap();
            }
        });
    }

    #[test]
    fn interrupted_rollback_keeps_quarantine_for_another_recovery_attempt() {
        tauri::async_runtime::block_on(async {
            for fault in ["rollback-entry-removed", "rollback-restored-entry"] {
                let root = temp_dir(fault);
                let source = root.join("source/Patina");
                let target = root.join("default/Patina");
                create_current_database(&source.join("patina.db"), 2).await;
                create_current_database(&target.join("patina.db"), 1).await;
                fs::create_dir_all(target.join("backups")).unwrap();
                fs::write(target.join("backups/original"), b"retained backup").unwrap();
                fs::create_dir_all(source.join("backups")).unwrap();
                fs::write(source.join("backups/new"), b"new backup").unwrap();
                let original_target = fs::read(target.join("patina.db")).unwrap();
                let request = pending(&source, &target);
                let current = paths(&root, source.clone());
                let mut calls = 0;
                let stopped = AssertUnwindSafe(execute_pending_with_hook(
                    &request,
                    &current,
                    |_| {
                        calls += 1;
                        if calls == 1 {
                            Err("anchor failure".to_string())
                        } else {
                            Ok(())
                        }
                    },
                    |_| Ok(()),
                    |step| {
                        if step == fault {
                            panic!("simulated rollback interruption at {step}");
                        }
                    },
                ))
                .catch_unwind()
                .await;
                assert!(stopped.is_err());
                let quarantine = quarantine_path(&target, &request.id, "data").unwrap();
                assert_eq!(
                    fs::read(quarantine.join("patina.db")).unwrap(),
                    original_target
                );
                assert_eq!(
                    journal::read(&current.control_root).unwrap().unwrap().state,
                    State::Active
                );
                execute_pending_with_deps(&request, &current, |_| Ok(()), |_| Ok(()))
                    .await
                    .unwrap();
                assert_eq!(
                    fs::read(quarantine.join("patina.db")).unwrap(),
                    original_target
                );
                assert_eq!(
                    fs::read(quarantine.join("backups/original")).unwrap(),
                    b"retained backup"
                );
                assert!(target.join("backups/new").exists());
                fs::remove_dir_all(root).unwrap();
            }
        });
    }

    #[test]
    fn unproven_anchor_rollback_preserves_recovery_receipt_and_original_target() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("unproven-rollback");
            let source = root.join("source/Patina");
            let target = root.join("default/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            create_current_database(&target.join("patina.db"), 1).await;
            let original_target = fs::read(target.join("patina.db")).unwrap();
            let request = pending(&source, &target);
            let current = paths(&root, source.clone());
            let error = execute_pending_with_deps(
                &request,
                &current,
                |_| Err("anchor unavailable".to_string()),
                |_| Ok(()),
            )
            .await
            .unwrap_err();
            assert!(error.contains("recovery is incomplete"));
            assert!(!failed_execution_can_finish(&request, &current).unwrap());
            assert!(finish_without_pending(&current).is_err());
            let quarantine = quarantine_path(&target, &request.id, "data").unwrap();
            assert_eq!(
                fs::read(quarantine.join("patina.db")).unwrap(),
                original_target
            );
            assert!(source.join("patina.db").exists());
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn restore_default_quarantines_stale_sidecars_absent_from_the_new_database() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("stale-sidecars");
            let source = root.join("source/Patina");
            let target = root.join("default/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            create_current_database(&target.join("patina.db"), 1).await;
            fs::write(target.join("patina.db-wal"), b"old-target-wal").unwrap();
            fs::write(target.join("patina.db-shm"), b"old-target-shm").unwrap();
            assert!(!source.join("patina.db-wal").exists());
            let request = pending(&source, &target);
            execute_pending_with_deps(&request, &paths(&root, source), |_| Ok(()), |_| Ok(()))
                .await
                .unwrap();
            assert!(!target.join("patina.db-wal").exists());
            assert!(!target.join("patina.db-shm").exists());
            let quarantine = quarantine_path(&target, &request.id, "data").unwrap();
            assert_eq!(
                fs::read(quarantine.join("patina.db-wal")).unwrap(),
                b"old-target-wal"
            );
            assert_eq!(
                fs::read(quarantine.join("patina.db-shm")).unwrap(),
                b"old-target-shm"
            );
            fs::remove_dir_all(root).unwrap();
        });
    }

    #[test]
    fn restoring_one_kind_to_the_shared_default_preserves_the_other_active_kind() {
        tauri::async_runtime::block_on(async {
            for kind in [TargetKind::Data, TargetKind::Webview] {
                let root = temp_dir("shared-default-restore");
                let custom = root.join("custom/Patina");
                let default = root.join("default/Patina");
                create_current_database(&custom.join("patina.db"), 2).await;
                create_current_database(&default.join("patina.db"), 1).await;
                for (path, content) in [(&custom, b"custom-webview"), (&default, b"active-webview")]
                {
                    fs::create_dir_all(path.join("localstorage")).unwrap();
                    fs::write(path.join("localstorage/settings"), content).unwrap();
                }
                fs::write(default.join("api_token"), b"stable-synthetic-token").unwrap();
                let original_db = fs::read(default.join("patina.db")).unwrap();
                let custom_db = fs::read(custom.join("patina.db")).unwrap();
                let current = StoragePaths::from_roots(
                    root.join("config/Patina"),
                    default.clone(),
                    if kind == TargetKind::Data {
                        custom.clone()
                    } else {
                        default.clone()
                    },
                    if kind == TargetKind::Webview {
                        custom.clone()
                    } else {
                        default.clone()
                    },
                    kind == TargetKind::Data,
                    kind == TargetKind::Webview,
                );
                let request = plan::plan_pending(
                    &current,
                    None,
                    (kind == TargetKind::Data).then(|| default.clone()),
                    (kind == TargetKind::Webview).then(|| default.clone()),
                    "shared-default",
                    "production",
                    1,
                )
                .unwrap();
                let mut data_anchor_calls = 0;
                let mut webview_anchor_calls = 0;
                execute_pending_with_deps(
                    &request,
                    &current,
                    |next| {
                        assert_eq!(next, None);
                        data_anchor_calls += 1;
                        Ok(())
                    },
                    |next| {
                        assert_eq!(next, None);
                        webview_anchor_calls += 1;
                        Ok(())
                    },
                )
                .await
                .unwrap();
                assert_eq!(data_anchor_calls, usize::from(kind == TargetKind::Data));
                assert_eq!(
                    webview_anchor_calls,
                    usize::from(kind == TargetKind::Webview)
                );
                if kind == TargetKind::Data {
                    let restored = SqlitePoolOptions::new()
                        .max_connections(1)
                        .connect_with(
                            SqliteConnectOptions::new()
                                .filename(default.join("patina.db"))
                                .read_only(true),
                        )
                        .await
                        .unwrap();
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                            .fetch_one(&restored)
                            .await
                            .unwrap(),
                        2
                    );
                    assert_eq!(
                        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                            .fetch_one(&restored)
                            .await
                            .unwrap(),
                        "ok"
                    );
                    restored.close().await;
                    assert_eq!(
                        fs::read(default.join("localstorage/settings")).unwrap(),
                        b"active-webview"
                    );
                } else {
                    assert!(
                        fs::read(default.join("patina.db")).unwrap() == original_db,
                        "WebView restoration must preserve the active database bytes"
                    );
                    assert_eq!(
                        fs::read(default.join("localstorage/settings")).unwrap(),
                        b"custom-webview"
                    );
                }
                assert!(
                    fs::read(custom.join("patina.db")).unwrap() == custom_db,
                    "restoration must preserve the source database bytes"
                );
                assert_eq!(
                    fs::read(custom.join("localstorage/settings")).unwrap(),
                    b"custom-webview"
                );
                assert_eq!(
                    fs::read(default.join("api_token")).unwrap(),
                    b"stable-synthetic-token"
                );
                fs::remove_dir_all(root).unwrap();
            }
        });
    }

    struct SharedDefaultRestore {
        root: PathBuf,
        current: StoragePaths,
        request: PendingStorageMigration,
        original_default_db: Vec<u8>,
        source_db: Vec<u8>,
    }

    impl SharedDefaultRestore {
        async fn new() -> Self {
            let root = temp_dir("restore-both-default");
            let data = root.join("custom-data/Patina");
            let webview = root.join("custom-webview/Patina/webview");
            let default = root.join("default/Patina");
            create_current_database(&data.join("patina.db"), 2).await;
            create_current_database(&default.join("patina.db"), 1).await;
            for (path, value) in [(&data, b"new backup"), (&default, b"old backup")] {
                fs::create_dir_all(path.join("backups")).unwrap();
                fs::write(path.join("backups/snapshot"), value).unwrap();
            }
            for (path, value) in [(&webview, b"new WebView"), (&default, b"old WebView")] {
                fs::create_dir_all(path.join("localstorage")).unwrap();
                fs::write(path.join("localstorage/settings"), value).unwrap();
            }
            fs::write(default.join("api_token"), b"stable synthetic token").unwrap();
            let original_default_db = fs::read(default.join("patina.db")).unwrap();
            let source_db = fs::read(data.join("patina.db")).unwrap();
            let current = StoragePaths::from_roots(
                root.join("config/Patina"),
                default.clone(),
                data,
                webview,
                true,
                true,
            );
            let first = plan::plan_pending(
                &current,
                None,
                Some(default.clone()),
                None,
                "restore-both",
                "production",
                1,
            )
            .unwrap();
            let request = plan::plan_pending(
                &current,
                Some(&first),
                None,
                Some(default),
                "unused-second-id",
                "production",
                2,
            )
            .unwrap();
            Self {
                root,
                current,
                request,
                original_default_db,
                source_db,
            }
        }

        fn assert_sources_and_token(&self) {
            assert_eq!(
                fs::read(self.current.data_root.join("patina.db")).unwrap(),
                self.source_db
            );
            assert_eq!(
                fs::read(self.current.data_root.join("backups/snapshot")).unwrap(),
                b"new backup"
            );
            assert_eq!(
                fs::read(self.current.webview_root.join("localstorage/settings")).unwrap(),
                b"new WebView"
            );
            assert_eq!(
                fs::read(self.current.stable_product_data_root.join("api_token")).unwrap(),
                b"stable synthetic token"
            );
        }

        async fn assert_completed(&self) {
            let default = &self.current.stable_product_data_root;
            let restored = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(default.join("patina.db"))
                        .read_only(true),
                )
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                    .fetch_one(&restored)
                    .await
                    .unwrap(),
                2
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                    .fetch_one(&restored)
                    .await
                    .unwrap(),
                "ok"
            );
            restored.close().await;
            assert_eq!(
                fs::read(default.join("backups/snapshot")).unwrap(),
                b"new backup"
            );
            assert_eq!(
                fs::read(default.join("localstorage/settings")).unwrap(),
                b"new WebView"
            );
            self.assert_sources_and_token();
            let data_original = quarantine_path(default, &self.request.id, "data").unwrap();
            let webview_original = quarantine_path(default, &self.request.id, "webview").unwrap();
            assert_eq!(
                fs::read(data_original.join("patina.db")).unwrap(),
                self.original_default_db
            );
            assert_eq!(
                fs::read(data_original.join("backups/snapshot")).unwrap(),
                b"old backup"
            );
            assert_eq!(
                fs::read(webview_original.join("localstorage/settings")).unwrap(),
                b"old WebView"
            );
            let finished = StoragePaths::from_roots(
                self.current.control_root.clone(),
                default.clone(),
                default.clone(),
                default.clone(),
                false,
                false,
            );
            finish_without_pending(&finished).unwrap();
            assert!(journal::read(&self.current.control_root).unwrap().is_none());
            assert!(data_original.join("patina.db").exists());
            assert!(webview_original.join("localstorage/settings").exists());
        }
    }

    #[test]
    fn both_storage_kinds_restore_to_one_default_root_without_crossing_ownership() {
        tauri::async_runtime::block_on(async {
            let fixture = SharedDefaultRestore::new().await;
            execute_pending_with_deps(
                &fixture.request,
                &fixture.current,
                |next| {
                    assert!(next.is_none());
                    Ok(())
                },
                |next| {
                    assert!(next.is_none());
                    Ok(())
                },
            )
            .await
            .unwrap();
            fixture.assert_completed().await;
            fs::remove_dir_all(fixture.root).unwrap();
        });
    }

    #[test]
    fn failed_combined_default_restore_recovers_both_original_kinds() {
        tauri::async_runtime::block_on(async {
            let fixture = SharedDefaultRestore::new().await;
            let mut data_anchor = Some(fixture.current.data_root.clone());
            let mut webview_anchor = Some(fixture.current.webview_root.clone());
            let mut webview_calls = 0;
            let error = execute_pending_with_deps(
                &fixture.request,
                &fixture.current,
                |next| {
                    data_anchor = next;
                    Ok(())
                },
                |next| {
                    webview_anchor = next;
                    webview_calls += 1;
                    if webview_calls == 1 {
                        Err("synthetic WebView anchor failure".into())
                    } else {
                        Ok(())
                    }
                },
            )
            .await
            .unwrap_err();
            assert!(error.contains("synthetic WebView anchor failure"));
            assert_eq!(data_anchor, Some(fixture.current.data_root.clone()));
            assert_eq!(webview_anchor, Some(fixture.current.webview_root.clone()));
            let default = &fixture.current.stable_product_data_root;
            assert_eq!(
                fs::read(default.join("patina.db")).unwrap(),
                fixture.original_default_db
            );
            assert_eq!(
                fs::read(default.join("backups/snapshot")).unwrap(),
                b"old backup"
            );
            assert_eq!(
                fs::read(default.join("localstorage/settings")).unwrap(),
                b"old WebView"
            );
            fixture.assert_sources_and_token();
            assert!(failed_execution_can_finish(&fixture.request, &fixture.current).unwrap());
            // A later retry must validate both promotions sharing the same root.
            execute_pending_with_deps(&fixture.request, &fixture.current, |_| Ok(()), |_| Ok(()))
                .await
                .unwrap();
            fixture.assert_completed().await;
            fs::remove_dir_all(fixture.root).unwrap();
        });
    }

    #[test]
    fn interrupted_combined_default_restore_and_rollback_keep_both_originals() {
        tauri::async_runtime::block_on(async {
            for fault in [
                "webview-quarantined",
                "webview-promoted",
                "data-anchor",
                "webview-anchor",
                "committed",
                "rollback-restored-entry",
            ] {
                let fixture = SharedDefaultRestore::new().await;
                let default = &fixture.current.stable_product_data_root;
                let mut data = fixture.current.data_root.clone();
                let mut webview = fixture.current.webview_root.clone();
                let mut webview_calls = 0;
                let stopped = AssertUnwindSafe(execute_pending_with_hook(
                    &fixture.request,
                    &fixture.current,
                    |next| {
                        data = next.unwrap_or_else(|| default.clone());
                        Ok(())
                    },
                    |next| {
                        webview = next.unwrap_or_else(|| default.clone());
                        webview_calls += 1;
                        if fault == "rollback-restored-entry" && webview_calls == 1 {
                            Err("synthetic anchor failure before interrupted rollback".into())
                        } else {
                            Ok(())
                        }
                    },
                    |step| {
                        let in_webview_promotion =
                            matches!(step, "quarantined-entry" | "promoted-entry")
                                && journal::read(&fixture.current.control_root)
                                    .unwrap()
                                    .unwrap()
                                    .promotions
                                    .last()
                                    .is_some_and(|promotion| {
                                        promotion.kind == PromotionKind::Webview
                                    });
                        if step == fault
                            || (fault == "webview-quarantined"
                                && step == "quarantined-entry"
                                && in_webview_promotion)
                            || (fault == "webview-promoted"
                                && step == "promoted-entry"
                                && in_webview_promotion)
                        {
                            panic!("synthetic combined restore interruption at {fault}");
                        }
                    },
                ))
                .catch_unwind()
                .await;
                assert!(stopped.is_err(), "fault point {fault} must be reached");
                let resumed = StoragePaths::from_roots(
                    fixture.current.control_root.clone(),
                    default.clone(),
                    data,
                    webview,
                    true,
                    true,
                );
                execute_pending_with_deps(&fixture.request, &resumed, |_| Ok(()), |_| Ok(()))
                    .await
                    .unwrap();
                fixture.assert_completed().await;
                fs::remove_dir_all(fixture.root).unwrap();
            }
        });
    }

    #[test]
    fn invalid_journal_entry_cannot_remove_unmanaged_target_files() {
        tauri::async_runtime::block_on(async {
            let root = temp_dir("invalid-journal-entry");
            let source = root.join("source/Patina");
            let target = root.join("target/Patina");
            create_current_database(&source.join("patina.db"), 2).await;
            let request = pending(&source, &target);
            let current = paths(&root, source);
            let interrupted = AssertUnwindSafe(execute_pending_with_hook(
                &request,
                &current,
                |_| Ok(()),
                |_| Ok(()),
                |step| {
                    if step == "promoted-entry" {
                        panic!("interrupted");
                    }
                },
            ))
            .catch_unwind()
            .await;
            assert!(interrupted.is_err());
            let mut receipt = journal::read(&current.control_root).unwrap().unwrap();
            receipt.promotions[0].entries.push("api_token".to_string());
            journal::write(&current.control_root, &receipt).unwrap();
            fs::write(target.join("api_token"), b"unmanaged-token").unwrap();
            let error = execute_pending_with_deps(&request, &current, |_| Ok(()), |_| Ok(()))
                .await
                .unwrap_err();
            assert!(error.contains("invalid managed entry"));
            assert_eq!(
                fs::read(target.join("api_token")).unwrap(),
                b"unmanaged-token"
            );
            assert!(journal::read(&current.control_root).unwrap().is_some());
            fs::remove_dir_all(root).unwrap();
        });
    }
}
