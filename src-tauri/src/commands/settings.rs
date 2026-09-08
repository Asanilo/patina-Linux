use crate::app::desktop_behavior;
use crate::app::state::DesktopBehaviorState;
use crate::data::app_settings_service::commit_app_setting_mutations_with_recovery;
use crate::data::classification_service::commit_classification_setting_mutations_with_recovery;
use crate::data::repositories::app_settings::AppSettingMutation;
use crate::data::repositories::classification_settings::ClassificationSettingMutation;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingMutationDto {
    key: String,
    value: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationSettingMutationDto {
    key: String,
    value: Option<String>,
}

impl From<AppSettingMutationDto> for AppSettingMutation {
    fn from(value: AppSettingMutationDto) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

impl From<ClassificationSettingMutationDto> for ClassificationSettingMutation {
    fn from(value: ClassificationSettingMutationDto) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

#[tauri::command]
pub fn cmd_set_desktop_behavior(
    close_behavior: String,
    minimize_behavior: String,
    app: AppHandle,
    desktop_behavior_state: State<DesktopBehaviorState>,
) -> Result<(), String> {
    desktop_behavior::set_desktop_behavior(
        &app,
        &desktop_behavior_state,
        &close_behavior,
        &minimize_behavior,
    );
    Ok(())
}

#[tauri::command]
pub fn cmd_set_launch_behavior(
    launch_at_login: bool,
    start_minimized: bool,
    app: AppHandle,
    desktop_behavior_state: State<DesktopBehaviorState>,
) -> Result<(), String> {
    desktop_behavior::set_launch_behavior(
        &app,
        &desktop_behavior_state,
        launch_at_login,
        start_minimized,
    )
}

#[tauri::command]
pub fn cmd_set_background_optimization(
    background_optimization: bool,
    desktop_behavior_state: State<DesktopBehaviorState>,
) -> Result<(), String> {
    desktop_behavior::set_background_optimization(&desktop_behavior_state, background_optimization);
    Ok(())
}

#[tauri::command]
pub async fn cmd_set_audio_participation_enabled(
    enabled: bool,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .set_audio_participation_enabled(enabled)
            .await
            .map_err(|error| error.to_string());
    }
    #[cfg(target_os = "linux")]
    crate::platform::linux::audio::set_signal_source_enabled(enabled);
    #[cfg(target_os = "windows")]
    crate::platform::windows::audio::set_signal_source_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn cmd_apply_local_api_port(
    port: u16,
    app: AppHandle,
    api_server_state: State<'_, crate::engine::api::server::ApiServerState>,
    api_credentials: State<'_, crate::engine::api::auth::ApiCredentialStore>,
) -> Result<crate::commands::diagnostics::LocalApiSettingsSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        let result = client
            .apply_local_api_port(port)
            .await
            .map_err(|error| error.to_string())?;
        let token = api_credentials.token()?;
        app.state::<crate::app::daemon_client::PatinadClientState>()
            .replace_configuration(result.configuration.port, token.clone())?;
        return local_api_settings_snapshot(
            &api_credentials,
            crate::domain::settings::LocalApiSettings {
                port: result.configuration.port,
                token,
            },
        );
    }
    let settings = crate::engine::api::configuration::apply_port(
        &app,
        &api_server_state,
        &api_credentials,
        port,
    )
    .await?;
    local_api_settings_snapshot(&api_credentials, settings)
}

#[tauri::command]
pub async fn cmd_rotate_local_api_token(
    app: AppHandle,
    api_credentials: State<'_, crate::engine::api::auth::ApiCredentialStore>,
) -> Result<crate::commands::diagnostics::LocalApiSettingsSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        let result = client
            .rotate_local_api_token()
            .await
            .map_err(|error| error.to_string())?;
        let token_path = api_credentials.token_path()?;
        let token = api_credentials.load_existing_at(&token_path)?;
        app.state::<crate::app::daemon_client::PatinadClientState>()
            .replace_configuration(result.configuration.port, token.clone())?;
        return local_api_settings_snapshot(
            &api_credentials,
            crate::domain::settings::LocalApiSettings {
                port: result.configuration.port,
                token,
            },
        );
    }
    let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let stored = crate::data::repositories::app_settings::load_local_api_settings(&pool)
        .await
        .map_err(|error| format!("failed to load local API settings: {error}"))?;
    let settings = crate::engine::api::configuration::rotate_token(&api_credentials, stored.port)?;
    local_api_settings_snapshot(&api_credentials, settings)
}

fn local_api_settings_snapshot(
    api_credentials: &crate::engine::api::auth::ApiCredentialStore,
    settings: crate::domain::settings::LocalApiSettings,
) -> Result<crate::commands::diagnostics::LocalApiSettingsSnapshot, String> {
    let token_path = api_credentials.token_path()?;
    Ok(crate::commands::diagnostics::LocalApiSettingsSnapshot {
        port: settings.port,
        token: settings.token,
        token_path: token_path.display().to_string(),
        base_url: format!("http://127.0.0.1:{}", settings.port),
    })
}

#[tauri::command]
pub async fn cmd_commit_app_settings(
    mutations: Vec<AppSettingMutationDto>,
    app: AppHandle,
) -> Result<(), String> {
    let mut mutations = mutations
        .into_iter()
        .map(AppSettingMutation::from)
        .collect::<Vec<_>>();
    if mutations
        .iter()
        .any(|mutation| mutation.key == "background_tracking_at_login")
    {
        return Err(
            "background tracking login preference requires the dedicated service command"
                .to_string(),
        );
    }

    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        mutations =
            crate::app::daemon_client::route_owned_app_settings(&app, &client, mutations).await?;
        if !mutations.is_empty() {
            let daemon_mutations = mutations
                .into_iter()
                .map(
                    |mutation| crate::engine::api::types::AppSettingMutationRequest {
                        key: mutation.key,
                        value: mutation.value,
                    },
                )
                .collect();
            client
                .commit_app_settings(daemon_mutations)
                .await
                .map_err(|error| error.to_string())?;
        }
        app.emit("app-settings-changed", json!({}))
            .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
        return Ok(());
    }

    if !mutations.is_empty() {
        let changes_tracking_policy = mutations.iter().any(|mutation| {
            matches!(
                mutation.key.as_str(),
                "tracking_paused"
                    | "web_activity_enabled"
                    | "web_activity_token"
                    | "web_activity_port"
            )
        });
        let runtime_state = app
            .state::<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>()
            .inner()
            .clone();
        let _transition_guard = if changes_tracking_policy {
            Some(runtime_state.lock_transition().await)
        } else {
            None
        };
        commit_app_setting_mutations_with_recovery(&app, &mutations).await?;
        if changes_tracking_policy {
            runtime_state.note_tracking_policy_change();
        }
    }
    app.emit("app-settings-changed", json!({}))
        .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_commit_classification_settings(
    mutations: Vec<ClassificationSettingMutationDto>,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        let mutations = mutations
            .into_iter()
            .map(
                |mutation| crate::engine::api::types::ClassificationMutationRequest {
                    key: mutation.key,
                    value: mutation.value,
                },
            )
            .collect();
        return client
            .commit_classification_settings(mutations)
            .await
            .map_err(|error| error.to_string());
    }
    let mutations = mutations
        .into_iter()
        .map(ClassificationSettingMutation::from)
        .collect::<Vec<_>>();

    commit_classification_setting_mutations_with_recovery(&app, &mutations).await
}
