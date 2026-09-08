use crate::engine::api::activity_import_owner::{
    ActivityImportOwner, ActivityImportOwnerError, ActivityImportOwnerFuture,
};
use crate::engine::runtime_context::RuntimeContext;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub struct DaemonActivityImportOwner {
    runtime: RuntimeContext,
    staging_root: PathBuf,
}

impl DaemonActivityImportOwner {
    pub fn new(runtime: RuntimeContext, staging_root: PathBuf) -> Self {
        Self {
            runtime,
            staging_root,
        }
    }
}

impl ActivityImportOwner for DaemonActivityImportOwner {
    fn commit_staged(
        &self,
        ticket: String,
        source_name: String,
        expected_fingerprint: String,
    ) -> ActivityImportOwnerFuture<'_, crate::domain::activity_import::ImportCommitReportDto> {
        Box::pin(async move {
            let bytes = crate::platform::activity_import_staging::consume_bytes(
                &self.staging_root,
                &ticket,
            )
            .map_err(map_staging_error)?;
            let actual_fingerprint = format!("{:x}", Sha256::digest(&bytes));
            if actual_fingerprint != expected_fingerprint {
                return Err(ActivityImportOwnerError::Conflict(
                    "staged canonical CSV does not match the preview fingerprint".to_string(),
                ));
            }
            let parsed = crate::engine::activity_import::parse_canonical_csv(&bytes)
                .map_err(ActivityImportOwnerError::InvalidInput)?;
            crate::data::repositories::activity_import::commit_records(
                self.runtime.pool(),
                &source_name,
                &actual_fingerprint,
                &parsed.records,
                parsed.errors.len(),
            )
            .await
            .map_err(ActivityImportOwnerError::Internal)
        })
    }

    fn delete_batch(
        &self,
        batch_id: String,
    ) -> ActivityImportOwnerFuture<'_, crate::domain::activity_import::ImportDeleteReportDto> {
        Box::pin(async move {
            crate::data::repositories::activity_import::delete(self.runtime.pool(), &batch_id)
                .await
                .map_err(|error| {
                    if error == "import batch no longer exists" {
                        ActivityImportOwnerError::NotFound(error)
                    } else {
                        ActivityImportOwnerError::Internal(error)
                    }
                })
        })
    }
}

fn map_staging_error(error: String) -> ActivityImportOwnerError {
    if error.contains("ticket is invalid") {
        ActivityImportOwnerError::InvalidInput(error)
    } else if error.contains("is unavailable") {
        ActivityImportOwnerError::NotFound("staged activity import no longer exists".to_string())
    } else {
        ActivityImportOwnerError::Internal(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::activity_import_owner::ActivityImportOwner;

    const CSV: &[u8] = b"record_type,start_time,end_time,duration_ms,exe_name,app_name,title,category\nexact_session,2026-01-15T09:00:00+08:00,2026-01-15T09:30:00+08:00,1800000,org.example.Editor,Editor,Work,Development\n";

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-daemon-import-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    #[tokio::test]
    async fn daemon_owner_revalidates_and_commits_staged_import() {
        let root = temp_root("commit");
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("patina.db"),
            true,
        )
        .await
        .unwrap();
        let staging_root = root.join("staging");
        let ticket =
            crate::platform::activity_import_staging::stage_bytes(&staging_root, CSV).unwrap();
        let fingerprint = format!("{:x}", Sha256::digest(CSV));
        let owner = DaemonActivityImportOwner::new(
            RuntimeContext::system(pool.clone()),
            staging_root.clone(),
        );

        let report = owner
            .commit_staged(ticket.clone(), "activity.csv".into(), fingerprint)
            .await
            .unwrap();

        assert_eq!(report.imported_records, 1);
        assert!(!staging_root.join(format!("{ticket}.csv")).exists());
        let fingerprints = crate::data::repositories::activity_import::load_fingerprints(&pool)
            .await
            .unwrap();
        assert_eq!(fingerprints.len(), 1);
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn daemon_owner_consumes_but_rejects_changed_staged_import() {
        let root = temp_root("changed");
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("patina.db"),
            true,
        )
        .await
        .unwrap();
        let staging_root = root.join("staging");
        let ticket =
            crate::platform::activity_import_staging::stage_bytes(&staging_root, CSV).unwrap();
        let owner = DaemonActivityImportOwner::new(
            RuntimeContext::system(pool.clone()),
            staging_root.clone(),
        );

        let error = owner
            .commit_staged(ticket.clone(), "activity.csv".into(), "0".repeat(64))
            .await
            .unwrap_err();

        assert!(matches!(error, ActivityImportOwnerError::Conflict(_)));
        assert!(!staging_root.join(format!("{ticket}.csv")).exists());
        let batches = crate::data::repositories::activity_import::list(&pool)
            .await
            .unwrap();
        assert!(batches.is_empty());
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
