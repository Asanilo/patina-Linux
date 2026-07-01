use crate::app::desktop_behavior;
use crate::app::state::DesktopBehaviorState;
use crate::data::app_settings_service::commit_app_setting_mutations_with_recovery;
use crate::data::classification_service::commit_classification_setting_mutations_with_recovery;
use crate::data::repositories::app_settings::AppSettingMutation;
use crate::data::repositories::classification_settings::ClassificationSettingMutation;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

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
pub fn cmd_set_audio_participation_enabled(enabled: bool) -> Result<(), String> {
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
) -> Result<crate::commands::diagnostics::LocalApiSettingsSnapshot, String> {
    let settings =
        crate::engine::api::configuration::apply_port(&app, &api_server_state, port).await?;
    Ok(local_api_settings_snapshot(settings))
}

#[tauri::command]
pub async fn cmd_rotate_local_api_token(
    app: AppHandle,
) -> Result<crate::commands::diagnostics::LocalApiSettingsSnapshot, String> {
    let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let stored = crate::data::repositories::app_settings::load_local_api_settings(&pool)
        .await
        .map_err(|error| format!("failed to load local API settings: {error}"))?;
    let settings = crate::engine::api::configuration::rotate_token(stored.port)?;
    Ok(local_api_settings_snapshot(settings))
}

fn local_api_settings_snapshot(
    settings: crate::domain::settings::LocalApiSettings,
) -> crate::commands::diagnostics::LocalApiSettingsSnapshot {
    let token_path = crate::engine::api::auth::token_file_path();
    crate::commands::diagnostics::LocalApiSettingsSnapshot {
        port: settings.port,
        token: settings.token,
        token_path: token_path.display().to_string(),
        base_url: format!("http://127.0.0.1:{}", settings.port),
    }
}

#[tauri::command]
pub async fn cmd_commit_app_settings(
    mutations: Vec<AppSettingMutationDto>,
    app: AppHandle,
) -> Result<(), String> {
    let mutations = mutations
        .into_iter()
        .map(AppSettingMutation::from)
        .collect::<Vec<_>>();

    commit_app_setting_mutations_with_recovery(&app, &mutations).await?;
    app.emit("app-settings-changed", json!({}))
        .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_commit_classification_settings(
    mutations: Vec<ClassificationSettingMutationDto>,
    app: AppHandle,
) -> Result<(), String> {
    let mutations = mutations
        .into_iter()
        .map(ClassificationSettingMutation::from)
        .collect::<Vec<_>>();

    commit_classification_setting_mutations_with_recovery(&app, &mutations).await
}
