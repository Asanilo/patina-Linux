use crate::platform::{app_paths::AppProfile, storage_paths::StoragePaths};

pub fn resolve(
    roots: &crate::platform::app_paths::AppPathRoots,
    profile: AppProfile,
) -> Result<StoragePaths, String> {
    crate::platform::storage_paths::resolve_storage_paths_for_profile(roots, profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_paths::AppPathRoots;
    use crate::platform::storage_anchor::{
        write_data_anchor_to_dir, write_pending_migration_to_dir, PendingStorageMigration,
        STORAGE_MIGRATION_PENDING_FORMAT,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_roots(label: &str) -> (PathBuf, AppPathRoots) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "patinad-storage-{label}-{}-{nonce}",
            std::process::id()
        ));
        let roots = AppPathRoots {
            config: root.join("config"),
            data: root.join("data"),
            local_data: root.join("local-data"),
        };
        (root, roots)
    }

    #[test]
    fn dev_profile_uses_dev_control_and_data_roots() {
        let (root, roots) = temp_roots("dev");

        let paths = resolve(&roots, AppProfile::Dev).unwrap();

        assert!(paths.control_root.ends_with("Patina Dev"));
        assert!(paths.data_root.ends_with("Patina Dev"));
        assert!(paths.webview_root.ends_with("Patina Dev"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn anchored_data_root_is_used_without_tauri() {
        let (root, roots) = temp_roots("anchor");
        let defaults = crate::platform::app_paths::profile_paths(&roots, AppProfile::Local);
        let custom = root.join("mounted/Patina Local");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("patina.db"), b"sqlite").unwrap();
        write_data_anchor_to_dir(&defaults.control_root, "local", custom.clone()).unwrap();

        let paths = resolve(&roots, AppProfile::Local).unwrap();

        assert_eq!(paths.data_root, custom);
        assert!(!paths.database_creation_allowed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_custom_data_root_fails_closed() {
        let (root, roots) = temp_roots("missing-anchor");
        let defaults = crate::platform::app_paths::profile_paths(&roots, AppProfile::Production);
        let missing = root.join("missing/Patina");
        write_data_anchor_to_dir(&defaults.control_root, "production", missing).unwrap();

        let error = resolve(&roots, AppProfile::Production).unwrap_err();

        assert!(error.contains("custom data directory"));
        assert!(!defaults.data_root.join("patina.db").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_migration_blocks_daemon_startup() {
        let (root, roots) = temp_roots("pending");
        let defaults = crate::platform::app_paths::profile_paths(&roots, AppProfile::Production);
        write_pending_migration_to_dir(
            &defaults.control_root,
            &PendingStorageMigration {
                format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
                id: "migration-1".to_string(),
                profile: "production".to_string(),
                source_data_root: defaults.data_root.clone(),
                target_data_root: root.join("target/Patina"),
                source_webview_root: defaults.webview_root.clone(),
                target_webview_root: root.join("target-webview/Patina"),
                created_at_ms: 1,
                state: "scheduled".to_string(),
            },
        )
        .unwrap();

        let error = resolve(&roots, AppProfile::Production).unwrap_err();

        assert!(error.contains("desktop app"));
        assert!(error.contains("migration-1"));
        fs::remove_dir_all(root).unwrap();
    }
}
