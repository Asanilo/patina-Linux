use crate::{Client, ClientError};
use patina_protocol::product_settings::{ProductSettingsSnapshot, ProductSettingsCommitRequest, MAX_PRODUCT_SETTINGS_RESPONSE_BYTES};
use std::time::Duration;

impl Client {
    pub async fn product_settings(&self) -> Result<ProductSettingsSnapshot, ClientError> {
        let snapshot: ProductSettingsSnapshot = self.get_json_with_limits(
            "/api/v1/settings/product", "product settings", Duration::from_secs(8),
            MAX_PRODUCT_SETTINGS_RESPONSE_BYTES,
        ).await?;
        validate_snapshot(snapshot)
    }

    pub async fn commit_product_settings(&self, request: &ProductSettingsCommitRequest) -> Result<ProductSettingsSnapshot, ClientError> {
        let capabilities = self.capabilities().await?;
        if !capabilities.write_api.available || !capabilities.write_api.operations.iter().any(|value| value == "product-settings-conditional") {
            return Err(ClientError::UnsupportedCapability("product-settings-conditional".into()));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        let snapshot = self.post_json_with_limits("/api/v1/settings/product/conditional", request,
            "conditional product settings commit", Duration::from_secs(8), MAX_PRODUCT_SETTINGS_RESPONSE_BYTES).await?;
        validate_snapshot(snapshot)
    }
}

fn validate_snapshot(snapshot: ProductSettingsSnapshot) -> Result<ProductSettingsSnapshot, ClientError> {
        if !patina_protocol::configuration::is_revision(&snapshot.revision)
            || snapshot.settings.web_activity_port < 1024
            || (snapshot.settings.web_activity_enabled && !snapshot.settings.web_activity_token_present)
            || !(60..=600).contains(&snapshot.settings.min_session_secs)
            || !snapshot.settings.min_session_secs.is_multiple_of(60)
            || snapshot.last_heartbeat_ms.is_some_and(|v| v < 0)
            || snapshot.last_successful_sample_ms.is_some_and(|v| v < 0)
        {
            return Err(ClientError::InvalidResponse("invalid product settings snapshot".into()));
        }
        Ok(snapshot)
}
