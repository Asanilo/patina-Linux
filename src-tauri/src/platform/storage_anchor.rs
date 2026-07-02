use crate::platform::app_paths;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

pub const DATA_ANCHOR_FORMAT: &str = "patina.data-anchor.v1";
pub const WEBVIEW_ANCHOR_FORMAT: &str = "patina.webview-anchor.v1";
pub const STORAGE_MIGRATION_PENDING_FORMAT: &str = "patina.storage-migration-pending.v1";
pub const STORAGE_MAINTENANCE_FORMAT: &str = "patina.storage-maintenance.v1";

const DATA_ANCHOR_FILE_NAME: &str = "data-anchor.json";
const WEBVIEW_ANCHOR_FILE_NAME: &str = "webview-anchor.json";
const PENDING_MIGRATION_FILE_NAME: &str = "storage-migration-pending.json";
const MAINTENANCE_STATE_FILE_NAME: &str = "storage-maintenance-state.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataAnchor {
    pub format: String,
    pub profile: String,
    pub data_root: PathBuf,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewAnchor {
    pub format: String,
    pub profile: String,
    pub webview_root: PathBuf,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingStorageMigration {
    pub format: String,
    pub id: String,
    pub profile: String,
    pub source_data_root: PathBuf,
    pub target_data_root: PathBuf,
    pub source_webview_root: PathBuf,
    pub target_webview_root: PathBuf,
    pub created_at_ms: u64,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMaintenanceState {
    pub format: String,
    pub profile: String,
    pub pending_webview_cache_clear: bool,
    pub last_webview_cache_clear_at_ms: Option<u64>,
    pub last_maintenance_error: Option<String>,
    pub last_migration_status: Option<String>,
    pub retained_previous_data_root: Option<PathBuf>,
    pub retained_previous_webview_root: Option<PathBuf>,
}

impl StorageMaintenanceState {
    pub fn new(profile: &str) -> Self {
        Self {
            format: STORAGE_MAINTENANCE_FORMAT.to_string(),
            profile: profile.to_string(),
            pending_webview_cache_clear: false,
            last_webview_cache_clear_at_ms: None,
            last_maintenance_error: None,
            last_migration_status: None,
            retained_previous_data_root: None,
            retained_previous_webview_root: None,
        }
    }
}

pub fn data_anchor_path(control_dir: &Path) -> PathBuf {
    control_dir.join(DATA_ANCHOR_FILE_NAME)
}

pub fn webview_anchor_path(control_dir: &Path) -> PathBuf {
    control_dir.join(WEBVIEW_ANCHOR_FILE_NAME)
}

pub fn pending_migration_path(control_dir: &Path) -> PathBuf {
    control_dir.join(PENDING_MIGRATION_FILE_NAME)
}

pub fn maintenance_state_path(control_dir: &Path) -> PathBuf {
    control_dir.join(MAINTENANCE_STATE_FILE_NAME)
}

pub fn read_data_anchor<R: Runtime>(app: &AppHandle<R>) -> Result<Option<DataAnchor>, String> {
    read_data_anchor_from_dir(&control_dir(app)?, app_paths::app_profile(app).key())
}

pub fn read_data_anchor_from_dir(
    control_dir: &Path,
    expected_profile: &str,
) -> Result<Option<DataAnchor>, String> {
    let Some(anchor) = read_json_optional::<DataAnchor>(&data_anchor_path(control_dir))? else {
        return Ok(None);
    };
    if anchor.format != DATA_ANCHOR_FORMAT {
        return Err(format!(
            "unsupported data anchor format `{}`",
            anchor.format
        ));
    }
    Ok((anchor.profile == expected_profile).then_some(anchor))
}

pub fn read_webview_anchor<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<WebviewAnchor>, String> {
    read_webview_anchor_from_dir(&control_dir(app)?, app_paths::app_profile(app).key())
}

pub fn read_webview_anchor_from_dir(
    control_dir: &Path,
    expected_profile: &str,
) -> Result<Option<WebviewAnchor>, String> {
    let Some(anchor) = read_json_optional::<WebviewAnchor>(&webview_anchor_path(control_dir))?
    else {
        return Ok(None);
    };
    if anchor.format != WEBVIEW_ANCHOR_FORMAT {
        return Err(format!(
            "unsupported WebView anchor format `{}`",
            anchor.format
        ));
    }
    Ok((anchor.profile == expected_profile).then_some(anchor))
}

pub fn write_data_anchor<R: Runtime>(app: &AppHandle<R>, data_root: PathBuf) -> Result<(), String> {
    write_data_anchor_to_dir(
        &control_dir(app)?,
        app_paths::app_profile(app).key(),
        data_root,
    )
}

pub fn write_data_anchor_to_dir(
    control_dir: &Path,
    profile: &str,
    data_root: PathBuf,
) -> Result<(), String> {
    write_json_atomic(
        &data_anchor_path(control_dir),
        &DataAnchor {
            format: DATA_ANCHOR_FORMAT.to_string(),
            profile: profile.to_string(),
            data_root,
            updated_at_ms: now_ms(),
        },
    )
}

pub fn write_webview_anchor<R: Runtime>(
    app: &AppHandle<R>,
    webview_root: PathBuf,
) -> Result<(), String> {
    write_webview_anchor_to_dir(
        &control_dir(app)?,
        app_paths::app_profile(app).key(),
        webview_root,
    )
}

pub fn write_webview_anchor_to_dir(
    control_dir: &Path,
    profile: &str,
    webview_root: PathBuf,
) -> Result<(), String> {
    write_json_atomic(
        &webview_anchor_path(control_dir),
        &WebviewAnchor {
            format: WEBVIEW_ANCHOR_FORMAT.to_string(),
            profile: profile.to_string(),
            webview_root,
            updated_at_ms: now_ms(),
        },
    )
}

pub fn remove_data_anchor<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    remove_file_if_exists(&data_anchor_path(&control_dir(app)?))
}

pub fn remove_webview_anchor<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    remove_file_if_exists(&webview_anchor_path(&control_dir(app)?))
}

