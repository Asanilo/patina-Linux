use crate::data::repositories::app_settings::{
    commit_app_setting_mutations, delete_legacy_local_api_token, save_local_api_port,
    AppSettingMutation,
};
use crate::data::sqlite_pool::{
    is_recoverable_sqlite_error, reopen_sqlite_pool, wait_for_sqlite_pool,
};
use tauri::{AppHandle, Runtime};

pub async fn commit_app_setting_mutations_with_recovery<R: Runtime>(
    app: &AppHandle<R>,
    mutations: &[AppSettingMutation],
) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    match commit_app_setting_mutations(&pool, mutations).await {
        Ok(()) => Ok(()),
        Err(error) if is_recoverable_sqlite_error(&error) => {
            let reopened_pool = reopen_sqlite_pool(app).await?;
            commit_app_setting_mutations(&reopened_pool, mutations).await
        }
        Err(error) => Err(error),
    }
}

pub async fn save_local_api_port_with_recovery<R: Runtime>(
    app: &AppHandle<R>,
    port: u16,
) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    match save_local_api_port(&pool, port).await {
        Ok(()) => Ok(()),
        Err(error) if is_recoverable_sqlite_error(&error) => {
            let reopened_pool = reopen_sqlite_pool(app).await?;
            save_local_api_port(&reopened_pool, port).await
        }
        Err(error) => Err(error),
    }
}

pub async fn delete_legacy_local_api_token_with_recovery<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    match delete_legacy_local_api_token(&pool).await {
        Ok(()) => Ok(()),
        Err(error) if is_recoverable_sqlite_error(&error) => {
            let reopened_pool = reopen_sqlite_pool(app).await?;
            delete_legacy_local_api_token(&reopened_pool).await
        }
        Err(error) => Err(error),
    }
}
