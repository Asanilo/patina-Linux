//! One bounded, read-only snapshot for precise History and application details.
use crate::domain::{
    activity_read_model::{summarize_activity_range, ActivityOrigin, OwnedActivityRange},
    activity_read_policy,
    product_classification::ProductClassification,
};
use futures_util::TryStreamExt;
use patina_protocol::history::*;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::Semaphore;

mod metadata;
#[cfg(test)]
mod tests;
static HISTORY_QUERY: Semaphore = Semaphore::const_new(1);

#[derive(Clone)]
struct Fact {
    origin: ExactActivityOrigin,
    id: i64,
    continuity_start_ms: i64,
    is_open: bool,
}

pub async fn load_exact_history(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    language: &str,
) -> Result<ExactHistorySnapshot, String> {
    if !valid_range(from_ms, to_ms) || !matches!(language, "en-US" | "zh-CN") {
        return Err("invalid exact history range or language".into());
    }
    let _permit = HISTORY_QUERY
        .try_acquire()
        .map_err(|_| "exact history query is busy")?;
    tokio::time::timeout(
        patina_protocol::read_budget::ANALYTICS.query,
        read_snapshot(pool, from_ms, to_ms, sampled_at_ms, language),
    )
    .await
    .map_err(|_| "exact history query exceeded its time budget".to_string())?
}

async fn read_snapshot(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    language: &str,
) -> Result<ExactHistorySnapshot, String> {
    let mut tx = pool.begin().await.map_err(query_error)?;
    let health = super::activity_read_health::read_health(&mut tx, sampled_at_ms).await?;
    let configuration =
        super::classification_settings::read_classification_snapshot(&mut tx, sampled_at_ms)
            .await?;
    let policy = ProductClassification::from_entries(&configuration.entries, language);
    let facts = load_facts(&mut tx, from_ms, to_ms, health.live_cutoff_ms).await?;
    let contributions = summarize_activity_range(&facts, from_ms, to_ms);
    if contributions.len() > MAX_HISTORY_RECORDS {
        return Err("exact history record budget exceeded".into());
    }
    let keys = contributions
        .iter()
        .map(|item| (item.value.origin, item.value.id))
        .collect::<BTreeSet<_>>();
    let metadata = metadata::load(&mut tx, &keys).await?;
    let mut records = Vec::new();
    let mut native_ids = BTreeSet::new();
    let mut bytes = 1024;
    for item in contributions {
        let key = (item.value.origin, item.value.id);
        let info = metadata.get(&key).ok_or("missing exact history metadata")?;
        let app_key = activity_read_policy::canonical_executable(&info.exe_name);
        if app_key.len() > MAX_HISTORY_NAME_BYTES {
            return Err("exact history canonical key budget exceeded".into());
        }
        if policy.excludes(&app_key)
            || !activity_read_policy::should_include_fact(
                &info.exe_name,
                &info.app_name,
                &info.title,
            )
        {
            continue;
        }
        let record = ExactActivityRecord {
            origin: item.value.origin,
            record_id: item.value.id,
            category: policy.category(&app_key).into(),
            display_name_override: policy.display_name_override(&app_key).map(str::to_owned),
            app_key,
            app_name: info.app_name.clone(),
            exe_name: info.exe_name.clone(),
            window_title: info.title.clone(),
            start_ms: item.start_ms,
            end_ms: item
                .start_ms
                .checked_add(item.duration_ms)
                .ok_or("history duration overflow")?,
            continuity_start_ms: if item.value.origin == ExactActivityOrigin::Native {
                item.value.continuity_start_ms.min(item.start_ms)
            } else {
                item.start_ms
            },
            is_open: item.value.is_open,
            title_samples: Vec::new(),
        };
        bytes += serde_json::to_vec(&record)
            .map_err(|e| e.to_string())?
            .len()
            + 1;
        if bytes > MAX_HISTORY_RESPONSE_BYTES {
            return Err("exact history response budget exceeded".into());
        }
        if record.origin == ExactActivityOrigin::Native {
            native_ids.insert(record.record_id);
        }
        records.push(record);
    }
    let samples =
        metadata::load_samples(&mut tx, &native_ids, from_ms, to_ms, health.live_cutoff_ms).await?;
    let mut sample_count = 0;
    for record in &mut records {
        if record.origin == ExactActivityOrigin::Native {
            if let Some(values) = samples.get(&record.record_id) {
                for value in values {
                    let start_ms = value.start_ms.max(record.start_ms);
                    let end_ms = value.end_ms.min(record.end_ms);
                    if end_ms <= start_ms {
                        continue;
                    }
                    let sample = ExactTitleSample {
                        title: value.title.clone(),
                        start_ms,
                        end_ms,
                    };
                    sample_count += 1;
                    bytes += serde_json::to_vec(&sample)
                        .map_err(|e| e.to_string())?
                        .len()
                        + 1;
                    if sample_count > MAX_HISTORY_TITLE_SAMPLES
                        || bytes > MAX_HISTORY_RESPONSE_BYTES
                    {
                        return Err("exact history title response budget exceeded".into());
                    }
                    record.title_samples.push(sample);
                }
            }
        }
    }
    records.sort_by_key(|record| {
        (
            record.start_ms,
            record.origin,
            record.record_id,
            record.end_ms,
        )
    });
    tx.commit().await.map_err(query_error)?;
    Ok(ExactHistorySnapshot {
        from_ms,
        to_ms,
        sampled_at_ms,
        configuration_revision: configuration.revision,
        tracking_health: health,
        records,
    })
}

