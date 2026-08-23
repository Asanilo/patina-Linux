use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, RouteResponse},
};
use serde_json::json;

use crate::domain::tools::TimerMode;
use crate::engine::api::types::{
    CreateReminderRequest, CreateSoftwareReminderRuleRequest, StartPomodoroRequest,
    StartTimerRequest,
};

const MAX_LABEL_CHARS: usize = 256;
const MAX_APP_CHARS: usize = 256;
const MAX_MESSAGE_CHARS: usize = 1_024;
const MINUTE_MS: i64 = 60_000;

pub async fn get_tools_snapshot(context: &ApiRuntimeContext) -> RouteResponse {
    match crate::engine::tools::get_snapshot_from_pool(context.pool(), context.now_ms()).await {
        Ok(snapshot) => RouteResponse {
            status: 200,
            body: json!({ "data": snapshot }),
        },
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}

pub async fn handle_tools_action(
    context: &ApiRuntimeContext,
    path: &str,
    body: &[u8],
) -> RouteResponse {
    if !context.tools_runtime_ready() {
        return unavailable();
    }
    let Some(owner) = context.tools_owner() else {
        return unavailable();
    };

    let result = match path {
        "/api/v1/tools/reminders" => {
            let request = match parse_body::<CreateReminderRequest>(body) {
                Ok(request) => request,
                Err(response) => return response,
            };
            if request.label.chars().count() > MAX_LABEL_CHARS {
                return bad_request("reminder label exceeds 256 characters");
            }
            if request.scheduled_at <= context.now_ms() {
                return bad_request("reminder time must be in the future");
            }
            owner
                .create_reminder(request.label, request.scheduled_at)
                .await
        }
        "/api/v1/tools/software-reminder-rules" => {
            let request = match parse_body::<CreateSoftwareReminderRuleRequest>(body) {
                Ok(request) => request,
                Err(response) => return response,
            };
            if request.app_name.trim().is_empty()
                || request.app_name.chars().count() > MAX_APP_CHARS
            {
                return bad_request("app_name must contain 1 to 256 characters");
            }
            if request
                .exe_name
                .as_ref()
                .is_some_and(|value| value.chars().count() > MAX_APP_CHARS)
            {
                return bad_request("exe_name exceeds 256 characters");
            }
            if request.message.chars().count() > MAX_MESSAGE_CHARS {
                return bad_request("message exceeds 1024 characters");
            }
            if !(MINUTE_MS..=1_440 * MINUTE_MS).contains(&request.limit_ms) {
                return bad_request("limit_ms must be between 60000 and 86400000");
            }
            owner
                .create_software_reminder_rule(
                    crate::engine::tools::CreateSoftwareReminderRuleRequest {
                        app_name: request.app_name,
                        exe_name: request.exe_name,
                        limit_ms: request.limit_ms,
                        message: request.message,
                    },
                )
                .await
        }
        "/api/v1/tools/timer/start" => {
            let request = match parse_body::<StartTimerRequest>(body) {
                Ok(request) => request,
                Err(response) => return response,
            };
            if request
                .label
                .as_ref()
                .is_some_and(|value| value.chars().count() > MAX_LABEL_CHARS)
            {
                return bad_request("timer label exceeds 256 characters");
            }
            if request.mode == TimerMode::Countdown
                && !request
                    .duration_ms
                    .is_some_and(|duration| (MINUTE_MS..=180 * MINUTE_MS).contains(&duration))
            {
                return bad_request("countdown duration_ms must be between 60000 and 10800000");
            }
            owner
                .start_timer(crate::engine::tools::StartTimerRequest {
                    mode: request.mode,
                    duration_ms: request.duration_ms,
                    label: request.label,
                })
                .await
        }
        "/api/v1/tools/pomodoro/start" => {
            let request = match parse_body::<StartPomodoroRequest>(body) {
                Ok(request) => request,
                Err(response) => return response,
            };
            if !(MINUTE_MS..=180 * MINUTE_MS).contains(&request.focus_ms) {
                return bad_request("focus_ms must be between 60000 and 10800000");
            }
            if !(MINUTE_MS..=60 * MINUTE_MS).contains(&request.short_break_ms) {
                return bad_request("short_break_ms must be between 60000 and 3600000");
            }
            if !(MINUTE_MS..=120 * MINUTE_MS).contains(&request.long_break_ms) {
                return bad_request("long_break_ms must be between 60000 and 7200000");
            }
            if !(2..=12).contains(&request.long_break_every) {
                return bad_request("long_break_every must be between 2 and 12");
            }
            owner
                .start_pomodoro(crate::engine::tools::StartPomodoroRequest {
                    focus_ms: request.focus_ms,
                    short_break_ms: request.short_break_ms,
                    long_break_ms: request.long_break_ms,
                    long_break_every: request.long_break_every,
                })
                .await
        }
        "/api/v1/tools/timer/pause" => owner.pause_timer().await,
        "/api/v1/tools/timer/resume" => owner.resume_timer().await,
        "/api/v1/tools/timer/reset" => owner.reset_timer().await,
        "/api/v1/tools/timer/laps" => owner.add_timer_lap().await,
        "/api/v1/tools/pomodoro/pause" => owner.pause_pomodoro().await,
        "/api/v1/tools/pomodoro/resume" => owner.resume_pomodoro().await,
        "/api/v1/tools/pomodoro/skip" => owner.skip_pomodoro_phase().await,
        "/api/v1/tools/pomodoro/reset" => owner.reset_pomodoro().await,
        _ => {
            if let Some(id) = action_id(path, "/api/v1/tools/reminders/", "/cancel") {
                owner.cancel_reminder(id).await
            } else if let Some(id) =
                action_id(path, "/api/v1/tools/software-reminder-rules/", "/disable")
            {
                owner.disable_software_reminder_rule(id).await
            } else {
                return not_found();
            }
        }
    };

    match result {
        Ok(snapshot) => RouteResponse {
            status: 200,
            body: json!({ "data": snapshot }),
        },
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, RouteResponse> {
    serde_json::from_slice(body).map_err(|_| bad_request("invalid JSON body"))
}

fn action_id(path: &str, prefix: &str, suffix: &str) -> Option<i64> {
    let value = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    if value.is_empty() || value.contains('/') {
        return None;
    }
    value.parse::<i64>().ok().filter(|id| *id > 0)
}

fn bad_request(message: &str) -> RouteResponse {
    RouteResponse {
        status: 400,
        body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
    }
}

fn unavailable() -> RouteResponse {
    RouteResponse {
        status: 503,
        body: serde_json::to_value(ApiError::unavailable(
            "Tools runtime is not owned by this API host",
        ))
        .unwrap_or_default(),
    }
}

fn not_found() -> RouteResponse {
    RouteResponse {
        status: 404,
        body: serde_json::to_value(ApiError::not_found("unknown Tools action")).unwrap_or_default(),
    }
}
