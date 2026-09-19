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
    assert!([
        "seed",
        "legacy",
        "daily",
        "observed-legacy",
        "observed",
        "migration",
        "trend",
        "trend-legacy",
        "apps",
        "apps-named",
        "apps-legacy"
    ]
    .contains(&mode.as_str()));
    let trend = std::env::var("PATINA_DAILY_BENCH_TREND").as_deref() == Ok("1");
    let day_count = if trend { 30 } else { 365 };
    let row_duration = if trend { 10_000 } else { 60_000 };
    tauri::async_runtime::block_on(async {
        let database = root.join("fixture.db");
        if mode == "seed" {
            assert!(!database.exists());
            let pool = open_prepared_sqlite_pool_at_path(&database, true)
                .await
                .unwrap();
            // 1 KiB synthetic titles recreate transfer volume without copying private data.
            sqlx::query("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i < ?) INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) SELECT 'Fixture App', 'fixture-app', ?, ? + i * ?, ? + i * ? + ?, ? FROM n")
                .bind(ROWS - 1).bind("Synthetic fixture title ".repeat(45)).bind(START).bind(if trend {50_000} else {600_000}).bind(START).bind(if trend {50_000} else {600_000}).bind(row_duration).bind(row_duration)
                .execute(&pool).await.unwrap();
            pool.close().await;
            write_new(
                &root.join("fixture.json"),
                json!({"rows": ROWS, "start_ms": START, "days": day_count, "expected_ms": ROWS * row_duration}),
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
        let boundaries: Vec<i64> = (0..=day_count).map(|day| START + day * DAY).collect();
        let end = *boundaries.last().unwrap();
        // Extract the actual legacy SQL so benchmark drift is detected instead of guessed.
        let source =
            include_str!("../../../../../src/platform/persistence/sessionReadRepository.ts");
        let function = source
            .split("export async function getSessionSummariesInRange")
            .nth(1)
            .unwrap();
        let legacy_sql = function.split('`').nth(1).unwrap();
        let legacy_values = [end, end, end, end, START, end, START, end, START];
        let mut plans = Vec::new();
        let from = boundaries[day_count as usize / 2];
        let to = boundaries[day_count as usize / 2 + 1];
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
        let output: Result<(usize, i64), String> = if mode.starts_with("apps") {
            let mut applications = None;
            let days = if mode == "apps-named" {
                let snapshot = load_daily_apps_named(&pool, &boundaries, end)
                    .await
                    .unwrap();
                applications = snapshot.applications;
                assert_eq!(applications.as_ref().unwrap()[0].app_name, "Fixture App");
                snapshot.days
            } else if mode == "apps" {
                load_daily_apps(&pool, &boundaries, end).await.unwrap().days
            } else {
                let snapshot = crate::data::repositories::activity_read_model::load_snapshot(
                    &pool, START, end, end,
                )
                .await
                .unwrap();
                boundaries
                    .windows(2)
                    .map(|day| {
                        let mut totals = std::collections::BTreeMap::<String, i64>::new();
                        for item in snapshot.contributions(day[0], day[1]) {
                            if item.duration_ms > 0 {
                                *totals
                                    .entry(activity_read_policy::canonical_executable(
                                        &item.value.exe_name,
                                    ))
                                    .or_default() += item.duration_ms;
                            }
                        }
                        DailyAppActivityDay {
                            start_ms: day[0],
                            end_ms: day[1],
                            active_ms: totals.values().sum(),
                            apps: totals
                                .into_iter()
                                .map(|(app_key, active_ms)| DailyAppTotal { app_key, active_ms })
                                .collect(),
                        }
                    })
                    .collect()
            };
            let total = days.iter().map(|day| day.active_ms).sum();
            write_new(&root.join(format!("{mode}-days.json")), json!(&days));
            Ok((
                serde_json::to_vec(
                    &json!({"data": DailyAppActivitySnapshot { sampled_at_ms: end, days, applications }}),
                )
                .unwrap()
                .len(),
                total,
            ))
        } else if mode == "trend" || mode == "trend-legacy" {
            let mut days = Vec::new();
            if mode == "trend" {
                let snapshot = load_daily_trend(&pool, &boundaries, end).await.unwrap();
                for (day, top) in snapshot.activity.days.iter().zip(snapshot.top_apps) {
                    days.push((day.start_ms, day.active_ms, top));
                }
            } else {
                let snapshot = crate::data::repositories::activity_read_model::load_snapshot(
                    &pool, START, end, end,
                )
                .await
                .unwrap();
                for day in boundaries.windows(2) {
                    let mut totals = HashMap::<String, i64>::new();
                    for item in snapshot.contributions(day[0], day[1]) {
                        *totals
                            .entry(item.value.exe_name.trim().to_ascii_lowercase())
                            .or_default() += item.duration_ms;
                    }
                    let top = totals
                        .iter()
                        .max_by(|(a, x), (b, y)| x.cmp(y).then_with(|| b.cmp(a)))
                        .map(|(key, _)| key.clone());
                    days.push((day[0], totals.values().sum(), top));
                }
            }
            for (_, total, top) in &days {
                assert_eq!(
                    top.as_deref(),
                    if *total > 0 {
                        Some("fixture-app")
                    } else {
                        None
                    }
                );
            }
            let total = days.iter().map(|(_, total, _)| total).sum();
            write_new(&root.join(format!("{mode}-days.json")), json!(&days));
            Ok((serde_json::to_vec(&days).unwrap().len(), total))
        } else if mode == "observed" || mode == "migration" {
            let stats = if mode == "migration" {
                crate::data::repositories::observed_apps::load_migration_observed_apps(
                    &pool, end, end,
                )
                .await
                .unwrap()
            } else {
                crate::data::repositories::observed_apps::load_observed_apps(&pool, START, end, end)
                    .await
                    .unwrap()
            };
            let total = stats.iter().map(|stat| stat.total_duration_ms).sum();
            let encoded = serde_json::to_vec(&json!({"data": stats})).unwrap();
            Ok((encoded.len(), total))
        } else if mode == "observed-legacy" {
            // Frozen reference: the unbounded reader no longer ships in the UI.
            let sql = include_str!("../../../../../tests/fixtures/legacy-observed-apps.sql");
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
            match load_daily_activity(&pool, &boundaries, end).await {
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
        assert_eq!(output.unwrap().1, ROWS * row_duration);
    });
}
