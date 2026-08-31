use crate::domain::activity_read_model::{
    summarize_activity_range, ActivityContribution, ActivityOrigin, OwnedActivityRange, HOUR_MS,
};
use sqlx::{Pool, Row, Sqlite};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityFact {
    pub record_id: i64,
    pub app_name: String,
    pub exe_name: String,
    pub window_title: String,
    pub source_category: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedAppFact {
    pub exe_name: String,
    pub app_name: String,
}

pub struct ActivityReadSnapshot {
    records: Vec<OwnedActivityRange<ActivityFact>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityAppSemantics {
    pub(crate) categories: HashMap<String, String>,
    pub(crate) excluded: HashSet<String>,
}

impl ActivityAppSemantics {
    pub fn category_for(&self, exe_name: &str) -> Option<&str> {
        self.categories
            .get(&normalize_app_key(exe_name))
            .map(String::as_str)
    }

    pub fn is_excluded(&self, exe_name: &str) -> bool {
        self.excluded.contains(&normalize_app_key(exe_name))
    }
}

impl ActivityReadSnapshot {
    pub fn contributions(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Vec<ActivityContribution<ActivityFact>> {
        summarize_activity_range(&self.records, from_ms, to_ms)
    }
}

pub async fn load_snapshot(
    pool: &Pool<Sqlite>,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
) -> Result<ActivityReadSnapshot, String> {
    if to_ms <= from_ms {
        return Err("activity range end must be after start".to_string());
    }
    let active_end_ms = sampled_at_ms.min(to_ms);
    let rows = sqlx::query(
        "SELECT id AS record_id, 'native' AS origin, app_name, exe_name,
                COALESCE(window_title, '') AS window_title, start_time,
                COALESCE(end_time, ?) AS effective_end_time,
                COALESCE(end_time, ?) AS capacity_end_time,
                NULL AS source_category
         FROM sessions
         WHERE start_time < ? AND COALESCE(end_time, ?) > ?
         UNION ALL
         SELECT id, 'import_exact', app_name, exe_name, window_title, start_time,
                end_time, end_time, source_category
         FROM import_exact_sessions
         WHERE start_time < ? AND end_time > ?
         UNION ALL
         SELECT id, 'import_bucket', app_name, exe_name, '', bucket_start_time,
                bucket_start_time + duration, bucket_start_time + ?, source_category
         FROM import_time_buckets
         WHERE bucket_start_time < ? AND bucket_start_time + ? > ?
         ORDER BY start_time ASC, origin ASC, record_id ASC",
    )
    .bind(active_end_ms)
    .bind(active_end_ms)
    .bind(to_ms)
    .bind(active_end_ms)
    .bind(from_ms)
    .bind(to_ms)
    .bind(from_ms)
    .bind(HOUR_MS)
    .bind(to_ms)
    .bind(HOUR_MS)
    .bind(from_ms)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load activity read snapshot: {error}"))?;

    let records = rows
        .into_iter()
        .filter_map(|row| {
            let origin = match row.get::<String, _>("origin").as_str() {
                "native" => ActivityOrigin::Native,
                "import_exact" => ActivityOrigin::ImportExact,
                "import_bucket" => ActivityOrigin::ImportBucket,
                _ => return None,
            };
            let start_ms = row.get::<i64, _>("start_time");
            let end_ms = row.get::<i64, _>("effective_end_time");
            let capacity_end_ms = row.get::<i64, _>("capacity_end_time");
            Some(OwnedActivityRange {
                origin,
                start_ms,
                end_ms,
                capacity_end_ms: Some(capacity_end_ms),
                value: ActivityFact {
                    record_id: row.get("record_id"),
                    app_name: row.get("app_name"),
                    exe_name: row.get("exe_name"),
                    window_title: row.get("window_title"),
                    source_category: row.get("source_category"),
                },
            })
        })
        .collect();
    Ok(ActivityReadSnapshot { records })
}

pub async fn load_recorded_apps(pool: &Pool<Sqlite>) -> Result<Vec<RecordedAppFact>, String> {
    let rows = sqlx::query(
        "WITH facts AS (
           SELECT exe_name, app_name, start_time AS seen_at, 0 AS origin_rank, id
           FROM sessions WHERE TRIM(exe_name) <> ''
           UNION ALL
           SELECT exe_name, app_name, start_time, 1, id
           FROM import_exact_sessions WHERE TRIM(exe_name) <> ''
           UNION ALL
           SELECT exe_name, app_name, bucket_start_time, 2, id
           FROM import_time_buckets WHERE TRIM(exe_name) <> ''
         ), keys AS (
           SELECT LOWER(TRIM(exe_name)) AS app_key FROM facts GROUP BY LOWER(TRIM(exe_name))
         )
         SELECT
           (SELECT exe_name FROM facts candidate
            WHERE LOWER(TRIM(candidate.exe_name)) = keys.app_key
            ORDER BY origin_rank ASC, seen_at DESC, id DESC LIMIT 1) AS exe_name,
           COALESCE((SELECT TRIM(app_name) FROM facts candidate
            WHERE LOWER(TRIM(candidate.exe_name)) = keys.app_key
              AND TRIM(COALESCE(app_name, '')) <> ''
            ORDER BY origin_rank ASC, seen_at DESC, id DESC LIMIT 1), '') AS app_name
         FROM keys ORDER BY app_key",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load recorded app catalog: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| RecordedAppFact {
            exe_name: row.get("exe_name"),
            app_name: row.get("app_name"),
        })
        .collect())
}

