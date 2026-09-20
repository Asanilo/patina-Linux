use crate::domain::storage::StorageMigrationPreview;
use crate::platform::storage_anchor::{PendingStorageMigration, STORAGE_MIGRATION_PENDING_FORMAT};
use crate::platform::storage_paths::StoragePaths;
use std::fs;
use std::path::{Path, PathBuf};

const MINIMUM_FREE_SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetKind {
    Data,
    Webview,
}

pub(crate) fn plan_pending(
    current: &StoragePaths,
    existing: Option<&PendingStorageMigration>,
    requested_data_root: Option<PathBuf>,
    requested_webview_root: Option<PathBuf>,
    migration_id: &str,
    profile: &str,
    created_at_ms: u64,
) -> Result<PendingStorageMigration, String> {
    if requested_data_root.is_none() && requested_webview_root.is_none() {
        return Err("storage migration must change at least one target".to_string());
    }

    if let Some(target) = requested_data_root.as_deref() {
        validate_target_relationships(current, target, TargetKind::Data)?;
    }
    if let Some(target) = requested_webview_root.as_deref() {
        validate_target_relationships(current, target, TargetKind::Webview)?;
    }

    if let Some(pending) = existing {
        if pending.profile != profile {
            return Err("pending storage migration belongs to a different profile".to_string());
        }
        if !same_path(&pending.source_data_root, &current.data_root)
            || !same_path(&pending.source_webview_root, &current.webview_root)
        {
            return Err(
                "pending storage migration source no longer matches active storage".to_string(),
            );
        }
        if pending.state != "pending-restart" {
            return Err(format!(
                "pending storage migration has unsupported state `{}`",
                pending.state
            ));
        }
    }

    Ok(PendingStorageMigration {
        format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
        id: existing
            .map(|pending| pending.id.clone())
            .unwrap_or_else(|| migration_id.to_string()),
        profile: profile.to_string(),
        source_data_root: current.data_root.clone(),
        target_data_root: requested_data_root.unwrap_or_else(|| {
            existing
                .map(|pending| pending.target_data_root.clone())
                .unwrap_or_else(|| current.data_root.clone())
        }),
        source_webview_root: current.webview_root.clone(),
        target_webview_root: requested_webview_root.unwrap_or_else(|| {
            existing
                .map(|pending| pending.target_webview_root.clone())
                .unwrap_or_else(|| current.webview_root.clone())
        }),
        created_at_ms: existing
            .map(|pending| pending.created_at_ms)
            .unwrap_or(created_at_ms),
        state: "pending-restart".to_string(),
    })
}

pub(crate) fn validate_target_relationships(
    current: &StoragePaths,
    target: &Path,
    kind: TargetKind,
) -> Result<(), String> {
    if !target.is_absolute() {
        return Err(format!(
            "storage target `{}` must be an absolute path",
            target.display()
        ));
    }

    reject_overlap(target, &current.control_root, "storage control directory")?;
    match kind {
        TargetKind::Data => {
            reject_overlap(target, &current.data_root, "active data directory")?;
            if !same_path(&current.webview_root, &current.data_root)
                && !shares_default_root(current, target, &current.webview_root, kind)?
            {
                reject_overlap(target, &current.webview_root, "active WebView directory")?;
            }
        }
        TargetKind::Webview => {
            reject_overlap(target, &current.webview_root, "active WebView directory")?;
            if !same_path(&current.data_root, &current.webview_root)
                && !shares_default_root(current, target, &current.data_root, kind)?
            {
                reject_overlap(target, &current.data_root, "active data directory")?;
            }
        }
    }
    Ok(())
}

fn shares_default_root(
    current: &StoragePaths,
    target: &Path,
    other: &Path,
    kind: TargetKind,
) -> Result<bool, String> {
    // The default root intentionally contains both product data and WebView
    // entries. Returning one side there is safe because the executor promotes
    // separate allowlists. No custom shared root or ancestor/child is exempt.
    if !same_path(target, &current.stable_product_data_root)
        || !same_path(other, &current.stable_product_data_root)
    {
        return Ok(false);
    }
    validate_preview_target(target, kind, true)?;
    let resolved = canonical_relationship_path(target)?;
    let (source, label) = match kind {
        TargetKind::Data => (&current.data_root, "active data directory"),
        TargetKind::Webview => (&current.webview_root, "active WebView directory"),
    };
    // An ancestor alias must not disguise overlap with this side's source or
    // the control root when admitting the shared default directory.
    reject_overlap(&resolved, &canonical_relationship_path(source)?, label)?;
    reject_overlap(
        &resolved,
        &canonical_relationship_path(&current.control_root)?,
        "storage control directory",
    )?;
    Ok(true)
}

