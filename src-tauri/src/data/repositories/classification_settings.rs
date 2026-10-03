use sqlx::{Pool, Sqlite};

mod snapshot;
pub use snapshot::load_classification_snapshot;
pub(crate) use snapshot::read_snapshot as read_classification_snapshot;

const APP_OVERRIDE_KEY_PREFIX: &str = "__app_override::";
const WEB_DOMAIN_OVERRIDE_KEY_PREFIX: &str = "__web_domain_override::";
use patina_protocol::configuration::MAX_CLASSIFICATION_VALUE_BYTES as MAX_SETTING_VALUE_LEN;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassificationSettingMutation {
    pub key: String,
    pub value: Option<String>,
}

pub async fn commit_classification_setting_mutations(
    pool: &Pool<Sqlite>,
    mutations: &[ClassificationSettingMutation],
) -> Result<(), String> {
    if mutations.is_empty() {
        return Ok(());
    }
    validate_classification_setting_mutations(mutations)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start classification settings transaction: {error}"))?;

    apply_mutations(&mut tx, mutations).await?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit classification settings transaction: {error}")
    })?;

    Ok(())
}

async fn apply_mutations(
    connection: &mut sqlx::SqliteConnection,
    mutations: &[ClassificationSettingMutation],
) -> Result<(), String> {
    for mutation in mutations {
        if let Some(value) = &mutation.value {
            sqlx::query(
                "INSERT INTO settings (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(&mutation.key)
            .bind(value)
            .execute(&mut *connection)
            .await
            .map_err(|error| format!("failed to save classification setting: {error}"))?;
        } else {
            sqlx::query("DELETE FROM settings WHERE key = ?")
                .bind(&mutation.key)
                .execute(&mut *connection)
                .await
                .map_err(|error| format!("failed to delete classification setting: {error}"))?;
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConditionalCommitError {
    Conflict,
    InvalidInput(String),
    Storage(String),
}

/// The write lock is acquired before computing the expected state, so two
/// conditional writers cannot both accept the same revision and lose an edit.
pub async fn commit_classification_if_revision(
    pool: &Pool<Sqlite>,
    mutations: &[ClassificationSettingMutation],
    expected_revision: &str,
    sampled_at_ms: i64,
) -> Result<patina_protocol::configuration::ClassificationCommitResult, ConditionalCommitError> {
    use ConditionalCommitError::{Conflict, InvalidInput, Storage};
    if !patina_protocol::configuration::is_revision(expected_revision)
        || mutations.len() > patina_protocol::configuration::MAX_CLASSIFICATION_MUTATIONS
    {
        return Err(InvalidInput(
            "invalid classification revision or mutation count".into(),
        ));
    }
    validate_classification_setting_mutations(mutations).map_err(InvalidInput)?;
    tokio::time::timeout(snapshot::CONFIGURATION_TIMEOUT, async {
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| Storage(format!("classification write transaction failed: {e}")))?;
        let before = snapshot::read_snapshot(&mut tx, sampled_at_ms)
            .await
            .map_err(Storage)?;
        if before.revision != expected_revision {
            return Err(Conflict);
        }
        apply_mutations(&mut tx, mutations).await.map_err(Storage)?;
        let after = snapshot::read_snapshot(&mut tx, sampled_at_ms)
            .await
            .map_err(Storage)?;
        tx.commit()
            .await
            .map_err(|e| Storage(format!("classification commit failed: {e}")))?;
        Ok(patina_protocol::configuration::ClassificationCommitResult {
            ok: true,
            revision: Some(after.revision),
        })
    })
    .await
    .map_err(|_| Storage("classification commit exceeded its time budget".into()))?
}

pub fn validate_classification_setting_mutations(
    mutations: &[ClassificationSettingMutation],
) -> Result<(), String> {
    for mutation in mutations {
        validate_classification_setting_mutation(mutation)?;
    }
    Ok(())
}

fn validate_classification_setting_mutation(
    mutation: &ClassificationSettingMutation,
) -> Result<(), String> {
    if !is_allowed_classification_setting_key(&mutation.key) {
        return Err(format!(
            "invalid classification setting key `{}`",
            mutation.key
        ));
    }

    if let Some(value) = &mutation.value {
        if value.len() > MAX_SETTING_VALUE_LEN {
            return Err(format!(
                "classification setting value is too large for key `{}`",
                mutation.key
            ));
        }

        if mutation.key.starts_with(APP_OVERRIDE_KEY_PREFIX)
            || mutation.key.starts_with(WEB_DOMAIN_OVERRIDE_KEY_PREFIX)
        {
            serde_json::from_str::<serde_json::Value>(value).map_err(|error| {
                format!(
                    "invalid classification override value for key `{}`: {error}",
                    mutation.key
                )
            })?;
        }
    }

    Ok(())
}

fn is_allowed_classification_setting_key(key: &str) -> bool {
    patina_protocol::configuration::is_classification_key(key)
}

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema as db_schema;
    use sqlx::{Executor, Row, SqlitePool};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    async fn load_setting(pool: &SqlitePool, key: &str) -> Option<String> {
        sqlx::query("SELECT value FROM settings WHERE key = ? LIMIT 1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .unwrap()
            .and_then(|row| row.try_get::<String, _>("value").ok())
    }

    #[test]
    fn commit_classification_setting_mutations_upserts_and_deletes_in_one_transaction() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let key = "__app_override::chrome.exe";

            commit_classification_setting_mutations(
                &pool,
                &[ClassificationSettingMutation {
                    key: key.to_string(),
                    value: Some(r#"{"enabled":true,"displayName":"Work"}"#.to_string()),
                }],
            )
            .await
            .unwrap();

            assert_eq!(
                load_setting(&pool, key).await,
                Some(r#"{"enabled":true,"displayName":"Work"}"#.to_string())
            );

            commit_classification_setting_mutations(
                &pool,
                &[ClassificationSettingMutation {
                    key: key.to_string(),
                    value: None,
                }],
            )
            .await
            .unwrap();

            assert_eq!(load_setting(&pool, key).await, None);
        });
    }

    #[test]
    fn commit_classification_setting_mutations_accepts_manual_confirmation_migration_marker() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let key = "__classification_manual_confirmation_migration::v1";

            commit_classification_setting_mutations(
                &pool,
                &[ClassificationSettingMutation {
                    key: key.to_string(),
                    value: Some("1780226815860".to_string()),
                }],
            )
            .await
            .unwrap();

            assert_eq!(
                load_setting(&pool, key).await,
                Some("1780226815860".to_string())
            );
        });
    }

    #[test]
    fn commit_classification_setting_mutations_accepts_custom_category_labels() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let key = "__category_label_override::custom:category_focus";

            commit_classification_setting_mutations(
                &pool,
                &[ClassificationSettingMutation {
                    key: key.to_string(),
                    value: Some("Deep Focus".to_string()),
                }],
            )
            .await
            .unwrap();

            assert_eq!(
                load_setting(&pool, key).await,
                Some("Deep Focus".to_string())
            );
        });
    }

    #[test]
    fn commit_classification_setting_mutations_rolls_back_invalid_batches() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let good_key = "__category_color_override::video";

            let result = commit_classification_setting_mutations(
                &pool,
                &[
                    ClassificationSettingMutation {
                        key: good_key.to_string(),
                        value: Some("#FF669A".to_string()),
                    },
                    ClassificationSettingMutation {
                        key: "tracking_paused".to_string(),
                        value: Some("1".to_string()),
                    },
                ],
            )
            .await;

            assert!(result.is_err());
            assert_eq!(load_setting(&pool, good_key).await, None);
        });
    }
}