async fn load_facts(
    connection: &mut SqliteConnection,
    from_ms: i64,
    to_ms: i64,
    cutoff: i64,
) -> Result<Vec<OwnedActivityRange<Arc<Fact>>>, String> {
    // Keep captions and samples out of the ordered UNION. Missing heartbeat never
    // creates an open interval, including in a pre-epoch query.
    let mut rows = sqlx::query(
        "SELECT id, 0 AS origin, start_time,
            COALESCE(end_time,CASE WHEN ?1>0 THEN ?1 ELSE start_time END) AS effective_end,
            COALESCE(continuity_group_start_time,start_time) AS continuity,
            end_time IS NULL AS is_open
         FROM sessions
         WHERE start_time < ?3 AND COALESCE(end_time,CASE WHEN ?1>0 THEN ?1 ELSE start_time END)>?2
         UNION ALL
         SELECT id,1,start_time,end_time,start_time,0 FROM import_exact_sessions
         WHERE start_time<?3 AND end_time>?2
         ORDER BY start_time,origin,id LIMIT ?4",
    )
    .bind(cutoff)
    .bind(from_ms)
    .bind(to_ms)
    .bind((MAX_HISTORY_FACTS + 1) as i64)
    .fetch(connection);
    let mut facts = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        if facts.len() >= MAX_HISTORY_FACTS {
            return Err("exact history input fact budget exceeded".into());
        }
        let id: i64 = row.try_get("id").map_err(query_error)?;
        let start: i64 = row.try_get("start_time").map_err(query_error)?;
        let end: i64 = row.try_get("effective_end").map_err(query_error)?;
        let continuity: i64 = row.try_get("continuity").map_err(query_error)?;
        if id <= 0
            || id > MAX_SAFE_TIMESTAMP
            || [start, end, continuity]
                .iter()
                .any(|v| !(-MAX_SAFE_TIMESTAMP..=MAX_SAFE_TIMESTAMP).contains(v))
        {
            return Err("invalid exact history fact identity or timestamp".into());
        }
        let native = row.try_get::<i64, _>("origin").map_err(query_error)? == 0;
        facts.push(OwnedActivityRange {
            origin: if native {
                ActivityOrigin::Native
            } else {
                ActivityOrigin::ImportExact
            },
            start_ms: start,
            end_ms: end,
            capacity_end_ms: None,
            value: Arc::new(Fact {
                origin: if native {
                    ExactActivityOrigin::Native
                } else {
                    ExactActivityOrigin::ImportExact
                },
                id,
                continuity_start_ms: continuity,
                is_open: row.try_get::<bool, _>("is_open").map_err(query_error)?,
            }),
        });
    }
    Ok(facts)
}

fn query_error(error: sqlx::Error) -> String {
    format!("exact history read failed: {error}")
}
