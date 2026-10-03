use crate::{Client, ClientError};
use patina_protocol::maintenance::*;
use std::time::Duration;
impl Client {
    /// The caller supplies confirmation from its own explicit interaction.
    /// No retry: a lost response can mean the deletion already committed.
    pub async fn delete_canonical_app_history(
        &self,
        request: &CanonicalAppCleanupRequest,
    ) -> Result<CanonicalAppCleanupResult, ClientError> {
        request
            .validate()
            .map_err(ClientError::InvalidConfiguration)?;
        let capabilities = self.capabilities().await?;
        if !capabilities.write_api.available
            || !capabilities
                .write_api
                .operations
                .iter()
                .any(|op| op == "canonical-app-cleanup")
        {
            return Err(ClientError::UnsupportedCapability(
                "canonical-app-cleanup".into(),
            ));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        let result: CanonicalAppCleanupResult = self
            .post_json_with_timeout(
                "/api/v1/data/apps/delete-canonical",
                request,
                "canonical application cleanup",
                Duration::from_secs(20),
            )
            .await?;
        if result.app_key.is_empty()
            || result.app_key.len() > MAX_CLEANUP_APP_KEY_BYTES
            || result.matched_executables > MAX_CLEANUP_EXECUTABLES
        {
            return Err(ClientError::InvalidResponse(
                "invalid canonical cleanup result".into(),
            ));
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unconfirmed_cleanup_is_rejected_before_any_network_request() {
        let client = Client::new(1, "synthetic").unwrap();
        let request = CanonicalAppCleanupRequest {
            app_key: "editor".into(),
            scope: AppCleanupScope::All,
            confirmed: false,
        };
        assert!(matches!(
            client.delete_canonical_app_history(&request).await,
            Err(ClientError::InvalidConfiguration(_))
        ));
    }
}
