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
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const INDEX_FILE_NAME: &str = "backup-index.json";
const INDEX_VERSION: u32 = 1;
const INDEX_PRODUCT: &str = "Patina";
const MAX_BACKUP_LIST_ITEMS: usize = 50;

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

fn remote_backup_file_name(id: &str) -> String {
    format!("Patina-backup-{id}.zip")
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

struct TempBackupGuard(PathBuf);

impl Drop for TempBackupGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
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
    let _temp_guard = TempBackupGuard(local_path.clone());
    backup::export_backup_from_pool(pool, &local_path).await?;
    let (preview, _, size_bytes) = backup::inspect_restore_archive(&local_path)?;
    let remote_path = remote_path(&config.remote_dir, &file_name);

    client.upload_file(&local_path, &remote_path).await?;

    let entry = build_entry(id, file_name, remote_path, size_bytes, &preview);
    let mut result = match load_index(&client, &config.remote_dir).await {
        Ok(mut index) => {
            index.backups.retain(|item| item.id != entry.id);
            index.backups.insert(0, entry.clone());
            index
                .backups
                .sort_by_key(|entry| Reverse(entry.created_at_ms));
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
    if trimmed_id.is_empty() {
        return Err("remote backup id cannot be empty".to_string());
    }

    let profile = crate::platform::app_paths::app_profile(&app);
    let (config, client) = webdav_client(profile, config).await?;
    let index = load_index(&client, &config.remote_dir).await?;
    let entry = index
        .backups
        .iter()
        .find(|entry| entry.id == trimmed_id)
        .ok_or_else(|| "remote backup was not found in the WebDAV index".to_string())?;
    let local_path = temp_backup_path(&app, &entry.file_name)?;
    client
        .download_file(&entry.remote_path, &local_path)
        .await?;
    let local_path_string = local_path.to_string_lossy().to_string();
    let preview = backup::preview_backup(local_path_string.clone()).await?;
    Ok(RemoteBackupDownloadResult {
        path: local_path_string,
        preview,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_temp_backup_dir, parse_index, remote_backup_file_name, remote_backup_id, remote_path,
    };

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
    fn parse_index_rejects_time_tracker_product() {
        let raw = r#"{"version":1,"product":"Time Tracker","updatedAtMs":1,"backups":[]}"#;
        assert!(parse_index(raw).is_err());
    }

    #[test]
    fn parse_index_rejects_other_products() {
        let raw = r#"{"version":1,"product":"Other","updatedAtMs":1,"backups":[]}"#;
        assert!(parse_index(raw).is_err());
    }
}
