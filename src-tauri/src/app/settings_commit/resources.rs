//! Existing Desktop save composition. Resource fields remain a sparse owner patch.
use crate::data::repositories::app_settings::{validate_app_setting_mutations, AppSettingMutation};
use patina_protocol::resource_settings::{
    BrowserResourcePatch, ResourceSettingsCommitRequest, ResourceSettingsPatch,
    ResourceSettingsSnapshot,
};
use tauri::{AppHandle, Runtime};

#[derive(serde::Serialize)]
pub struct SettingsConfirmation {
    pub product: patina_protocol::product_settings::ProductSettingsSnapshot,
    pub resources: ResourceSettingsSnapshot,
}

pub async fn commit<R: Runtime>(
    app: &AppHandle<R>,
    mutations: Vec<AppSettingMutation>,
    expected_product_revision: Option<String>,
    expected_resource_revision: String,
) -> Result<SettingsConfirmation, String> {
    validate_app_setting_mutations(&mutations)?;
    if mutations.len() > 256 {
        return Err("too many settings mutations".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut resources = ResourceSettingsPatch::default();
    let mut browser = BrowserResourcePatch::default();
    let mut policy = Vec::new();
    let mut preferences = Vec::new();
    for mutation in mutations {
        if !seen.insert(mutation.key.clone()) {
            return Err("duplicate settings key".into());
        }
        let boolean = || match mutation.value.as_str() {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err("resource switch must be 0 or 1".to_string()),
        };
        match mutation.key.as_str() {
            "audio_participation_enabled" => {
                resources.audio_participation_enabled = Some(boolean()?)
            }
            "web_activity_enabled" => browser.enabled = Some(boolean()?),
            "web_activity_port" => {
                let port = mutation
                    .value
                    .parse::<u16>()
                    .map_err(|_| "invalid browser port")?;
                if port < 1024 {
                    return Err("invalid browser port".into());
                }
                browser.port = Some(port);
            }
            "web_activity_token" => {
                let token = mutation.value.trim();
                if token.len() > 512 || token.chars().any(char::is_control) {
                    return Err("invalid browser token".into());
                }
                browser.token = Some(token.into());
            }
            "web_activity_url_privacy" => {
                browser.url_privacy = Some(
                    serde_json::from_value(serde_json::Value::String(mutation.value))
                        .map_err(|_| "invalid browser URL privacy")?,
                )
            }
            "idle_timeout_secs"
            | "timeline_merge_gap_secs"
            | "min_session_secs"
            | "tracking_paused" => policy.push(mutation),
            _ => preferences.push(mutation),
        }
    }
    if browser != BrowserResourcePatch::default() {
        resources.browser_activity = Some(browser);
    }
    if resources.is_empty() {
        return Err("resource settings patch is empty".into());
    }
    let client = crate::app::daemon_client::command_client(app)?
        .ok_or("conditional resource settings require the daemon owner")?;
    // Validate unrelated local/host settings before either conditional write.
    crate::app::daemon_client::prepare_owned_app_settings(app, preferences.clone()).await?;
    let capabilities = client
        .transport()
        .capabilities()
        .await
        .map_err(|e| e.to_string())?;
    if !capabilities.write_api.available
        || !capabilities
            .write_api
            .operations
            .iter()
            .any(|value| value == "runtime-settings-conditional")
    {
        return Err("daemon does not support conditional resource settings".into());
    }
    patina_client::negotiate_tracking_capabilities(capabilities).map_err(|e| e.to_string())?;
    let before = client
        .transport()
        .resource_settings()
        .await
        .map_err(|e| e.to_string())?;
    if before.revision != expected_resource_revision {
        return Err("resource-settings-conflict: settings changed since they were read".into());
    }
    if let Some(browser) = resources.browser_activity.as_ref() {
        let enabled = browser.enabled.unwrap_or(before.browser_activity.enabled);
        let token_present = browser
            .token
            .as_ref()
            .map_or(before.browser_activity.token_present, |v| !v.is_empty());
        if enabled && !token_present {
            return Err(
                "browser activity token is required when synchronization is enabled".into(),
            );
        }
    }
    if !policy.is_empty() {
        super::commit_if_revision(
            app,
            policy,
            expected_product_revision.ok_or("product settings baseline is unavailable")?,
        )
        .await?;
    }
    // The original resource baseline is retained even if another writer wins
    // after the preflight. This is not a transaction spanning the policy endpoint.
    let confirmed = client
        .transport()
        .commit_resource_settings(&ResourceSettingsCommitRequest {
            expected_revision: expected_resource_revision,
            patch: resources,
        })
        .await
        .map_err(|e| e.to_string())?;
    super::commit_legacy(app, preferences).await?;
    let product = client.product_settings().await.map_err(|e| e.to_string())?;
    Ok(SettingsConfirmation {
        product,
        resources: confirmed,
    })
}
