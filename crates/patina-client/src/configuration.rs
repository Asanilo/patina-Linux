use crate::{Client, ClientError};
use patina_protocol::configuration::{
    ClassificationCommitResult, ClassificationMutationRequest, ClassificationMutationsRequest,
    ClassificationSnapshot, MAX_CLASSIFICATION_ENTRIES, MAX_CLASSIFICATION_RESPONSE_BYTES,
    MAX_CLASSIFICATION_VALUE_BYTES,
};
use std::time::Duration;

impl Client {
    pub async fn classification_snapshot(&self) -> Result<ClassificationSnapshot, ClientError> {
        let snapshot: ClassificationSnapshot = self
            .get_json_with_limits(
                "/api/v1/settings/classification",
                "classification snapshot",
                Duration::from_secs(8),
                MAX_CLASSIFICATION_RESPONSE_BYTES,
            )
            .await?;
        if !patina_protocol::configuration::is_revision(&snapshot.revision)
            || snapshot.entries.len() > MAX_CLASSIFICATION_ENTRIES
            || snapshot.entries.iter().any(|e| {
                !patina_protocol::configuration::is_classification_key(&e.key)
                    || e.value.len() > MAX_CLASSIFICATION_VALUE_BYTES
            })
            || snapshot
                .entries
                .windows(2)
                .any(|pair| pair[0].key >= pair[1].key)
        {
            return Err(ClientError::InvalidResponse(
                "invalid classification snapshot".into(),
            ));
        }
        Ok(snapshot)
    }

    /// Explicit compare-and-set. A 409 is surfaced to the caller; no retry or rebase.
    pub async fn commit_classification(
        &self,
        expected_revision: &str,
        mutations: Vec<ClassificationMutationRequest>,
    ) -> Result<ClassificationCommitResult, ClientError> {
        if !patina_protocol::configuration::is_revision(expected_revision)
            || mutations.len() > patina_protocol::configuration::MAX_CLASSIFICATION_MUTATIONS
            || mutations.iter().any(|m| {
                !patina_protocol::configuration::is_classification_key(&m.key)
                    || m.value
                        .as_ref()
                        .is_some_and(|v| v.len() > MAX_CLASSIFICATION_VALUE_BYTES)
            })
        {
            return Err(ClientError::InvalidConfiguration(
                "invalid classification revision, mutation count, key or value".into(),
            ));
        }
        let capabilities = self.capabilities().await?;
        if !capabilities.write_api.available
            || !capabilities
                .write_api
                .operations
                .iter()
                .any(|operation| operation == "classification-conditional")
        {
            return Err(ClientError::UnsupportedCapability(
                "classification-conditional".into(),
            ));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        let result: ClassificationCommitResult = self
            .post_json_with_timeout(
                "/api/v1/settings/classification/conditional",
                &ClassificationMutationsRequest {
                    mutations,
                    expected_revision: Some(expected_revision.to_owned()),
                },
                "classification commit",
                Duration::from_secs(8),
            )
            .await?;
        if !result.ok
            || !result
                .revision
                .as_deref()
                .is_some_and(patina_protocol::configuration::is_revision)
        {
            return Err(ClientError::InvalidResponse(
                "classification commit omitted its confirmed revision".into(),
            ));
        }
        Ok(result)
    }
}
