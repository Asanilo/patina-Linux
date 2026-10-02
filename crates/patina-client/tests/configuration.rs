use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use patina_client::protocol::configuration::ClassificationMutationRequest;
use patina_client::{Client, ClientError};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn capabilities(conditional: bool) -> Value {
    let operations = if conditional {
        vec!["classification-conditional"]
    } else {
        vec!["classification"]
    };
    json!({"data":{"server_version":"fixture","protocol_version":2,"protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "runtime_host":"daemon","event_stream":{"available":true},"tracking":{"owned":true,"ready":true},"tools":{"owned":true,"ready":true},
        "browser_activity_bridge":{"owned":true,"ready":false},"daemon_service":{"owned":false,"ready":false},
        "write_api":{"available":true,"operations":operations}}})
}

async fn serve(router: Router) -> (Client, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(
        listener.local_addr().unwrap().port(),
        "configuration-fixture",
    )
    .unwrap();
    (
        client,
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn conditional_updates_never_fall_back_to_a_legacy_unconditional_endpoint() {
    for advertised in [false, true] {
        let legacy_writes = Arc::new(AtomicUsize::new(0));
        let router = Router::new()
            .route(
                "/api/v1/capabilities",
                get(move || async move { Json(capabilities(advertised)) }),
            )
            .route(
                "/api/v1/settings/classification",
                post(|State(count): State<Arc<AtomicUsize>>| async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(json!({"data":{"ok":true}}))
                }),
            )
            .with_state(legacy_writes.clone());
        let (client, server) = serve(router).await;
        let error = client
            .commit_classification(
                &"0".repeat(64),
                vec![ClassificationMutationRequest {
                    key: "__category_label_override::office".into(),
                    value: Some("Office".into()),
                }],
            )
            .await
            .unwrap_err();
        if advertised {
            assert!(matches!(error, ClientError::Http { status: 404, .. }));
        } else {
            assert!(matches!(error, ClientError::UnsupportedCapability(_)));
        }
        assert_eq!(legacy_writes.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

#[tokio::test]
async fn classification_snapshots_reject_duplicate_keys_secrets_and_invalid_revisions() {
    for body in [
        json!({"revision":"bad","sampled_at_ms":1,"entries":[]}),
        json!({"revision":"0".repeat(64),"sampled_at_ms":1,"entries":[{"key":"local_api_token","value":"secret"}]}),
        json!({"revision":"0".repeat(64),"sampled_at_ms":1,"entries":[{"key":"__custom_category::a","value":"1"},{"key":"__custom_category::a","value":"1"}]}),
    ] {
        let router = Router::new().route(
            "/api/v1/settings/classification",
            get(move || {
                let value = body.clone();
                async move { Json(json!({"data":value})) }
            }),
        );
        let (client, server) = serve(router).await;
        assert!(matches!(
            client.classification_snapshot().await,
            Err(ClientError::InvalidResponse(_))
        ));
        server.abort();
    }
}

#[tokio::test]
async fn conflicts_are_returned_once_without_reloading_and_resubmitting() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .route(
            "/api/v1/capabilities",
            get(|| async { Json(capabilities(true)) }),
        )
        .route(
            "/api/v1/settings/classification/conditional",
            post(|State(count): State<Arc<AtomicUsize>>| async move {
                count.fetch_add(1, Ordering::SeqCst);
                (
                    StatusCode::CONFLICT,
                    Json(json!({"error":{"code":"conflict","message":"reload configuration"}})),
                )
            }),
        )
        .with_state(attempts.clone());
    let (client, server) = serve(router).await;
    assert!(matches!(
        client.commit_classification(&"0".repeat(64), vec![]).await,
        Err(ClientError::Http { status: 409, .. })
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    server.abort();
}
