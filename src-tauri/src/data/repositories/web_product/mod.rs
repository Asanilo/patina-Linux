//! Bounded web product reads. Facts, privacy and classification share a transaction.
use crate::domain::web_product::{apply_url_privacy, confirmed_end, WebProductPolicy};
use futures_util::TryStreamExt;
use patina_protocol::{
    history::{valid_range, MAX_SAFE_TIMESTAMP},
    web_history::*,
};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use tokio::sync::Semaphore;
mod metadata;
#[cfg(test)]
mod tests;
static WEB_PRODUCT_QUERY: Semaphore = Semaphore::const_new(1);

struct Fact {
    id: i64,
    start: i64,
    end: Option<i64>,
    observed: i64,
    native: Option<Option<i64>>,
}

pub async fn load_web_history(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    language: &str,
) -> Result<WebHistorySnapshot, String> {
    if !valid_range(from_ms, to_ms)
        || !matches!(language, "en-US" | "zh-CN")
        || !(0..=MAX_SAFE_TIMESTAMP).contains(&sampled_at_ms)
    {
        return Err("invalid web history range, sample time or language".into());
    }
    let _permit = WEB_PRODUCT_QUERY
        .try_acquire()
        .map_err(|_| "web history query is busy")?;
    tokio::time::timeout(
        patina_protocol::read_budget::WEB_HISTORY.query,
        read_snapshot(pool, from_ms, to_ms, sampled_at_ms, language),
    )
    .await
    .map_err(|_| "web history query exceeded its time budget".to_string())?
}
async fn read_snapshot(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    language: &str,
) -> Result<WebHistorySnapshot, String> {
    let mut tx = pool.begin().await.map_err(query_error)?;
    let health = super::activity_read_health::read_health(&mut tx, sampled_at_ms).await?;
    let config =
        super::classification_settings::read_classification_snapshot(&mut tx, sampled_at_ms)
            .await?;
    let privacy:Option<String>=sqlx::query_scalar("SELECT CASE WHEN length(CAST(value AS BLOB))<=32 THEN value ELSE '' END FROM settings WHERE key='web_activity_url_privacy'").fetch_optional(&mut *tx).await.map_err(query_error)?;
    let url_privacy = crate::domain::settings::parse_web_activity_url_privacy(privacy.as_deref());
    let policy = WebProductPolicy::from_entries(&config.entries, language);
    let mut rows=sqlx::query("SELECT w.id,w.start_time,w.end_time,w.updated_at,n.id AS native_id,n.end_time AS native_end
        FROM web_activity_segments w
        LEFT JOIN web_activity_native_sessions link ON link.segment_id=w.id
        LEFT JOIN sessions n ON n.id=link.session_id
        WHERE w.start_time<?1 AND COALESCE(w.end_time,CASE WHEN ?3>0 THEN ?3 ELSE w.start_time END)>?2
        ORDER BY w.start_time,COALESCE(w.end_time,?3),w.id LIMIT ?4")
        .bind(to_ms).bind(from_ms).bind(health.live_cutoff_ms).bind((MAX_WEB_HISTORY_FACTS+1) as i64).fetch(&mut *tx);
    let mut facts = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        if facts.len() >= MAX_WEB_HISTORY_FACTS {
            return Err("web history input fact budget exceeded".into());
        }
        let fact = Fact {
            id: row.try_get("id").map_err(query_error)?,
            start: row.try_get("start_time").map_err(query_error)?,
            end: row.try_get("end_time").map_err(query_error)?,
            observed: row.try_get("updated_at").map_err(query_error)?,
            native: if row
                .try_get::<Option<i64>, _>("native_id")
                .map_err(query_error)?
                .is_some()
            {
                Some(row.try_get("native_end").map_err(query_error)?)
            } else {
                None
            },
        };
        if !(1..=MAX_SAFE_TIMESTAMP).contains(&fact.id)
            || [
                Some(fact.start),
                fact.end,
                Some(fact.observed),
                fact.native.flatten(),
            ]
            .into_iter()
            .flatten()
            .any(|v| !(-MAX_SAFE_TIMESTAMP..=MAX_SAFE_TIMESTAMP).contains(&v))
        {
            return Err("invalid web history identity or timestamp".into());
        }
        facts.push(fact);
    }
    drop(rows);
    let facts = facts
        .into_iter()
        .filter_map(|fact| {
            let (end, live) = confirmed_end(
                fact.start,
                fact.end,
                fact.observed,
                fact.native,
                &health,
                sampled_at_ms,
            )?;
            let start = fact.start.max(from_ms);
            let end = end.min(to_ms);
            (end > start).then_some((fact, start, end, live && end == sampled_at_ms))
        })
        .collect::<Vec<_>>();
    let ids = facts
        .iter()
        .map(|(fact, _, _, _)| fact.id)
        .collect::<Vec<_>>();
    let mut metadata = metadata::load(&mut tx, &ids, url_privacy).await?;
    // A duplicate within one browser source/domain contributes only its uncovered
    // tail. Other browser sources remain independent; labels never form a key.
    let mut source_ends: HashMap<(String, String, String, String), i64> = HashMap::new();
    let mut records = Vec::new();
    let mut bytes = 1024;
    for (fact, start, end, live) in facts {
        let info = metadata
            .remove(&fact.id)
            .ok_or("missing web history metadata")?;
        let key = (
            info.browser_client_id.clone(),
            info.browser_kind.clone(),
            info.browser_exe_name.clone(),
            info.normalized_domain.clone(),
        );
        let start = start.max(source_ends.get(&key).copied().unwrap_or(start));
        if end <= start {
            continue;
        }
        source_ends.insert(key, end);
        let domain = policy.metadata(&info.normalized_domain);
        if domain.category.len() > 1024
            || domain.display_name.as_ref().is_some_and(|v| v.len() > 4096)
        {
            return Err("web history classification budget exceeded".into());
        }
        let record = WebHistoryRecord {
            record_id: fact.id,
            browser_client_id: info.browser_client_id,
            browser_kind: info.browser_kind,
            browser_exe_name: info.browser_exe_name,
            domain: info.domain,
            normalized_domain: info.normalized_domain,
            category: domain.category,
            display_name_override: domain.display_name,
            color_override: domain.color,
            recording_enabled: domain.recording_enabled,
            url: apply_url_privacy(info.url, url_privacy),
            title: info.title,
            favicon_url: info.favicon_url,
            start_ms: start,
            end_ms: end,
            is_open: fact.end.is_none(),
            is_live: live,
        };
        bytes += serde_json::to_vec(&record)
            .map_err(|e| e.to_string())?
            .len()
            + 1;
        if bytes > MAX_WEB_HISTORY_RESPONSE_BYTES {
            return Err("web history response budget exceeded".into());
        }
        records.push(record);
    }
    records.sort_by_key(|r| (r.start_ms, r.record_id, r.end_ms));
    tx.commit().await.map_err(query_error)?;
    Ok(WebHistorySnapshot {
        from_ms,
        to_ms,
        sampled_at_ms,
        classification_revision: config.revision,
        tracking_health: health,
        url_privacy,
        records,
    })
}
fn query_error(error: sqlx::Error) -> String {
    format!("web history read failed: {error}")
}
