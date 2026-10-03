//! Desktop settings write composition. SQL and tracking transitions stay in their owners.
use crate::data::app_settings_service::commit_app_setting_mutations_with_recovery;
use crate::data::repositories::app_settings::AppSettingMutation;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub async fn commit_if_revision<R: Runtime>(
    app: &AppHandle<R>,
    mutations: Vec<AppSettingMutation>,
    expected_revision: String,
) -> Result<patina_protocol::product_settings::ProductSettingsSnapshot, String> {
    use patina_protocol::product_settings::{ProductSettingsCommitRequest, ProductSettingsPatch};
    crate::data::repositories::app_settings::validate_app_setting_mutations(&mutations)?;
    if mutations.len() > 256 {
        return Err("too many settings mutations".into());
    }
    let mut keys = std::collections::HashSet::new();
    let mut patch = ProductSettingsPatch::default();
    let mut remaining = Vec::new();
    for mutation in mutations {
        if !keys.insert(mutation.key.clone()) {
            return Err("duplicate settings key".into());
        }
        let number = || {
            mutation
                .value
                .parse::<u64>()
                .map_err(|_| "policy value must be an integer".to_string())
        };
        match mutation.key.as_str() {
            "idle_timeout_secs" => patch.idle_timeout_secs = Some(number()?),
            "timeline_merge_gap_secs" => patch.timeline_merge_gap_secs = Some(number()?),
            "min_session_secs" => patch.min_session_secs = Some(number()?),
            "tracking_paused" => {
                patch.tracking_paused = Some(match mutation.value.as_str() {
                    "0" => false,
                    "1" => true,
                    _ => return Err("pause policy must be 0 or 1".into()),
                })
            }
            _ => remaining.push(mutation),
        }
    }
    let request = ProductSettingsCommitRequest {
        expected_revision,
        patch,
    };
    crate::data::repositories::product_settings::conditional::validate(&request)
        .map_err(|e| e.to_string())?;
    // Preparing the remainder validates its configuration without touching resources.
    let plan =
        crate::app::daemon_client::prepare_owned_app_settings(app, remaining.clone()).await?;
    if let Some(client) = crate::app::daemon_client::command_client(app)? {
        // A conflict exits here, before any ordinary client preference or runtime resource write.
        client
            .commit_product_settings(&request)
            .await
            .map_err(|e| e.to_string())?;
        let remaining = plan.apply(&client).await?;
        if !remaining.is_empty() {
            client
                .commit_app_settings(
                    remaining
                        .into_iter()
                        .map(|m| crate::engine::api::types::AppSettingMutationRequest {
                            key: m.key,
                            value: m.value,
                        })
                        .collect(),
                )
                .await
                .map_err(|e| e.to_string())?;
        }
        app.emit("app-settings-changed", json!({}))
            .map_err(|e| e.to_string())?;
        return client.product_settings().await.map_err(|e| e.to_string());
    }
    let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(app).await?;
    let state =
        app.state::<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>();
    let runtime = crate::engine::runtime_context::RuntimeContext::system(pool.clone());
    crate::engine::tracking::runtime_settings::commit_product_settings(
        &runtime,
        Some(state.inner()),
        &request,
    )
    .await
    .map_err(|e| e.to_string())?;
    let audio_enabled = remaining
        .iter()
        .find(|mutation| mutation.key == "audio_participation_enabled")
        .map(|mutation| crate::domain::settings::parse_boolean_setting(&mutation.value, true));
    commit_legacy(app, remaining).await?;
    if let Some(enabled) = audio_enabled {
        #[cfg(target_os = "linux")]
        crate::platform::linux::audio::set_signal_source_enabled(enabled);
        #[cfg(target_os = "windows")]
        crate::platform::windows::audio::set_signal_source_enabled(enabled);
    }
    crate::data::repositories::product_settings::load_snapshot(
        &pool,
        crate::engine::runtime_context::now_ms().min(i64::MAX as u64) as i64,
    )
    .await
}

