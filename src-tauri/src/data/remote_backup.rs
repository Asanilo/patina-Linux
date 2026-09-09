use crate::data::backup;
use crate::domain::backup::BackupPreview;
pub use crate::domain::remote_backup::{
    RemoteBackupDownloadResult, RemoteBackupEntry, RemoteBackupUploadResult, WebDavBackupConfig,
    WebDavTestResult,
};
use crate::platform::credentials;
use crate::platform::storage_paths;
use crate::platform::webdav::{normalize_remote_dir, WebDavClient, WebDavConfig};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const INDEX_FILE_NAME: &str = "backup-index.json";
const INDEX_VERSION: u32 = 1;
const INDEX_PRODUCT: &str = "Patina";
const MAX_BACKUP_LIST_ITEMS: usize = 50;

#[cfg(test)]
mod transfer_tests;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteBackupIndex {
    version: u32,
    product: String,
    updated_at_ms: u64,
    backups: Vec<RemoteBackupEntry>,
}

fn config_to_webdav(config: WebDavBackupConfig) -> Result<WebDavConfig, String> {
    let username = config.username.trim().to_string();
    if username.is_empty() {
        return Err("WebDAV username cannot be empty".to_string());
    }

    Ok(WebDavConfig {
        url: config.url.trim().to_string(),
        username,
        remote_dir: normalize_remote_dir(&config.remote_dir)?,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn remote_backup_id() -> Result<String, String> {
    let mut random = [0_u8; 4];
    getrandom::fill(&mut random)
        .map_err(|error| format!("failed to generate remote backup id: {error}"))?;
    Ok(format!(
        "{}-{}",
        Local::now().format("%Y%m%d-%H%M%S"),
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn random_download_file_name() -> Result<String, String> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| format!("failed to generate remote download file name: {error}"))?;
    Ok(format!(
        ".remote-download-{}.zip",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn remote_backup_file_name(id: &str) -> String {
    format!("Patina-backup-{id}.zip")
}

fn validate_backup_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("WebDAV backup id is invalid".to_string());
    }
    Ok(())
}

fn is_managed_download_file_name(file_name: &str) -> bool {
    let Some(random) = file_name
        .strip_prefix(".remote-download-")
        .and_then(|value| value.strip_suffix(".zip"))
    else {
        return false;
    };
    random.len() == 32
        && random
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn remote_path(remote_dir: &str, file_name: &str) -> String {
    format!("{remote_dir}/{file_name}")
}

fn index_path(remote_dir: &str) -> String {
    remote_path(remote_dir, INDEX_FILE_NAME)
}

fn parse_index(raw: &str) -> Result<RemoteBackupIndex, String> {
    let index: RemoteBackupIndex = serde_json::from_str(raw)
        .map_err(|error| format!("failed to parse WebDAV backup index: {error}"))?;
    if index.version != INDEX_VERSION {
        return Err(format!(
            "unsupported WebDAV backup index version {}",
            index.version
        ));
    }
    if index.product != INDEX_PRODUCT {
        return Err("WebDAV backup index belongs to another product".to_string());
    }
    Ok(index)
}

fn validate_index(index: &RemoteBackupIndex, remote_dir: &str) -> Result<(), String> {
    let mut ids = HashSet::with_capacity(index.backups.len());
    for entry in &index.backups {
        validate_backup_id(&entry.id)
            .map_err(|_| "WebDAV backup index contains an invalid backup id".to_string())?;
        if !ids.insert(entry.id.as_str()) {
            return Err("WebDAV backup index contains duplicate backup ids".to_string());
        }
        let expected_file_name = remote_backup_file_name(&entry.id);
        if entry.file_name != expected_file_name
            || entry.remote_path != remote_path(remote_dir, &expected_file_name)
        {
            return Err("WebDAV backup index contains an unsafe backup path".to_string());
        }
        if entry.size_bytes > crate::domain::backup::MAX_BACKUP_ARCHIVE_BYTES {
            return Err("WebDAV backup index contains an oversized backup".to_string());
        }
    }
    Ok(())
}

fn empty_index() -> RemoteBackupIndex {
    RemoteBackupIndex {
        version: INDEX_VERSION,
        product: INDEX_PRODUCT.to_string(),
        updated_at_ms: now_ms(),
        backups: Vec::new(),
    }
}

async fn load_index(client: &WebDavClient, remote_dir: &str) -> Result<RemoteBackupIndex, String> {
    match client.read_text_optional(&index_path(remote_dir)).await? {
        Some(raw) => parse_index(&raw),
        None => Ok(empty_index()),
    }
}

async fn save_index(
    client: &WebDavClient,
    remote_dir: &str,
    index: &RemoteBackupIndex,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index)
        .map_err(|error| format!("failed to serialize WebDAV backup index: {error}"))?;
    client.write_text(&index_path(remote_dir), &raw).await
}

fn temp_backup_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = storage_paths::resolve_storage_paths(app)?.remote_backup_temp_dir;
    ensure_temp_backup_dir(&dir)?;
    Ok(dir)
}

fn temp_backup_path(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    Ok(temp_backup_dir(app)?.join(file_name))
}

fn ensure_temp_backup_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("failed to create temp backup dir: {error}"))?;
    let metadata = fs::symlink_metadata(dir)
        .map_err(|error| format!("failed to inspect temp backup dir: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("remote backup temp path must be a real directory".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect temp backup dir: {error}"))?;
    }
    Ok(())
}

struct TempBackupGuard(Option<PathBuf>);

impl TempBackupGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn persist(mut self) {
        self.0 = None;
    }
}

