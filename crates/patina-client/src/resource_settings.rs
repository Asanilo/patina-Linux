use crate::{Client, ClientError};
use patina_protocol::resource_settings::{
    ResourceSettingsCommitRequest, ResourceSettingsSnapshot, MAX_RESOURCE_SETTINGS_RESPONSE_BYTES,
};
use std::time::Duration;

impl Client {
    pub async fn resource_settings(&self) -> Result<ResourceSettingsSnapshot, ClientError> {
        let snapshot = self
            .get_json_with_limits(
                "/api/v1/settings/resources",
                "resource settings",
                Duration::from_secs(8),
                MAX_RESOURCE_SETTINGS_RESPONSE_BYTES,
            )
            .await?;
        validate(snapshot)
    }

    pub async fn commit_resource_settings(
        &self,
        request: &ResourceSettingsCommitRequest,
    ) -> Result<ResourceSettingsSnapshot, ClientError> {
        let capabilities = self.capabilities().await?;
        if !capabilities.write_api.available
            || !capabilities
                .write_api
                .operations
                .iter()
                .any(|v| v == "runtime-settings-conditional")
        {
            return Err(ClientError::UnsupportedCapability(
                "runtime-settings-conditional".into(),
            ));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        let snapshot = self
            .post_json_with_limits(
                "/api/v1/settings/resources/conditional",
                request,
                "conditional resource settings commit",
                Duration::from_secs(18),
                MAX_RESOURCE_SETTINGS_RESPONSE_BYTES,
            )
            .await?;
        validate(snapshot)
    }
}

fn validate(snapshot: ResourceSettingsSnapshot) -> Result<ResourceSettingsSnapshot, ClientError> {
    if !patina_protocol::configuration::is_revision(&snapshot.revision)
        || snapshot.sampled_at_ms < 0
        || snapshot.browser_activity.port < 1024
        || (snapshot.browser_activity.enabled && !snapshot.browser_activity.token_present)
    {
        return Err(ClientError::InvalidResponse(
            "invalid resource settings snapshot".into(),
        ));
    }
    Ok(snapshot)
}
