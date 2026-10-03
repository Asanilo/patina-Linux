use crate::domain::activity_read_policy::canonical_executable;
use futures_util::TryStreamExt;
use patina_protocol::maintenance::*;
use sqlx::{Row, SqlitePool};
use std::time::Duration;

pub async fn delete_canonical_app(
    pool: &SqlitePool,
    request: &CanonicalAppCleanupRequest,
    now_ms: i64,
) -> Result<CanonicalAppCleanupResult, String> {
    let app_key = crate::domain::data_maintenance::canonical_cleanup_key(request)?;
    let range = match request.scope {
        AppCleanupScope::All => None,
        AppCleanupScope::Today => {
            use chrono::TimeZone;
            let day = chrono::Local
                .timestamp_millis_opt(now_ms)
                .single()
                .ok_or("invalid cleanup time")?
                .date_naive();
            let next = day.succ_opt().ok_or("invalid cleanup day")?;
            let boundaries = crate::domain::daily_activity::local_day_boundaries(
                &day.format("%Y-%m-%d").to_string(),
                &next.format("%Y-%m-%d").to_string(),
            )?;
            Some((boundaries[0], boundaries[1]))
        }
    };
    tokio::time::timeout(Duration::from_secs(12), async {
        // Acquire the writer before enumerating aliases: no insertion can slip
        // between name discovery and the canonical deletion transaction.
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| e.to_string())?;
        let mut rows = sqlx::query(
            "SELECT DISTINCT name FROM (
            SELECT substr(CAST(exe_name AS BLOB),1,1025) AS name FROM sessions
            UNION SELECT substr(CAST(exe_name AS BLOB),1,1025) FROM import_exact_sessions
            UNION SELECT substr(CAST(exe_name AS BLOB),1,1025) FROM import_time_buckets
        ) ORDER BY name LIMIT ?",
        )
        .bind((MAX_CLEANUP_EXECUTABLES + 1) as i64)
        .fetch(&mut *tx);
        let mut names = Vec::new();
        let mut count = 0;
        let mut bytes = 0;
        while let Some(row) = rows.try_next().await.map_err(|e| e.to_string())? {
            count += 1;
            let raw: Vec<u8> = row.try_get(0).map_err(|e| e.to_string())?;
            bytes += raw.len();
            if count > MAX_CLEANUP_EXECUTABLES
                || raw.len() > MAX_CLEANUP_APP_KEY_BYTES
                || bytes > 1024 * 1024
            {
                return Err("application cleanup name budget exceeded".into());
            }
            let name = String::from_utf8(raw).map_err(|_| "invalid application cleanup name")?;
            if canonical_executable(&name) == app_key {
                names.push(name);
            }
        }
        drop(rows);
        let deleted = if names.is_empty() {
            AppTrackingDataCleanupResult::default()
        } else {
            super::delete_app_tracking_data_tx(
                &mut tx,
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
                range.map(|r| r.0),
                range.map(|r| r.1),
            )
            .await?
        };
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(CanonicalAppCleanupResult {
            app_key,
            matched_executables: names.len(),
            deleted,
        })
    })
    .await
    .map_err(|_| "application cleanup exceeded its time budget".to_owned())?
}

#[cfg(test)]
mod tests;
