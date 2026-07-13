use crate::platform::{app_paths, storage_anchor};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

pub const SQLITE_DB_FILE_NAME: &str = "patina.db";
pub const BACKUP_DIR_NAME: &str = "backups";
pub const REMOTE_BACKUP_TEMP_DIR_NAME: &str = "remote-backup-temp";
pub const API_TOKEN_FILE_NAME: &str = "api_token";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePaths {
    pub control_root: PathBuf,
    pub stable_product_data_root: PathBuf,
    pub data_root: PathBuf,
    pub db_path: PathBuf,
    pub backup_dir: PathBuf,
    pub remote_backup_temp_dir: PathBuf,
    pub api_token_path: PathBuf,
    pub webview_root: PathBuf,
    pub is_custom_data_root: bool,
    pub is_custom_webview_root: bool,
    pub database_creation_allowed: bool,
}

impl StoragePaths {
    pub fn from_roots(
        control_root: PathBuf,
        stable_product_data_root: PathBuf,
        data_root: PathBuf,
        webview_root: PathBuf,
        is_custom_data_root: bool,
        is_custom_webview_root: bool,
    ) -> Self {
        Self {
            control_root,
            db_path: data_root.join(SQLITE_DB_FILE_NAME),
            backup_dir: data_root.join(BACKUP_DIR_NAME),
            remote_backup_temp_dir: data_root.join(REMOTE_BACKUP_TEMP_DIR_NAME),
            api_token_path: stable_product_data_root.join(API_TOKEN_FILE_NAME),
            stable_product_data_root,
            data_root,
            webview_root,
            is_custom_data_root,
            is_custom_webview_root,
            database_creation_allowed: !is_custom_data_root,
        }
    }
}

pub fn default_production_storage_paths_from_environment() -> StoragePaths {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let roots = app_paths::AppPathRoots {
        config,
        data: data.clone(),
        local_data: data,
    };

    default_production_storage_paths_from_roots(&roots)
}

pub fn default_production_storage_paths_from_roots(
    roots: &app_paths::AppPathRoots,
) -> StoragePaths {
    let paths = app_paths::profile_paths(roots, app_paths::AppProfile::Production);
    StoragePaths::from_roots(
        paths.control_root,
        paths.data_root.clone(),
        paths.data_root,
        paths.webview_root,
        false,
        false,
    )
}

pub fn default_storage_paths<R: Runtime>(app: &AppHandle<R>) -> Result<StoragePaths, String> {
    let defaults = app_paths::default_profile_paths(app)?;
    Ok(StoragePaths::from_roots(
        defaults.control_root,
        defaults.data_root.clone(),
        defaults.data_root,
        defaults.webview_root,
        false,
        false,
    ))
}

pub fn resolve_storage_paths<R: Runtime>(app: &AppHandle<R>) -> Result<StoragePaths, String> {
    let defaults = default_storage_paths(app)?;
    let data_root = storage_anchor::read_data_anchor(app)?.map(|anchor| anchor.data_root);
    let webview_root = storage_anchor::read_webview_anchor(app)?.map(|anchor| anchor.webview_root);
    resolve_storage_paths_from(&defaults, data_root, webview_root)
}

pub fn resolve_storage_paths_from(
    defaults: &StoragePaths,
    anchored_data_root: Option<PathBuf>,
    anchored_webview_root: Option<PathBuf>,
) -> Result<StoragePaths, String> {
    let data_root = anchored_data_root.unwrap_or_else(|| defaults.data_root.clone());
    let webview_root = anchored_webview_root.unwrap_or_else(|| defaults.webview_root.clone());
    let is_custom_data_root = !same_path(&data_root, &defaults.data_root);
    let is_custom_webview_root = !same_path(&webview_root, &defaults.webview_root);

    if is_custom_data_root {
        validate_custom_directory(&data_root, "custom data directory")?;
        let db_path = data_root.join(SQLITE_DB_FILE_NAME);
        validate_regular_file(&db_path, "custom Patina database")?;
    }
    if is_custom_webview_root {
        validate_custom_directory(&webview_root, "custom WebView directory")?;
    }

    Ok(StoragePaths::from_roots(
        defaults.control_root.clone(),
        defaults.stable_product_data_root.clone(),
        data_root,
        webview_root,
        is_custom_data_root,
        is_custom_webview_root,
    ))
}

