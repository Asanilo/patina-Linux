use futures_util::TryStreamExt;
use patina_protocol::configuration::{
    ClassificationEntry, ClassificationSnapshot, CLASSIFICATION_PREFIXES,
    MAX_CLASSIFICATION_ENTRIES, MAX_CLASSIFICATION_KEY_BYTES, MAX_CLASSIFICATION_RESPONSE_BYTES,
    MAX_CLASSIFICATION_VALUE_BYTES,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::time::Duration;

pub(super) const CONFIGURATION_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn load_classification_snapshot(
    pool: &SqlitePool,
    sampled_at_ms: i64,
) -> Result<ClassificationSnapshot, String> {
    tokio::time::timeout(CONFIGURATION_TIMEOUT, async {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|e| format!("classification snapshot transaction failed: {e}"))?;
        let snapshot = read_snapshot(&mut transaction, sampled_at_ms).await?;
        transaction
            .commit()
            .await
            .map_err(|e| format!("classification snapshot completion failed: {e}"))?;
        Ok(snapshot)
    })
    .await
    .map_err(|_| "classification snapshot exceeded its time budget".to_string())?
}

pub(crate) async fn read_snapshot(
    connection: &mut SqliteConnection,
    sampled_at_ms: i64,
) -> Result<ClassificationSnapshot, String> {
    let predicates = CLASSIFICATION_PREFIXES
        .iter()
        .map(|_| "key GLOB ?")
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!("SELECT length(CAST(key AS BLOB)) AS key_bytes, CAST(substr(CAST(key AS BLOB),1,?) AS TEXT) AS key, CAST(substr(CAST(value AS BLOB),1,?) AS TEXT) AS value FROM settings WHERE {predicates} ORDER BY key COLLATE BINARY LIMIT ?");
    let mut query = sqlx::query(&sql)
        .bind((MAX_CLASSIFICATION_KEY_BYTES + 1) as i64)
        .bind((MAX_CLASSIFICATION_VALUE_BYTES + 1) as i64);
    for prefix in CLASSIFICATION_PREFIXES {
        query = query.bind(format!("{prefix}*"));
    }
    let mut rows = query
        .bind((MAX_CLASSIFICATION_ENTRIES + 1) as i64)
        .fetch(connection);
    let mut entries = Vec::new();
    let mut scanned = 0;
    let mut bytes = 512; // Envelope, revision, timestamp and array overhead.
    let mut hash = Sha256::new();
    hash.update(b"patina.classification.v1\0");
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|e| format!("classification snapshot query failed: {e}"))?
    {
        scanned += 1;
        if scanned > MAX_CLASSIFICATION_ENTRIES {
            return Err("classification snapshot exceeded its entry budget".into());
        }
        if row
            .try_get::<i64, _>("key_bytes")
            .map_err(|_| "classification key size is unavailable")?
            > MAX_CLASSIFICATION_KEY_BYTES as i64
        {
            continue; // Preserve the legacy reader's handling of unaddressable keys.
        }
        let entry = ClassificationEntry {
            key: row
                .try_get("key")
                .map_err(|_| "classification key is not text")?,
            value: row
                .try_get("value")
                .map_err(|_| "classification value is not text")?,
        };
        if !patina_protocol::configuration::is_classification_key(&entry.key) {
            continue;
        }
        if entry.value.len() > MAX_CLASSIFICATION_VALUE_BYTES {
            return Err("classification snapshot exceeded a key, value or entry budget".into());
        }
        bytes += serde_json::to_vec(&entry)
            .map_err(|_| "classification snapshot cannot be encoded")?
            .len()
            + 1;
        if bytes > MAX_CLASSIFICATION_RESPONSE_BYTES {
            return Err("classification snapshot exceeded its response budget".into());
        }
        hash.update((entry.key.len() as u64).to_be_bytes());
        hash.update(entry.key.as_bytes());
        hash.update((entry.value.len() as u64).to_be_bytes());
        hash.update(entry.value.as_bytes());
        entries.push(entry);
    }
    Ok(ClassificationSnapshot {
        revision: format!("{:x}", hash.finalize()),
        sampled_at_ms,
        entries,
    })
}