impl Drop for TempBackupGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.as_ref() {
            let _ = fs::remove_file(path);
        }
    }
}

fn validate_downloaded_entry(
    entry: &RemoteBackupEntry,
    preview: &BackupPreview,
    size_bytes: u64,
) -> Result<(), String> {
    if entry.size_bytes != size_bytes
        || entry.app_version != preview.app_version
        || entry.backup_version != preview.version
        || entry.schema_version != preview.schema_version
        || entry.session_count != preview.session_count
        || entry.title_sample_count != preview.title_sample_count
        || entry.setting_count != preview.setting_count
        || entry.icon_cache_count != preview.icon_cache_count
    {
        return Err(
            "downloaded WebDAV backup does not match its validated index metadata".to_string(),
        );
    }
    Ok(())
}

fn build_entry(
    id: String,
    file_name: String,
    remote_path: String,
    size_bytes: u64,
    preview: &BackupPreview,
) -> RemoteBackupEntry {
    RemoteBackupEntry {
        id,
        file_name,
        remote_path,
        created_at_ms: now_ms(),
        size_bytes,
        app_version: preview.app_version.clone(),
        backup_version: preview.version,
        schema_version: preview.schema_version,
        session_count: preview.session_count,
        title_sample_count: preview.title_sample_count,
        setting_count: preview.setting_count,
        icon_cache_count: preview.icon_cache_count,
    }
}

async fn webdav_client(
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
) -> Result<(WebDavConfig, WebDavClient), String> {
    let config = config_to_webdav(config)?;
    let password = credentials::read_webdav_backup_password(profile)
        .await?
        .ok_or_else(|| "WebDAV password is missing".to_string())?;
    let client = WebDavClient::new(&config, password)?;
    Ok((config, client))
}

async fn webdav_client_with_password(
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
    password: Option<String>,
) -> Result<(WebDavConfig, WebDavClient), String> {
    let config = config_to_webdav(config)?;
    let password = match password {
        Some(password) if !password.is_empty() => password,
        _ => credentials::read_webdav_backup_password(profile)
            .await?
            .ok_or_else(|| "WebDAV password is missing".to_string())?,
    };
    let client = WebDavClient::new(&config, password)?;
    Ok((config, client))
}

pub async fn save_webdav_backup_secret(
    profile: crate::platform::app_paths::AppProfile,
    username: String,
    password: String,
) -> Result<(), String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("WebDAV username cannot be empty".to_string());
    }
    if password.is_empty() {
        return Err("WebDAV password cannot be empty".to_string());
    }
    credentials::save_webdav_backup_password(profile, username, &password).await
}