pub async fn load_app_semantics(pool: &Pool<Sqlite>) -> Result<ActivityAppSemantics, String> {
    let rows = sqlx::query(
        "SELECT key, value FROM settings
         WHERE key LIKE '__app_category::%' OR key LIKE '__app_excluded::%'",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load activity app semantics: {error}"))?;
    let mut semantics = ActivityAppSemantics::default();
    for row in rows {
        let key: String = row.get("key");
        let value: String = row.get("value");
        if let Some(exe_name) = key.strip_prefix("__app_category::") {
            semantics
                .categories
                .insert(normalize_app_key(exe_name), value);
        } else if let Some(exe_name) = key.strip_prefix("__app_excluded::") {
            if matches!(value.as_str(), "1" | "true") {
                semantics.excluded.insert(normalize_app_key(exe_name));
            }
        }
    }
    Ok(semantics)
}

fn normalize_app_key(exe_name: &str) -> String {
    exe_name.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema;
    use sqlx::{Executor, SqlitePool};

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    async fn create_import_batch(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO import_batches (
               id, imported_at, source_name, source_kind, source_fingerprint,
               exact_session_count, hour_bucket_count
             ) VALUES ('batch', 1, 'test.csv', 'patina-csv', ?, 1, 1)",
        )
        .bind("a".repeat(64))
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn snapshot_applies_native_exact_bucket_precedence() {
        tauri::async_runtime::block_on(async {
            let pool = setup_pool().await;
            create_import_batch(&pool).await;
            sqlx::query(
                "INSERT INTO sessions (app_name, exe_name, start_time, end_time, duration)
                 VALUES ('Native', 'native', 1000, 2000, 1000)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO import_exact_sessions (
                   batch_id, fingerprint, app_name, exe_name, window_title,
                   start_time, end_time, duration, source_category
                 ) VALUES ('batch', ?, 'Exact', 'exact', '', 0, 3000, 3000, 'Imported')",
            )
            .bind("b".repeat(64))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO import_time_buckets (
                   batch_id, fingerprint, app_name, exe_name, bucket_start_time,
                   duration, source_category
                 ) VALUES ('batch', ?, 'Bucket', 'bucket', 0, 3600000, 'Imported')",
            )
            .bind("c".repeat(64))
            .execute(&pool)
            .await
            .unwrap();

            let snapshot = load_snapshot(&pool, 0, HOUR_MS, HOUR_MS).await.unwrap();
            let contributions = snapshot.contributions(0, HOUR_MS);
            assert_eq!(
                contributions
                    .iter()
                    .map(|item| item.duration_ms)
                    .sum::<i64>(),
                HOUR_MS
            );
            assert_eq!(
                contributions
                    .iter()
                    .find(|item| item.value.exe_name == "native")
                    .map(|item| item.duration_ms),
                Some(1000)
            );
            assert_eq!(
                contributions
                    .iter()
                    .filter(|item| item.value.exe_name == "exact")
                    .map(|item| item.duration_ms)
                    .sum::<i64>(),
                2000
            );
        });
    }

    #[test]
    fn recorded_apps_merge_case_and_prefer_native_identity() {
        tauri::async_runtime::block_on(async {
            let pool = setup_pool().await;
            create_import_batch(&pool).await;
            sqlx::query(
                "INSERT INTO sessions (app_name, exe_name, start_time, end_time, duration)
                 VALUES ('Zen Browser', 'zen', 10, 20, 10)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO import_exact_sessions (
                   batch_id, fingerprint, app_name, exe_name, window_title,
                   start_time, end_time, duration
                 ) VALUES ('batch', ?, 'Imported Zen', 'ZEN', '', 30, 40, 10)",
            )
            .bind("d".repeat(64))
            .execute(&pool)
            .await
            .unwrap();

            let apps = load_recorded_apps(&pool).await.unwrap();
            assert_eq!(apps.len(), 1);
            assert_eq!(apps[0].exe_name, "zen");
            assert_eq!(apps[0].app_name, "Zen Browser");
        });
    }

    #[test]
    fn active_native_fact_stops_at_sample_time() {
        tauri::async_runtime::block_on(async {
            let pool = setup_pool().await;
            sqlx::query(
                "INSERT INTO sessions (app_name, exe_name, start_time, end_time, duration)
                 VALUES ('Ghostty', 'ghostty', 2000, NULL, NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let snapshot = load_snapshot(&pool, 1000, 5000, 3000).await.unwrap();
            let contributions = snapshot.contributions(1000, 5000);
            assert_eq!(contributions.len(), 1);
            assert_eq!(contributions[0].duration_ms, 1000);
        });
    }

    #[test]
    fn app_semantics_merge_case_for_category_and_exclusion() {
        tauri::async_runtime::block_on(async {
            let pool = setup_pool().await;
            sqlx::query(
                "INSERT INTO settings (key, value) VALUES
                 ('__app_category::ZEN', 'Browser'),
                 ('__app_excluded::Ghostty', 'true')",
            )
            .execute(&pool)
            .await
            .unwrap();

            let semantics = load_app_semantics(&pool).await.unwrap();
            assert_eq!(semantics.category_for("zen"), Some("Browser"));
            assert!(semantics.is_excluded("ghostty"));
        });
    }
}
