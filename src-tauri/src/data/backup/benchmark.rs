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
    let multi_table = std::env::var("PATINA_BACKUP_BENCH_MULTI").as_deref() == Ok("1");
    assert!([
        "seed",
        "legacy-export",
        "export",
        "legacy-preview",
        "preview",
        "preview-only",
        "sha256",
        "responsiveness",
        "write-failure",
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
            if multi_table {
                seed_related_tables(&pool).await;
            }
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
        if mode == "write-failure" {
            verify_export_write_failure(&pool, &root).await;
            pool.close().await;
            return;
        }
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
                let trigger = if multi_table {
                    "CREATE TRIGGER fail_last_import BEFORE INSERT ON import_time_buckets WHEN NEW.bucket_start_time=599940000 BEGIN SELECT RAISE(ABORT, 'benchmark injected late failure'); END"
                } else {
                    "CREATE TRIGGER fail_last_session BEFORE INSERT ON sessions WHEN NEW.exe_name='fixture' AND NEW.start_time=2999940000 BEGIN SELECT RAISE(ABORT, 'benchmark injected late failure'); END"
                };
                sqlx::query(trigger).execute(&target).await.unwrap();
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
            if multi_table {
                verify_related_tables(&target, mode.starts_with("restore-")).await;
            }
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
                "multi_table": multi_table,
            }),
        );
        pool.close().await;
    });
}

async fn seed_related_tables(pool: &Pool<Sqlite>) {
    for statement in [
        "INSERT INTO session_title_samples(session_id,title,start_time,end_time) SELECT id,window_title,start_time,end_time FROM sessions",
        "INSERT INTO web_activity_segments(id,browser_client_id,browser_kind,browser_exe_name,domain,normalized_domain,url,title,start_time,end_time,duration,created_at,updated_at) SELECT id,'synthetic','firefox','fixture','example.test','example.test','https://example.test/'||id,window_title,start_time,end_time,duration,start_time,end_time FROM sessions",
        "INSERT INTO web_activity_native_sessions(segment_id,session_id) SELECT id,id FROM sessions",
        "INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES ('fixture',1,'synthetic','patina-csv',printf('%064d',1),10000,10000)",
        "INSERT INTO import_exact_sessions(batch_id,fingerprint,app_name,exe_name,window_title,start_time,end_time,duration) SELECT 'fixture',printf('%064d',id),'Imported','imported',window_title,start_time,end_time,duration FROM sessions WHERE id<=10000",
        "INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) SELECT 'fixture',printf('%064d',id),'Imported','imported',start_time,1000 FROM sessions WHERE id<=10000",
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<1000) INSERT INTO settings(key,value) SELECT 'fixture_'||i,'synthetic' FROM n",
    ] {
        sqlx::query(statement).execute(pool).await.unwrap();
    }
}

async fn verify_export_write_failure(pool: &Pool<Sqlite>, root: &Path) {
    let existing = root.join("write-failure-existing.zip");
    let unrelated = root.join("write-failure-unrelated.txt");
    fs::write(&existing, b"preserve existing destination").unwrap();
    fs::write(&unrelated, b"not owned by exporter").unwrap();
    let before: std::collections::BTreeSet<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    for create_new in [false, true] {
        let target = if create_new {
            root.join("write-failure-new.zip")
        } else {
            existing.clone()
        };
        // This opt-in test runs in its own process. Inject a real kernel file-write
        // error without filling a shared filesystem or changing production limits.
        let limit = FileSizeLimit::new(4096);
        let result = streaming::export(pool, &target, create_new).await;
        drop(limit);
        let error = result.unwrap_err();
        assert!(
            matches!(error, CreateNewBackupError::Failed(ref message) if message.contains("File too large") || message.contains("os error 27")),
            "{error:?}"
        );
        assert_eq!(
            fs::read(&existing).unwrap(),
            b"preserve existing destination"
        );
        assert_eq!(fs::read(&unrelated).unwrap(), b"not owned by exporter");
        let after: std::collections::BTreeSet<_> = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            before, after,
            "failed export must remove only its staged file"
        );
    }
    write_new(
        &root.join("write-failure.json"),
        json!({
            "passed": true, "failure": "kernel EFBIG via isolated RLIMIT_FSIZE",
            "existing_target_and_unrelated_preserved": true, "new_target_absent": true,
            "staged_files_removed": true,
        }),
    );
}

struct FileSizeLimit {
    previous: libc::rlimit,
    signal: libc::sighandler_t,
}

impl FileSizeLimit {
    fn new(bytes: libc::rlim_t) -> Self {
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid stack pointers, current isolated worker process only.
        unsafe {
            assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut previous), 0);
            let signal = libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            assert_ne!(signal, libc::SIG_ERR);
            let guard = Self { previous, signal };
            assert_eq!(
                libc::setrlimit(
                    libc::RLIMIT_FSIZE,
                    &libc::rlimit {
                        rlim_cur: bytes.min(previous.rlim_max),
                        rlim_max: previous.rlim_max,
                    }
                ),
                0
            );
            guard
        }
    }
}

impl Drop for FileSizeLimit {
    fn drop(&mut self) {
        // SAFETY: restore the limits and signal handler saved by this process.
        unsafe {
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &self.previous), 0);
            libc::signal(libc::SIGXFSZ, self.signal);
        }
    }
}

async fn verify_related_tables(pool: &Pool<Sqlite>, restored: bool) {
    for (table, count) in [
        ("session_title_samples", 50_000),
        ("web_activity_segments", 50_000),
        ("web_activity_native_sessions", 50_000),
        ("import_batches", 1),
        ("import_exact_sessions", 10_000),
        ("import_time_buckets", 10_000),
    ] {
        let actual: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(actual, if restored { count } else { 0 }, "{table}");
    }
    let settings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM settings WHERE key GLOB 'fixture_*' AND value='synthetic'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(settings, if restored { 1000 } else { 0 });
    if restored {
        let linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM web_activity_native_sessions r JOIN sessions s ON s.id=r.session_id JOIN web_activity_segments w ON w.id=r.segment_id WHERE s.exe_name='fixture' AND s.start_time=w.start_time AND s.end_time=w.end_time AND w.duration=s.duration")
            .fetch_one(pool).await.unwrap();
        assert_eq!(linked, 50_000);
        let titles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_title_samples t JOIN sessions s ON s.id=t.session_id WHERE t.title=TRIM(s.window_title) AND t.start_time=s.start_time AND t.end_time=s.end_time")
            .fetch_one(pool).await.unwrap();
        assert_eq!(titles, 50_000);
    }
    let foreign_keys = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .unwrap();
    assert!(foreign_keys.is_empty());
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
