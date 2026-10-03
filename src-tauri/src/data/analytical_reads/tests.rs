use super::*;
use sqlx::Executor;

const EXPENSIVE: &str = "WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<10000000) SELECT sum(x) FROM n";

async fn writer() -> (std::path::PathBuf, SqlitePool) {
    let root = std::env::temp_dir().join(format!(
        "patina-analytical-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pool =
        crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(&root.join("patina.db"), true)
            .await
            .unwrap();
    (root, pool)
}

#[tokio::test]
async fn isolated_read_snapshot_does_not_block_writer_and_cannot_mutate_database() {
    let (root, writer) = writer().await;
    // Negative control reproduces the previous one-connection bottleneck.
    let held = writer.acquire().await.unwrap();
    assert!(tokio::time::timeout(
        Duration::from_millis(30),
        sqlx::query("INSERT INTO settings VALUES('probe','value')").execute(&writer)
    )
    .await
    .is_err());
    drop(held);
    let reads = AnalyticalReads::open(&writer).await.unwrap();
    let read = reads.try_read().unwrap();
    let mut snapshot = read.pool().begin().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sessions")
            .fetch_one(&mut *snapshot)
            .await
            .unwrap(),
        0
    );
    tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query(
            "INSERT INTO sessions(app_name,exe_name,start_time) VALUES('Fixture','fixture',1000)",
        )
        .execute(&writer),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sessions")
            .fetch_one(&mut *snapshot)
            .await
            .unwrap(),
        0
    );
    snapshot.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sessions")
            .fetch_one(read.pool())
            .await
            .unwrap(),
        1
    );
    assert!(
        sqlx::query("INSERT INTO settings VALUES('forbidden','write')")
            .execute(read.pool())
            .await
            .is_err()
    );
    assert!(sqlx::query("CREATE TEMP TABLE forbidden(id INTEGER)")
        .execute(read.pool())
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&writer)
            .await
            .unwrap(),
        "ok"
    );
    drop(read);
    reads.close().await;
    writer.close().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn capacity_is_shared_by_clones_and_close_does_not_close_the_writer() {
    let (root, writer) = writer().await;
    let reads = AnalyticalReads::open(&writer).await.unwrap();
    let clone = reads.clone();
    let first = reads.try_read().unwrap();
    let second = clone.try_read().unwrap();
    assert!(reads.try_read().is_err());
    drop(first);
    let replacement = clone.try_read().unwrap();
    drop(second);
    drop(replacement);
    reads.close().await;
    assert!(clone.try_read().is_err());
    writer
        .execute("INSERT INTO settings VALUES('still-writable','1')")
        .await
        .unwrap();
    writer.close().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_vm_deadline_survives_caller_cancellation_and_the_connection_recovers() {
    let (root, writer) = writer().await;
    let reads = AnalyticalReads::open_with_deadline(&writer, Duration::from_millis(100))
        .await
        .unwrap();
    // Occupy the other connection so recovery cannot silently use a spare.
    let other = reads.pool.acquire().await.unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query_scalar::<_, i64>(EXPENSIVE).fetch_one(&reads.pool),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("interrupted"), "{error}");
    let pool = reads.pool.clone();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connection = pool.acquire().await.unwrap();
        entered.send(()).unwrap();
        sqlx::query_scalar::<_, i64>(EXPENSIVE)
            .fetch_one(&mut *connection)
            .await
    });
    ready.await.unwrap();
    task.abort();
    let _ = task.await;
    let value=tokio::time::timeout(Duration::from_secs(2),sqlx::query_scalar::<_,i64>(
        "WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<1000) SELECT sum(x) FROM n"
    ).fetch_one(&reads.pool)).await.unwrap().unwrap();
    assert_eq!(value, 500500);
    drop(other);
    reads.close().await;
    writer.close().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn shutdown_interrupts_an_active_native_query() {
    let (root, writer) = writer().await;
    let reads = AnalyticalReads::open(&writer).await.unwrap();
    let pool = reads.pool.clone();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let query = tokio::spawn(async move {
        let mut connection = pool.acquire().await.unwrap();
        entered.send(()).unwrap();
        sqlx::query_scalar::<_, i64>(EXPENSIVE)
            .fetch_one(&mut *connection)
            .await
    });
    ready.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), reads.close())
        .await
        .unwrap();
    let error = query.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("interrupted"), "{error}");
    writer.close().await;
    std::fs::remove_dir_all(root).unwrap();
}
