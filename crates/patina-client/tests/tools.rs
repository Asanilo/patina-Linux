use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use patina_client::{protocol::tools::*, Client, ClientError};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn snapshot() -> Value {
    json!({"settings":{"default_countdown_minutes":25,"pomodoro_focus_minutes":25,
        "pomodoro_short_break_minutes":5,"pomodoro_long_break_minutes":15,"pomodoro_long_break_every":4},
        "reminders":[],"software_reminder_rules":[],"current_timer":null,"timer_laps":[],
        "current_pomodoro":null,"today_completed_pomodoros":0,"next_reminder_at":null,"sampled_at_ms":1000})
}

fn capabilities() -> Value {
    json!({"data":{"server_version":"fixture","protocol_version":2,
        "protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "runtime_host":"daemon","event_stream":{"available":true},"tracking":{"owned":true,"ready":true},
        "tools":{"owned":true,"ready":true},"browser_activity_bridge":{"owned":true,"ready":false},
        "daemon_service":{"owned":false,"ready":false},"write_api":{"available":true,"operations":["tools"]}}})
}

async fn serve(router: Router) -> (Client, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(listener.local_addr().unwrap().port(), "tools-fixture").unwrap();
    (
        client,
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn typed_tools_use_exact_routes_and_preserve_request_fields() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let captured = received.clone();
    let router = Router::new()
        .route(
            "/api/v1/capabilities",
            get(|| async { Json(capabilities()) }),
        )
        .fallback(post(
            move |uri: axum::http::Uri, Json(body): Json<Value>| {
                captured
                    .lock()
                    .unwrap()
                    .push((uri.path().to_string(), body));
                async { Json(json!({"data":snapshot()})) }
            },
        ));
    let (client, server) = serve(router).await;
    client
        .create_reminder(&CreateReminderRequest {
            label: "Review".into(),
            scheduled_at: 2000,
        })
        .await
        .unwrap();
    client.cancel_reminder(3).await.unwrap();
    client
        .create_software_reminder_rule(&CreateSoftwareReminderRuleRequest {
            app_name: "Editor".into(),
            exe_name: None,
            limit_ms: 60000,
            message: "Rest".into(),
        })
        .await
        .unwrap();
    client.disable_software_reminder_rule(4).await.unwrap();
    client
        .start_timer(&StartTimerRequest {
            mode: TimerMode::Countdown,
            duration_ms: Some(60000),
            label: Some("Tea".into()),
        })
        .await
        .unwrap();
    client
        .start_pomodoro(&StartPomodoroRequest {
            focus_ms: 60000,
            short_break_ms: 60000,
            long_break_ms: 120000,
            long_break_every: 2,
        })
        .await
        .unwrap();
    for action in [
        ToolsAction::PauseTimer,
        ToolsAction::ResumeTimer,
        ToolsAction::ResetTimer,
        ToolsAction::AddTimerLap,
        ToolsAction::PausePomodoro,
        ToolsAction::ResumePomodoro,
        ToolsAction::SkipPomodoroPhase,
        ToolsAction::ResetPomodoro,
    ] {
        client.tools_action(action).await.unwrap();
    }
    let received = received.lock().unwrap();
    assert_eq!(
        received.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        vec![
            "/api/v1/tools/reminders",
            "/api/v1/tools/reminders/3/cancel",
            "/api/v1/tools/software-reminder-rules",
            "/api/v1/tools/software-reminder-rules/4/disable",
            "/api/v1/tools/timer/start",
            "/api/v1/tools/pomodoro/start",
            "/api/v1/tools/timer/pause",
            "/api/v1/tools/timer/resume",
            "/api/v1/tools/timer/reset",
            "/api/v1/tools/timer/laps",
            "/api/v1/tools/pomodoro/pause",
            "/api/v1/tools/pomodoro/resume",
            "/api/v1/tools/pomodoro/skip",
            "/api/v1/tools/pomodoro/reset"
        ]
    );
    assert_eq!(received[0].1, json!({"label":"Review","scheduled_at":2000}));
    assert_eq!(
        received[2].1,
        json!({"app_name":"Editor","exe_name":null,"limit_ms":60000,"message":"Rest"})
    );
    assert_eq!(
        received[4].1,
        json!({"mode":"countdown","duration_ms":60000,"label":"Tea"})
    );
    server.abort();
}

#[tokio::test]
async fn tools_reject_missing_ownership_readiness_scope_and_wrong_host_before_writing() {
    for (field, value) in [
        ("/data/tools/owned", json!(false)),
        ("/data/tools/ready", json!(false)),
        ("/data/write_api/available", json!(false)),
        ("/data/write_api/operations", json!([])),
        ("/data/runtime_host", json!("desktop")),
    ] {
        let mut caps = capabilities();
        *caps.pointer_mut(field).unwrap() = value;
        let (client, server) = serve(Router::new().route(
            "/api/v1/capabilities",
            get(move || {
                let caps = caps.clone();
                async { Json(caps) }
            }),
        ))
        .await;
        assert!(matches!(
            client.tools_action(ToolsAction::ResetTimer).await,
            Err(ClientError::UnsupportedCapability(_) | ClientError::WrongRuntimeHost(_))
        ));
        server.abort();
    }
}

#[tokio::test]
async fn tools_write_failure_is_not_retried_and_invalid_ids_never_contact_server() {
    let writes = Arc::new(Mutex::new(0));
    let captured = writes.clone();
    let (client, server) = serve(
        Router::new()
            .route(
                "/api/v1/capabilities",
                get(|| async { Json(capabilities()) }),
            )
            .fallback(post(move || {
                *captured.lock().unwrap() += 1;
                async { StatusCode::CONFLICT }
            })),
    )
    .await;
    assert!(matches!(
        client.cancel_reminder(0).await,
        Err(ClientError::InvalidConfiguration(_))
    ));
    assert!(matches!(
        client.disable_software_reminder_rule(-1).await,
        Err(ClientError::InvalidConfiguration(_))
    ));
    assert_eq!(*writes.lock().unwrap(), 0);
    assert!(matches!(
        client.tools_action(ToolsAction::AddTimerLap).await,
        Err(ClientError::Http { status: 409, .. })
    ));
    assert_eq!(*writes.lock().unwrap(), 1);
    server.abort();
}

#[tokio::test]
async fn tools_snapshot_preserves_strict_wire_shape_and_response_budget() {
    for (index,value) in [snapshot(),
        {let mut value=snapshot();value.as_object_mut().unwrap().remove("settings");value},
        {let mut value=snapshot();value["reminders"]=json!([{"id":1,"label":"x","scheduled_at":1000,"created_at":1,"status":"unknown","fired_at":null,"cancelled_at":null}]);value},
        {let mut value=snapshot();value["extra"]=json!("x".repeat(MAX_TOOLS_RESPONSE_BYTES));value},
    ].into_iter().enumerate() {
        let (client,server)=serve(Router::new().route("/api/v1/tools/snapshot",get(move || {let value=value.clone();async move{Json(json!({"data":value}))}}))).await;
        let result=client.tools_snapshot().await;
        if index==0 {assert_eq!(result.unwrap().sampled_at_ms,1000);} else {assert!(result.is_err());}
        server.abort();
    }
}
