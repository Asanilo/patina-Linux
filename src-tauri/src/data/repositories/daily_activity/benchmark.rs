//! Opt-in query-worker benchmark. This is not a Desktop/WebKit memory measurement.
use super::*;
use crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

const ROWS: i64 = 50_000;
const DAY: i64 = 24 * HOUR_MS;
const START: i64 = 1_735_689_600_000; // 2025-01-01 UTC; fixture uses fixed UTC days.

fn root() -> PathBuf {
    let root =
        PathBuf::from(std::env::var_os("PATINA_DAILY_BENCH_ROOT").expect("use benchmark runner"));
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-daily-bench-"));
    assert_eq!(root.canonicalize().unwrap(), root);
    assert_eq!(
        std::fs::read_to_string(root.join("marker")).unwrap(),
        "daily-query-benchmark\n"
    );
    root
}

fn write_new(path: &Path, value: serde_json::Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&value).unwrap().as_bytes())
        .unwrap();
    file.sync_all().unwrap();
}

#[test]
#[ignore = "isolated performance evidence; use scripts/perf/daily-activity-benchmark.mjs"]
fn query_worker() {
    let root = root();
    let mode = std::env::var("PATINA_DAILY_BENCH_MODE").unwrap();
    assert!(["seed", "legacy", "daily", "observed-legacy", "observed"].contains(&mode.as_str()));
    tauri::async_runtime::block_on(async {
        let database = root.join("fixture.db");
        if mode == "seed" {
            assert!(!database.exists());
            let pool = open_prepared_sqlite_pool_at_path(&database, true)
                .await
                .unwrap();
            // 1 KiB synthetic titles recreate transfer volume without copying private data.
            sqlx::query("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i < ?) INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) SELECT 'Fixture App', 'fixture-app', ?, ? + i * 600000, ? + i * 600000 + 60000, 60000 FROM n")
                .bind(ROWS - 1).bind("Synthetic fixture title ".repeat(45)).bind(START).bind(START)
                .execute(&pool).await.unwrap();
            pool.close().await;
            write_new(
                &root.join("fixture.json"),
                json!({"rows": ROWS, "start_ms": START, "days": 365, "expected_ms": ROWS * 60000}),
            );
            return;
        }
        assert!(std::fs::symlink_metadata(&database).unwrap().is_file());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .read_only(true),
            )
            .await
            .unwrap();
        let boundaries: Vec<i64> = (0..=365).map(|day| START + day * DAY).collect();
        // Extract the actual legacy SQL so benchmark drift is detected instead of guessed.
        let source =
            include_str!("../../../../../src/platform/persistence/sessionReadRepository.ts");
        let function = source
            .split("export async function getSessionSummariesInRange")
            .nth(1)
            .unwrap();
        let legacy_sql = function.split('`').nth(1).unwrap();
        let legacy_values = [
            boundaries[365],
            boundaries[365],
            boundaries[365],
            boundaries[365],
            START,
            boundaries[365],
            START,
            boundaries[365],
            START,
        ];
        let mut plans = Vec::new();
        let from = boundaries[182];
        let to = boundaries[183];
        for (name, sql, values) in [
            ("legacy-full", legacy_sql, legacy_values.to_vec()),
            (
                "daily-union",
                DAY_FACTS_SQL,
                vec![
                    to,
                    to,
                    to,
                    to,
                    from,
                    to,
                    from,
                    HOUR_MS,
                    to,
                    from - HOUR_MS,
                    20001,
                ],
            ),
        ] {
            let explain = format!("EXPLAIN QUERY PLAN {sql}");
            let mut query = sqlx::query(&explain);
            for value in values {
                query = query.bind(value);
            }
            let rows = query.fetch_all(&pool).await.unwrap();
            plans.push(json!({"query": name, "details": rows.iter().map(|row| row.get::<String,_>("detail")).collect::<Vec<_>>()}));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let sampler_stop = stop.clone();
        let sampler_samples = samples.clone();
        let sampler = std::thread::spawn(move || {
            let started = Instant::now();
            while !sampler_stop.load(Ordering::Relaxed) {
                let memory = crate::platform::linux::resource::current_process_resource_snapshot();
                sampler_samples
                    .lock()
                    .unwrap()
                    .push(json!({"elapsed_ms": started.elapsed().as_millis(), "memory": memory}));
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let baseline = crate::platform::linux::resource::current_process_resource_snapshot();
        let started = Instant::now();
        let output: Result<(usize, i64), String> = if mode == "observed" {
            let stats = crate::data::repositories::observed_apps::load_observed_apps(
                &pool,
                START,
                boundaries[365],
                boundaries[365],
            )
            .await
            .unwrap();
            let total = stats.iter().map(|stat| stat.total_duration_ms).sum();
            let encoded = serde_json::to_vec(&json!({"data": stats})).unwrap();
            Ok((encoded.len(), total))
        } else if mode == "observed-legacy" {
            let source = include_str!(
                "../../../../../src/platform/persistence/classificationPersistence.ts"
            );
            let sql = source
                .split("export async function loadObservedSessionStats")
                .nth(1)
                .unwrap()
                .split('`')
                .nth(1)
                .unwrap();
            let mut query = sqlx::query(sql);
            for value in legacy_values {
                query = query.bind(value);
            }
            let rows = query.fetch_all(&pool).await.unwrap();
            let values: Vec<_> = rows.iter().map(|row| json!({
                "id": row.get::<i64,_>("id"), "origin": row.get::<String,_>("origin"),
                "exe_name": row.get::<String,_>("exe_name"), "app_name": row.get::<String,_>("app_name"),
                "start_time": row.get::<i64,_>("start_time"), "end_time": row.get::<i64,_>("end_time"),
                "capacity_end_time": row.get::<i64,_>("capacity_end_time"),
            })).collect();
            let encoded = serde_json::to_vec(&values).unwrap();
            let total = rows
                .iter()
                .map(|row| row.get::<i64, _>("end_time") - row.get::<i64, _>("start_time"))
                .sum();
            std::hint::black_box((&values, &encoded));
            Ok((encoded.len(), total))
        } else if mode == "daily" {
            match load_daily_activity(&pool, &boundaries, boundaries[365]).await {
                Ok(snapshot) => {
                    let total = snapshot.days.iter().map(|day| day.active_ms).sum();
                    let encoded = serde_json::to_vec(&json!({"data": snapshot})).unwrap();
                    Ok((encoded.len(), total))
                }
                Err(error) => Err(error),
            }
        } else {
            let mut query = sqlx::query(legacy_sql);
            for value in legacy_values {
                query = query.bind(value);
            }
            let rows = query.fetch_all(&pool).await.unwrap();
            assert_eq!(rows.len(), ROWS as usize);
            // Logical SQL/JSON baseline, not a claim to reproduce plugin allocator internals.
            let values: Vec<_> = rows.iter().map(|row| json!({
                "record_id": row.get::<i64,_>("record_id"), "origin": row.get::<String,_>("origin"),
                "app_name": row.get::<String,_>("app_name"), "exe_name": row.get::<String,_>("exe_name"),
                "window_title": row.get::<String,_>("window_title"), "start_time": row.get::<i64,_>("start_time"),
                "effective_end_time": row.get::<i64,_>("effective_end_time"), "capacity_end_time": row.get::<i64,_>("capacity_end_time"),
                "is_live": row.get::<i64,_>("is_live"),
            })).collect();
            let encoded = serde_json::to_vec(&values).unwrap();
            let total = rows
                .iter()
                .map(|row| {
                    row.get::<i64, _>("effective_end_time") - row.get::<i64, _>("start_time")
                })
                .sum();
            std::hint::black_box((&values, &encoded));
            Ok((encoded.len(), total))
        };
        let elapsed_ms = started.elapsed().as_millis();
        std::thread::sleep(Duration::from_millis(100));
        stop.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        let after = crate::platform::linux::resource::current_process_resource_snapshot();
        let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, ROWS);
        write_new(
            &root.join(format!("{mode}.json")),
            json!({
                "mode": mode, "scope": "query worker only; no UI, IPC, HTTP, tracker or daemon host",
                "baseline": baseline, "after": after, "elapsed_ms": elapsed_ms,
                "result": output, "samples": *samples.lock().unwrap(), "plans": plans,
                "fixture_rows": count, "integrity": integrity,
            }),
        );
        pool.close().await;
        assert_eq!(output.unwrap().1, ROWS * 60000);
    });
}