pub fn read_pending_migration<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<PendingStorageMigration>, String> {
    read_pending_migration_from_dir(&control_dir(app)?, app_paths::app_profile(app).key())
}

pub fn read_pending_migration_from_dir(
    control_dir: &Path,
    expected_profile: &str,
) -> Result<Option<PendingStorageMigration>, String> {
    let Some(pending) =
        read_json_optional::<PendingStorageMigration>(&pending_migration_path(control_dir))?
    else {
        return Ok(None);
    };
    if pending.format != STORAGE_MIGRATION_PENDING_FORMAT {
        return Err(format!(
            "unsupported storage migration format `{}`",
            pending.format
        ));
    }
    Ok((pending.profile == expected_profile).then_some(pending))
}

pub fn write_pending_migration<R: Runtime>(
    app: &AppHandle<R>,
    pending: &PendingStorageMigration,
) -> Result<(), String> {
    if pending.profile != app_paths::app_profile(app).key() {
        return Err("pending storage migration profile does not match the app profile".to_string());
    }
    write_pending_migration_to_dir(&control_dir(app)?, pending)
}

pub fn write_pending_migration_to_dir(
    control_dir: &Path,
    pending: &PendingStorageMigration,
) -> Result<(), String> {
    if pending.format != STORAGE_MIGRATION_PENDING_FORMAT {
        return Err("cannot write unsupported storage migration format".to_string());
    }
    write_json_atomic(&pending_migration_path(control_dir), pending)
}

pub fn remove_pending_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    remove_pending_migration_from_dir(&control_dir(app)?)
}

pub fn remove_pending_migration_from_dir(control_dir: &Path) -> Result<(), String> {
    remove_file_if_exists(&pending_migration_path(control_dir))
}

pub fn read_maintenance_state<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<StorageMaintenanceState, String> {
    read_maintenance_state_from_dir(&control_dir(app)?, app_paths::app_profile(app).key())
}

pub fn read_maintenance_state_from_dir(
    control_dir: &Path,
    expected_profile: &str,
) -> Result<StorageMaintenanceState, String> {
    let Some(state) =
        read_json_optional::<StorageMaintenanceState>(&maintenance_state_path(control_dir))?
    else {
        return Ok(StorageMaintenanceState::new(expected_profile));
    };
    if state.format != STORAGE_MAINTENANCE_FORMAT {
        return Err(format!(
            "unsupported storage maintenance format `{}`",
            state.format
        ));
    }
    if state.profile != expected_profile {
        return Ok(StorageMaintenanceState::new(expected_profile));
    }
    Ok(state)
}

pub fn write_maintenance_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &StorageMaintenanceState,
) -> Result<(), String> {
    if state.profile != app_paths::app_profile(app).key() {
        return Err("storage maintenance profile does not match the app profile".to_string());
    }
    write_maintenance_state_to_dir(&control_dir(app)?, state)
}

pub fn write_maintenance_state_to_dir(
    control_dir: &Path,
    state: &StorageMaintenanceState,
) -> Result<(), String> {
    if state.format != STORAGE_MAINTENANCE_FORMAT {
        return Err("cannot write unsupported storage maintenance format".to_string());
    }
    let path = maintenance_state_path(control_dir);
    if !maintenance_state_requires_file(state) {
        return remove_file_if_exists(&path);
    }
    write_json_atomic(&path, state)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn control_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app_paths::product_config_dir(app)
}

