use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use patina_client::{Client, ClientError};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn snapshot() -> Value {
    json!({"service_name":"patinad.service","managed_by_systemd":true,"instance_id":"instance_fixture","restart":null})
}
fn executable() -> Value {
    json!({"build":{"format_version":1,"package_version":"2.0.0","target":"x86_64-unknown-linux-gnu",
        "protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "desktop_feature":false,"debug_assertions":false},"binary_sha256":"a".repeat(64)})
}
fn capabilities() -> Value {
    json!({"data":{"server_version":"2.0.0","protocol_version":2,
        "protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "runtime_host":"daemon","event_stream":{"available":true},"tracking":{"owned":true,"ready":true},
        "tools":{"owned":true,"ready":true},"browser_activity_bridge":{"owned":true,"ready":false},
        "daemon_service":{"owned":true,"ready":true},"write_api":{"available":true,"operations":["service-lifecycle"]}}})
}
async fn serve(router: Router) -> (Client, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(listener.local_addr().unwrap().port(), "service-fixture").unwrap();
    (
        client,
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn service_reads_legacy_identity_and_measurement_failure_without_version_equality() {
    for mode in ["legacy", "identity", "unavailable"] {
        let mut value = snapshot();
        if mode == "identity" {
            value["executable"] = executable();
        }
        if mode == "unavailable" {
            value["executable_error"] = json!("measurement unavailable");
        }
        let (client, server) = serve(Router::new().route(
            "/api/v1/system/service",
            get(move |headers: axum::http::HeaderMap| {
                assert_eq!(
                    headers.get("authorization").unwrap(),
                    "Bearer service-fixture"
                );
                let value = value.clone();
                async move { Json(json!({"data":value})) }
            }),
        ))
        .await;
        let result = client.service_snapshot().await.unwrap();
        assert_eq!(result.executable.is_some(), mode == "identity");
        assert_eq!(result.executable_error.is_some(), mode == "unavailable");
        if let Some(identity) = result.executable {
            assert_eq!(identity.build.package_version, "2.0.0");
        }
        server.abort();
    }
}

#[tokio::test]
async fn service_rejects_malformed_or_contradictory_identity_and_oversized_body() {
    for mode in [
        "digest", "format", "range", "target", "both", "instance", "size",
    ] {
        let mut value = snapshot();
        value["executable"] = executable();
        match mode {
            "digest" => value["executable"]["binary_sha256"] = json!("z".repeat(64)),
            "format" => value["executable"]["build"]["format_version"] = json!(99),
            "range" => value["executable"]["build"]["protocol"]["min_supported_client"] = json!(3),
            "target" => value["executable"]["build"]["target"] = json!(""),
            "both" => value["executable_error"] = json!("unavailable"),
            "instance" => value["instance_id"] = json!(""),
            "size" => value["padding"] = json!("x".repeat(patina_client::MAX_RESPONSE_BYTES)),
            _ => unreachable!(),
        }
        let (client, server) = serve(Router::new().route(
            "/api/v1/system/service",
            get(move || {
                let value = value.clone();
                async move { Json(json!({"data":value})) }
            }),
        ))
        .await;
        assert!(client.service_snapshot().await.is_err(), "{mode}");
        server.abort();
    }
}

#[tokio::test]
async fn service_restart_requires_ownership_readiness_scope_and_protocol_before_post() {
    for mode in ["owner", "ready", "write", "scope", "host", "protocol"] {
        let mut caps = capabilities();
        match mode {
            "owner" => caps["data"]["daemon_service"]["owned"] = json!(false),
            "ready" => caps["data"]["daemon_service"]["ready"] = json!(false),
            "write" => caps["data"]["write_api"]["available"] = json!(false),
            "scope" => caps["data"]["write_api"]["operations"] = json!([]),
            "host" => caps["data"]["runtime_host"] = json!("embedded"),
            "protocol" => caps["data"]["protocol"]["min_supported_client"] = json!(9),
            _ => unreachable!(),
        }
        let posts = Arc::new(AtomicUsize::new(0));
        let seen = posts.clone();
        let (client, server) = serve(
            Router::new()
                .route(
                    "/api/v1/capabilities",
                    get(move || {
                        let caps = caps.clone();
                        async move { Json(caps) }
                    }),
                )
                .route(
                    "/api/v1/system/service/restart",
                    post(move || {
                        seen.fetch_add(1, Ordering::SeqCst);
                        async { StatusCode::CONFLICT }
                    }),
                ),
        )
        .await;
        assert!(client.restart_service().await.is_err(), "{mode}");
        assert_eq!(posts.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

#[tokio::test]
async fn explicit_restart_posts_once_with_confirmation_even_if_response_is_rejected_or_invalid() {
    for mode in ["success", "conflict", "unavailable", "malformed"] {
        let posts = Arc::new(AtomicUsize::new(0));
        let seen = posts.clone();
        let (client,server)=serve(Router::new()
            .route("/api/v1/capabilities",get(|| async {Json(capabilities())}))
            .route("/api/v1/system/service/restart",post(move |headers:axum::http::HeaderMap,Json(body):Json<Value>| {
                assert_eq!(headers.get("authorization").unwrap(),"Bearer service-fixture");
                assert_eq!(body,json!({"confirmed":true})); seen.fetch_add(1,Ordering::SeqCst);
                async move {
                    match mode {
                        "conflict" => (StatusCode::CONFLICT,Json(json!({}))),
                        "unavailable" => (StatusCode::SERVICE_UNAVAILABLE,Json(json!({}))),
                        "malformed" => (StatusCode::ACCEPTED,Json(json!({"data":{"reconnect_required":true}}))),
                        _ => (StatusCode::ACCEPTED,Json(json!({"data":{"service":snapshot(),"reconnect_required":true}}))),
                    }
                }
            }))).await;
        let result = client.restart_service().await;
        assert_eq!(result.is_ok(), mode == "success");
        if mode == "conflict" {
            assert!(matches!(result, Err(ClientError::Http { status: 409, .. })));
        }
        assert_eq!(posts.load(Ordering::SeqCst), 1);
        server.abort();
    }
}