fn validate_custom_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label} `{}` is unavailable: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} `{}` must not be a symbolic link",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!("{label} `{}` is not a directory", path.display()));
    }
    Ok(())
}

fn validate_regular_file(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label} `{}` is unavailable: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} `{}` must not be a symbolic link",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{label} `{}` is not a file", path.display()));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}

fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }

    #[cfg(target_os = "windows")]
    {
        key.make_ascii_lowercase();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "patina-storage-paths-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn defaults(root: &std::path::Path) -> StoragePaths {
        StoragePaths::from_roots(
            root.join("config/Patina"),
            root.join("stable-data/Patina"),
            root.join("stable-data/Patina"),
            root.join("stable-data/Patina"),
            false,
            false,
        )
    }

    #[test]
    fn production_environment_defaults_are_derived_by_platform_owner() {
        let roots = app_paths::AppPathRoots {
            config: PathBuf::from("/home/test/.config"),
            data: PathBuf::from("/home/test/.local/share"),
            local_data: PathBuf::from("/home/test/.local/share"),
        };

        let paths = default_production_storage_paths_from_roots(&roots);

        assert_eq!(
            paths.control_root,
            PathBuf::from("/home/test/.config/Patina")
        );
        assert_eq!(
            paths.data_root,
            PathBuf::from("/home/test/.local/share/Patina")
        );
        assert_eq!(
            paths.webview_root,
            PathBuf::from("/home/test/.local/share/Patina")
        );
        assert_eq!(
            paths.api_token_path,
            PathBuf::from("/home/test/.local/share/Patina/api_token")
        );
        assert!(paths.database_creation_allowed);
    }

    #[test]
    fn default_path_may_start_without_an_existing_database() {
        let root = temp_dir("default");
        let defaults = defaults(&root);

        let paths = resolve_storage_paths_from(&defaults, None, None).unwrap();

        assert_eq!(paths.db_path, defaults.db_path);
        assert!(paths.database_creation_allowed);
        assert_eq!(paths.api_token_path, defaults.api_token_path);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_custom_database_is_an_error_without_default_fallback() {
        let root = temp_dir("missing-custom");
        let defaults = defaults(&root);
        let custom = root.join("custom/Patina");

        let error = resolve_storage_paths_from(&defaults, Some(custom.clone()), None).unwrap_err();

        assert!(error.contains("custom data directory"));
        assert!(error.contains(custom.to_string_lossy().as_ref()));
        assert!(!defaults.db_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_custom_database_becomes_the_only_active_database() {
        let root = temp_dir("custom");
        let defaults = defaults(&root);
        let custom = root.join("custom/Patina");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join(SQLITE_DB_FILE_NAME), b"sqlite").unwrap();

        let paths = resolve_storage_paths_from(&defaults, Some(custom.clone()), None).unwrap();

        assert_eq!(paths.data_root, custom);
        assert!(!paths.database_creation_allowed);
        assert!(paths.is_custom_data_root);
        assert_eq!(paths.api_token_path, defaults.api_token_path);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_custom_webview_profile_is_an_error() {
        let root = temp_dir("missing-webview");
        let defaults = defaults(&root);
        let custom_webview = root.join("custom-webview/Patina");

        let error =
            resolve_storage_paths_from(&defaults, None, Some(custom_webview.clone())).unwrap_err();

        assert!(error.contains("custom WebView directory"));
        assert!(error.contains(custom_webview.to_string_lossy().as_ref()));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn custom_data_root_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("symlink");
        let defaults = defaults(&root);
        let actual = root.join("actual/Patina");
        let linked = root.join("linked/Patina");
        fs::create_dir_all(&actual).unwrap();
        fs::create_dir_all(linked.parent().unwrap()).unwrap();
        fs::write(actual.join(SQLITE_DB_FILE_NAME), b"sqlite").unwrap();
        symlink(&actual, &linked).unwrap();

        let error = resolve_storage_paths_from(&defaults, Some(linked), None).unwrap_err();

        assert!(error.contains("symbolic link"));
        fs::remove_dir_all(root).unwrap();
    }
}