fn canonical_relationship_path(path: &Path) -> Result<PathBuf, String> {
    match fs::canonicalize(path) {
        Ok(resolved) => Ok(resolved),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Preview must not create missing directories merely to compare
            // them. Resolve the existing ancestor and retain the missing tail.
            let parent = path.parent().ok_or_else(|| error.to_string())?;
            let name = path.file_name().ok_or_else(|| error.to_string())?;
            Ok(canonical_relationship_path(parent)?.join(name))
        }
        Err(error) => Err(format!(
            "failed to resolve storage directory `{}`: {error}",
            path.display()
        )),
    }
}

pub(crate) fn build_preview(
    current: &StoragePaths,
    target_data_root: PathBuf,
    target_webview_root: PathBuf,
    payload_size_bytes: u64,
    available_space_bytes: u64,
) -> StorageMigrationPreview {
    let margin = (payload_size_bytes / 10).max(MINIMUM_FREE_SPACE_MARGIN_BYTES);
    StorageMigrationPreview {
        current_data_root: current.data_root.clone(),
        target_data_root,
        current_webview_root: current.webview_root.clone(),
        target_webview_root,
        payload_size_bytes,
        available_space_bytes,
        required_space_bytes: payload_size_bytes.saturating_add(margin),
        requires_restart: true,
    }
}

pub(crate) fn preview_with_deps<Payload, Available>(
    current: &StoragePaths,
    kind: TargetKind,
    target: PathBuf,
    payload_size: Payload,
    available_space: Available,
) -> Result<StorageMigrationPreview, String>
where
    Payload: FnOnce() -> Result<u64, String>,
    Available: FnOnce(&Path) -> Result<u64, String>,
{
    preview_with_policy(current, kind, target, payload_size, available_space, false)
}

pub(crate) fn preview_restore_with_deps<Payload, Available>(
    current: &StoragePaths,
    kind: TargetKind,
    target: PathBuf,
    payload_size: Payload,
    available_space: Available,
) -> Result<StorageMigrationPreview, String>
where
    Payload: FnOnce() -> Result<u64, String>,
    Available: FnOnce(&Path) -> Result<u64, String>,
{
    preview_with_policy(current, kind, target, payload_size, available_space, true)
}

fn preview_with_policy<Payload, Available>(
    current: &StoragePaths,
    kind: TargetKind,
    target: PathBuf,
    payload_size: Payload,
    available_space: Available,
    allow_existing_database: bool,
) -> Result<StorageMigrationPreview, String>
where
    Payload: FnOnce() -> Result<u64, String>,
    Available: FnOnce(&Path) -> Result<u64, String>,
{
    validate_target_relationships(current, &target, kind)?;
    validate_preview_target(&target, kind, allow_existing_database)?;

    let payload_size_bytes = payload_size()?;
    let available_space_bytes = available_space(&target)?;
    let (target_data_root, target_webview_root) = match kind {
        TargetKind::Data => (target, current.webview_root.clone()),
        TargetKind::Webview => (current.data_root.clone(), target),
    };
    let preview = build_preview(
        current,
        target_data_root,
        target_webview_root,
        payload_size_bytes,
        available_space_bytes,
    );
    if preview.available_space_bytes < preview.required_space_bytes {
        return Err(format!(
            "storage target has {} bytes available but {} bytes are required",
            preview.available_space_bytes, preview.required_space_bytes
        ));
    }
    Ok(preview)
}

fn validate_preview_target(
    target: &Path,
    kind: TargetKind,
    allow_existing_database: bool,
) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "failed to inspect storage target `{}`: {error}",
                target.display()
            ));
        }
    };
    if let Some(metadata) = metadata {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "storage target `{}` must not be a symbolic link",
                target.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!(
                "storage target `{}` is not a directory",
                target.display()
            ));
        }
    }

    if kind == TargetKind::Data && !allow_existing_database {
        let database = target.join("patina.db");
        if fs::symlink_metadata(&database).is_ok() {
            return Err(format!(
                "storage target `{}` contains an existing Patina database",
                target.display()
            ));
        }
    }
    Ok(())
}