pub async fn delete_webdav_backup_secret(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<(), String> {
    credentials::delete_webdav_backup_password(profile).await
}

pub async fn has_webdav_backup_secret(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<bool, String> {
    credentials::has_webdav_backup_password(profile).await
}

pub async fn reveal_webdav_backup_secret(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<Option<String>, String> {
    credentials::read_webdav_backup_password(profile).await
}

pub async fn test_webdav_backup_target(
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
    password: Option<String>,
) -> Result<WebDavTestResult, String> {
    let (config, client) = webdav_client_with_password(profile, config, password).await?;
    client.ping(&config.remote_dir).await?;
    Ok(WebDavTestResult { ok: true })
}

pub async fn upload_webdav_backup(
    app: AppHandle,
    config: WebDavBackupConfig,
) -> Result<RemoteBackupUploadResult, String> {
    let profile = crate::platform::app_paths::app_profile(&app);
    let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let temp_dir = temp_backup_dir(&app)?;
    upload_webdav_backup_from_pool(&pool, &temp_dir, profile, config).await
}

pub async fn upload_webdav_backup_from_pool(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    temp_dir: &Path,
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
) -> Result<RemoteBackupUploadResult, String> {
    let (config, client) = webdav_client(profile, config).await?;
    client.ensure_dir(&config.remote_dir).await?;
    ensure_temp_backup_dir(temp_dir)?;

    let id = remote_backup_id()?;
    let file_name = remote_backup_file_name(&id);
    let local_path = temp_dir.join(&file_name);
    let _temp_guard = TempBackupGuard::new(local_path.clone());
    backup::export_backup_from_pool(pool, &local_path).await?;
    let (preview, _, size_bytes) = backup::inspect_restore_archive(&local_path)?;
    let remote_path = remote_path(&config.remote_dir, &file_name);

    client.upload_file(&local_path, &remote_path).await?;

    let entry = build_entry(id, file_name, remote_path, size_bytes, &preview);
    let mut result = match load_index(&client, &config.remote_dir).await {
        Ok(mut index) if validate_index(&index, &config.remote_dir).is_ok() => {
            index.backups.retain(|item| item.id != entry.id);
            index.backups.insert(0, entry.clone());
            index
                .backups
                .sort_by_key(|entry| Reverse(entry.created_at_ms));
            index.backups.truncate(MAX_BACKUP_LIST_ITEMS);
            index.updated_at_ms = now_ms();
            match save_index(&client, &config.remote_dir, &index).await {
                Ok(()) => RemoteBackupUploadResult {
                    entry,
                    index_updated: true,
                    index_message: None,
                },
                Err(error) => RemoteBackupUploadResult {
                    entry,
                    index_updated: false,
                    index_message: Some(error),
                },
            }
        }
        Ok(index) => RemoteBackupUploadResult {
            entry,
            index_updated: false,
            index_message: validate_index(&index, &config.remote_dir).err(),
        },
        Err(error) => RemoteBackupUploadResult {
            entry,
            index_updated: false,
            index_message: Some(error),
        },
    };
    let last_backup_at_ms = result.entry.created_at_ms.to_string();
    if let Err(error) = crate::data::repositories::app_settings::commit_app_setting_mutations(
        pool,
        &[
            crate::data::repositories::app_settings::AppSettingMutation {
                key: "webdav_backup_last_backup_at_ms".to_string(),
                value: last_backup_at_ms,
            },
        ],
    )
    .await
    {
        let warning =
            format!("remote backup uploaded, but local completion state was not saved: {error}");
        result.index_message = Some(match result.index_message.take() {
            Some(existing) => format!("{existing}; {warning}"),
            None => warning,
        });
    }
    Ok(result)
}

pub async fn list_webdav_backups(
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
) -> Result<Vec<RemoteBackupEntry>, String> {
    let (config, client) = webdav_client(profile, config).await?;
    let mut index = load_index(&client, &config.remote_dir).await?;
    validate_index(&index, &config.remote_dir)?;
    index
        .backups
        .sort_by_key(|entry| Reverse(entry.created_at_ms));
    index.backups.truncate(MAX_BACKUP_LIST_ITEMS);
    Ok(index.backups)
}

pub async fn download_webdav_backup(
    app: AppHandle,
    config: WebDavBackupConfig,
    id: String,
) -> Result<RemoteBackupDownloadResult, String> {
    let trimmed_id = id.trim();
    validate_backup_id(trimmed_id)?;

    let profile = crate::platform::app_paths::app_profile(&app);
    let (config, client) = webdav_client(profile, config).await?;
    let index = load_index(&client, &config.remote_dir).await?;
    validate_index(&index, &config.remote_dir)?;
    let entry = index
        .backups
        .iter()
        .find(|entry| entry.id == trimmed_id)
        .ok_or_else(|| "remote backup was not found in the WebDAV index".to_string())?;
    let local_path = temp_backup_path(&app, &random_download_file_name()?)?;
    client
        .download_file_bounded(
            &entry.remote_path,
            &local_path,
            crate::domain::backup::MAX_BACKUP_ARCHIVE_BYTES,
        )
        .await?;
    let temp_guard = TempBackupGuard::new(local_path.clone());
    let local_path_string = local_path.to_string_lossy().to_string();
    let (preview, _, size_bytes) = backup::inspect_restore_archive(&local_path)?;
    validate_downloaded_entry(entry, &preview, size_bytes)?;
    temp_guard.persist();
    Ok(RemoteBackupDownloadResult {
        path: local_path_string,
        preview,
    })
}

pub fn discard_downloaded_webdav_backup(app: &AppHandle, path: &str) -> Result<(), String> {
    let root = temp_backup_dir(app)?;
    let candidate = PathBuf::from(path);
    let file_name = candidate
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "remote backup download path is invalid".to_string())?;
    if candidate.parent() != Some(root.as_path()) || !is_managed_download_file_name(file_name) {
        return Err("refusing to remove an unmanaged remote backup download".to_string());
    }
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("remote backup download must be a regular non-symlink file".to_string())
        }
        Ok(_) => fs::remove_file(&candidate)
            .map_err(|error| format!("failed to remove remote backup download: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect remote backup download: {error}")),
    }
}

