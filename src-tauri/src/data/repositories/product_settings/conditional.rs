use super::read_snapshot;
use crate::data::repositories::app_settings::{apply_app_settings_tx, AppSettingMutation};
use patina_protocol::product_settings::{ProductSettingsCommitRequest, ProductSettingsSnapshot};
use sqlx::SqlitePool;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    Conflict,
    InvalidInput(String),
    Storage(String),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => formatter
                .write_str("product-settings-conflict: settings changed since they were read"),
            Self::InvalidInput(message) | Self::Storage(message) => formatter.write_str(message),
        }
    }
}

pub fn validate(request: &ProductSettingsCommitRequest) -> Result<(), CommitError> {
    let patch = &request.patch;
    if !patina_protocol::configuration::is_revision(&request.expected_revision)
        || patch
            .idle_timeout_secs
            .is_some_and(|v| !(60..=86400).contains(&v))
        || patch.timeline_merge_gap_secs.is_some_and(|v| v > 86400)
        || patch
            .min_session_secs
            .is_some_and(|v| !(60..=600).contains(&v) || !v.is_multiple_of(60))
    {
        return Err(CommitError::InvalidInput(
            "invalid product revision or policy value".into(),
        ));
    }
    Ok(())
}

pub async fn commit(
    pool: &SqlitePool,
    request: &ProductSettingsCommitRequest,
    sampled_at_ms: i64,
    seal_at_ms: i64,
) -> Result<ProductSettingsSnapshot, CommitError> {
    use CommitError::{Conflict, Storage};
    validate(request)?;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| Storage(e.to_string()))?;
        let before = read_snapshot(&mut tx, sampled_at_ms)
            .await
            .map_err(Storage)?;
        if before.revision != request.expected_revision {
            return Err(Conflict);
        }
        let mut mutations = Vec::new();
        for (key, value) in [
            ("idle_timeout_secs", request.patch.idle_timeout_secs),
            (
                "timeline_merge_gap_secs",
                request.patch.timeline_merge_gap_secs,
            ),
            ("min_session_secs", request.patch.min_session_secs),
        ] {
            if let Some(value) = value {
                mutations.push(AppSettingMutation {
                    key: key.into(),
                    value: value.to_string(),
                });
            }
        }
        if let Some(paused) = request.patch.tracking_paused {
            mutations.push(AppSettingMutation {
                key: "tracking_paused".into(),
                value: if paused { "1" } else { "0" }.into(),
            });
        }
        apply_app_settings_tx(&mut tx, &mutations, seal_at_ms)
            .await
            .map_err(Storage)?;
        let snapshot = read_snapshot(&mut tx, sampled_at_ms)
            .await
            .map_err(Storage)?;
        tx.commit().await.map_err(|e| Storage(e.to_string()))?;
        Ok(snapshot)
    })
    .await
    .map_err(|_| Storage("product settings commit exceeded its time budget".into()))?
}
