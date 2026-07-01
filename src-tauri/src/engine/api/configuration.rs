use crate::data::app_settings_service::{
    delete_legacy_local_api_token_with_recovery, save_local_api_port_with_recovery,
};
use crate::domain::settings::{parse_local_api_port, LocalApiSettings};
use crate::engine::api::{auth, server::ApiServerState};
use tauri::AppHandle;

pub async fn initialize(
    app: &AppHandle,
    server: &ApiServerState,
    stored: LocalApiSettings,
) -> Result<(), String> {
    auth::initialize_api_token(Some(&stored.token))?;
    if !stored.token.trim().is_empty() {
        if let Err(error) = delete_legacy_local_api_token_with_recovery(app).await {
            eprintln!("[api] failed to remove migrated legacy token setting: {error}");
        }
    }

    let prepared = server.prepare_listener(stored.port).await?;
    server.install_prepared(app.clone(), prepared);
    Ok(())
}

pub async fn apply_port(
    app: &AppHandle,
    server: &ApiServerState,
    requested_port: u16,
) -> Result<LocalApiSettings, String> {
    let port = parse_local_api_port(&requested_port.to_string())
        .ok_or_else(|| "invalid local API port".to_string())?;

    if server.confirmed_port() == Some(port) {
        save_local_api_port_with_recovery(app, port).await?;
        return Ok(current_settings(port));
    }

    let prepared = server.prepare_listener(port).await?;
    save_local_api_port_with_recovery(app, port).await?;
    server.install_prepared(app.clone(), prepared);
    Ok(current_settings(port))
}

pub fn rotate_token(port: u16) -> Result<LocalApiSettings, String> {
    let token = auth::rotate_api_token()?;
    Ok(LocalApiSettings { port, token })
}

fn current_settings(port: u16) -> LocalApiSettings {
    LocalApiSettings {
        port,
        token: auth::get_api_token(),
    }
}
