//! Bounded daily aggregation. Transport adapters supply consecutive local-day boundaries.

use crate::domain::activity_read_model::{
    summarize_activity_range, ActivityOrigin, OwnedActivityRange, HOUR_MS,
};
use crate::domain::activity_read_policy;
use crate::domain::daily_activity::{
    DailyActivitySnapshot, DailyActivityTotal, MAX_DAILY_ACTIVITY_DAYS,
};
use futures_util::TryStreamExt;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const MAX_FACTS_PER_DAY: usize = 20_000;
const MAX_EXCLUSION_SETTINGS: usize = 20_000;
const MAX_APP_KEY_BYTES: usize = 1024;
const MAX_OVERRIDE_BYTES: usize = 16_384;
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
static DAILY_ACTIVITY_QUERY: Semaphore = Semaphore::const_new(1);

#[derive(Debug)]
pub struct DailyActivityTrend {
    pub activity: DailyActivitySnapshot,
    pub top_apps: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
struct DayFact {
    included: bool,
    app: Option<Arc<str>>,
}

// Use existing covering indexes to avoid rereading title-heavy table pages for each day.
const DAY_FACTS_SQL: &str = "SELECT id AS record_id, 'native' AS origin, substr(exe_name, 1, 1025) AS exe_name,
                start_time, COALESCE(end_time, ?) AS effective_end_time,
                COALESCE(end_time, ?) AS capacity_end_time
         FROM sessions INDEXED BY idx_sessions_exe_usage_time WHERE start_time < ? AND COALESCE(end_time, ?) > ?
         UNION ALL
         SELECT id, 'import_exact', substr(exe_name, 1, 1025), start_time, end_time, end_time
         FROM import_exact_sessions INDEXED BY idx_import_exact_sessions_exe_time WHERE start_time < ? AND end_time > ?
         UNION ALL
         SELECT id, 'import_bucket', substr(exe_name, 1, 1025), bucket_start_time,
                bucket_start_time + duration, bucket_start_time + ?
         FROM import_time_buckets WHERE bucket_start_time < ? AND bucket_start_time > ?
         ORDER BY start_time ASC, origin ASC, record_id ASC LIMIT ?";

#[cfg(all(test, target_os = "linux"))]
mod benchmark;
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod desktop_fixture;

/// Reads one SQLite snapshot, retaining at most one day's compact facts. Budget
/// failures return no partial totals; callers must not fall back to full-history reads.
pub async fn load_daily_activity(
    pool: &SqlitePool,
    day_boundaries: &[i64],
    sampled_at_ms: i64,
) -> Result<DailyActivitySnapshot, String> {
    Ok(
        load_bounded_snapshot(pool, day_boundaries, sampled_at_ms, false)
            .await?
            .activity,
    )
}

pub async fn load_daily_trend(
    pool: &SqlitePool,
    day_boundaries: &[i64],
    sampled_at_ms: i64,
) -> Result<DailyActivityTrend, String> {
    load_bounded_snapshot(pool, day_boundaries, sampled_at_ms, true).await
}

async fn load_bounded_snapshot(
    pool: &SqlitePool,
    day_boundaries: &[i64],
    sampled_at_ms: i64,
    retain_apps: bool,
) -> Result<DailyActivityTrend, String> {
    validate_boundaries(day_boundaries)?;
    let _permit = DAILY_ACTIVITY_QUERY
        .try_acquire()
        .map_err(|_| "daily activity query is busy".to_string())?;
    tokio::time::timeout(
        QUERY_TIMEOUT,
        load_snapshot_with_apps(pool, day_boundaries, sampled_at_ms, retain_apps),
    )
    .await
    .map_err(|_| "daily activity query exceeded its time budget".to_string())?
}

fn validate_boundaries(boundaries: &[i64]) -> Result<(), String> {
    if !(2..=MAX_DAILY_ACTIVITY_DAYS + 1).contains(&boundaries.len()) {
        return Err("daily activity requires 1 to 378 days".to_string());
    }
    for day in boundaries.windows(2) {
        let duration = day[1].checked_sub(day[0]);
        if !matches!(duration, Some(value) if value > 0 && value <= 48 * HOUR_MS) {
            return Err(
                "daily activity boundaries must be consecutive increasing days".to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
async fn load_snapshot(
    pool: &SqlitePool,
    boundaries: &[i64],
    sampled_at_ms: i64,
) -> Result<DailyActivitySnapshot, String> {
    Ok(
        load_snapshot_with_apps(pool, boundaries, sampled_at_ms, false)
            .await?
            .activity,
    )
}

async fn load_snapshot_with_apps(
    pool: &SqlitePool,
    boundaries: &[i64],
    sampled_at_ms: i64,
    retain_apps: bool,
) -> Result<DailyActivityTrend, String> {
    let mut transaction = pool.begin().await.map_err(query_error)?;
    let excluded = load_excluded_apps(&mut transaction).await?;
    let earliest_start_ms = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MIN(first_start) FROM (
           SELECT MIN(start_time) AS first_start FROM sessions
           UNION ALL SELECT MIN(start_time) FROM import_exact_sessions
           UNION ALL SELECT MIN(bucket_start_time) FROM import_time_buckets
         )",
    )
    .fetch_one(&mut *transaction)
    .await
    .map_err(query_error)?;
    let mut days = Vec::with_capacity(boundaries.len() - 1);
    let mut top_apps = Vec::with_capacity(boundaries.len() - 1);
    for day in boundaries.windows(2) {
        let records = load_day_facts(
            &mut transaction,
            day[0],
            day[1],
            sampled_at_ms,
            &excluded,
            MAX_FACTS_PER_DAY,
            retain_apps,
        )
        .await?;
        // Excluded native activity must still suppress overlapping imported facts.
        let mut app_totals = HashMap::<Arc<str>, i64>::new();
        let active_ms = summarize_activity_range(&records, day[0], day[1])
            .into_iter()
            .filter(|contribution| contribution.value.included)
            .try_fold(0_i64, |sum, contribution| {
                if contribution.duration_ms > 0 {
                    if let Some(app) = contribution.value.app {
                        let total = app_totals.entry(app).or_default();
                        *total = total
                            .checked_add(contribution.duration_ms)
                            .ok_or_else(|| "daily activity duration overflow".to_string())?;
                    }
                }
                sum.checked_add(contribution.duration_ms)
                    .ok_or_else(|| "daily activity duration overflow".to_string())
            })?;
        top_apps.push(
            app_totals
                .iter()
                .max_by(|(left, a), (right, b)| a.cmp(b).then_with(|| right.cmp(left)))
                .map(|(app, _)| app.to_string()),
        );
        days.push(DailyActivityTotal {
            start_ms: day[0],
            end_ms: day[1],
            active_ms,
        });
    }
    transaction.commit().await.map_err(query_error)?;
    Ok(DailyActivityTrend {
        activity: DailyActivitySnapshot {
            sampled_at_ms,
            earliest_start_ms,
            days,
        },
        top_apps,
    })
}

async fn load_excluded_apps(connection: &mut SqliteConnection) -> Result<HashSet<String>, String> {
    let mut rows = sqlx::query(
        "SELECT substr(key, 1, 1100) AS key, substr(value, 1, 16385) AS value FROM settings
         WHERE key GLOB '__app_excluded::*' OR key GLOB '__app_override::*' LIMIT ?",
    )
    .bind((MAX_EXCLUSION_SETTINGS + 1) as i64)
    .fetch(connection);
    let mut excluded = HashSet::new();
    let mut current = HashMap::new();
    let mut count = 0;
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        count += 1;
        if count > MAX_EXCLUSION_SETTINGS {
            return Err("daily activity exclusion settings exceed budget".to_string());
        }
        let key: String = row.try_get("key").map_err(query_error)?;
        let value: String = row.try_get("value").map_err(query_error)?;
        if value.len() > MAX_OVERRIDE_BYTES {
            return Err("daily activity app setting exceeds budget".to_string());
        }
        if let Some(app) = key
            .strip_prefix("__app_excluded::")
            .or_else(|| key.strip_prefix("__app_override::"))
        {
            if app.len() > MAX_APP_KEY_BYTES {
                return Err("daily activity app key exceeds budget".to_string());
            }
            let app = activity_read_policy::canonical_executable(app);
            if key.starts_with("__app_override::") {
                let value: serde_json::Value = serde_json::from_str(&value)
                    .map_err(|_| "daily activity app override is invalid JSON".to_string())?;
                let object = value
                    .as_object()
                    .ok_or("daily activity app override must be an object")?;
                let disabled = object.get("enabled") == Some(&serde_json::Value::Bool(false));
                let is_excluded =
                    !disabled && object.get("track") == Some(&serde_json::Value::Bool(false));
                if current
                    .insert(app, is_excluded)
                    .is_some_and(|previous| previous != is_excluded)
                {
                    return Err("daily activity app aliases have conflicting overrides".to_string());
                }
            } else if crate::domain::settings::parse_boolean_setting(&value, false) {
                excluded.insert(app);
            }
        }
    }
    for (app, is_excluded) in current {
        if is_excluded {
            excluded.insert(app);
        } else {
            excluded.remove(&app);
        }
    }
    Ok(excluded)
}

async fn load_day_facts(
    connection: &mut SqliteConnection,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    excluded: &HashSet<String>,
    limit: usize,
    retain_apps: bool,
) -> Result<Vec<OwnedActivityRange<DayFact>>, String> {
    let active_end = sampled_at_ms.min(to_ms);
    let mut rows = sqlx::query(DAY_FACTS_SQL)
        .bind(active_end)
        .bind(active_end)
        .bind(to_ms)
        .bind(active_end)
        .bind(from_ms)
        .bind(to_ms)
        .bind(from_ms)
        .bind(HOUR_MS)
        .bind(to_ms)
        .bind(from_ms.saturating_sub(HOUR_MS))
        .bind((limit + 1) as i64)
        .fetch(&mut *connection);
    let mut records = Vec::new();
    let mut metadata_requests = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        if records.len() == limit {
            return Err("daily activity facts exceed per-day budget".to_string());
        }
        let app: String = row.try_get("exe_name").map_err(query_error)?;
        if app.len() > MAX_APP_KEY_BYTES {
            return Err("daily activity app key exceeds budget".to_string());
        }
        let origin = match row
            .try_get::<String, _>("origin")
            .map_err(query_error)?
            .as_str()
        {
            "native" => ActivityOrigin::Native,
            "import_exact" => ActivityOrigin::ImportExact,
            "import_bucket" => ActivityOrigin::ImportBucket,
            _ => return Err("unknown daily activity origin".to_string()),
        };
        let include = !excluded.contains(&activity_read_policy::canonical_executable(&app))
            && activity_read_policy::should_include_fact(&app, "", "");
        let app_key =
            retain_apps.then(|| Arc::<str>::from(activity_read_policy::canonical_executable(&app)));
        if include && activity_read_policy::needs_metadata(&app) {
            metadata_requests.push((
                records.len(),
                origin,
                row.try_get::<i64, _>("record_id").map_err(query_error)?,
                app,
            ));
        }
        records.push(OwnedActivityRange {
            origin,
            start_ms: row.try_get("start_time").map_err(query_error)?,
            end_ms: row.try_get("effective_end_time").map_err(query_error)?,
            capacity_end_ms: Some(row.try_get("capacity_end_time").map_err(query_error)?),
            value: DayFact {
                included: include,
                app: app_key,
            },
        });
    }
    drop(rows);
    // Keep titles out of the ordered UNION and retained facts. Fetch sensitive
    // metadata one row at a time inside the same snapshot, with explicit limits.
    for (index, origin, id, exe) in metadata_requests {
        let query = match origin {
            ActivityOrigin::Native => "SELECT substr(app_name,1,1025) AS app_name, substr(COALESCE(window_title,''),1,16385) AS title FROM sessions WHERE id = ?",
            ActivityOrigin::ImportExact => "SELECT substr(app_name,1,1025) AS app_name, substr(window_title,1,16385) AS title FROM import_exact_sessions WHERE id = ?",
            ActivityOrigin::ImportBucket => "SELECT substr(app_name,1,1025) AS app_name, '' AS title FROM import_time_buckets WHERE id = ?",
        };
        let row = sqlx::query(query)
            .bind(id)
            .fetch_one(&mut *connection)
            .await
            .map_err(query_error)?;
        let app: String = row.try_get("app_name").map_err(query_error)?;
        let title: String = row.try_get("title").map_err(query_error)?;
        if app.len() > MAX_APP_KEY_BYTES || title.len() > 16_384 {
            return Err("daily activity classification metadata exceeds budget".to_string());
        }
        records[index].value.included =
            activity_read_policy::should_include_fact(&exe, &app, &title);
    }
    Ok(records)
}

fn query_error(error: sqlx::Error) -> String {
    format!("failed to read daily activity: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{repositories::activity_read_model, schema};
    use sqlx::Executor;

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    async fn native(pool: &SqlitePool, app: &str, start: i64, end: Option<i64>) {
        sqlx::query("INSERT INTO sessions (app_name, exe_name, start_time, end_time, duration) VALUES ('App', ?, ?, ?, ?)")
            .bind(app).bind(start).bind(end).bind(end.map(|end| end - start))
            .execute(pool).await.unwrap();
    }

    #[test]
    fn trend_reuses_daily_policy_without_retaining_titles_and_has_stable_ties() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, "zeta", 0, Some(1000)).await;
            native(&pool, "alpha", 1000, Some(2000)).await;
            native(&pool, "steamwebhelper.exe", 2000, Some(4000)).await;
            native(&pool, "steam.exe", 4000, Some(5000)).await;
            sqlx::query("UPDATE sessions SET window_title = ?")
                .bind("private title".repeat(10_000))
                .execute(&pool)
                .await
                .unwrap();
            let snapshot =
                load_snapshot_with_apps(&pool, &[0, HOUR_MS, 2 * HOUR_MS], 2 * HOUR_MS, true)
                    .await
                    .unwrap();
            assert_eq!(snapshot.top_apps, vec![Some("steam.exe".into()), None]);
            assert_eq!(snapshot.activity.days[0].active_ms, 5000);
            pool.execute("INSERT INTO settings(key,value) VALUES ('__app_override::steam.exe','{\"track\":false}')").await.unwrap();
            let snapshot = load_snapshot_with_apps(&pool, &[0, HOUR_MS], HOUR_MS, true)
                .await
                .unwrap();
            assert_eq!(snapshot.top_apps, vec![Some("alpha".into())]);
            assert_eq!(snapshot.activity.days[0].active_ms, 2000);
            let totals = load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS).await.unwrap();
            assert_eq!(totals, snapshot.activity);
        });
    }

    #[test]
    fn daily_query_uses_covering_facts_and_a_bounded_bucket_seek() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            let explain = format!("EXPLAIN QUERY PLAN {DAY_FACTS_SQL}");
            let mut query = sqlx::query(&explain);
            for value in [
                HOUR_MS, HOUR_MS, HOUR_MS, HOUR_MS, 0, HOUR_MS, 0, HOUR_MS, HOUR_MS, -HOUR_MS,
                20001,
            ] {
                query = query.bind(value);
            }
            let rows = query.fetch_all(&pool).await.unwrap();
            let plan = rows
                .iter()
                .map(|row| row.get::<String, _>("detail"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                plan.contains("COVERING INDEX idx_sessions_exe_usage_time"),
                "{plan}"
            );
            assert!(
                plan.contains("COVERING INDEX idx_import_exact_sessions_exe_time"),
                "{plan}"
            );
            assert!(
                plan.contains("bucket_start_time>? AND bucket_start_time<?"),
                "{plan}"
            );
        });
    }

    #[test]
    fn daily_totals_match_existing_precedence_and_exclusion_semantics() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, " ZEN ", 1000, Some(2000)).await;
            native(&pool, "terminal", HOUR_MS - 1000, Some(HOUR_MS + 1000)).await;
            native(&pool, "live", 2 * HOUR_MS, None).await;
            pool.execute("INSERT INTO settings (key,value) VALUES ('__app_excluded::zen','true')")
                .await
                .unwrap();
            sqlx::query("INSERT INTO import_batches (id, imported_at, source_name, source_kind, source_fingerprint, exact_session_count, hour_bucket_count) VALUES ('b',1,'test','patina-csv',?,1,2)")
                .bind("a".repeat(64)).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO import_exact_sessions (batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration) VALUES ('b',?,'Import','import','private title',0,3000,3000)")
                .bind("e".repeat(64)).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO import_time_buckets (batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES ('b',?,'Bucket','bucket',0,3600000),('b',?,'Other','other',0,1800000)")
                .bind("b".repeat(64)).bind("c".repeat(64)).execute(&pool).await.unwrap();
            // An intentionally uneven boundary also exercises bucket proportional allocation.
            let boundaries = [0, HOUR_MS / 2, HOUR_MS, 3 * HOUR_MS, 4 * HOUR_MS];
            let sampled = 2 * HOUR_MS + 5000;
            let result = load_snapshot(&pool, &boundaries, sampled).await.unwrap();
            let reference = activity_read_model::load_snapshot(&pool, 0, 4 * HOUR_MS, sampled)
                .await
                .unwrap();
            let semantics = activity_read_model::load_app_semantics(&pool)
                .await
                .unwrap();
            for day in &result.days {
                let expected: i64 = reference
                    .contributions(day.start_ms, day.end_ms)
                    .into_iter()
                    .filter(|item| !semantics.is_excluded(&item.value.exe_name))
                    .map(|item| item.duration_ms)
                    .sum();
                assert_eq!(day.active_ms, expected);
            }
            assert_eq!(result.earliest_start_ms, Some(0));
            assert_eq!(result.sampled_at_ms, sampled);
            assert_eq!(result.days[2].active_ms, 6000);
            assert_eq!(result.days[3].active_ms, 0);
            let encoded = serde_json::to_string(&result).unwrap();
            assert!(!encoded.contains("private title"));
            assert!(!encoded.contains("terminal"));
        });
    }

    #[test]
    fn daily_totals_support_short_and_long_local_days_and_active_cutoff() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            let boundaries = [0, 23 * HOUR_MS, 48 * HOUR_MS, 72 * HOUR_MS];
            native(&pool, "live", 0, None).await;
            let result = load_snapshot(&pool, &boundaries, 50 * HOUR_MS)
                .await
                .unwrap();
            assert_eq!(
                result
                    .days
                    .iter()
                    .map(|day| day.active_ms)
                    .collect::<Vec<_>>(),
                vec![23 * HOUR_MS, 25 * HOUR_MS, 2 * HOUR_MS]
            );
        });
    }

    #[test]
    fn empty_snapshot_is_complete_and_read_only() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            let result = load_snapshot(&pool, &[0, HOUR_MS, 2 * HOUR_MS], HOUR_MS)
                .await
                .unwrap();
            assert_eq!(result.earliest_start_ms, None);
            assert_eq!(result.days.len(), 2);
            assert!(result.days.iter().all(|day| day.active_ms == 0));
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        });
    }

    #[test]
    fn rejects_invalid_ranges_overflow_and_excess_concurrency() {
        for boundaries in [
            vec![],
            vec![0],
            vec![1, 1],
            vec![1, 0],
            vec![i64::MIN, i64::MAX],
            vec![0, 49 * HOUR_MS],
            vec![0; 380],
        ] {
            assert!(validate_boundaries(&boundaries).is_err());
        }
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            let _permit = DAILY_ACTIVITY_QUERY.acquire().await.unwrap();
            assert!(load_daily_activity(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("busy"));
            assert!(load_daily_trend(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("busy"));
            drop(_permit);
            use chrono::TimeZone;
            struct FixedClock(i64);
            impl crate::engine::runtime_context::RuntimeClock for FixedClock {
                fn now_ms(&self) -> i64 {
                    self.0
                }
            }
            let now = chrono::Local
                .with_ymd_and_hms(2026, 1, 3, 0, 0, 0)
                .unwrap()
                .timestamp_millis();
            native(&pool, "fixture", now - 1000, None).await;
            let context = crate::engine::api::context::ApiRuntimeContext::new(
                crate::engine::runtime_context::RuntimeContext::new(
                    pool,
                    Arc::new(FixedClock(now)),
                ),
            );
            for surface in [
                crate::engine::api::surface::ApiSurface::Desktop,
                crate::engine::api::surface::ApiSurface::DaemonReadOnly,
                crate::engine::api::surface::ApiSurface::DaemonTracking,
            ] {
                let response = crate::engine::api::router::route_request(
                    crate::engine::api::router::ApiRequest {
                        method: "GET".into(),
                        path: "/api/v1/heatmap".into(),
                        query: Some("from=2026-01-01&to=2026-01-03".into()),
                        body: Vec::new(),
                    },
                    &context,
                    surface,
                )
                .await;
                assert_eq!(response.status, 200, "{:?}", response.body);
                assert_eq!(response.body["data"]["days"].as_array().unwrap().len(), 2);
                assert_eq!(response.body["data"]["days"][0]["active_ms"], 0);
                for (period, count) in [("week", 7), ("month", 30)] {
                    let response = crate::engine::api::router::route_request(
                        crate::engine::api::router::ApiRequest {
                            method: "GET".into(),
                            path: "/api/v1/trend".into(),
                            query: Some(format!("period={period}&granularity=day")),
                            body: Vec::new(),
                        },
                        &context,
                        surface,
                    )
                    .await;
                    assert_eq!(response.status, 200, "{:?}", response.body);
                    let points = response.body["data"]["data_points"].as_array().unwrap();
                    assert_eq!(points.len(), count);
                    assert_eq!(points[count - 1]["active_ms"], 0);
                    assert!(points[count - 1]["top_app"].is_null());
                    assert_eq!(points[count - 2]["active_ms"], 1000);
                    assert_eq!(points[count - 2]["top_app"], "fixture");
                }
            }
        });
    }

    #[test]
    fn current_overrides_and_aliases_take_priority_without_writing_settings() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, " STEAMWEBHELPER.EXE ", 0, Some(1000)).await;
            super::super::app_mappings::update_app_override(
                &pool,
                "steam.exe",
                super::super::app_mappings::AppOverrideUpdate::Excluded(true),
                1000,
            )
            .await
            .unwrap();
            assert_eq!(
                load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                    .await
                    .unwrap()
                    .days[0]
                    .active_ms,
                0
            );
            for value in [
                r#"{"track":true}"#,
                r#"{"track":false,"enabled":false}"#,
                r#"{"track":"false"}"#,
            ] {
                sqlx::query(
                    "UPDATE settings SET value = ? WHERE key = '__app_override::steam.exe'",
                )
                .bind(value)
                .execute(&pool)
                .await
                .unwrap();
                pool.execute("INSERT OR REPLACE INTO settings (key,value) VALUES ('__app_excluded::steamwebhelper.exe','true')").await.unwrap();
                assert_eq!(
                    load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                        .await
                        .unwrap()
                        .days[0]
                        .active_ms,
                    1000,
                    "{value}"
                );
                let stored: String = sqlx::query_scalar(
                    "SELECT value FROM settings WHERE key = '__app_override::steam.exe'",
                )
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(stored, value);
            }
            pool.execute("DELETE FROM settings WHERE key = '__app_override::steam.exe'")
                .await
                .unwrap();
            assert_eq!(
                load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                    .await
                    .unwrap()
                    .days[0]
                    .active_ms,
                0
            );
            pool.execute(
                "INSERT INTO settings (key,value) VALUES ('__app_override::steam.exe','{bad')",
            )
            .await
            .unwrap();
            assert!(load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("invalid JSON"));
            pool.execute("UPDATE settings SET value = '{\"track\":false}' WHERE key = '__app_override::steam.exe'").await.unwrap();
            pool.execute("INSERT INTO settings (key,value) VALUES ('__app_override::steamwebhelper.exe','{\"track\":true}')").await.unwrap();
            assert!(load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("conflicting overrides"));
        });
    }

    #[test]
    fn excluded_native_masks_imports_and_cross_boundary_buckets_remain_proportional() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, "steamwebhelper.exe", 0, Some(1000)).await;
            pool.execute("INSERT INTO settings (key,value) VALUES ('__app_override::steam.exe','{\"track\":false}')").await.unwrap();
            sqlx::query("INSERT INTO import_batches (id, imported_at, source_name, source_kind, source_fingerprint, exact_session_count, hour_bucket_count) VALUES ('b',1,'test','patina-csv',?,1,1)")
                .bind("a".repeat(64)).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO import_exact_sessions (batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration) VALUES ('b',?,'Import','import','',0,2000,2000)")
                .bind("e".repeat(64)).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO import_time_buckets (batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES ('b',?,'Bucket','bucket',?,?)")
                .bind("b".repeat(64)).bind(HOUR_MS / 2).bind(HOUR_MS / 2).execute(&pool).await.unwrap();
            let snapshot = load_snapshot(&pool, &[0, HOUR_MS, 2 * HOUR_MS], 2 * HOUR_MS)
                .await
                .unwrap();
            assert_eq!(
                snapshot
                    .days
                    .iter()
                    .map(|day| day.active_ms)
                    .collect::<Vec<_>>(),
                vec![1000 + HOUR_MS / 4, HOUR_MS / 4]
            );
            let mut connection = pool.acquire().await.unwrap();
            sqlx::query("UPDATE settings SET value = ? WHERE key = '__app_override::steam.exe'")
                .bind("x".repeat(MAX_OVERRIDE_BYTES + 1))
                .execute(&mut *connection)
                .await
                .unwrap();
            assert!(load_excluded_apps(&mut connection)
                .await
                .unwrap_err()
                .contains("setting exceeds budget"));
        });
    }

    #[test]
    fn historical_filter_reads_only_sensitive_metadata_with_a_budget() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, "zen", 0, Some(1000)).await;
            native(&pool, "app-1.0-x64.exe", 1000, Some(2000)).await;
            native(&pool, "lockapp.exe", 2000, Some(3000)).await;
            sqlx::query("UPDATE sessions SET window_title = ? WHERE exe_name = 'zen'")
                .bind("x".repeat(20_000))
                .execute(&pool)
                .await
                .unwrap();
            pool.execute("UPDATE sessions SET window_title = 'Installing' WHERE exe_name = 'app-1.0-x64.exe'").await.unwrap();
            let snapshot = load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS).await.unwrap();
            assert_eq!(snapshot.days[0].active_ms, 1000);
            pool.execute(
                "UPDATE sessions SET window_title = 'Document' WHERE exe_name = 'app-1.0-x64.exe'",
            )
            .await
            .unwrap();
            assert_eq!(
                load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                    .await
                    .unwrap()
                    .days[0]
                    .active_ms,
                2000
            );
            sqlx::query("UPDATE sessions SET window_title = ? WHERE exe_name = 'app-1.0-x64.exe'")
                .bind("x".repeat(16_385))
                .execute(&pool)
                .await
                .unwrap();
            assert!(load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("metadata exceeds budget"));
        });
    }

    #[test]
    fn per_day_budget_rejects_instead_of_returning_truncated_totals() {
        tauri::async_runtime::block_on(async {
            let pool = setup().await;
            native(&pool, "app", 0, Some(1000)).await;
            native(&pool, "app", 1000, Some(2000)).await;
            let mut connection = pool.acquire().await.unwrap();
            assert!(load_day_facts(
                &mut connection,
                0,
                HOUR_MS,
                HOUR_MS,
                &HashSet::new(),
                1,
                false
            )
            .await
            .unwrap_err()
            .contains("per-day budget"));
            assert_eq!(
                load_day_facts(
                    &mut connection,
                    0,
                    HOUR_MS,
                    HOUR_MS,
                    &HashSet::new(),
                    2,
                    false
                )
                .await
                .unwrap()
                .len(),
                2
            );
            drop(connection);
            native(&pool, &"x".repeat(MAX_APP_KEY_BYTES + 1), 2000, Some(3000)).await;
            assert!(load_snapshot(&pool, &[0, HOUR_MS], HOUR_MS)
                .await
                .unwrap_err()
                .contains("app key"));
        });
    }
}
