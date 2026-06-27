use crate::engine::api::types::RouteResponse;
use serde_json::{json, Value};

pub fn get_openapi() -> RouteResponse {
    RouteResponse {
        status: 200,
        body: json!({
            "openapi": "3.1.0",
            "info": {
                "title": "Patina Local API",
                "version": env!("CARGO_PKG_VERSION"),
                "description": "Local-first Patina API for scripts, MCP wrappers, and external AI analysis."
            },
            "servers": [
                {
                    "url": "http://127.0.0.1:{port}",
                    "description": "Patina localhost API. The port can be changed in Settings.",
                    "variables": {
                        "port": {
                            "default": "14840",
                            "description": "Configured Patina API port."
                        }
                    }
                }
            ],
            "security": [
                { "bearerAuth": [] }
            ],
            "components": {
                "securitySchemes": {
                    "bearerAuth": {
                        "type": "http",
                        "scheme": "bearer"
                    }
                },
                "schemas": schemas()
            },
            "paths": paths()
        }),
    }
}

fn paths() -> Value {
    json!({
        "/api/v1/health": {
            "get": get_operation("API health, app version, and platform.", "HealthResponse")
        },
        "/api/v1/openapi.json": {
            "get": get_operation("Machine-readable OpenAPI schema for the local API.", "OpenApiDocument")
        },
        "/api/v1/diagnostics": {
            "get": get_operation("Platform, tracker runtime, and browser bridge diagnostics.", "DiagnosticsResponse")
        },
        "/api/v1/current": {
            "get": get_operation("Current foreground window snapshot.", "CurrentWindowResponse")
        },
        "/api/v1/sessions": {
            "get": get_operation_with_parameters(
                "Closed session query by time range, app, and limit.",
                "SessionsResponse",
                vec![
                    query_param("from", "integer", "Optional lower start timestamp in milliseconds."),
                    query_param("to", "integer", "Optional upper start timestamp in milliseconds."),
                    query_param("app", "string", "Optional exact exe_name filter."),
                    query_param("limit", "integer", "Optional result limit. Defaults to 100."),
                ],
            )
        },
        "/api/v1/sessions/active": {
            "get": get_operation("Current active tracking session, if present.", "ActiveSessionResponse")
        },
        "/api/v1/summary/today": {
            "get": get_operation("Local-day activity summary.", "SummaryResponse")
        },
        "/api/v1/summary/range": {
            "get": get_operation_with_parameters(
                "Caller-provided millisecond range summary.",
                "SummaryResponse",
                vec![
                    required_query_param("from", "integer", "Required range start timestamp in milliseconds."),
                    required_query_param("to", "integer", "Required range end timestamp in milliseconds."),
                ],
            )
        },
        "/api/v1/summary/week": {
            "get": get_operation("Local-week activity summary.", "SummaryResponse")
        },
        "/api/v1/trend": {
            "get": get_operation_with_parameters(
                "Daily activity trend for week or month.",
                "TrendResponse",
                vec![
                    query_param("period", "string", "Optional period. Supported values: week, month."),
                    query_param("granularity", "string", "Optional granularity. Currently day."),
                ],
            )
        },
        "/api/v1/web-activity": {
            "get": get_operation_with_parameters(
                "Browser activity segments captured through the browser extension.",
                "WebActivityResponse",
                vec![
                    query_param("from", "integer", "Optional lower timestamp in milliseconds."),
                    query_param("to", "integer", "Optional upper timestamp in milliseconds."),
                    query_param("domain", "string", "Optional normalized domain filter."),
                    query_param("limit", "integer", "Optional result limit."),
                ],
            )
        },
        "/api/v1/ai/activity-context": {
            "get": get_operation("Aggregated local activity context for external AI analysis.", "ActivityContextResponse")
        },
        "/api/v1/apps": {
            "get": get_operation("Known apps from recorded sessions.", "AppsResponse")
        },
        "/api/v1/apps/{exe_name}/classify": {
            "post": post_operation(
                "Assign a category to an app.",
                vec![path_param("exe_name", "Exact app executable name.")],
                "ClassifyRequest",
                "OkResponse",
            )
        },
        "/api/v1/apps/{exe_name}/rename": {
            "post": post_operation(
                "Assign a display name to an app.",
                vec![path_param("exe_name", "Exact app executable name.")],
                "RenameRequest",
                "OkResponse",
            )
        },
        "/api/v1/apps/{exe_name}/exclude": {
            "post": post_operation(
                "Set an app exclusion flag.",
                vec![path_param("exe_name", "Exact app executable name.")],
                "ExcludeRequest",
                "OkResponse",
            )
        },
        "/api/v1/settings/tracker": {
            "get": get_operation("Tracker settings snapshot.", "TrackerSettingsResponse")
        },
        "/api/v1/settings/tracker/afk-threshold": {
            "post": post_operation(
                "Update idle timeout threshold.",
                vec![],
                "AfkThresholdRequest",
                "OkResponse",
            )
        },
        "/api/v1/tools/snapshot": {
            "get": get_operation("Current Tools runtime snapshot.", "ToolsSnapshotResponse")
        }
    })
}

