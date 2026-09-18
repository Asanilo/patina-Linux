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
        "preview"
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
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&database)
                    .read_only(true),
            )
            .await
            .unwrap();
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
        assert_eq!(count, 50_000);
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
