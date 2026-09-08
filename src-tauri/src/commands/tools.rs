use crate::domain::tools::{TimerMode, ToolAlert, ToolsRuntimeSnapshot};
use crate::engine::tools::{
    self, CreateSoftwareReminderRuleRequest, StartPomodoroRequest, StartTimerRequest,
};
use serde::Deserialize;
use tauri::AppHandle;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReminderDto {
    label: String,
    scheduled_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSoftwareReminderRuleDto {
    app_name: String,
    exe_name: Option<String>,
    limit_ms: i64,
    message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerDto {
    mode: TimerMode,
    duration_ms: Option<i64>,
    label: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPomodoroDto {
    focus_ms: i64,
    short_break_ms: i64,
    long_break_ms: i64,
    long_break_every: i64,
}

#[tauri::command]
pub async fn cmd_get_tools_snapshot(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .tools_snapshot()
            .await
            .map_err(|error| error.to_string());
    }
    tools::get_snapshot(&app).await
}

#[tauri::command]
pub fn cmd_get_tool_alerts(app: AppHandle) -> Vec<ToolAlert> {
    tools::get_alerts(&app)
}

#[tauri::command]
pub fn cmd_dismiss_tool_alert(alert_id: String, app: AppHandle) {
    tools::dismiss_alert(&app, &alert_id);
}

#[tauri::command]
pub async fn cmd_create_reminder(
    input: CreateReminderDto,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .create_reminder(crate::engine::api::types::CreateReminderRequest {
                label: input.label,
                scheduled_at: input.scheduled_at,
            })
            .await
            .map_err(|error| error.to_string());
    }
    tools::create_reminder(&app, input.label, input.scheduled_at).await
}

#[tauri::command]
pub async fn cmd_cancel_reminder(
    reminder_id: i64,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .cancel_reminder(reminder_id)
            .await
            .map_err(|error| error.to_string());
    }
    tools::cancel_reminder(&app, reminder_id).await
}

#[tauri::command]
pub async fn cmd_create_software_reminder_rule(
    input: CreateSoftwareReminderRuleDto,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .create_software_reminder_rule(
                crate::engine::api::types::CreateSoftwareReminderRuleRequest {
                    app_name: input.app_name,
                    exe_name: input.exe_name,
                    limit_ms: input.limit_ms,
                    message: input.message,
                },
            )
            .await
            .map_err(|error| error.to_string());
    }
    tools::create_software_reminder_rule(
        &app,
        CreateSoftwareReminderRuleRequest {
            app_name: input.app_name,
            exe_name: input.exe_name,
            limit_ms: input.limit_ms,
            message: input.message,
        },
    )
    .await
}

#[tauri::command]
pub async fn cmd_disable_software_reminder_rule(
    rule_id: i64,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .disable_software_reminder_rule(rule_id)
            .await
            .map_err(|error| error.to_string());
    }
    tools::disable_software_reminder_rule(&app, rule_id).await
}

#[tauri::command]
pub async fn cmd_start_timer(
    input: StartTimerDto,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .start_timer(crate::engine::api::types::StartTimerRequest {
                mode: input.mode,
                duration_ms: input.duration_ms,
                label: input.label,
            })
            .await
            .map_err(|error| error.to_string());
    }
    tools::start_timer(
        &app,
        StartTimerRequest {
            mode: input.mode,
            duration_ms: input.duration_ms,
            label: input.label,
        },
    )
    .await
}

#[tauri::command]
pub async fn cmd_pause_timer(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/timer/pause", "timer pause").await;
    }
    tools::pause_timer(&app).await
}

#[tauri::command]
pub async fn cmd_resume_timer(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/timer/resume", "timer resume").await;
    }
    tools::resume_timer(&app).await
}

#[tauri::command]
pub async fn cmd_reset_timer(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/timer/reset", "timer reset").await;
    }
    tools::reset_timer(&app).await
}

#[tauri::command]
pub async fn cmd_add_timer_lap(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/timer/laps", "timer lap").await;
    }
    tools::add_timer_lap(&app).await
}

#[tauri::command]
pub async fn cmd_start_pomodoro(
    input: StartPomodoroDto,
    app: AppHandle,
) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .start_pomodoro(crate::engine::api::types::StartPomodoroRequest {
                focus_ms: input.focus_ms,
                short_break_ms: input.short_break_ms,
                long_break_ms: input.long_break_ms,
                long_break_every: input.long_break_every,
            })
            .await
            .map_err(|error| error.to_string());
    }
    tools::start_pomodoro(
        &app,
        StartPomodoroRequest {
            focus_ms: input.focus_ms,
            short_break_ms: input.short_break_ms,
            long_break_ms: input.long_break_ms,
            long_break_every: input.long_break_every,
        },
    )
    .await
}

#[tauri::command]
pub async fn cmd_pause_pomodoro(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/pomodoro/pause", "Pomodoro pause")
            .await;
    }
    tools::pause_pomodoro(&app).await
}

#[tauri::command]
pub async fn cmd_resume_pomodoro(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/pomodoro/resume", "Pomodoro resume")
            .await;
    }
    tools::resume_pomodoro(&app).await
}

#[tauri::command]
pub async fn cmd_skip_pomodoro_phase(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(
            &client,
            "/api/v1/tools/pomodoro/skip",
            "Pomodoro phase skip",
        )
        .await;
    }
    tools::skip_pomodoro_phase(&app).await
}

#[tauri::command]
pub async fn cmd_reset_pomodoro(app: AppHandle) -> Result<ToolsRuntimeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return daemon_tools_action(&client, "/api/v1/tools/pomodoro/reset", "Pomodoro reset")
            .await;
    }
    tools::reset_pomodoro(&app).await
}

async fn daemon_tools_action(
    client: &crate::platform::daemon_client::PatinadClient,
    path: &str,
    response_name: &str,
) -> Result<ToolsRuntimeSnapshot, String> {
    client
        .tools_action(path, response_name)
        .await
        .map_err(|error| error.to_string())
}