pub async fn stage_webdav_backup_for_restore(
    profile: crate::platform::app_paths::AppProfile,
    config: WebDavBackupConfig,
    id: String,
    temp_dir: &Path,
    restore_staging_dir: &Path,
) -> Result<crate::platform::backup_restore_staging::StagedBackupArchive, String> {
    let trimmed_id = id.trim();
    validate_backup_id(trimmed_id)?;
    let (config, client) = webdav_client(profile, config).await?;
    stage_webdav_backup_with_client(
        &client,
        &config.remote_dir,
        trimmed_id,
        temp_dir,
        restore_staging_dir,
    )
    .await
}

async fn stage_webdav_backup_with_client(
    client: &WebDavClient,
    remote_dir: &str,
    id: &str,
    temp_dir: &Path,
    restore_staging_dir: &Path,
) -> Result<crate::platform::backup_restore_staging::StagedBackupArchive, String> {
    validate_backup_id(id)?;
    let index = load_index(client, remote_dir).await?;
    validate_index(&index, remote_dir)?;
    let entry = index
        .backups
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "remote backup was not found in the WebDAV index".to_string())?;

    ensure_temp_backup_dir(temp_dir)?;
    let local_path = temp_dir.join(random_download_file_name()?);
    let _temp_guard = TempBackupGuard::new(local_path.clone());
    client
        .download_file_bounded(
            &entry.remote_path,
            &local_path,
            crate::domain::backup::MAX_BACKUP_ARCHIVE_BYTES,
        )
        .await?;
    let local_path_for_inspection = local_path.clone();
    let (preview, _, size_bytes) = tokio::task::spawn_blocking(move || {
        backup::inspect_restore_archive(&local_path_for_inspection)
    })
    .await
    .map_err(|error| format!("remote backup inspection task failed: {error}"))??;
    validate_downloaded_entry(entry, &preview, size_bytes)?;

    let staging_root = restore_staging_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        crate::platform::backup_restore_staging::stage_file(&staging_root, &local_path)
    })
    .await
    .map_err(|error| format!("remote backup staging task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_temp_backup_dir, is_managed_download_file_name, parse_index,
        remote_backup_file_name, remote_backup_id, remote_path, validate_downloaded_entry,
        validate_index,
    };
    use crate::domain::backup::BackupPreview;
    use crate::domain::remote_backup::RemoteBackupEntry;

    fn matching_entry_and_preview() -> (RemoteBackupEntry, BackupPreview) {
        let entry = RemoteBackupEntry {
            id: "safe-id".to_string(),
            file_name: "Patina-backup-safe-id.zip".to_string(),
            remote_path: "/Patina/Patina-backup-safe-id.zip".to_string(),
            created_at_ms: 1,
            size_bytes: 42,
            app_version: "1.8.4".to_string(),
            backup_version: 1,
            schema_version: 10,
            session_count: 2,
            title_sample_count: 3,
            setting_count: 4,
            icon_cache_count: 5,
        };
        let preview = BackupPreview {
            version: 1,
            exported_at_ms: 1,
            schema_version: 10,
            app_version: "1.8.4".to_string(),
            restore_supported: true,
            restore_message_key: "backup.restore.compatible".to_string(),
            restore_message_args: Vec::new(),
            restore_message: "compatible".to_string(),
            session_count: 2,
            title_sample_count: 3,
            setting_count: 4,
            icon_cache_count: 5,
            web_activity_segment_count: 0,
            tool_reminder_count: 0,
            tool_timer_count: 0,
            tool_timer_lap_count: 0,
            tool_pomodoro_run_count: 0,
            tool_daily_stats_count: 0,
            import_batch_count: 0,
            import_exact_session_count: 0,
            import_time_bucket_count: 0,
        };
        (entry, preview)
    }

    #[test]
    fn remote_file_name_uses_zip_format() {
        assert_eq!(
            remote_backup_file_name("20260603-213000"),
            "Patina-backup-20260603-213000.zip"
        );
    }

    #[test]
    fn generated_remote_backup_id_has_timestamp_and_random_suffix() {
        let id = remote_backup_id().unwrap();
        let bytes = id.as_bytes();

        assert_eq!(bytes.len(), 24);
        assert_eq!(bytes[8], b'-');
        assert_eq!(bytes[15], b'-');
        assert!(bytes[..8].iter().all(u8::is_ascii_digit));
        assert!(bytes[9..15].iter().all(u8::is_ascii_digit));
        assert!(bytes[16..].iter().all(u8::is_ascii_hexdigit));
    }

    #[cfg(unix)]
    #[test]
    fn temp_backup_dir_is_private_and_rejects_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let test_id = remote_backup_id().unwrap();
        let root = std::env::temp_dir().join(format!("patina-remote-backup-{test_id}"));
        let private_dir = root.join("private");
        ensure_temp_backup_dir(&private_dir).unwrap();
        assert_eq!(
            std::fs::symlink_metadata(&private_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );

        let linked_dir = root.join("linked");
        symlink(&private_dir, &linked_dir).unwrap();
        assert!(ensure_temp_backup_dir(&linked_dir).is_err());

        std::fs::remove_file(linked_dir).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remote_path_joins_normalized_dir_and_file() {
        assert_eq!(
            remote_path("/Patina/backups", "backup.zip"),
            "/Patina/backups/backup.zip"
        );
    }

    #[test]
    fn managed_download_names_require_the_exact_random_shape() {
        assert!(is_managed_download_file_name(
            ".remote-download-0123456789abcdef0123456789abcdef.zip"
        ));
        assert!(!is_managed_download_file_name(".remote-download-other.zip"));
        assert!(!is_managed_download_file_name(
            ".remote-download-0123456789abcdef0123456789abcdef.zip.old"
        ));
    }

    #[test]
    fn parse_index_rejects_time_tracker_product() {
        let raw = r#"{"version":1,"product":"Time Tracker","updatedAtMs":1,"backups":[]}"#;
        assert!(parse_index(raw).is_err());
    }

    #[test]
    fn parse_index_rejects_other_products() {
        let raw = r#"{"version":1,"product":"Other","updatedAtMs":1,"backups":[]}"#;
        assert!(parse_index(raw).is_err());
    }

    #[test]
    fn remote_index_rejects_paths_not_derived_from_the_backup_id() {
        let index = parse_index(
            r#"{
                "version": 1,
                "product": "Patina",
                "updatedAtMs": 1,
                "backups": [{
                    "id": "safe-id",
                    "fileName": "../other.zip",
                    "remotePath": "/Patina/../other.zip",
                    "createdAtMs": 1,
                    "sizeBytes": 1,
                    "appVersion": "1.8.4",
                    "backupVersion": 1,
                    "schemaVersion": 1,
                    "sessionCount": 0,
                    "titleSampleCount": 0,
                    "settingCount": 0,
                    "iconCacheCount": 0
                }]
            }"#,
        )
        .unwrap();

        assert!(validate_index(&index, "/Patina").is_err());
    }

    #[test]
    fn downloaded_backup_must_match_the_confirmed_index_metadata() {
        let (entry, mut preview) = matching_entry_and_preview();
        assert!(validate_downloaded_entry(&entry, &preview, 42).is_ok());

        preview.session_count += 1;
        assert!(validate_downloaded_entry(&entry, &preview, 42).is_err());
        preview.session_count -= 1;
        assert!(validate_downloaded_entry(&entry, &preview, 41).is_err());
    }
}
