use crate::domain::backup::BackupSetting;
use sqlx::{Row, Sqlite, Transaction};

const HOST_INTEGRATION_SETTING_KEYS: &[&str] = &[
    "local_api_port",
    "local_api_token",
    "web_activity_port",
    "web_activity_token",
    "remote_status_bridge_enabled",
    "remote_status_bridge_url",
    "remote_status_bridge_token",
    "remote_status_bridge_machine_id",
    "webdav_backup_url",
    "webdav_backup_username",
    "webdav_backup_remote_dir",
    "webdav_backup_last_backup_at_ms",
];

pub async fn fetch_all_for_backup<'e, E>(executor: E) -> Result<Vec<BackupSetting>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query("SELECT key, value FROM settings ORDER BY key ASC")
        .fetch_all(executor)
        .await
        .map_err(|error| format!("failed to read settings for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupSetting {
            key: row.get("key"),
            value: row.get("value"),
        })
        .collect())
}

pub async fn clear_for_restore(tx: &mut Transaction<'_, Sqlite>) -> Result<(), String> {
    sqlx::query("DELETE FROM settings")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear settings before restore: {error}"))?;
    Ok(())
}

pub async fn replace_for_restore_preserving_host_integrations(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &[BackupSetting],
) -> Result<(), String> {
    let preserved = fetch_host_integration_settings(&mut **tx).await?;
    clear_for_restore(tx).await?;
    insert_filtered_for_restore(tx, settings, false).await?;
    insert_for_restore(tx, &preserved).await
}

pub async fn insert_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &[BackupSetting],
) -> Result<(), String> {
    for setting in settings {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
            .bind(&setting.key)
            .bind(&setting.value)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("failed to restore settings: {error}"))?;
    }

    Ok(())
}

pub async fn insert_missing_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &[BackupSetting],
) -> Result<(), String> {
    insert_filtered_for_restore(tx, settings, true).await
}

async fn fetch_host_integration_settings<'e, E>(executor: E) -> Result<Vec<BackupSetting>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query("SELECT key, value FROM settings ORDER BY key ASC")
        .fetch_all(executor)
        .await
        .map_err(|error| format!("failed to preserve host integration settings: {error}"))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let key: String = row.get("key");
            is_host_integration_setting(&key).then(|| BackupSetting {
                key,
                value: row.get("value"),
            })
        })
        .collect())
}

async fn insert_filtered_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &[BackupSetting],
    only_if_missing: bool,
) -> Result<(), String> {
    for setting in settings {
        if is_host_integration_setting(&setting.key) {
            continue;
        }
        let query = if only_if_missing {
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)"
        } else {
            "INSERT INTO settings (key, value) VALUES (?, ?)"
        };
        sqlx::query(query)
            .bind(&setting.key)
            .bind(&setting.value)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("failed to restore settings: {error}"))?;
    }
    Ok(())
}

fn is_host_integration_setting(key: &str) -> bool {
    HOST_INTEGRATION_SETTING_KEYS.contains(&key)
}
