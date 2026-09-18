//! Bounded classification evidence, before UI aliases, filtering and candidate limits.
use crate::domain::activity_read_model::{
    summarize_activity_range, ActivityOrigin, OwnedActivityRange, HOUR_MS,
};
use crate::domain::observed_apps::{validate_range, ObservedAppStat, MAX_OBSERVED_APPS};
use futures_util::TryStreamExt;
use sqlx::{Row, SqlitePool};
use std::{collections::HashMap, time::Duration};
use tokio::sync::Semaphore;

const MAX_FACTS: usize = 50_000;
const MAX_TEXT_BYTES: usize = 1024;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
static QUERY: Semaphore = Semaphore::const_new(1);

pub async fn load_observed_apps(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
) -> Result<Vec<ObservedAppStat>, String> {
    validate_range(from_ms, to_ms)?;
    let _permit = QUERY
        .try_acquire()
        .map_err(|_| "observed apps query is busy")?;
    tokio::time::timeout(
        Duration::from_secs(15),
        load_snapshot(pool, from_ms, to_ms, sampled_at_ms, MAX_FACTS),
    )
    .await
    .map_err(|_| "observed apps query exceeded its time budget".to_string())?
}

async fn load_snapshot(
    pool: &SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    fact_limit: usize,
) -> Result<Vec<ObservedAppStat>, String> {
    let mut tx = pool.begin().await.map_err(query_error)?;
    let active_end = sampled_at_ms.min(to_ms);
    // Only compact metadata crosses the reader boundary. Explicit source/id order
    // makes precedence ties and bucket rounding reproducible across query plans.
    let mut rows = sqlx::query(
        "SELECT id, 0 AS source, substr(exe_name,1,1025) AS exe_name,
                substr(COALESCE(app_name,''),1,1025) AS app_name,
                start_time, COALESCE(end_time,?) AS end_time, COALESCE(end_time,?) AS capacity_end
         FROM sessions WHERE start_time < ? AND COALESCE(end_time,?) > ?
         UNION ALL
         SELECT id, 1, substr(exe_name,1,1025), substr(app_name,1,1025), start_time, end_time, end_time
         FROM import_exact_sessions WHERE start_time < ? AND end_time > ?
         UNION ALL
         SELECT id, 2, substr(exe_name,1,1025), substr(app_name,1,1025), bucket_start_time,
                bucket_start_time + duration, bucket_start_time + ?
         FROM import_time_buckets WHERE bucket_start_time < ? AND bucket_start_time > ?
         ORDER BY source, id LIMIT ?")
        .bind(active_end).bind(active_end).bind(to_ms).bind(active_end).bind(from_ms)
        .bind(to_ms).bind(from_ms).bind(HOUR_MS).bind(to_ms).bind(from_ms.saturating_sub(HOUR_MS))
        .bind((fact_limit + 1) as i64).fetch(&mut *tx);
    let mut records = Vec::new();
    let mut metadata = Vec::new();
    let mut metadata_bytes = 0;
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        if records.len() == fact_limit {
            return Err("observed apps facts exceed budget".into());
        }
        let exe: String = row.try_get("exe_name").map_err(query_error)?;
        let app: String = row.try_get("app_name").map_err(query_error)?;
        metadata_bytes += exe.len() + app.len();
        if exe.len() > MAX_TEXT_BYTES
            || app.len() > MAX_TEXT_BYTES
            || metadata_bytes > MAX_METADATA_BYTES
        {
            return Err("observed apps metadata exceeds budget".into());
        }
        let origin = match row.try_get::<i64, _>("source").map_err(query_error)? {
            0 => ActivityOrigin::Native,
            1 => ActivityOrigin::ImportExact,
            _ => ActivityOrigin::ImportBucket,
        };
        records.push(OwnedActivityRange {
            origin,
            start_ms: row.try_get("start_time").map_err(query_error)?,
            end_ms: row.try_get("end_time").map_err(query_error)?,
            capacity_end_ms: Some(row.try_get("capacity_end").map_err(query_error)?),
            value: metadata.len(),
        });
        metadata.push((exe, app));
    }
    drop(rows);
    tx.commit().await.map_err(query_error)?;
    let mut contributions = summarize_activity_range(&records, from_ms, to_ms);
    // Native zero-length facts are classification evidence in the desktop reader.
    for record in &records {
        if record.origin == ActivityOrigin::Native && record.start_ms == record.end_ms {
            contributions.push(crate::domain::activity_read_model::ActivityContribution {
                origin: record.origin,
                start_ms: record.start_ms,
                duration_ms: 0,
                value: record.value,
            });
        }
    }
    contributions.sort_by_key(|item| (item.start_ms, item.origin, item.value));
    let mut indexes: HashMap<String, usize> = HashMap::new();
    let mut stats: Vec<ObservedAppStat> = Vec::new();
    for item in contributions {
        let (exe, app) = &metadata[item.value];
        let index = if let Some(index) = indexes.get(exe) {
            *index
        } else {
            if stats.len() == MAX_OBSERVED_APPS {
                return Err("observed apps count exceeds budget".into());
            }
            let index = stats.len();
            indexes.insert(exe.clone(), index);
            stats.push(ObservedAppStat {
                exe_name: exe.clone(),
                app_name: app.clone(),
                total_duration_ms: 0,
                last_seen_ms: item.start_ms,
            });
            index
        };
        let stat = &mut stats[index];
        stat.total_duration_ms = stat
            .total_duration_ms
            .checked_add(item.duration_ms)
            .ok_or("observed apps duration overflow")?;
        if item.start_ms >= stat.last_seen_ms {
            stat.last_seen_ms = item.start_ms;
            stat.app_name.clone_from(app);
        }
    }
    let encoded = serde_json::to_vec(&stats).map_err(|_| "failed to encode observed apps")?;
    if encoded.len() + 9 > crate::domain::observed_apps::MAX_OBSERVED_APPS_RESPONSE_BYTES {
        return Err("observed apps response exceeds budget".into());
    }
    Ok(stats)
}

