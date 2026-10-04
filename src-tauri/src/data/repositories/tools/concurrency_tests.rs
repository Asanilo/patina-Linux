use super::*;
use sqlx::{Executor, SqlitePool};
use std::sync::Arc;

#[tokio::test]
async fn snapshot_fields_remain_on_one_read_transaction_during_reset() {
    let path = std::env::temp_dir().join(format!(
        "patina-tools-snapshot-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal),
        )
        .await
        .unwrap();
    for schema in [
        crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL,
        crate::data::schema::TOOLS_TABLES_SCHEMA_SQL,
        crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    start_timer(&pool, TimerMode::Stopwatch, None, None, 1000)
        .await
        .unwrap();
    add_timer_lap(&pool, 2000).await.unwrap();
    let before = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    let mut read = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM tool_timers")
        .fetch_all(&mut *read)
        .await
        .unwrap();
    reset_timer(&pool, 2000).await.unwrap();
    assert_eq!(
        state::fetch_tools_snapshot(&mut read, 2000, "2026-10-04")
            .await
            .unwrap(),
        before
    );
    read.commit().await.unwrap();
    let after = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    assert!(after.current_timer.is_none());
    assert!(after.timer_laps.is_empty());
    pool.close().await;
    std::fs::remove_file(path).unwrap();
}

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::TOOLS_TABLES_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn concurrent_laps_have_unique_contiguous_indices() {
    let pool = pool().await;
    start_timer(&pool, TimerMode::Stopwatch, None, None, 1000)
        .await
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            add_timer_lap(&pool, 2000).await.unwrap().unwrap()
        }));
    }
    let mut indices = Vec::new();
    for task in tasks {
        indices.push(task.await.unwrap().lap_index);
    }
    indices.sort();
    assert_eq!(indices, (1..=16).collect::<Vec<_>>());
    let snapshot = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    assert_eq!(
        snapshot
            .timer_laps
            .iter()
            .map(|lap| lap.duration_ms)
            .sum::<i64>(),
        1000
    );
}

#[tokio::test]
async fn concurrent_due_checks_count_one_focus_and_advance_one_phase() {
    let pool = pool().await;
    start_pomodoro(&pool, 1000, 5000, 10000, 4, 1000)
        .await
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            complete_due_pomodoro_phase(&pool, "2026-10-04", 2000)
                .await
                .unwrap()
        }));
    }
    let mut completions = 0;
    for task in tasks {
        completions += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(completions, 1);
    let snapshot = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    assert_eq!(snapshot.today_completed_pomodoros, 1);
    let run = snapshot.current_pomodoro.unwrap();
    assert_eq!(run.phase, PomodoroPhase::ShortBreak);
    assert_eq!(run.completed_focus_count, 1);
}

#[tokio::test]
async fn failed_lap_deletion_rolls_back_timer_reset() {
    let pool = pool().await;
    start_timer(&pool, TimerMode::Stopwatch, None, None, 1000)
        .await
        .unwrap();
    add_timer_lap(&pool, 2000).await.unwrap();
    let before = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    pool.execute("CREATE TRIGGER reject_lap_delete BEFORE DELETE ON tool_timer_laps BEGIN SELECT RAISE(ABORT, 'fixture deletion failure'); END").await.unwrap();
    assert!(reset_timer(&pool, 2000).await.is_err());
    assert_eq!(
        before,
        fetch_tools_snapshot(&pool, 2000, "2026-10-04")
            .await
            .unwrap()
    );
    pool.execute("DROP TRIGGER reject_lap_delete")
        .await
        .unwrap();
    reset_timer(&pool, 2000).await.unwrap();
    let after = fetch_tools_snapshot(&pool, 2000, "2026-10-04")
        .await
        .unwrap();
    assert!(after.current_timer.is_none());
    assert!(after.timer_laps.is_empty());
}
