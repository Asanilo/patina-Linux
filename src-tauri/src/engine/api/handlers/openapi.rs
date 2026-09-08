use crate::engine::api::surface::ApiSurface;
use crate::engine::api::types::RouteResponse;
use serde_json::{json, Value};

pub fn get_openapi(surface: ApiSurface) -> RouteResponse {
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
            "paths": paths(surface)
        }),
    }
}

fn paths(surface: ApiSurface) -> Value {
    let mut paths = json!({
        "/api/v1/health": {
            "get": get_operation("API health, app version, and platform.", "HealthResponse")
        },
        "/api/v1/capabilities": {
            "get": get_operation("Runtime host, protocol, and feature capability negotiation.", "CapabilitiesResponse")
        },
        "/api/v1/events": {
            "get": event_stream_operation()
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
        "/api/v1/settings/runtime": {
            "get": get_operation(
                "Sanitized audio and browser activity runtime settings.",
                "RuntimeSettingsResponse",
            )
        },
        "/api/v1/settings/tracker/afk-threshold": {
            "post": post_operation(
                "Update idle timeout threshold.",
                vec![],
                "AfkThresholdRequest",
                "OkResponse",
            )
        },
        "/api/v1/settings/tracker/pause": {
            "post": post_operation(
                "Set tracking paused state.",
                vec![],
                "TrackingPausedRequest",
                "OkResponse",
            )
        },
        "/api/v1/settings/classification": {
            "post": post_operation(
                "Commit a validated batch of classification setting mutations.",
                vec![],
                "ClassificationMutationsRequest",
                "OkResponse",
            )
        },
        "/api/v1/settings/runtime/audio-participation": {
            "post": post_operation(
                "Enable or disable the Linux audio participation signal source.",
                vec![],
                "AudioParticipationRequest",
                "AudioParticipationResponse",
            )
        },
        "/api/v1/settings/runtime/browser-activity": {
            "post": post_operation(
                "Atomically apply browser activity listener, credential, and URL privacy settings.",
                vec![],
                "BrowserActivityConfigurationRequest",
                "BrowserActivityConfigurationResponse",
            )
        },
        "/api/v1/tools/snapshot": {
            "get": get_operation("Current Tools runtime snapshot.", "ToolsSnapshotResponse")
        },
        "/api/v1/tools/reminders": {
            "post": post_operation(
                "Create a scheduled reminder.",
                vec![],
                "CreateReminderRequest",
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/reminders/{id}/cancel": {
            "post": post_action_operation(
                "Cancel a scheduled reminder.",
                vec![integer_path_param("id", "Reminder ID.")],
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/software-reminder-rules": {
            "post": post_operation(
                "Create a daily software usage reminder rule.",
                vec![],
                "CreateSoftwareReminderRuleRequest",
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/software-reminder-rules/{id}/disable": {
            "post": post_action_operation(
                "Disable a software usage reminder rule.",
                vec![integer_path_param("id", "Software reminder rule ID.")],
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/timer/start": {
            "post": post_operation(
                "Start a stopwatch or countdown.",
                vec![],
                "StartTimerRequest",
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/timer/pause": {
            "post": post_action_operation("Pause the current timer.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/timer/resume": {
            "post": post_action_operation("Resume the current timer.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/timer/reset": {
            "post": post_action_operation("Reset the current timer.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/timer/laps": {
            "post": post_action_operation("Add a lap to the running stopwatch.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/pomodoro/start": {
            "post": post_operation(
                "Start a Pomodoro run.",
                vec![],
                "StartPomodoroRequest",
                "ToolsSnapshotResponse",
            )
        },
        "/api/v1/tools/pomodoro/pause": {
            "post": post_action_operation("Pause the current Pomodoro run.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/pomodoro/resume": {
            "post": post_action_operation("Resume the current Pomodoro run.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/pomodoro/skip": {
            "post": post_action_operation("Skip the current Pomodoro phase.", vec![], "ToolsSnapshotResponse")
        },
        "/api/v1/tools/pomodoro/reset": {
            "post": post_action_operation("Reset the current Pomodoro run.", vec![], "ToolsSnapshotResponse")
        }
    });
    let object = paths.as_object_mut().expect("OpenAPI paths object");
    object.insert(
        "/api/v1/settings/local-api".to_string(),
        json!({
            "get": get_operation(
                "Sanitized local API listener and credential-file configuration.",
                "LocalApiConfigurationResponse",
            )
        }),
    );
    object.insert(
        "/api/v1/settings/local-api/port".to_string(),
        json!({
            "post": post_operation(
                "Atomically move the local API listener to a new loopback port.",
                vec![],
                "LocalApiPortRequest",
                "LocalApiPortApplyResponse",
            )
        }),
    );
    object.insert(
        "/api/v1/settings/local-api/token/rotate".to_string(),
        json!({
            "post": post_action_operation(
                "Rotate the owner-only local API credential and revoke existing authentication.",
                vec![],
                "LocalApiTokenRotationResponse",
            )
        }),
    );
    object.insert(
        "/api/v1/system/service".to_string(),
        json!({
            "get": get_operation(
                "Current patinad systemd user-service identity and restart ticket state.",
                "DaemonServiceResponse",
            )
        }),
    );
    object.insert(
        "/api/v1/system/service/restart".to_string(),
        json!({
            "post": accepted_post_operation(
                "Persist a restart ticket and gracefully hand the daemon back to systemd.",
                "ConfirmedActionRequest",
                "DaemonServiceRestartResponse",
            )
        }),
    );
    object.retain(|path, operations| {
        let Some(operations) = operations.as_object_mut() else {
            return false;
        };
        operations.retain(|method, _| surface.allows(&method.to_ascii_uppercase(), path));
        !operations.is_empty()
    });
    paths
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
        "AvailabilityCapability".to_string(),
        object_schema(vec![("available", bool_schema())]),
    );
    schemas.insert(
        "ProtocolCapability".to_string(),
        object_schema(vec![
            ("current", integer_schema()),
            ("min_supported_client", integer_schema()),
            ("max_supported_client", integer_schema()),
        ]),
    );
    schemas.insert(
        "WriteApiCapability".to_string(),
        object_schema(vec![
            ("available", bool_schema()),
            (
                "operations",
                array_schema(enum_schema(vec![
                    "app-mapping",
                    "classification",
                    "local-api-configuration",
                    "runtime-settings",
                    "service-lifecycle",
                    "tools",
                    "tracker-settings",
                ])),
            ),
        ]),
    );
    schemas.insert(
        "OwnedRuntimeCapability".to_string(),
        object_schema(vec![("owned", bool_schema()), ("ready", bool_schema())]),
    );
    schemas.insert(
        "CapabilitiesData".to_string(),
        object_schema(vec![
            ("server_version", string_schema()),
            ("protocol_version", integer_schema()),
            ("protocol", schema_ref("ProtocolCapability")),
            ("runtime_host", enum_schema(vec!["desktop", "daemon"])),
            ("event_stream", schema_ref("AvailabilityCapability")),
            ("tracking", schema_ref("OwnedRuntimeCapability")),
            (
                "browser_activity_bridge",
                schema_ref("OwnedRuntimeCapability"),
            ),
            ("tools", schema_ref("OwnedRuntimeCapability")),
            ("daemon_service", schema_ref("OwnedRuntimeCapability")),
            ("write_api", schema_ref("WriteApiCapability")),
        ]),
    );
    schemas.insert(
        "CapabilitiesResponse".to_string(),
        envelope(schema_ref("CapabilitiesData")),
    );
    schemas.insert(
        "TrackingDataChangedEvent".to_string(),
        object_schema(vec![
            ("type", enum_schema(vec!["tracking-data-changed"])),
            ("reason", string_schema()),
            ("changed_at_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolsRuntimeChangedEvent".to_string(),
        object_schema(vec![
            ("type", enum_schema(vec!["tools-runtime-changed"])),
            ("changed_at_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "ToolAlertEvent".to_string(),
        object_schema(vec![
            ("type", enum_schema(vec!["tool-alert"])),
            ("alert", schema_ref("ToolAlert")),
        ]),
    );
    schemas.insert(
        "RuntimeEvent".to_string(),
        json!({
            "oneOf": [
                schema_ref("TrackingDataChangedEvent"),
                schema_ref("ToolsRuntimeChangedEvent"),
                schema_ref("ToolAlertEvent")
            ],
            "discriminator": { "propertyName": "type" }
        }),
    );
    schemas.insert(
        "RuntimeEventEnvelope".to_string(),
        object_schema(vec![
            ("sequence", integer_schema()),
            ("event", schema_ref("RuntimeEvent")),
        ]),
    );
    schemas.insert(
        "WindowInfo".to_string(),
        object_schema(vec![
            ("hwnd", string_schema()),
            ("root_owner_hwnd", string_schema()),
            ("process_id", integer_schema()),
            ("window_class", string_schema()),
            ("title", string_schema()),
            ("exe_name", string_schema()),
            ("process_path", string_schema()),
            ("is_afk", bool_schema()),
            ("idle_time_ms", integer_schema()),
        ]),
    );
    schemas.insert(
        "SustainedParticipationSignalSnapshot".to_string(),
        object_schema(vec![
            ("is_available", bool_schema()),
            ("is_active", bool_schema()),
            (
                "signal_source",
                nullable_enum_schema(vec!["system-media", "audio-session"]),
            ),
            ("source_app_id", nullable_string_schema()),
            (
                "source_app_identity",
                nullable_enum_schema(vec![
                    "chrome", "edge", "firefox", "brave", "zoom", "teams", "vlc", "bilibili",
                    "douyin", "we-meet",
                ]),
            ),
            (
                "playback_type",
                nullable_enum_schema(vec!["unknown", "audio", "video", "image"]),
            ),
        ]),
    );
    schemas.insert(
        "SustainedParticipationSignalEvaluationSnapshot".to_string(),
        object_schema(vec![
            ("signal", schema_ref("SustainedParticipationSignalSnapshot")),
            (
                "match_result",
                enum_schema(vec![
                    "unavailable",
                    "inactive",
                    "identity-mismatch",
                    "matched",
                ]),
            ),
        ]),
    );
    schemas.insert(
        "SustainedParticipationDiagnosticsSnapshot".to_string(),
        object_schema(vec![
            (
                "state",
                enum_schema(vec!["inactive", "candidate", "active", "grace", "expired"]),
            ),
            (
                "reason",
                enum_schema(vec![
                    "no-signal",
                    "tracking-paused",
                    "empty-window",
                    "not-eligible",
                    "signal-inactive",
                    "identity-mismatch",
                    "signal-matched",
                    "grace-window",
                    "grace-expired",
                    "sustained-window-expired",
                ]),
            ),
            (
                "window_identity",
                nullable_enum_schema(vec![
                    "chrome", "edge", "firefox", "brave", "zoom", "teams", "vlc", "bilibili",
                    "douyin", "we-meet",
                ]),
            ),
            (
                "effective_signal_source",
                nullable_enum_schema(vec!["system-media", "audio-session"]),
            ),
            ("last_match_at_ms", nullable_integer_schema()),
            ("grace_deadline_ms", nullable_integer_schema()),
            (
                "system_media",
                schema_ref("SustainedParticipationSignalEvaluationSnapshot"),
            ),
            (
                "audio_session",
                schema_ref("SustainedParticipationSignalEvaluationSnapshot"),
            ),
        ]),
    );
    schemas.insert(
        "TrackingStatusSnapshot".to_string(),
        object_schema(vec![
            ("is_tracking_active", bool_schema()),
            ("sustained_participation_eligible", bool_schema()),
            ("sustained_participation_active", bool_schema()),
            (
                "sustained_participation_kind",
                nullable_enum_schema(vec!["audio"]),
            ),
            (
                "sustained_participation_state",
                enum_schema(vec!["inactive", "candidate", "active", "grace", "expired"]),
            ),
            (
                "sustained_participation_signal_source",
                nullable_enum_schema(vec!["system-media", "audio-session"]),
            ),
            (
                "sustained_participation_reason",
                enum_schema(vec![
                    "no-signal",
                    "tracking-paused",
                    "empty-window",
                    "not-eligible",
                    "signal-inactive",
                    "identity-mismatch",
                    "signal-matched",
                    "grace-window",
                    "grace-expired",
                    "sustained-window-expired",
                ]),
            ),
            (
                "sustained_participation_diagnostics",
                schema_ref("SustainedParticipationDiagnosticsSnapshot"),
            ),
        ]),
    );
    schemas.insert(
        "TrackingRuntimeProbeDiagnostics".to_string(),
        object_schema(vec![
            ("last_successful_sample_at_ms", nullable_integer_schema()),
            ("fallback_started_at_ms", nullable_integer_schema()),
            ("fallback_count", integer_schema()),
            ("consecutive_fallback_count", integer_schema()),
            ("recovery_attempt_count", integer_schema()),
            ("last_recovery_attempt_at_ms", nullable_integer_schema()),
        ]),
    );
    schemas.insert(
        "TrackingRuntimeSnapshot".to_string(),
        object_schema(vec![
            ("window", schema_ref("WindowInfo")),
            ("status", schema_ref("TrackingStatusSnapshot")),
            ("sampled_at_ms", integer_schema()),
            (
                "probe_status",
                enum_schema(vec![
                    "ok",
                    "timeout-fallback",
                    "timeout-inactive",
                    "backing-off-fallback",
                    "backing-off-inactive",
                    "recovery-attempted-fallback",
                    "recovery-attempted-inactive",
                    "hard-degraded-fallback",
                    "hard-degraded-inactive",
                    "task-failed-fallback",
                    "task-failed-inactive",
                ]),
            ),
            ("degraded_reason", nullable_string_schema()),
            (
                "probe_diagnostics",
                schema_ref("TrackingRuntimeProbeDiagnostics"),
            ),
        ]),
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
            ("sampled_at_ms", integer_schema()),
            ("runtime_snapshot", schema_ref("TrackingRuntimeSnapshot")),
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
    let browser_activity_settings = object_schema(vec![
        ("enabled", bool_schema()),
        ("port", bounded_integer_schema(1024, 65_535)),
        ("token_present", bool_schema()),
        (
            "url_privacy",
            enum_schema(vec!["full", "strip_query", "domain_only"]),
        ),
    ]);
    schemas.insert(
        "BrowserActivitySettings".to_string(),
        browser_activity_settings.clone(),
    );
    schemas.insert(
        "RuntimeSettingsResponse".to_string(),
        envelope(object_schema(vec![
            ("audio_participation_enabled", bool_schema()),
            ("browser_activity", schema_ref("BrowserActivitySettings")),
        ])),
    );
    let local_api_configuration = object_schema(vec![
        ("port", bounded_integer_schema(1024, 65_535)),
        ("base_url", string_schema()),
        ("token_path", string_schema()),
        ("token_present", bool_schema()),
    ]);
    schemas.insert(
        "LocalApiConfiguration".to_string(),
        local_api_configuration.clone(),
    );
    schemas.insert(
        "LocalApiConfigurationResponse".to_string(),
        envelope(local_api_configuration),
    );
    schemas.insert(
        "LocalApiPortRequest".to_string(),
        object_schema(vec![("port", bounded_integer_schema(1024, 65_535))]),
    );
    schemas.insert(
        "LocalApiPortApplyResponse".to_string(),
        envelope(object_schema(vec![
            ("configuration", schema_ref("LocalApiConfiguration")),
            ("previous_port", bounded_integer_schema(1024, 65_535)),
            ("reconnect_required", bool_schema()),
        ])),
    );
    schemas.insert(
        "LocalApiTokenRotationResponse".to_string(),
        envelope(object_schema(vec![
            ("configuration", schema_ref("LocalApiConfiguration")),
            ("reauthentication_required", bool_schema()),
        ])),
    );
    schemas.insert(
        "ConfirmedActionRequest".to_string(),
        object_schema(vec![("confirmed", bool_schema())]),
    );
    schemas.insert(
        "DaemonServiceRestart".to_string(),
        object_schema(vec![
            ("request_id", string_schema()),
            ("status", enum_schema(vec!["pending", "completed"])),
            ("requested_at_ms", integer_schema()),
            ("requested_instance_id", string_schema()),
            ("completed_at_ms", nullable_integer_schema()),
            ("completed_instance_id", nullable_string_schema()),
        ]),
    );
    schemas.insert(
        "DaemonService".to_string(),
        object_schema(vec![
            ("service_name", string_schema()),
            ("managed_by_systemd", bool_schema()),
            ("instance_id", string_schema()),
            ("restart", nullable_ref_schema("DaemonServiceRestart")),
        ]),
    );
    schemas.insert(
        "DaemonServiceResponse".to_string(),
        envelope(schema_ref("DaemonService")),
    );
    schemas.insert(
        "DaemonServiceRestartResponse".to_string(),
        envelope(object_schema(vec![
            ("service", schema_ref("DaemonService")),
            ("reconnect_required", bool_schema()),
        ])),
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
            ("listening", bool_schema()),
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
        "ToolAlert".to_string(),
        object_schema(vec![
            ("id", string_schema()),
            (
                "kind",
                enum_schema(vec![
                    "reminder",
                    "countdown",
                    "pomodoro",
                    "software_reminder",
                ]),
            ),
            ("title", string_schema()),
            ("body", string_schema()),
            ("occurred_at", integer_schema()),
        ]),
    );
    schemas.insert(
        "CreateReminderRequest".to_string(),
        object_schema_with_required(
            vec![
                ("label", bounded_string_schema(0, 256)),
                ("scheduled_at", integer_schema()),
            ],
            vec!["label", "scheduled_at"],
        ),
    );
    schemas.insert(
        "CreateSoftwareReminderRuleRequest".to_string(),
        object_schema_with_required(
            vec![
                ("app_name", bounded_string_schema(1, 256)),
                ("exe_name", bounded_nullable_string_schema(256)),
                ("limit_ms", bounded_integer_schema(60_000, 86_400_000)),
                ("message", bounded_string_schema(0, 1_024)),
            ],
            vec!["app_name", "limit_ms", "message"],
        ),
    );
    schemas.insert(
        "StartTimerRequest".to_string(),
        object_schema_with_required(
            vec![
                ("mode", enum_schema(vec!["stopwatch", "countdown"])),
                ("duration_ms", nullable_integer_schema()),
                ("label", bounded_nullable_string_schema(256)),
            ],
            vec!["mode"],
        ),
    );
    schemas.insert(
        "StartPomodoroRequest".to_string(),
        object_schema(vec![
            ("focus_ms", bounded_integer_schema(60_000, 10_800_000)),
            ("short_break_ms", bounded_integer_schema(60_000, 3_600_000)),
            ("long_break_ms", bounded_integer_schema(60_000, 7_200_000)),
            ("long_break_every", bounded_integer_schema(2, 12)),
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
        object_schema(vec![("seconds", bounded_integer_schema(60, 86_400))]),
    );
    schemas.insert(
        "TrackingPausedRequest".to_string(),
        object_schema(vec![("paused", bool_schema())]),
    );
    schemas.insert(
        "AudioParticipationRequest".to_string(),
        object_schema(vec![("enabled", bool_schema())]),
    );
    schemas.insert(
        "AudioParticipationResponse".to_string(),
        envelope(object_schema(vec![("enabled", bool_schema())])),
    );
    let browser_activity_configuration = object_schema(vec![
        ("enabled", bool_schema()),
        ("port", bounded_integer_schema(1024, 65_535)),
        ("token", bounded_string_schema(0, 512)),
        (
            "url_privacy",
            enum_schema(vec!["full", "strip_query", "domain_only"]),
        ),
    ]);
    schemas.insert(
        "BrowserActivityConfigurationRequest".to_string(),
        browser_activity_configuration.clone(),
    );
    schemas.insert(
        "BrowserActivityConfigurationResponse".to_string(),
        envelope(browser_activity_settings),
    );
    schemas.insert(
        "ClassificationMutationRequest".to_string(),
        object_schema(vec![
            ("key", bounded_string_schema(1, 256)),
            ("value", bounded_nullable_string_schema(4096)),
        ]),
    );
    schemas.insert(
        "ClassificationMutationsRequest".to_string(),
        object_schema(vec![(
            "mutations",
            bounded_array_schema(schema_ref("ClassificationMutationRequest"), 256),
        )]),
    );

    Value::Object(schemas)
}

fn get_operation(summary: &str, response_schema: &str) -> Value {
    get_operation_with_parameters(summary, response_schema, vec![])
}

fn event_stream_operation() -> Value {
    json!({
        "summary": "Authenticated runtime event stream with Last-Event-ID replay.",
        "parameters": [
            {
                "name": "Last-Event-ID",
                "in": "header",
                "required": false,
                "description": "Last processed sequence ID for bounded in-process replay.",
                "schema": integer_schema()
            }
        ],
        "responses": {
            "200": {
                "description": "Server-Sent Events stream. Each data field is a RuntimeEventEnvelope.",
                "content": {
                    "text/event-stream": {
                        "schema": {
                            "type": "string"
                        }
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
            "403": {
                "description": "Request origin is not allowed.",
                "content": {
                    "application/json": {
                        "schema": schema_ref("ApiError")
                    }
                }
            },
            "503": {
                "description": "Event stream is unavailable or its connection budget is full.",
                "content": {
                    "application/json": {
                        "schema": schema_ref("ApiError")
                    }
                }
            }
        }
    })
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

fn post_action_operation(summary: &str, parameters: Vec<Value>, response_schema: &str) -> Value {
    json!({
        "summary": summary,
        "parameters": parameters,
        "responses": standard_responses(response_schema)
    })
}

fn accepted_post_operation(summary: &str, request_schema: &str, response_schema: &str) -> Value {
    let mut operation = post_operation(summary, vec![], request_schema, response_schema);
    let responses = operation
        .get_mut("responses")
        .and_then(Value::as_object_mut)
        .expect("post operation responses");
    if let Some(success) = responses.remove("200") {
        responses.insert("202".to_string(), success);
    }
    operation
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
        "403": {
            "description": "Request origin is not allowed.",
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
        "409": {
            "description": "Requested runtime resource conflicts with an active local resource.",
            "content": {
                "application/json": {
                    "schema": schema_ref("ApiError")
                }
            }
        },
        "413": {
            "description": "Request body is too large.",
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
        },
        "503": {
            "description": "Runtime capability is unavailable, the request timed out, or the concurrency budget is full.",
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

fn integer_path_param(name: &str, description: &str) -> Value {
    parameter("path", name, "integer", description, true)
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

fn object_schema_with_required(properties: Vec<(&str, Value)>, required: Vec<&str>) -> Value {
    let properties = properties
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties
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

fn bounded_array_schema(item_schema: Value, max_items: usize) -> Value {
    json!({
        "type": "array",
        "items": item_schema,
        "maxItems": max_items
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

fn nullable_enum_schema(values: Vec<&str>) -> Value {
    json!({
        "oneOf": [
            enum_schema(values),
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

fn bounded_string_schema(min_length: usize, max_length: usize) -> Value {
    json!({
        "type": "string",
        "minLength": min_length,
        "maxLength": max_length
    })
}

fn nullable_string_schema() -> Value {
    json!({ "type": ["string", "null"] })
}

fn bounded_nullable_string_schema(max_length: usize) -> Value {
    json!({
        "type": ["string", "null"],
        "maxLength": max_length
    })
}

fn integer_schema() -> Value {
    json!({ "type": "integer", "format": "int64" })
}

fn bounded_integer_schema(minimum: i64, maximum: i64) -> Value {
    json!({
        "type": "integer",
        "format": "int64",
        "minimum": minimum,
        "maximum": maximum
    })
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
    use serde_json::json;

    #[test]
    fn openapi_exposes_field_level_schemas_and_parameters() {
        let response = super::get_openapi(crate::engine::api::surface::ApiSurface::Desktop);
        assert_eq!(response.status, 200);

        let schemas = response
            .body
            .pointer("/components/schemas")
            .and_then(|value| value.as_object())
            .expect("schemas object");

        assert!(schemas.contains_key("HealthResponse"));
        assert!(schemas.contains_key("CapabilitiesResponse"));
        assert!(schemas.contains_key("RuntimeEventEnvelope"));
        assert!(schemas.contains_key("TrackingRuntimeSnapshot"));
        assert!(schemas.contains_key("TrackingStatusSnapshot"));
        assert!(schemas.contains_key("SessionEntry"));
        assert!(schemas.contains_key("WebActivityEntry"));
        assert!(schemas.contains_key("ActivityContextResponse"));
        assert!(schemas.contains_key("ToolsRuntimeSnapshot"));
        assert!(schemas.contains_key("ToolAlert"));
        assert!(schemas.contains_key("CreateReminderRequest"));
        assert!(schemas.contains_key("StartPomodoroRequest"));
        assert!(schemas.contains_key("ClassifyRequest"));
        assert!(schemas.contains_key("ProtocolCapability"));
        assert!(schemas.contains_key("WriteApiCapability"));
        assert!(schemas.contains_key("TrackingPausedRequest"));
        assert!(schemas.contains_key("AudioParticipationRequest"));
        assert!(schemas.contains_key("RuntimeSettingsResponse"));
        assert!(schemas.contains_key("BrowserActivitySettings"));
        assert!(schemas.contains_key("BrowserActivityConfigurationRequest"));
        assert!(schemas.contains_key("LocalApiConfiguration"));
        assert!(schemas.contains_key("LocalApiPortRequest"));
        assert!(schemas.contains_key("LocalApiPortApplyResponse"));
        assert!(schemas.contains_key("LocalApiTokenRotationResponse"));
        assert!(schemas.contains_key("DaemonService"));
        assert!(schemas.contains_key("DaemonServiceRestart"));
        assert!(schemas.contains_key("DaemonServiceRestartResponse"));
        assert!(schemas.contains_key("ConfirmedActionRequest"));
        assert!(schemas.contains_key("ClassificationMutationsRequest"));
        assert!(response
            .body
            .pointer("/components/schemas/BrowserActivitySettings/properties/token")
            .is_none());
        assert_eq!(
            response
                .body
                .pointer(
                    "/components/schemas/BrowserActivitySettings/properties/token_present/type"
                )
                .and_then(|value| value.as_str()),
            Some("boolean")
        );
        assert!(response
            .body
            .pointer("/components/schemas/LocalApiConfiguration/properties/token")
            .is_none());

        assert_eq!(
            response
                .body
                .pointer(
                    "/components/schemas/CurrentWindowResponse/properties/data/properties/runtime_snapshot/$ref"
                )
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/TrackingRuntimeSnapshot")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/TrackingRuntimeSnapshot/properties/status/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/TrackingStatusSnapshot")
        );
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
                .pointer("/components/schemas/RuntimeEvent/oneOf/1/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/ToolsRuntimeChangedEvent")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/CapabilitiesData/properties/tools/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/OwnedRuntimeCapability")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/StartTimerRequest/required/0")
                .and_then(|value| value.as_str()),
            Some("mode")
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
                .pointer("/paths/~1api~1v1~1settings~1tracker~1pause/post/requestBody/content/application~1json/schema/$ref")
                .and_then(|value| value.as_str()),
            Some("#/components/schemas/TrackingPausedRequest")
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/AfkThresholdRequest/properties/seconds/minimum"),
            Some(&json!(60))
        );
        assert_eq!(
            response
                .body
                .pointer("/components/schemas/AfkThresholdRequest/properties/seconds/maximum"),
            Some(&json!(86_400))
        );
        assert_eq!(
            response.body.pointer(
                "/components/schemas/ClassificationMutationsRequest/properties/mutations/maxItems"
            ),
            Some(&json!(256))
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
                .pointer(
                    "/components/schemas/ActivityContextData/properties/diagnostics/oneOf/1/$ref"
                )
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