fn query_error(error: sqlx::Error) -> String {
    format!("failed to read observed apps: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema;
    use serde::Deserialize;
    use sqlx::Executor;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        from: i64,
        to: i64,
        sampled: i64,
        facts: Vec<Fact>,
        expected: Vec<ObservedAppStat>,
    }
    #[derive(Deserialize)]
    struct Fact {
        origin: String,
        exe: String,
        app: String,
        start: i64,
        end: Option<i64>,
    }

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        sqlx::query("INSERT INTO import_batches(id, imported_at, source_name, source_kind, source_fingerprint, exact_session_count, hour_bucket_count) VALUES ('b',1,'fixture','patina-csv',?,0,0)").bind("a".repeat(64)).execute(&pool).await.unwrap();
        pool
    }

    async fn insert(pool: &SqlitePool, index: usize, fact: &Fact) {
        match fact.origin.as_str() {
            "native" => {
                sqlx::query("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES (?,?,'private title not returned',?,?,?)")
                    .bind(&fact.app).bind(&fact.exe).bind(fact.start).bind(fact.end).bind(fact.end.map(|end| end-fact.start)).execute(pool).await.unwrap();
            }
            "import_exact" => {
                sqlx::query("INSERT INTO import_exact_sessions(batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration) VALUES ('b',?,?,?,'private title',?,?,?)")
                    .bind(format!("{index:064x}")).bind(&fact.app).bind(&fact.exe).bind(fact.start).bind(fact.end.unwrap()).bind(fact.end.unwrap()-fact.start).execute(pool).await.unwrap();
            }
            _ => {
                sqlx::query("INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES ('b',?,?,?,?,?)")
                    .bind(format!("{index:064x}")).bind(&fact.app).bind(&fact.exe).bind(fact.start).bind(fact.end.unwrap()-fact.start).execute(pool).await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn matches_shared_desktop_fixtures_without_titles_or_exclusion_filtering() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/observed-apps.json"
        ))
        .unwrap();
        for case in cases {
            let pool = setup().await;
            for (index, fact) in case.facts.iter().enumerate() {
                insert(&pool, index, fact).await;
            }
            pool.execute("INSERT INTO settings(key,value) VALUES ('__app_override::zen','{\"track\":false}')").await.unwrap();
            let result = load_snapshot(&pool, case.from, case.to, case.sampled, MAX_FACTS)
                .await
                .unwrap();
            assert_eq!(result, case.expected, "{}", case.name);
            assert!(!serde_json::to_string(&result)
                .unwrap()
                .contains("private title"));
        }
    }

    #[tokio::test]
    async fn budgets_fail_without_partial_results_and_reads_do_not_write() {
        let pool = setup().await;
        let fact = Fact {
            origin: "native".into(),
            exe: "zen".into(),
            app: "Zen".into(),
            start: 1000,
            end: Some(2000),
        };
        insert(&pool, 0, &fact).await;
        insert(&pool, 1, &fact).await;
        assert!(load_snapshot(&pool, 0, 3000, 3000, 1)
            .await
            .unwrap_err()
            .contains("facts exceed"));
        sqlx::query("UPDATE sessions SET app_name = ?")
            .bind("x".repeat(1025))
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_snapshot(&pool, 0, 3000, 3000, MAX_FACTS)
            .await
            .unwrap_err()
            .contains("metadata exceeds"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        pool.execute("DELETE FROM sessions").await.unwrap();
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<4097) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'App','app-'||i,1000,2000,1000 FROM n").await.unwrap();
        assert!(load_snapshot(&pool, 0, 3000, 3000, MAX_FACTS)
            .await
            .unwrap_err()
            .contains("count exceeds"));
    }

    #[tokio::test]
    async fn fifty_thousand_facts_return_one_stat_and_the_next_fact_fails_closed() {
        let pool = setup().await;
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<49999) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'Zen','zen',i*1000,i*1000+500,500 FROM n").await.unwrap();
        let result = load_snapshot(&pool, 0, 50_000_000, 50_000_000, MAX_FACTS)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_duration_ms, 25_000_000);
        assert_eq!(result[0].last_seen_ms, 49_999_000);
        assert!(serde_json::to_vec(&result).unwrap().len() < 200);
        insert(
            &pool,
            0,
            &Fact {
                origin: "native".into(),
                exe: "extra".into(),
                app: "Extra".into(),
                start: 1,
                end: Some(2),
            },
        )
        .await;
        assert!(load_snapshot(&pool, 0, 50_000_000, 50_000_000, MAX_FACTS)
            .await
            .unwrap_err()
            .contains("facts exceed"));
    }

    #[tokio::test]
    async fn aggregate_response_and_total_metadata_budgets_are_independent() {
        let pool = setup().await;
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<2000) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'App','app-'||i,1000,2000,1000 FROM n").await.unwrap();
        sqlx::query("UPDATE sessions SET app_name = ?")
            .bind("x".repeat(1024))
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_snapshot(&pool, 0, 3000, 3000, MAX_FACTS)
            .await
            .unwrap_err()
            .contains("response exceeds"));
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<7000) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'App','same',1000,2000,1000 FROM n").await.unwrap();
        sqlx::query("UPDATE sessions SET app_name = ?")
            .bind("x".repeat(1024))
            .execute(&pool)
            .await
            .unwrap();
        assert!(load_snapshot(&pool, 0, 3000, 3000, MAX_FACTS)
            .await
            .unwrap_err()
            .contains("metadata exceeds"));
    }

    #[tokio::test]
    async fn all_surfaces_share_the_bounded_reader_and_busy_queries_fail_closed() {
        use crate::engine::api::{
            context::ApiRuntimeContext,
            router::{route_request, ApiRequest},
            surface::ApiSurface,
        };
        let pool = setup().await;
        let permit = QUERY.acquire().await.unwrap();
        assert!(load_observed_apps(&pool, 0, 3000, 3000)
            .await
            .unwrap_err()
            .contains("busy"));
        drop(permit);
        let context =
            ApiRuntimeContext::new(crate::engine::runtime_context::RuntimeContext::system(pool));
        for surface in [
            ApiSurface::Desktop,
            ApiSurface::DaemonReadOnly,
            ApiSurface::DaemonTracking,
        ] {
            let response = route_request(
                ApiRequest {
                    method: "GET".into(),
                    path: "/api/v1/classification/observed-apps".into(),
                    query: Some("from_ms=0&to_ms=3000".into()),
                    body: Vec::new(),
                },
                &context,
                surface,
            )
            .await;
            assert_eq!(response.status, 200, "{:?}", response.body);
            assert_eq!(response.body["data"], serde_json::json!([]));
        }
    }
}
