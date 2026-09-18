//! Opt-in synthetic backup worker. No production profile or runtime discovery.
use super::*;
use crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

fn write_new(path: &Path, value: serde_json::Value) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.sync_all().unwrap();
}

#[test]
#[ignore = "synthetic backup memory evidence; use scripts/perf/backup-benchmark.mjs"]
fn worker() {
    let root =
        PathBuf::from(std::env::var_os("PATINA_BACKUP_BENCH_ROOT").expect("use benchmark runner"));
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-backup-bench-"));
    assert_eq!(root.canonicalize().unwrap(), root);
    assert_eq!(
        fs::read_to_string(root.join("marker")).unwrap(),
        "backup-benchmark\n"
    );
    let mode = std::env::var("PATINA_BACKUP_BENCH_MODE").unwrap();
    assert!([
        "seed",
        "legacy-export",
        "export",
        "legacy-preview",
        "preview",
        "preview-only",
        "sha256",
        "responsiveness",
        "restore-replace",
        "restore-merge",
        "rollback-replace",
        "rollback-merge"
    ]
    .contains(&mode.as_str()));
    tauri::async_runtime::block_on(async {
        let database = root.join("fixture.db");
        if mode == "seed" {
            assert!(!database.exists());
            let pool = open_prepared_sqlite_pool_at_path(&database, true)
                .await
                .unwrap();
            sqlx::query("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<49999) INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) SELECT 'Fixture','fixture',?,i*60000,i*60000+30000,30000 FROM n")
                .bind("Synthetic fixture title ".repeat(45)).execute(&pool).await.unwrap();
            export_backup_from_pool(&pool, &root.join("fixture.zip"))
                .await
                .unwrap();
            pool.close().await;
            return;
        }
        if mode == "responsiveness" {
            measure_responsiveness(&root).await;
            return;
        }
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .read_only(true),
            )
            .await
            .unwrap();
        let restore_pool = if mode.starts_with("restore-") || mode.starts_with("rollback-") {
            let target = open_prepared_sqlite_pool_at_path(&root.join(format!("{mode}.db")), true)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES ('Baseline','baseline','keep',-2000,-1000,1000)")
                .execute(&target).await.unwrap();
            sqlx::query("INSERT INTO settings(key,value) VALUES ('benchmark_baseline','keep')")
                .execute(&target)
                .await
                .unwrap();
            if mode.starts_with("rollback-") {
                sqlx::query("CREATE TRIGGER fail_last_session BEFORE INSERT ON sessions WHEN NEW.exe_name='fixture' AND NEW.start_time=2999940000 BEGIN SELECT RAISE(ABORT, 'benchmark injected late failure'); END")
                    .execute(&target).await.unwrap();
            }
            Some(target)
        } else {
            None
        };
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_samples = samples.clone();
        let sampler = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                thread_samples
                    .lock()
                    .unwrap()
                    .push(crate::platform::linux::resource::current_process_resource_snapshot());
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let baseline = crate::platform::linux::resource::current_process_resource_snapshot();
        let started = Instant::now();
        let target = root.join(format!("{mode}.zip"));
        let count = match mode.as_str() {
            mode if mode.starts_with("restore-") || mode.starts_with("rollback-") => {
                let target = restore_pool.as_ref().unwrap();
                let strategy = if mode.ends_with("merge") {
                    RestoreStrategy::Merge
                } else {
                    RestoreStrategy::Replace
                };
                let result = restore_backup_from_path(
                    target,
                    &root.join("fixture.zip"),
                    strategy,
                    4_000_000_000,
                )
                .await;
                if mode.starts_with("rollback-") {
                    assert!(result
                        .unwrap_err()
                        .contains("benchmark injected late failure"));
                } else {
                    result.unwrap();
                }
                50_000
            }
            "legacy-export" => {
                let payload = load_backup_payload_from_pool(&pool).await.unwrap();
                let bytes = encode_backup_archive(&payload).unwrap();
                write_backup_archive_atomic(&target, &bytes).unwrap();
                std::hint::black_box((&payload, &bytes));
                payload.sessions.len()
            }
            "export" => {
                export_backup_from_pool(&pool, &target).await.unwrap();
                50_000
            }
            "legacy-preview" => {
                // Full decoded payload baseline, already benefits from the disk ZIP reader.
                let payload = read_backup_payload(&root.join("fixture.zip")).unwrap();
                std::hint::black_box(&payload);
                payload.preview().session_count
            }
            "preview-only" => {
                let path = root.join("fixture.zip");
                preview::decode(&mut open_backup_archive(&path).unwrap(), &path)
                    .unwrap()
                    .session_count
            }
            "sha256" => {
                let path = root.join("fixture.zip");
                let (_, bytes) = fingerprint_backup_file(File::open(&path).unwrap()).unwrap();
                assert_eq!(bytes, fs::metadata(path).unwrap().len());
                0
            }
            _ => {
                inspect_restore_archive(&root.join("fixture.zip"))
                    .unwrap()
                    .0
                    .session_count
            }
        };
        let elapsed_ms = started.elapsed().as_millis();
        std::thread::sleep(Duration::from_millis(50));
        stop.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        if let Some(target) = restore_pool {
            let expected = if mode.starts_with("rollback-") {
                1
            } else if mode.ends_with("merge") {
                50_001
            } else {
                50_000
            };
            let actual: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
                .fetch_one(&target)
                .await
                .unwrap();
            assert_eq!(actual, expected);
            if mode.starts_with("restore-") {
                let total: i64 = sqlx::query_scalar(
                    "SELECT SUM(duration) FROM sessions WHERE exe_name='fixture'",
                )
                .fetch_one(&target)
                .await
                .unwrap();
                assert_eq!(total, 1_500_000_000);
            }
            let baseline_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE exe_name='baseline' AND window_title='keep' AND start_time=-2000 AND end_time=-1000 AND duration=1000").fetch_one(&target).await.unwrap();
            assert_eq!(
                baseline_count,
                if mode == "restore-replace" { 0 } else { 1 }
            );
            if mode.starts_with("rollback-") || mode.ends_with("merge") {
                let setting: String =
                    sqlx::query_scalar("SELECT value FROM settings WHERE key='benchmark_baseline'")
                        .fetch_one(&target)
                        .await
                        .unwrap();
                assert_eq!(setting, "keep");
            }
            let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
                .fetch_one(&target)
                .await
                .unwrap();
            assert_eq!(integrity, "ok");
            target.close().await;
        }
        assert_eq!(count, if mode == "sha256" { 0 } else { 50_000 });
        if mode.ends_with("export") {
            assert_eq!(
                inspect_restore_archive(&target).unwrap().0.session_count,
                count
            );
        }
        let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
        write_new(
            &root.join(format!("{mode}.json")),
            json!({
                "mode": mode, "baseline": baseline, "samples": *samples.lock().unwrap(),
                "elapsed_ms": elapsed_ms, "records": count, "integrity": integrity,
            }),
        );
        pool.close().await;
    });
}

