use crate::data::app_settings_service::{
    delete_legacy_local_api_token_with_recovery, save_local_api_port_with_recovery,
};
use crate::domain::settings::{parse_local_api_port, LocalApiSettings};
use crate::engine::api::{auth, server::ApiServerState};
use tauri::AppHandle;

pub async fn initialize(
    app: &AppHandle,
    server: &ApiServerState,
    credentials: &auth::ApiCredentialStore,
    token_path: &std::path::Path,
    stored: LocalApiSettings,
) -> Result<(), String> {
    credentials.initialize_at(token_path, Some(&stored.token))?;
    if !stored.token.trim().is_empty() {
        if let Err(error) = delete_legacy_local_api_token_with_recovery(app).await {
            eprintln!("[api] failed to remove migrated legacy token setting: {error}");
        }
    }

    let prepared = server.prepare_listener(stored.port).await?;
    server.install_prepared(app.clone(), credentials.clone(), prepared);
    Ok(())
}

pub async fn apply_port(
    app: &AppHandle,
    server: &ApiServerState,
    credentials: &auth::ApiCredentialStore,
    requested_port: u16,
) -> Result<LocalApiSettings, String> {
    let port = parse_local_api_port(&requested_port.to_string())
        .ok_or_else(|| "invalid local API port".to_string())?;

    if server.confirmed_port() == Some(port) {
        save_local_api_port_with_recovery(app, port).await?;
        return current_settings(credentials, port);
    }

    let prepared = server.prepare_listener(port).await?;
    save_local_api_port_with_recovery(app, port).await?;
    server.install_prepared(app.clone(), credentials.clone(), prepared);
    current_settings(credentials, port)
}

pub fn rotate_token(
    credentials: &auth::ApiCredentialStore,
    port: u16,
) -> Result<LocalApiSettings, String> {
    let token = credentials.rotate()?;
    Ok(LocalApiSettings { port, token })
}

fn current_settings(
    credentials: &auth::ApiCredentialStore,
    port: u16,
) -> Result<LocalApiSettings, String> {
    Ok(LocalApiSettings {
        port,
        token: credentials.token()?,
    })
}
