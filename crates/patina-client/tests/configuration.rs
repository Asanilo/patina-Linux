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
async fn policy_conflicts_are_not_retried_and_missing_capability_never_uses_legacy_writes() {
    use patina_client::protocol::product_settings::{ProductSettingsCommitRequest, ProductSettingsPatch};
    for advertised in [false, true] {
        let writes = Arc::new(AtomicUsize::new(0));
        let legacy = Arc::new(AtomicUsize::new(0));
        let legacy_count = legacy.clone();
        let router = Router::new()
            .route("/api/v1/capabilities", get(move || async move {
                let mut value = capabilities(true);
                if advertised { value["data"]["write_api"]["operations"] = json!(["product-settings-conditional"]); }
                Json(value)
            }))
            .route("/api/v1/settings/product/conditional", post(|State(count): State<Arc<AtomicUsize>>, Json(body): Json<Value>| async move {
                count.fetch_add(1, Ordering::SeqCst);
                assert_eq!(body["expected_revision"], "a".repeat(64));
                (StatusCode::CONFLICT, Json(json!({"error":{"code":"conflict","message":"changed"}})))
            }))
            .route("/api/v1/settings/app", post(move || { let count = legacy_count.clone(); async move {
                count.fetch_add(1, Ordering::SeqCst); Json(json!({"data":{"ok":true}}))
            }})).with_state(writes.clone());
        let (client, server) = serve(router).await;
        let error = client.commit_product_settings(&ProductSettingsCommitRequest {expected_revision:"a".repeat(64),patch:ProductSettingsPatch {min_session_secs:Some(360),..Default::default()}}).await.unwrap_err();
        if advertised { assert!(matches!(error, ClientError::Http {status:409,..})); }
        else { assert!(matches!(error, ClientError::UnsupportedCapability(_))); }
        assert_eq!(writes.load(Ordering::SeqCst), usize::from(advertised));
        assert_eq!(legacy.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

#[tokio::test]
async fn product_settings_decode_effective_policy_and_reject_invalid_snapshots() {
    let valid = json!({"revision":"a".repeat(64),"sampled_at_ms":100,"last_heartbeat_ms":99,"last_successful_sample_ms":98,
        "settings":{"idle_timeout_secs":60,"timeline_merge_gap_secs":30,"min_session_secs":300,
        "tracking_paused":true,"audio_participation_enabled":false,"web_activity_enabled":true,
        "web_activity_port":12345,"web_activity_token_present":true,"web_activity_url_privacy":"strip_query"}});
    for (index, body) in [valid.clone(), {let mut v=valid.clone();v["settings"]["web_activity_token_present"]=json!(false);v},
        {let mut v=valid.clone();v["settings"]["min_session_secs"]=json!(301);v},
        {let mut v=valid.clone();v["last_heartbeat_ms"]=json!(-1);v},
        {let mut v=valid.clone();v["revision"]=json!("bad");v},
        {let mut v=valid.clone();v["padding"]=json!("x".repeat(8192));v},
    ].into_iter().enumerate() {
        let router = Router::new().route("/api/v1/settings/product", get(move || {
            let value = body.clone(); async move { Json(json!({"data":value})) }
        }));
        let (client, server) = serve(router).await;
        let result = client.product_settings().await;
        if index == 0 { assert_eq!(result.unwrap().settings.idle_timeout_secs, 60); }
        else { assert!(result.is_err()); }
        server.abort();
    }
}

#[tokio::test]
async fn resource_conflicts_are_not_retried_and_never_use_legacy_replacement() {
    use patina_client::protocol::resource_settings::{ResourceSettingsCommitRequest, ResourceSettingsPatch};
    for advertised in [false, true] {
        let writes = Arc::new(AtomicUsize::new(0));
        let legacy = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let legacy_count = legacy.clone();
        let router = Router::new()
            .route("/api/v1/capabilities", get(move || async move {
                let mut value = capabilities(false);
                value["data"]["write_api"]["operations"] = json!(if advertised { vec!["runtime-settings-conditional"] } else { vec!["runtime-settings"] });
                Json(value)
            }))
            .route("/api/v1/settings/resources/conditional", post(move || {
                let count = count.clone(); async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::CONFLICT,Json(json!({"error":{"code":"conflict","message":"stale resources"}})))
                }
            }))
            .fallback(move || { let count=legacy_count.clone(); async move {
                count.fetch_add(1,Ordering::SeqCst); StatusCode::INTERNAL_SERVER_ERROR
            }});
        let (client, server) = serve(router).await;
        let error = client.commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision:"a".repeat(64), patch:ResourceSettingsPatch {audio_participation_enabled:Some(false),..Default::default()},
        }).await.unwrap_err();
        if advertised { assert!(matches!(error,ClientError::Http {status:409,..})); }
        else { assert!(matches!(error,ClientError::UnsupportedCapability(_))); }
        assert_eq!(writes.load(Ordering::SeqCst),usize::from(advertised));
        assert_eq!(legacy.load(Ordering::SeqCst),0);
        server.abort();
    }
}

#[tokio::test]
async fn resource_read_rejects_invalid_or_oversized_snapshots() {
    let valid = json!({"revision":"a".repeat(64),"sampled_at_ms":100,"audio_participation_enabled":false,
        "browser_activity":{"enabled":true,"port":12345,"token_present":true,"url_privacy":"domain_only"}});
    for (index, body) in [valid.clone(),
        {let mut v=valid.clone();v["browser_activity"]["token_present"]=json!(false);v},
        {let mut v=valid.clone();v["browser_activity"]["port"]=json!(80);v},
        {let mut v=valid.clone();v["sampled_at_ms"]=json!(-1);v},
        {let mut v=valid.clone();v["revision"]=json!("bad");v},
        {let mut v=valid.clone();v["padding"]=json!("x".repeat(8192));v},
    ].into_iter().enumerate() {
        let router = Router::new().route("/api/v1/settings/resources", get(move || {
            let body=body.clone(); async move {Json(json!({"data":body}))}
        }));
        let (client,server)=serve(router).await;
        let result=client.resource_settings().await;
        if index==0 { assert!(result.unwrap().browser_activity.enabled); } else { assert!(result.is_err()); }
        server.abort();
    }
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