async fn measure_responsiveness(root: &Path) {
    use crate::data::repositories::tracker_settings::{
        TRACKER_LAST_HEARTBEAT_KEY, TRACKER_LAST_SUCCESSFUL_SAMPLE_KEY,
    };
    use crate::data::tracking_runtime::TrackingRuntimeDataStore;
    let pool = open_prepared_sqlite_pool_at_path(&root.join("live.db"), true)
        .await
        .unwrap();
    let data = TrackingRuntimeDataStore::new(pool.clone());
    let started_at = now_ms_i64();
    data.start_session("Fixture", "fixture", "start", started_at, started_at)
        .await
        .unwrap();
    let archive = root.join("fixture.zip").to_string_lossy().into_owned();
    let requests = async {
        let results = tokio::join!(
            preview_backup(archive.clone()),
            preview_backup(archive.clone()),
            preview_backup(archive)
        );
        for result in [results.0, results.1, results.2] {
            assert_eq!(result.unwrap().session_count, 50_000);
        }
    };
    tokio::pin!(requests);
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let started = Instant::now();
    let mut last = Instant::now();
    let mut max_gap_ms = 0;
    let mut ticks = 0;
    loop {
        tokio::select! {
            () = &mut requests => break,
            _ = interval.tick() => {
                max_gap_ms = max_gap_ms.max(last.elapsed().as_millis());
                last = Instant::now();
                ticks += 1;
                let timestamp = now_ms_i64();
                data.save_tracker_timestamp(TRACKER_LAST_HEARTBEAT_KEY, timestamp).await.unwrap();
                data.save_tracker_timestamp(TRACKER_LAST_SUCCESSFUL_SAMPLE_KEY, timestamp).await.unwrap();
                data.refresh_active_session_metadata("fixture", &format!("tick {ticks}"), timestamp).await.unwrap();
            }
        }
    }
    max_gap_ms = max_gap_ms.max(last.elapsed().as_millis());
    let elapsed_ms = started.elapsed().as_millis();
    data.end_active_sessions(now_ms_i64()).await.unwrap();
    let duration: i64 = sqlx::query_scalar("SELECT duration FROM sessions LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let samples: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_title_samples")
        .fetch_one(&pool)
        .await
        .unwrap();
    let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    let passed = ticks >= 10
        && max_gap_ms <= 250
        && duration > 0
        && samples == ticks + 1
        && integrity == "ok";
    write_new(
        &root.join("responsiveness.json"),
        json!({
            "scope": "Three asynchronous previews plus real tracking datastore writes; no GNOME provider, daemon service or GUI",
            "elapsed_ms": elapsed_ms, "ticks": ticks, "max_tick_gap_ms": max_gap_ms,
            "session_duration_ms": duration, "title_samples": samples, "integrity": integrity, "passed": passed,
        }),
    );
    pool.close().await;
    assert!(passed, "see responsiveness.json");
}
