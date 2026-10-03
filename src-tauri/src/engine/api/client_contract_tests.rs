//! Real loopback server + SQLite transactions, with independent and Desktop clients.
//! No AppHandle, user profile, systemd operation or installed application is used.
use super::{
    auth::ApiCredentialStore,
    context::{ApiRuntimeContext, UnavailableApiRuntimeState},
    server::prepare_standalone_server_with_events,
    surface::ApiSurface,
};
use crate::engine::{runtime_context::RuntimeContext, runtime_event::RuntimeEventHub};
use crate::platform::daemon_client::{PatinadClient, PatinadStreamEvent};
use serde_json::{json, Value};
use sqlx::Executor;
use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn independent_and_desktop_clients_observe_the_same_committed_classification_and_replay() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES('Fixture','fixture-app','synthetic',1000,2000,1000)").await.unwrap();
    let hub = Arc::new(RuntimeEventHub::new(2));
    let context = ApiRuntimeContext::with_state_and_events(
        RuntimeContext::system(pool.clone()),
        "1.9.2",
        "linux",
        Arc::new(UnavailableApiRuntimeState),
        Some(hub.clone()),
    );
    let credentials = ApiCredentialStore::new();
    let token_path = std::env::temp_dir().join(format!(
        "patina-sdk-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    credentials
        .initialize_at(&token_path, Some("synthetic-client-contract"))
        .unwrap();
    std::fs::remove_file(&token_path).unwrap(); // Store already owns the fixture credential.
    let server = prepare_standalone_server_with_events(
        0,
        credentials,
        context,
        ApiSurface::DaemonTracking,
        hub,
    )
    .await
    .unwrap();
    let port = server.port();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let native = patina_client::Client::new(port, "synthetic-client-contract").unwrap();
    let desktop = PatinadClient::new(port, "synthetic-client-contract").unwrap();
    assert_eq!(
        native.negotiate_tracking_owner().await.unwrap(),
        desktop.negotiate_tracking_owner().await.unwrap()
    );

    // Subscribe first, then load a snapshot: a commit cannot fall into a blind gap.
    let mut native_events = native.open_event_stream(None).await.unwrap();
    let mut desktop_events = desktop.open_event_stream(None).await.unwrap();
    assert!(native_events.instance_id().is_some());
    assert_eq!(native_events.instance_id(), desktop_events.instance_id());
    let before: Value = native.get_json("/api/v1/apps", "apps").await.unwrap();
    assert_eq!(before["apps"][0]["exe_name"], "fixture-app");
    native
        .post_ack(
            "/api/v1/apps/fixture-app/classify",
            &json!({"category":"research"}),
            "classify",
        )
        .await
        .unwrap();
    let native_event = tokio::time::timeout(Duration::from_secs(3), native_events.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let desktop_event = tokio::time::timeout(Duration::from_secs(3), desktop_events.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(native_event.event, "tracking-data-changed");
    assert_eq!(
        desktop_event.sequence(),
        Some(native_event.id.parse().unwrap())
    );
    assert!(matches!(desktop_event, PatinadStreamEvent::Runtime(_)));
    let after: Value = native.get_json("/api/v1/apps", "apps").await.unwrap();
    assert_eq!(after["apps"][0]["category"], "research");

    // A fresh client sees committed state through the same API, with no DB access.
    let another = patina_client::Client::new(port, "synthetic-client-contract").unwrap();
    assert_eq!(
        after,
        another
            .get_json::<Value>("/api/v1/apps", "apps")
            .await
            .unwrap()
    );
    drop(native_events);
    native
        .post_ack(
            "/api/v1/apps/fixture-app/rename",
            &json!({"display_name":"Updated fixture"}),
            "rename",
        )
        .await
        .unwrap();
    let mut reconnected = native
        .open_event_stream(Some(native_event.id.parse().unwrap()))
        .await
        .unwrap();
    let replay = tokio::time::timeout(Duration::from_secs(3), reconnected.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(replay.event, "tracking-data-changed");
    assert!(replay.id.parse::<u64>().unwrap() > native_event.id.parse::<u64>().unwrap());
    let refreshed: Value = another.get_json("/api/v1/apps", "apps").await.unwrap();
    assert_eq!(refreshed["apps"][0]["display_name"], "Updated fixture");

    // The complete configuration is a public, versioned read, not a Desktop SQL shortcut.
    sqlx::query("INSERT INTO settings(key,value) VALUES('local_api_token','sensitive-fixture')")
        .execute(&pool)
        .await
        .unwrap();
    let baseline = native.classification_snapshot().await.unwrap();
    assert!(!serde_json::to_string(&baseline)
        .unwrap()
        .contains("sensitive-fixture"));
    let desktop_configuration = desktop.classification_snapshot().await.unwrap();
    assert_eq!(baseline.revision, desktop_configuration.revision);
    assert_eq!(baseline.entries, desktop_configuration.entries);
    let mut conditional_events = another.open_runtime_event_stream(None).await.unwrap();
    let result = native
        .commit_classification(
            &baseline.revision,
            vec![
                patina_protocol::configuration::ClassificationMutationRequest {
                    key: "__category_label_override::office".into(),
                    value: Some("Office".into()),
                },
            ],
        )
        .await
        .unwrap();
    let conflict = another
        .commit_classification(
            &baseline.revision,
            vec![
                patina_protocol::configuration::ClassificationMutationRequest {
                    key: "__category_label_override::music".into(),
                    value: Some("Lost edit".into()),
                },
            ],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        conflict,
        patina_client::ClientError::Http { status: 409, .. }
    ));
    let committed = another.classification_snapshot().await.unwrap();
    assert_eq!(Some(committed.revision), result.revision);
    assert!(!committed
        .entries
        .iter()
        .any(|entry| entry.value == "Lost edit"));
    let notice = tokio::time::timeout(Duration::from_secs(1), conditional_events.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(notice, PatinadStreamEvent::Runtime(_)));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), conditional_events.next_event())
            .await
            .is_err()
    );
    let missing_precondition = native
        .post_ack(
            "/api/v1/settings/classification/conditional",
            &json!({"mutations":[]}),
            "conditional update",
        )
        .await
        .unwrap_err();
    assert!(matches!(
        missing_precondition,
        patina_client::ClientError::Http { status: 400, .. }
    ));
    drop(conditional_events);
    // The independent SDK and Desktop consume the same product projection.
    use chrono::TimeZone;
    let day = chrono::Local
        .timestamp_millis_opt(1000)
        .single()
        .unwrap()
        .date_naive();
    let from = day.format("%Y-%m-%d").to_string();
    let to = day.succ_opt().unwrap().format("%Y-%m-%d").to_string();
    let product = native.daily_product(&from, &to, "en-US").await.unwrap();
    let desktop_product = desktop.daily_product(&from, &to, "en-US").await.unwrap();
    assert_eq!(product.days, desktop_product.days);
    assert_eq!(product.applications, desktop_product.applications);
    assert_eq!(
        product.configuration_revision,
        desktop_product.configuration_revision
    );
    assert_eq!(product.days[0].active_ms, 1000);
    let dashboard = native.dashboard(&from, "en-US").await.unwrap();
    let history = native.exact_history(1000, 2000, "en-US").await.unwrap();
    assert_eq!(history.records.len(), 1);
    assert_eq!(history.records[0].start_ms, 1000);
    assert_eq!(history.records[0].end_ms, 2000);
    assert_eq!(history.records[0].window_title, "synthetic");
    assert!(history.records[0].title_samples.is_empty()); // Captions are not invented dated samples.
    assert_eq!(
        history.configuration_revision,
        product.configuration_revision
    );
    assert_eq!(
        history.records,
        another
            .exact_history(1000, 2000, "en-US")
            .await
            .unwrap()
            .records
    );
    let bad_history = native
        .get_json::<Value>(
            "/api/v1/activity/history?from_ms=0&to_ms=1&from_ms=2",
            "invalid history",
        )
        .await
        .unwrap_err();
    assert!(matches!(
        bad_history,
        patina_client::ClientError::Http { status: 400, .. }
    ));
    let desktop_dashboard = desktop.dashboard(&from, "en-US").await.unwrap();
    assert_eq!(dashboard.current, desktop_dashboard.current);
    assert_eq!(dashboard.hours, desktop_dashboard.hours);
    assert_eq!(dashboard.current, product.days[0]);
    assert_eq!(
        dashboard.configuration_revision,
        product.configuration_revision
    );
    assert_eq!(dashboard.hours.len(), 24);
    assert_eq!(
        dashboard
            .hours
            .iter()
            .map(|hour| hour.active_ms)
            .sum::<i64>(),
        1000
    );
    assert_eq!(product.applications[0].category, "other"); // Legacy free-text 'research' is not a user-assignable product category.
    assert_eq!(
        product.applications[0].display_name_override.as_deref(),
        Some("Updated fixture")
    );
    drop(reconnected);
    drop(desktop_events);
    shutdown.shutdown();
    task.await.unwrap();
    pool.close().await;
}