fn reject_overlap(target: &Path, protected: &Path, label: &str) -> Result<(), String> {
    let target = path_components(target);
    let protected = path_components(protected);
    if starts_with_components(&target, &protected) || starts_with_components(&protected, &target) {
        return Err(format!("storage target must not overlap the {label}"));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_components(left) == path_components(right)
}

fn starts_with_components(path: &[String], prefix: &[String]) -> bool {
    prefix.len() <= path.len() && path[..prefix.len()] == *prefix
}

fn path_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let value = component.as_os_str().to_string_lossy().into_owned();
            #[cfg(target_os = "windows")]
            let value = value.to_ascii_lowercase();
            value
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::storage_anchor::{
        PendingStorageMigration, STORAGE_MIGRATION_PENDING_FORMAT,
    };
    use crate::platform::storage_paths::StoragePaths;
    use std::path::PathBuf;
    use std::{fs, time::SystemTime};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "patina-storage-preview-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn current() -> StoragePaths {
        StoragePaths::from_roots(
            PathBuf::from("/home/u/.config/Patina"),
            PathBuf::from("/home/u/.local/share/Patina"),
            PathBuf::from("/home/u/.local/share/Patina"),
            PathBuf::from("/home/u/.local/share/Patina"),
            false,
            false,
        )
    }

    #[test]
    fn data_and_webview_requests_merge_into_one_pending_plan() {
        let current = current();
        let data_target = PathBuf::from("/mnt/data/Patina");
        let webview_target = PathBuf::from("/mnt/cache/Patina/webview");
        let first = plan_pending(
            &current,
            None,
            Some(data_target.clone()),
            None,
            "migration-1",
            "production",
            10,
        )
        .unwrap();
        let merged = plan_pending(
            &current,
            Some(&first),
            None,
            Some(webview_target.clone()),
            "migration-2",
            "production",
            20,
        )
        .unwrap();

        assert_eq!(merged.id, "migration-1");
        assert_eq!(merged.target_data_root, data_target);
        assert_eq!(merged.target_webview_root, webview_target);
    }

    #[test]
    fn target_cannot_overlap_control_or_active_data_roots() {
        let current = current();

        let control_error = validate_target_relationships(
            &current,
            &PathBuf::from("/home/u/.config/Patina/child"),
            TargetKind::Data,
        )
        .unwrap_err();
        let active_error = validate_target_relationships(
            &current,
            &PathBuf::from("/home/u/.local/share"),
            TargetKind::Data,
        )
        .unwrap_err();

        assert!(control_error.contains("storage control directory"));
        assert!(active_error.contains("active data directory"));
    }

    #[test]
    fn target_cannot_overlap_the_other_active_storage_root() {
        let mut current = current();
        current.webview_root = PathBuf::from("/home/u/.cache/Patina");

        let data_error = validate_target_relationships(
            &current,
            &PathBuf::from("/home/u/.cache/Patina/new-data"),
            TargetKind::Data,
        )
        .unwrap_err();
        let webview_error = validate_target_relationships(
            &current,
            &PathBuf::from("/home/u/.local/share/Patina/webview"),
            TargetKind::Webview,
        )
        .unwrap_err();

        assert!(data_error.contains("active WebView directory"));
        assert!(webview_error.contains("active data directory"));
    }

    #[test]
    fn relative_target_is_rejected() {
        let error = validate_target_relationships(
            &current(),
            &PathBuf::from("relative/Patina"),
            TargetKind::Data,
        )
        .unwrap_err();
        assert!(error.contains("absolute"));
    }

    #[test]
    fn preview_adds_ten_percent_or_sixty_four_mib_margin() {
        let preview = build_preview(
            &current(),
            PathBuf::from("/mnt/data/Patina"),
            PathBuf::from("/home/u/.local/share/Patina"),
            100 * 1024 * 1024,
            500 * 1024 * 1024,
        );

        assert_eq!(preview.required_space_bytes, 164 * 1024 * 1024);
        assert!(preview.requires_restart);
    }

    #[test]
    fn pending_plan_rejects_a_different_source() {
        let current = current();
        let existing = PendingStorageMigration {
            format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
            id: "migration-1".to_string(),
            profile: "production".to_string(),
            source_data_root: PathBuf::from("/other/Patina"),
            target_data_root: PathBuf::from("/mnt/data/Patina"),
            source_webview_root: current.webview_root.clone(),
            target_webview_root: current.webview_root.clone(),
            created_at_ms: 10,
            state: "pending-restart".to_string(),
        };

        let error = plan_pending(
            &current,
            Some(&existing),
            None,
            Some(PathBuf::from("/mnt/cache/Patina/webview")),
            "migration-2",
            "production",
            20,
        )
        .unwrap_err();

        assert!(error.contains("source no longer matches"));
    }

    #[test]
    fn preview_does_not_create_target() {
        let root = temp_dir("read-only");
        let target = root.join("future/Patina");

        let preview = preview_with_deps(
            &current(),
            TargetKind::Data,
            target.clone(),
            || Ok(12),
            |_| Ok(256 * 1024 * 1024),
        )
        .unwrap();

        assert_eq!(preview.target_data_root, target);
        assert!(!preview.target_data_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_rejects_an_existing_target_database() {
        let root = temp_dir("existing-db");
        let target = root.join("Patina");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("patina.db"), b"existing").unwrap();

        let error = preview_with_deps(
            &current(),
            TargetKind::Data,
            target,
            || Ok(12),
            |_| Ok(256 * 1024 * 1024),
        )
        .unwrap_err();

        assert!(error.contains("existing Patina database"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_preview_allows_an_existing_default_database() {
        let root = temp_dir("restore-default");
        let target = root.join("default/Patina");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("patina.db"), b"retained default").unwrap();

        let preview = preview_restore_with_deps(
            &current(),
            TargetKind::Data,
            target.clone(),
            || Ok(12),
            |_| Ok(256 * 1024 * 1024),
        )
        .unwrap();

        assert_eq!(preview.target_data_root, target);
        fs::remove_dir_all(root).unwrap();
    }

    fn separated_paths(root: &Path, kind: TargetKind) -> StoragePaths {
        let default = root.join("default/Patina");
        let custom = root.join("custom/Patina");
        fs::create_dir_all(&default).unwrap();
        fs::create_dir_all(&custom).unwrap();
        StoragePaths::from_roots(
            root.join("config/Patina"),
            default.clone(),
            if kind == TargetKind::Data {
                custom.clone()
            } else {
                default.clone()
            },
            if kind == TargetKind::Webview {
                custom
            } else {
                default
            },
            kind == TargetKind::Data,
            kind == TargetKind::Webview,
        )
    }

    #[test]
    fn returning_either_storage_kind_to_the_shared_default_can_be_previewed_and_scheduled() {
        for kind in [TargetKind::Data, TargetKind::Webview] {
            let root = temp_dir("shared-default");
            let current = separated_paths(&root, kind);
            let target = current.stable_product_data_root.clone();
            fs::write(target.join("patina.db"), b"retained default database").unwrap();
            let preview = preview_restore_with_deps(
                &current,
                kind,
                target.clone(),
                || Ok(12),
                |_| Ok(256 * 1024 * 1024),
            )
            .unwrap();
            assert_eq!(preview.target_data_root, target);
            assert_eq!(preview.target_webview_root, target);
            let pending = plan_pending(
                &current,
                None,
                (kind == TargetKind::Data).then(|| target.clone()),
                (kind == TargetKind::Webview).then(|| target.clone()),
                "restore-default",
                "production",
                10,
            )
            .unwrap();
            assert_eq!(pending.target_data_root, target);
            assert_eq!(pending.target_webview_root, target);
            assert_eq!(pending.source_data_root, current.data_root);
            assert_eq!(pending.source_webview_root, current.webview_root);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn default_sharing_does_not_allow_other_custom_roots_or_parent_child_overlap() {
        for kind in [TargetKind::Data, TargetKind::Webview] {
            let root = temp_dir("shared-default-limits");
            let current = separated_paths(&root, kind);
            for target in [
                current.stable_product_data_root.join("child"),
                current
                    .stable_product_data_root
                    .parent()
                    .unwrap()
                    .to_path_buf(),
                root.clone(),
                current.control_root.clone(),
            ] {
                assert!(
                    validate_target_relationships(&current, &target, kind).is_err(),
                    "{target:?}"
                );
            }
            let mut custom_other = current.clone();
            let other = root.join("other/Patina");
            match kind {
                TargetKind::Data => custom_other.webview_root = other.clone(),
                TargetKind::Webview => custom_other.data_root = other.clone(),
            }
            assert!(validate_target_relationships(&custom_other, &other, kind).is_err());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn shared_default_exception_rejects_leaf_symlinks_and_canonical_source_or_control_overlap() {
        use std::os::unix::fs::symlink;
        for kind in [TargetKind::Data, TargetKind::Webview] {
            let root = temp_dir("shared-default-aliases");
            let current = separated_paths(&root, kind);
            let target = &current.stable_product_data_root;
            symlink(root.join("default"), root.join("alias")).unwrap();
            let mut aliased_source = current.clone();
            match kind {
                TargetKind::Data => aliased_source.data_root = root.join("alias/Patina"),
                TargetKind::Webview => aliased_source.webview_root = root.join("alias/Patina"),
            }
            assert!(validate_target_relationships(&aliased_source, target, kind)
                .unwrap_err()
                .contains("active"));
            let mut aliased_control = current.clone();
            aliased_control.control_root = root.join("alias/Patina/control-not-created");
            assert!(
                validate_target_relationships(&aliased_control, target, kind)
                    .unwrap_err()
                    .contains("storage control directory")
            );
            fs::remove_dir(target).unwrap();
            symlink(root.join("custom/Patina"), target).unwrap();
            assert!(validate_target_relationships(&current, target, kind)
                .unwrap_err()
                .contains("symbolic link"));
            fs::remove_dir_all(root).unwrap();
        }
    }
}