pub async fn commit_legacy<R: Runtime>(
    app: &AppHandle<R>,
    mut mutations: Vec<AppSettingMutation>,
) -> Result<(), String> {
    if mutations
        .iter()
        .any(|mutation| mutation.key == "background_tracking_at_login")
    {
        return Err(
            "background tracking login preference requires the dedicated service command"
                .to_string(),
        );
    }

    if let Some(client) = crate::app::daemon_client::command_client(app)? {
        mutations =
            crate::app::daemon_client::route_owned_app_settings(app, &client, mutations).await?;
        if !mutations.is_empty() {
            let daemon_mutations = mutations
                .into_iter()
                .map(
                    |mutation| crate::engine::api::types::AppSettingMutationRequest {
                        key: mutation.key,
                        value: mutation.value,
                    },
                )
                .collect();
            client
                .commit_app_settings(daemon_mutations)
                .await
                .map_err(|error| error.to_string())?;
        }
        app.emit("app-settings-changed", json!({}))
            .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
        return Ok(());
    }

    if !mutations.is_empty() {
        let changes_tracking_policy = mutations.iter().any(|mutation| {
            matches!(
                mutation.key.as_str(),
                "tracking_paused"
                    | "web_activity_enabled"
                    | "web_activity_token"
                    | "web_activity_port"
            )
        });
        let runtime_state = app
            .state::<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>()
            .inner()
            .clone();
        let _transition_guard = if changes_tracking_policy {
            Some(runtime_state.lock_transition().await)
        } else {
            None
        };
        commit_app_setting_mutations_with_recovery(app, &mutations).await?;
        if changes_tracking_policy {
            runtime_state.note_tracking_policy_change();
        }
    }
    app.emit("app-settings-changed", json!({}))
        .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
    Ok(())
}

#[cfg(all(test, feature = "desktop-tests"))]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[tokio::test]
    async fn mixed_desktop_save_guards_conflicts_and_reports_later_resource_failure() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        let hub = std::sync::Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(8));
        let context = crate::engine::api::context::ApiRuntimeContext::with_state_and_events(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
            "fixture",
            "linux",
            std::sync::Arc::new(crate::engine::api::context::UnavailableApiRuntimeState),
            Some(hub.clone()),
        );
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        let token_path = std::env::temp_dir().join(format!(
            "patina-mixed-settings-{}-{}",
            std::process::id(),
            crate::engine::runtime_context::now_ms()
        ));
        credentials
            .initialize_at(&token_path, Some("synthetic-policy-test"))
            .unwrap();
        std::fs::remove_file(token_path).unwrap();
        let server = crate::engine::api::server::prepare_standalone_server_with_events(
            0,
            credentials,
            context,
            crate::engine::api::surface::ApiSurface::DaemonTracking,
            hub,
        )
        .await
        .unwrap();
        let client = crate::platform::daemon_client::PatinadClient::new(
            server.port(),
            "synthetic-policy-test",
        )
        .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let state = crate::app::daemon_client::PatinadClientState::default();
        state.install(client);
        let app = tauri::test::mock_builder()
            .manage(crate::app::runtime::DesktopRuntimeMode::DaemonClientPreview)
            .manage(state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let before = crate::data::repositories::product_settings::load_snapshot(&pool, 1)
            .await
            .unwrap();
        let error = commit_if_revision(
            app.handle(),
            vec![
                AppSettingMutation {
                    key: "min_session_secs".into(),
                    value: "360".into(),
                },
                AppSettingMutation {
                    key: "theme_mode".into(),
                    value: "dark".into(),
                },
                AppSettingMutation {
                    key: "audio_participation_enabled".into(),
                    value: "0".into(),
                },
            ],
            "f".repeat(64),
        )
        .await
        .unwrap_err();
        assert!(error.contains("409"), "{error}");
        assert_eq!(
            crate::data::repositories::product_settings::load_snapshot(&pool, 2)
                .await
                .unwrap()
                .revision,
            before.revision
        );
        for key in [
            "min_session_secs",
            "theme_mode",
            "audio_participation_enabled",
        ] {
            assert_eq!(
                crate::data::repositories::tracker_settings::load_setting_value(&pool, key)
                    .await
                    .unwrap(),
                None
            );
        }
        let confirmed = commit_if_revision(
            app.handle(),
            vec![
                AppSettingMutation {
                    key: "min_session_secs".into(),
                    value: "360".into(),
                },
                AppSettingMutation {
                    key: "theme_mode".into(),
                    value: "dark".into(),
                },
            ],
            before.revision,
        )
        .await
        .unwrap();
        assert_eq!(confirmed.settings.min_session_secs, 360);
        assert_eq!(
            crate::data::repositories::tracker_settings::load_setting_value(&pool, "theme_mode")
                .await
                .unwrap()
                .as_deref(),
            Some("dark")
        );
        // This fixture has no resource controller. The later audio operation must
        // report failure, without pretending to roll back the already committed policy.
        let error = commit_if_revision(
            app.handle(),
            vec![
                AppSettingMutation {
                    key: "min_session_secs".into(),
                    value: "420".into(),
                },
                AppSettingMutation {
                    key: "audio_participation_enabled".into(),
                    value: "0".into(),
                },
            ],
            confirmed.revision,
        )
        .await
        .unwrap_err();
        assert!(error.contains("503"), "{error}");
        let current = crate::data::repositories::product_settings::load_snapshot(&pool, 3)
            .await
            .unwrap();
        assert_eq!(current.settings.min_session_secs, 420);
        assert!(current.settings.audio_participation_enabled);
        shutdown.shutdown();
        task.await.unwrap();
        pool.close().await;
    }
}