fn schemas() -> Value {
    let mut schemas = serde_json::Map::new();

    schemas.insert("OpenApiDocument".to_string(), open_object_schema(vec![]));
    schemas.insert(
        "ApiError".to_string(),
        object_schema(vec![("error", schema_ref("ApiErrorDetail"))]),
    );
    schemas.insert(
        "ApiErrorDetail".to_string(),
        object_schema(vec![
            ("code", string_schema()),
            ("message", string_schema()),
        ]),
    );
    schemas.insert(
        "ComponentFailure".to_string(),
        object_schema(vec![("error", schema_ref("ApiError"))]),
    );
    schemas.insert(
        "OkResponse".to_string(),
        envelope(object_schema(vec![("ok", bool_schema())])),
    );
    schemas.insert(
        "HealthResponse".to_string(),
        envelope(object_schema(vec![
            ("status", string_schema()),
            ("version", string_schema()),
            ("platform", string_schema()),
        ])),
    );
    schemas.insert(
        "CurrentWindowResponse".to_string(),
        envelope(object_schema(vec![
            ("exe_name", string_schema()),
            ("title", string_schema()),
            ("process_id", integer_schema()),
            ("is_afk", bool_schema()),
            ("idle_time_ms", integer_schema()),
            ("process_path", string_schema()),
        ])),
    );
    schemas.insert(
        "SessionEntry".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("app_name", string_schema()),
            ("exe_name", string_schema()),
            ("window_title", nullable_string_schema()),
            ("start_time", integer_schema()),
            ("end_time", nullable_integer_schema()),
            ("duration", nullable_integer_schema()),
        ]),
    );
    schemas.insert(
        "SessionsResponse".to_string(),
        envelope(object_schema(vec![(
            "sessions",
            array_schema(schema_ref("SessionEntry")),
        )])),
    );
    schemas.insert(
        "ActiveSessionData".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("app_name", string_schema()),
            ("exe_name", string_schema()),
            ("window_title", nullable_string_schema()),
            ("start_time", integer_schema()),
            ("end_time", nullable_integer_schema()),
            ("duration", integer_schema()),
            ("continuity_group_start_time", integer_schema()),
            ("sampled_at_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "ActiveSessionResponse".to_string(),
        object_schema(vec![("data", nullable_ref_schema("ActiveSessionData"))]),
    );
    schemas.insert(
        "AppSummaryEntry".to_string(),
        object_schema(vec![
            ("exe_name", string_schema()),
            ("total_ms", integer_schema()),
            ("percentage", number_schema()),
        ]),
    );
    schemas.insert(
        "CategorySummaryEntry".to_string(),
        object_schema(vec![
            ("name", string_schema()),
            ("total_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "SummaryData".to_string(),
        object_schema(vec![
            ("date", string_schema()),
            ("total_active_ms", integer_schema()),
            ("apps", array_schema(schema_ref("AppSummaryEntry"))),
            (
                "categories",
                array_schema(schema_ref("CategorySummaryEntry")),
            ),
        ]),
    );
    schemas.insert(
        "SummaryResponse".to_string(),
        envelope(schema_ref("SummaryData")),
    );
    schemas.insert(
        "TrendDataPoint".to_string(),
        object_schema(vec![
            ("date", string_schema()),
            ("active_ms", integer_schema()),
            ("top_app", nullable_string_schema()),
        ]),
    );
    schemas.insert(
        "TrendData".to_string(),
        object_schema(vec![
            ("period", string_schema()),
            ("granularity", string_schema()),
            ("from_ms", integer_schema()),
            ("to_ms", integer_schema()),
            ("data_points", array_schema(schema_ref("TrendDataPoint"))),
        ]),
    );
    schemas.insert(
        "TrendResponse".to_string(),
        envelope(schema_ref("TrendData")),
    );
    schemas.insert(
        "WebActivityEntry".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("browser_client_id", string_schema()),
            ("browser_kind", string_schema()),
            ("browser_exe_name", string_schema()),
            ("domain", string_schema()),
            ("normalized_domain", string_schema()),
            ("url", nullable_string_schema()),
            ("title", nullable_string_schema()),
            ("favicon_url", nullable_string_schema()),
            ("start_time", integer_schema()),
            ("end_time", nullable_integer_schema()),
            ("duration", integer_schema()),
            ("source", string_schema()),
        ]),
    );
    schemas.insert(
        "WebActivityData".to_string(),
        object_schema(vec![(
            "items",
            array_schema(schema_ref("WebActivityEntry")),
        )]),
    );
    schemas.insert(
        "WebActivityResponse".to_string(),
        envelope(schema_ref("WebActivityData")),
    );
    schemas.insert(
        "AppEntry".to_string(),
        object_schema(vec![
            ("exe_name", string_schema()),
            ("display_name", string_schema()),
            ("category", nullable_string_schema()),
            ("excluded", bool_schema()),
        ]),
    );
    schemas.insert(
        "AppsData".to_string(),
        object_schema(vec![("apps", array_schema(schema_ref("AppEntry")))]),
    );
    schemas.insert("AppsResponse".to_string(), envelope(schema_ref("AppsData")));
    schemas.insert(
        "TrackerSettingsData".to_string(),
        object_schema(vec![
            ("idle_timeout_secs", integer_schema()),
            ("timeline_merge_gap_secs", integer_schema()),
            ("tracking_paused", bool_schema()),
        ]),
    );
    schemas.insert(
        "TrackerSettingsResponse".to_string(),
        envelope(schema_ref("TrackerSettingsData")),
    );
    schemas.insert(
        "WindowTrackingDiagnostics".to_string(),
        object_schema(vec![
            ("status", string_schema()),
            ("reason", nullable_string_schema()),
            ("provider", string_schema()),
            ("session_type", nullable_string_schema()),
            ("desktop", nullable_string_schema()),
        ]),
    );
    schemas.insert(
        "TrackerRuntimeDiagnostics".to_string(),
        object_schema(vec![
            ("probe_status", string_schema()),
            ("degraded_reason", nullable_string_schema()),
            (
                "probe_diagnostics",
                object_schema(vec![
                    ("last_successful_sample_at_ms", nullable_integer_schema()),
                    ("fallback_started_at_ms", nullable_integer_schema()),
                    ("fallback_count", integer_schema()),
                    ("consecutive_fallback_count", integer_schema()),
                    ("recovery_attempt_count", integer_schema()),
                    ("last_recovery_attempt_at_ms", nullable_integer_schema()),
                ]),
            ),
        ]),
    );
    schemas.insert(
        "WebActivityBridgeDiagnostics".to_string(),
        object_schema(vec![
            ("enabled", bool_schema()),
            ("connected", bool_schema()),
            ("browserClientId", nullable_string_schema()),
            ("browserKind", nullable_string_schema()),
            ("extensionVersion", nullable_string_schema()),
            ("lastActivityAtMs", nullable_integer_schema()),
        ]),
    );
    schemas.insert(
        "DiagnosticsData".to_string(),
        object_schema(vec![
            ("window_tracking", schema_ref("WindowTrackingDiagnostics")),
            (
                "tracker_runtime",
                nullable_ref_schema("TrackerRuntimeDiagnostics"),
            ),
            (
                "web_activity_bridge",
                nullable_ref_schema("WebActivityBridgeDiagnostics"),
            ),
        ]),
    );
    schemas.insert(
        "DiagnosticsResponse".to_string(),
        envelope(schema_ref("DiagnosticsData")),
    );
    schemas.insert(
        "ActivityContextData".to_string(),
        object_schema(vec![
            ("diagnostics", fallible_ref_schema("DiagnosticsData")),
            (
                "active_session",
                fallible_nullable_ref_schema("ActiveSessionData"),
            ),
            ("today_summary", fallible_ref_schema("SummaryData")),
            ("week_summary", fallible_ref_schema("SummaryData")),
            (
                "recent_web_activity",
                fallible_ref_schema("WebActivityData"),
            ),
        ]),
    );
    schemas.insert(
        "ActivityContextResponse".to_string(),
        envelope(schema_ref("ActivityContextData")),
    );
    schemas.insert(
        "ToolsSnapshotResponse".to_string(),
        envelope(schema_ref("ToolsRuntimeSnapshot")),
    );
    schemas.insert(
        "ToolsRuntimeSnapshot".to_string(),
        object_schema(vec![
            ("settings", schema_ref("ToolRuntimeSettings")),
            ("reminders", array_schema(schema_ref("ToolReminder"))),
            (
                "software_reminder_rules",
                array_schema(schema_ref("ToolSoftwareReminderRule")),
            ),
            ("current_timer", nullable_ref_schema("ToolTimer")),
            ("timer_laps", array_schema(schema_ref("ToolTimerLap"))),
            ("current_pomodoro", nullable_ref_schema("ToolPomodoroRun")),
            ("today_completed_pomodoros", integer_schema()),
            ("next_reminder_at", nullable_integer_schema()),
            ("sampled_at_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolRuntimeSettings".to_string(),
        object_schema(vec![
            ("default_countdown_minutes", integer_schema()),
            ("pomodoro_focus_minutes", integer_schema()),
            ("pomodoro_short_break_minutes", integer_schema()),
            ("pomodoro_long_break_minutes", integer_schema()),
            ("pomodoro_long_break_every", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolReminder".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("label", string_schema()),
            ("scheduled_at", integer_schema()),
            ("created_at", integer_schema()),
            (
                "status",
                enum_schema(vec!["scheduled", "fired", "cancelled"]),
            ),
            ("fired_at", nullable_integer_schema()),
            ("cancelled_at", nullable_integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolSoftwareReminderRule".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("app_name", string_schema()),
            ("exe_name", nullable_string_schema()),
            ("limit_ms", integer_schema()),
            ("message", string_schema()),
            ("created_at", integer_schema()),
            ("updated_at", integer_schema()),
            ("disabled_at", nullable_integer_schema()),
            ("last_fired_date_key", nullable_string_schema()),
        ]),
    );
    schemas.insert(
        "ToolTimer".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("mode", enum_schema(vec!["stopwatch", "countdown"])),
            ("label", nullable_string_schema()),
            ("duration_ms", nullable_integer_schema()),
            ("accumulated_ms", integer_schema()),
            ("started_at", nullable_integer_schema()),
            ("paused_at", nullable_integer_schema()),
            ("completed_at", nullable_integer_schema()),
            (
                "status",
                enum_schema(vec!["idle", "running", "paused", "completed"]),
            ),
            ("created_at", integer_schema()),
            ("updated_at", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolTimerLap".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            ("timer_id", integer_schema()),
            ("lap_index", integer_schema()),
            ("started_at", integer_schema()),
            ("ended_at", integer_schema()),
            ("duration_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolPomodoroRun".to_string(),
        object_schema(vec![
            ("id", integer_schema()),
            (
                "phase",
                enum_schema(vec!["focus", "short_break", "long_break"]),
            ),
            (
                "status",
                enum_schema(vec!["idle", "running", "paused", "completed"]),
            ),
            ("cycle_index", integer_schema()),
            ("focus_ms", integer_schema()),
            ("short_break_ms", integer_schema()),
            ("long_break_ms", integer_schema()),
            ("long_break_every", integer_schema()),
            ("phase_started_at", nullable_integer_schema()),
            ("phase_paused_at", nullable_integer_schema()),
            ("phase_remaining_ms", nullable_integer_schema()),
            ("completed_focus_count", integer_schema()),
            ("created_at", integer_schema()),
            ("updated_at", integer_schema()),
        ]),
    );
    schemas.insert(
        "ClassifyRequest".to_string(),
        object_schema(vec![("category", string_schema())]),
    );
    schemas.insert(
        "RenameRequest".to_string(),
        object_schema(vec![("display_name", string_schema())]),
    );
    schemas.insert(
        "ExcludeRequest".to_string(),
        object_schema(vec![("excluded", bool_schema())]),
    );
    schemas.insert(
        "AfkThresholdRequest".to_string(),
        object_schema(vec![("seconds", integer_schema())]),
    );

    Value::Object(schemas)
}

fn get_operation(summary: &str, response_schema: &str) -> Value {
    get_operation_with_parameters(summary, response_schema, vec![])
}

fn get_operation_with_parameters(
    summary: &str,
    response_schema: &str,
    parameters: Vec<Value>,
) -> Value {
    json!({
        "summary": summary,
        "parameters": parameters,
        "responses": standard_responses(response_schema)
    })
}

fn post_operation(
    summary: &str,
    parameters: Vec<Value>,
    request_schema: &str,
    response_schema: &str,
) -> Value {
    json!({
        "summary": summary,
        "parameters": parameters,
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema_ref(request_schema)
                }
            }
        },
        "responses": standard_responses(response_schema)
    })
}

fn standard_responses(schema: &str) -> Value {
    json!({
        "200": {
            "description": "Successful JSON response.",
            "content": {
                "application/json": {
                    "schema": schema_ref(schema)
                }
            }
        },
        "400": {
            "description": "Bad request.",
            "content": {
                "application/json": {
                    "schema": schema_ref("ApiError")
                }
            }
        },
        "401": {
            "description": "Missing or invalid bearer token.",
            "content": {
                "application/json": {
                    "schema": schema_ref("ApiError")
                }
            }
        },
        "404": {
            "description": "Endpoint or resource not found.",
            "content": {
                "application/json": {
                    "schema": schema_ref("ApiError")
                }
            }
        },
        "500": {
            "description": "Internal error.",
            "content": {
                "application/json": {
                    "schema": schema_ref("ApiError")
                }
            }
        }
    })
}

fn query_param(name: &str, kind: &str, description: &str) -> Value {
    parameter("query", name, kind, description, false)
}

fn required_query_param(name: &str, kind: &str, description: &str) -> Value {
    parameter("query", name, kind, description, true)
}

fn path_param(name: &str, description: &str) -> Value {
    parameter("path", name, "string", description, true)
}

fn parameter(location: &str, name: &str, kind: &str, description: &str, required: bool) -> Value {
    json!({
        "name": name,
        "in": location,
        "required": required,
        "description": description,
        "schema": { "type": kind }
    })
}

fn envelope(data_schema: Value) -> Value {
    object_schema(vec![("data", data_schema)])
}

fn object_schema(properties: Vec<(&str, Value)>) -> Value {
    let mut map = serde_json::Map::new();
    let mut required = Vec::new();
    for (key, value) in properties {
        required.push(key.to_string());
        map.insert(key.to_string(), value);
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": map
    })
}

fn open_object_schema(properties: Vec<(&str, Value)>) -> Value {
    let mut map = serde_json::Map::new();
    let mut required = Vec::new();
    for (key, value) in properties {
        required.push(key.to_string());
        map.insert(key.to_string(), value);
    }
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": required,
        "properties": map
    })
}

fn array_schema(item_schema: Value) -> Value {
    json!({
        "type": "array",
        "items": item_schema
    })
}

fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn nullable_ref_schema(name: &str) -> Value {
    json!({
        "oneOf": [
            schema_ref(name),
            { "type": "null" }
        ]
    })
}

fn fallible_ref_schema(name: &str) -> Value {
    json!({
        "oneOf": [
            schema_ref(name),
            schema_ref("ComponentFailure")
        ]
    })
}

fn fallible_nullable_ref_schema(name: &str) -> Value {
    json!({
        "oneOf": [
            schema_ref(name),
            { "type": "null" },
            schema_ref("ComponentFailure")
        ]
    })
}

fn string_schema() -> Value {
    json!({ "type": "string" })
}

fn nullable_string_schema() -> Value {
    json!({ "type": ["string", "null"] })
}

fn integer_schema() -> Value {
    json!({ "type": "integer", "format": "int64" })
}

fn nullable_integer_schema() -> Value {
    json!({ "type": ["integer", "null"], "format": "int64" })
}

fn number_schema() -> Value {
    json!({ "type": "number" })
}

fn bool_schema() -> Value {
    json!({ "type": "boolean" })
}

fn enum_schema(values: Vec<&str>) -> Value {
    json!({
        "type": "string",
        "enum": values
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn openapi_exposes_field_level_schemas_and_parameters() {
        let response = super::get_openapi();
        assert_eq!(response.status, 200);

        let schemas = response
            .body
            .pointer("/components/schemas")
            .and_then(|value| value.as_object())
            .expect("schemas object");

        assert!(schemas.contains_key("HealthResponse"));
        assert!(schemas.contains_key("SessionEntry"));
        assert!(schemas.contains_key("WebActivityEntry"));
        assert!(schemas.contains_key("ActivityContextResponse"));
        assert!(schemas.contains_key("ToolsRuntimeSnapshot"));
        assert!(schemas.contains_key("ClassifyRequest"));

        assert_eq!(
            response
                .body
                .pointer("/components/schemas/SessionEntry/properties/exe_name/type")
                .and_then(|value| value.as_str()),
            Some("string")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/SessionEntry/required/2")
                .and_then(|value| value.as_str()),
            Some("exe_name")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/ToolsRuntimeSnapshot/properties/current_timer/oneOf/0/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/ToolTimer")
        );
        assert_eq!(
            response
                .body
                .pointer("/paths/~1api~1v1~1sessions/get/parameters/0/name")
                .and_then(|value| value.as_str()),
            Some("from")
        );
        assert_eq!(
            response
                .body
                .pointer("/paths/~1api~1v1~1apps~1{exe_name}~1rename/post/requestBody/content/application~1json/schema/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/RenameRequest")
        );
        assert_eq!(
            response
                .body
                .pointer("/servers/0/url")
                .and_then(|value| value.as_str()),
            Some("http://127.0.0.1:{port}")
        );
        assert_eq!(
            response
                .body
                .pointer("/servers/0/variables/port/default")
                .and_then(|value| value.as_str()),
            Some("14840")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/ActivityContextData/properties/diagnostics/oneOf/1/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/ComponentFailure")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/ActivityContextData/properties/active_session/oneOf/2/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/ComponentFailure")
        );
    }
}
