use crate::engine::api::runtime_control::{ApiRuntimeControl, BrowserActivityRuntimeConfiguration};
use patina_protocol::resource_settings::{
    BrowserResourcePatch, ResourceSettingsCommitRequest, ResourceSettingsPatch,
};

fn port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[cfg(feature = "desktop-tests")]
#[tokio::test]
async fn desktop_resource_save_is_sparse_and_conflict_keeps_other_settings_unchanged() {
    use crate::data::repositories::app_settings::AppSettingMutation;
    let (pool, control, web_control, _, audio, listener, credentials, _, token_path) =
        super::super::tests::test_control().await;
    let context = crate::engine::api::context::ApiRuntimeContext::new(
        crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
    )
    .with_runtime_control(control.clone());
    let api_port = listener.start(0, context).await.unwrap();
    control
        .configure_browser_activity(BrowserActivityRuntimeConfiguration {
            enabled: true,
            port: port(),
            token: "desktop-fixture-secret".into(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
        })
        .await
        .unwrap();
    let native =
        crate::platform::daemon_client::PatinadClient::new(api_port, credentials.token().unwrap())
            .unwrap();
    let product = native.product_settings().await.unwrap();
    let resource = native.transport().resource_settings().await.unwrap();
    let state = crate::app::daemon_client::PatinadClientState::default();
    state.install(native);
    // No Desktop SQLite pool is installed: assembling this resource patch must
    // not read browser credentials or aggregate a replacement from client SQL.
    let app = tauri::test::mock_builder()
        .manage(crate::app::runtime::DesktopRuntimeMode::DaemonClientPreview)
        .manage(state)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let mutation = |key: &str, value: &str| AppSettingMutation {
        key: key.into(),
        value: value.into(),
    };
    let failure = crate::app::settings_commit::resources::commit(
        app.handle(),
        vec![
            mutation("web_activity_port", "12346"),
            mutation("theme_mode", "dark"),
        ],
        None,
        "f".repeat(64),
    )
    .await;
    assert!(matches!(failure,Err(message) if message.contains("resource-settings-conflict")));
    assert!(
        crate::data::repositories::tracker_settings::load_setting_value(&pool, "theme_mode")
            .await
            .unwrap()
            .is_none()
    );
    let new_port = port();
    let confirmed = crate::app::settings_commit::resources::commit(
        app.handle(),
        vec![
            mutation("web_activity_port", &new_port.to_string()),
            mutation("audio_participation_enabled", "0"),
            mutation("min_session_secs", "360"),
            mutation("theme_mode", "dark"),
        ],
        Some(product.revision),
        resource.revision.clone(),
    )
    .await
    .unwrap();
    assert_eq!(confirmed.resources.browser_activity.port, new_port);
    assert_eq!(confirmed.product.settings.min_session_secs, 360);
    assert!(!audio.is_enabled());
    assert_eq!(
        crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
            .await
            .unwrap()
            .token,
        "desktop-fixture-secret"
    );
    assert_eq!(
        crate::data::repositories::tracker_settings::load_setting_value(&pool, "theme_mode")
            .await
            .unwrap()
            .as_deref(),
        Some("dark")
    );
    let rejected = crate::app::settings_commit::resources::commit(
        app.handle(),
        vec![
            mutation("web_activity_port", &port().to_string()),
            mutation("min_session_secs", "420"),
            mutation("theme_mode", "light"),
        ],
        Some(confirmed.product.revision),
        resource.revision,
    )
    .await;
    assert!(matches!(rejected,Err(message) if message.contains("resource-settings-conflict")));
    assert_eq!(
        crate::data::repositories::product_settings::load_snapshot(&pool, 1)
            .await
            .unwrap()
            .settings
            .min_session_secs,
        360
    );
    assert_eq!(
        crate::data::repositories::tracker_settings::load_setting_value(&pool, "theme_mode")
            .await
            .unwrap()
            .as_deref(),
        Some("dark")
    );
    control.close_and_drain_resources().await;
    web_control.shutdown().await;
    listener.shutdown().await;
    pool.close().await;
    let _ = std::fs::remove_file(token_path);
}

#[tokio::test]
async fn independent_sdk_resource_patch_preserves_omitted_fields_and_rejects_old_versions() {
    let (pool, control, web_control, sink, audio, listener, credentials, _, token_path) =
        super::super::tests::test_control().await;
    let context = crate::engine::api::context::ApiRuntimeContext::new(
        crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
    )
    .with_runtime_control(control.clone());
    let api_port = listener.start(0, context).await.unwrap();
    let first = patina_client::Client::new(api_port, credentials.token().unwrap()).unwrap();
    let second = patina_client::Client::new(api_port, credentials.token().unwrap()).unwrap();
    let baseline = first.resource_settings().await.unwrap();
    let enabled = first
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: baseline.revision,
            patch: ResourceSettingsPatch {
                browser_activity: Some(BrowserResourcePatch {
                    enabled: Some(true),
                    port: Some(port()),
                    token: Some(" private-browser-token ".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    assert!(enabled.browser_activity.enabled);
    let changed = second
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: enabled.revision.clone(),
            patch: ResourceSettingsPatch {
                audio_participation_enabled: Some(false),
                browser_activity: Some(BrowserResourcePatch {
                    port: Some(port()),
                    url_privacy: Some(
                        patina_protocol::web_history::WebActivityUrlPrivacyMode::DomainOnly,
                    ),
                    ..Default::default()
                }),
            },
        })
        .await
        .unwrap();
    assert!(!audio.is_enabled());
    assert!(!changed.audio_participation_enabled);
    let stored = crate::data::repositories::app_settings::load_web_activity_bridge_settings(&pool)
        .await
        .unwrap();
    assert_eq!(stored.token, "private-browser-token");
    assert!(stored.enabled);
    assert!(!serde_json::to_string(&changed)
        .unwrap()
        .contains("private-browser-token"));
    drop(
        tokio::net::TcpStream::connect(("127.0.0.1", changed.browser_activity.port))
            .await
            .unwrap(),
    );
    let events = sink.events().len();
    let stale = first
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: enabled.revision,
            patch: ResourceSettingsPatch {
                audio_participation_enabled: Some(true),
                ..Default::default()
            },
        })
        .await;
    assert!(matches!(
        stale,
        Err(patina_client::ClientError::Http { status: 409, .. })
    ));
    assert_eq!(sink.events().len(), events);
    assert!(!audio.is_enabled());

    // An old complete-replacement caller also invalidates the conditional baseline,
    // even when only the credential changes and the public fields stay identical.
    control
        .configure_browser_activity(BrowserActivityRuntimeConfiguration {
            enabled: true,
            port: stored.port,
            token: "rotated-browser-token".into(),
            url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly,
        })
        .await
        .unwrap();
    assert!(matches!(
        first
            .commit_resource_settings(&ResourceSettingsCommitRequest {
                expected_revision: changed.revision,
                patch: ResourceSettingsPatch {
                    browser_activity: Some(BrowserResourcePatch {
                        enabled: Some(false),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            })
            .await,
        Err(patina_client::ClientError::Http { status: 409, .. })
    ));
    let fresh = first.resource_settings().await.unwrap();
    let events = sink.events().len();
    let no_op = first
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: fresh.revision.clone(),
            patch: Default::default(),
        })
        .await
        .unwrap();
    assert_eq!(no_op.revision, fresh.revision);
    assert_eq!(sink.events().len(), events);

    // Listener reservation failure cannot partially change the audio switch.
    let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let failure = second
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: fresh.revision.clone(),
            patch: ResourceSettingsPatch {
                audio_participation_enabled: Some(true),
                browser_activity: Some(BrowserResourcePatch {
                    port: Some(occupied.local_addr().unwrap().port()),
                    ..Default::default()
                }),
            },
        })
        .await;
    assert!(matches!(
        failure,
        Err(patina_client::ClientError::Http { status: 409, .. })
    ));
    assert_eq!(
        first.resource_settings().await.unwrap().revision,
        fresh.revision
    );
    assert!(!audio.is_enabled());
    assert_eq!(sink.events().len(), events);
    drop(occupied);
    control.close_and_drain_resources().await;
    web_control.shutdown().await;
    listener.shutdown().await;
    pool.close().await;
    let _ = std::fs::remove_file(token_path);
}