fn maintenance_state_requires_file(state: &StorageMaintenanceState) -> bool {
    state.pending_webview_cache_clear
        || state.last_webview_cache_clear_at_ms.is_some()
        || state.last_maintenance_error.is_some()
        || state.last_migration_status.is_some()
        || state.retained_previous_data_root.is_some()
        || state.retained_previous_webview_root.is_some()
}

fn read_json_optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read `{}`: {error}", path.display())),
    };
    serde_json::from_slice(&raw)
        .map(Some)
        .map_err(|error| format!("failed to parse `{}`: {error}", path.display()))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("storage metadata path `{}` has no parent", path.display()))?;
    create_private_dir(parent)?;
    let raw = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize `{}`: {error}", path.display()))?;
    let temp_path = unique_temp_path(path)?;

    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp_path)
            .map_err(|error| format!("failed to create `{}`: {error}", temp_path.display()))?;
        file.write_all(&raw)
            .map_err(|error| format!("failed to write `{}`: {error}", temp_path.display()))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync `{}`: {error}", temp_path.display()))?;
        drop(file);
        restrict_file_permissions(&temp_path)?;
        replace_file(&temp_path, path)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("failed to sync `{}`: {error}", parent.display()))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn unique_temp_path(path: &Path) -> Result<PathBuf, String> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate storage metadata nonce: {error}"))?;
    let nonce = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid storage metadata path `{}`", path.display()))?;
    Ok(path.with_file_name(format!(".{name}.{nonce}.tmp")))
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create `{}`: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("failed to inspect `{}`: {error}", path.display()))?
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure `{}`: {error}", path.display()))?;
    }
    Ok(())
}

fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("failed to inspect `{}`: {error}", path.display()))?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure `{}`: {error}", path.display()))?;
    }
    Ok(())
}

fn replace_file(temp_path: &Path, target_path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if target_path.exists() {
        fs::remove_file(target_path).map_err(|error| {
            format!(
                "failed to replace storage metadata `{}`: {error}",
                target_path.display()
            )
        })?;
    }
    fs::rename(temp_path, target_path).map_err(|error| {
        format!(
            "failed to replace `{}` with `{}`: {error}",
            target_path.display(),
            temp_path.display()
        )
    })
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| {
                        format!(
                            "failed to sync `{}` after removal: {error}",
                            parent.display()
                        )
                    })?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove `{}`: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "patina-storage-anchor-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_raw(path: &Path, raw: &str) {
        fs::write(path, raw.as_bytes()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_anchor_write_restricts_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_dir("anchor-mode");
        write_data_anchor_to_dir(&root, "production", PathBuf::from("/mnt/patina/Patina")).unwrap();

        let file_mode = fs::metadata(data_anchor_path(&root))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
        assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mismatched_profile_anchor_is_ignored() {
        let root = temp_dir("profile");
        write_data_anchor_to_dir(&root, "local", PathBuf::from("/mnt/local/Patina Local")).unwrap();

        assert!(read_data_anchor_from_dir(&root, "production")
            .unwrap()
            .is_none());
        assert_eq!(
            read_data_anchor_from_dir(&root, "local")
                .unwrap()
                .unwrap()
                .data_root,
            PathBuf::from("/mnt/local/Patina Local")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_anchor_format_is_rejected() {
        let root = temp_dir("format");
        write_raw(
            &data_anchor_path(&root),
            r#"{"format":"patina.data-anchor.v0","profile":"production","dataRoot":"/tmp/Patina","updatedAtMs":1}"#,
        );

        let error = read_data_anchor_from_dir(&root, "production").unwrap_err();
        assert!(error.contains("unsupported data anchor format"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_migration_round_trips_and_can_be_cancelled() {
        let root = temp_dir("pending");
        let pending = PendingStorageMigration {
            format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
            id: "migration-1".to_string(),
            profile: "production".to_string(),
            source_data_root: PathBuf::from("/old/Patina"),
            target_data_root: PathBuf::from("/new/Patina"),
            source_webview_root: PathBuf::from("/old/Patina"),
            target_webview_root: PathBuf::from("/new/Patina/webview"),
            created_at_ms: 10,
            state: "pending-restart".to_string(),
        };

        write_pending_migration_to_dir(&root, &pending).unwrap();
        assert_eq!(
            read_pending_migration_from_dir(&root, "production")
                .unwrap()
                .unwrap()
                .id,
            "migration-1"
        );
        remove_pending_migration_from_dir(&root).unwrap();
        assert!(read_pending_migration_from_dir(&root, "production")
            .unwrap()
            .is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_maintenance_state_does_not_require_a_file() {
        let root = temp_dir("maintenance-empty");
        let state = StorageMaintenanceState::new("production");

        write_maintenance_state_to_dir(&root, &state).unwrap();

        assert!(!maintenance_state_path(&root).exists());
        assert_eq!(
            read_maintenance_state_from_dir(&root, "production").unwrap(),
            StorageMaintenanceState::new("production")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
