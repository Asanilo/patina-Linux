use crate::domain::tools::{
    PomodoroPhase, PomodoroStatus, ReminderStatus, TimerMode, TimerStatus, ToolPomodoroRun,
    ToolReminder, ToolRuntimeSettings, ToolSoftwareReminderRule, ToolTimer, ToolTimerLap,
    ToolsRuntimeSnapshot,
};
use sqlx::{Pool, Sqlite};

mod state;

mod backup_restore;

pub use backup_restore::{clear_for_restore, insert_for_restore, insert_missing_for_restore};
#[cfg(test)]
pub use backup_restore::{
    fetch_all_daily_stats_for_backup, fetch_all_pomodoro_runs_for_backup,
    fetch_all_reminders_for_backup, fetch_all_timer_laps_for_backup, fetch_all_timers_for_backup,
};

const RECENT_REMINDER_LIMIT: i64 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedTimerNotification {
    pub timer_id: i64,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedPomodoroNotification {
    pub run_id: i64,
    pub completed_phase: PomodoroPhase,
    pub next_phase: PomodoroPhase,
    pub completed_focus_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftwareReminderNotification {
    pub rule_id: i64,
    pub app_name: String,
    pub limit_ms: i64,
    pub usage_ms: i64,
    pub message: String,
}

// State decisions and writes share a writer transaction, including early reads.
// A cancelled/error future drops the transaction rather than leaving half a reset.
macro_rules! write_operation {
    ($name:ident($($arg:ident: $ty:ty),* $(,)?) -> $result:ty) => {
        pub async fn $name(pool: &Pool<Sqlite>, $($arg: $ty),*) -> Result<$result, String> {
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await
                .map_err(|error| format!("failed to begin Tools write: {error}"))?;
            let result = state::$name(&mut tx, $($arg),*).await?;
            tx.commit().await.map_err(|error| format!("failed to commit Tools write: {error}"))?;
            Ok(result)
        }
    };
}

write_operation!(create_reminder(
    label: &str,
    scheduled_at: i64,
    now_ms: i64,
) -> ToolReminder);

write_operation!(cancel_reminder(
    reminder_id: i64,
    now_ms: i64,
) -> ());

write_operation!(create_software_reminder_rule(
    app_name: &str,
    exe_name: Option<&str>,
    limit_ms: i64,
    message: &str,
    now_ms: i64,
) -> ToolSoftwareReminderRule);

write_operation!(disable_software_reminder_rule(
    rule_id: i64,
    now_ms: i64,
) -> ());

write_operation!(fire_due_reminders(now_ms: i64) -> Vec<ToolReminder>);

write_operation!(fire_due_software_reminders(
    date_key: &str,
    day_start_ms: i64,
    now_ms: i64,
) -> Vec<SoftwareReminderNotification>);

write_operation!(start_timer(
    mode: TimerMode,
    duration_ms: Option<i64>,
    label: Option<&str>,
    now_ms: i64,
) -> ToolTimer);

write_operation!(pause_timer(now_ms: i64) -> ());

write_operation!(resume_timer(now_ms: i64) -> ());

write_operation!(reset_timer(now_ms: i64) -> ());

write_operation!(add_timer_lap(now_ms: i64) -> Option<ToolTimerLap>);

write_operation!(complete_due_countdown(now_ms: i64) -> Option<CompletedTimerNotification>);

write_operation!(pause_running_stopwatch_after_restart(now_ms: i64) -> bool);

write_operation!(start_pomodoro(
    focus_ms: i64,
    short_break_ms: i64,
    long_break_ms: i64,
    long_break_every: i64,
    now_ms: i64,
) -> ToolPomodoroRun);

write_operation!(pause_pomodoro(now_ms: i64) -> ());

write_operation!(resume_pomodoro(now_ms: i64) -> ());

write_operation!(skip_pomodoro_phase(
    date_key: &str,
    now_ms: i64,
) -> Option<CompletedPomodoroNotification>);

write_operation!(complete_due_pomodoro_phase(
    date_key: &str,
    now_ms: i64,
) -> Option<CompletedPomodoroNotification>);

write_operation!(reset_pomodoro(now_ms: i64) -> ());

/// All fields describe one database snapshot, even while another connection writes.
pub async fn fetch_tools_snapshot(
    pool: &Pool<Sqlite>,
    now_ms: i64,
    date_key: &str,
) -> Result<ToolsRuntimeSnapshot, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to begin Tools snapshot: {error}"))?;
    let snapshot = state::fetch_tools_snapshot(&mut tx, now_ms, date_key).await?;
    tx.commit()
        .await
        .map_err(|error| format!("failed to finish Tools snapshot: {error}"))?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema as db_schema;
    use sqlx::{Executor, SqlitePool};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::TOOLS_TABLES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    #[test]
    fn created_reminder_can_be_read_in_snapshot() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;

            create_reminder(&pool, "'; DROP TABLE tool_reminders; --", 2_000, 1_000)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 1_000, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.reminders.len(), 1);
            assert_eq!(
                snapshot.reminders[0].label,
                "'; DROP TABLE tool_reminders; --"
            );
            assert_eq!(snapshot.next_reminder_at, Some(2_000));
        });
    }

    #[test]
    fn due_reminder_fires_only_once() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            create_reminder(&pool, "Stand up", 1_000, 900)
                .await
                .unwrap();

            let first = fire_due_reminders(&pool, 1_100).await.unwrap();
            let second = fire_due_reminders(&pool, 1_200).await.unwrap();

            assert_eq!(first.len(), 1);
            assert_eq!(first[0].status, ReminderStatus::Fired);
            assert!(second.is_empty());
        });
    }

    #[test]
    fn software_reminder_counts_today_usage_and_active_session_once() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            sqlx::query(
                "INSERT INTO sessions (
                    app_name, exe_name, window_title, start_time, end_time, duration,
                    continuity_group_start_time
                 ) VALUES (?, ?, ?, ?, ?, ?, ?), (?, ?, ?, ?, NULL, NULL, ?)",
            )
            .bind("Editor")
            .bind("editor.exe")
            .bind("Doc")
            .bind(0_i64)
            .bind(40_000_i64)
            .bind(40_000_i64)
            .bind(0_i64)
            .bind("Editor")
            .bind("editor.exe")
            .bind("Doc")
            .bind(40_000_i64)
            .bind(40_000_i64)
            .execute(&pool)
            .await
            .unwrap();
            create_software_reminder_rule(
                &pool,
                "Editor",
                Some("editor.exe"),
                60_000,
                "Take a break",
                900,
            )
            .await
            .unwrap();

            let first = fire_due_software_reminders(&pool, "2026-06-07", 0, 70_000)
                .await
                .unwrap();
            let second = fire_due_software_reminders(&pool, "2026-06-07", 0, 71_000)
                .await
                .unwrap();

            assert_eq!(first.len(), 1);
            assert_eq!(first[0].usage_ms, 70_000);
            assert_eq!(first[0].message, "Take a break");
            assert!(second.is_empty());
        });
    }

    #[test]
    fn timer_laps_are_committed_in_order() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();

            add_timer_lap(&pool, 1_500).await.unwrap();
            add_timer_lap(&pool, 2_000).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.timer_laps.len(), 2);
            assert_eq!(snapshot.timer_laps[0].duration_ms, 500);
            assert_eq!(
                snapshot.timer_laps[1].started_at,
                snapshot.timer_laps[0].ended_at
            );
        });
    }

    #[test]
    fn countdown_completion_updates_current_timer_once() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Countdown, Some(1_000), None, 1_000)
                .await
                .unwrap();

            let completed = complete_due_countdown(&pool, 2_100).await.unwrap();
            let second = complete_due_countdown(&pool, 2_200).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_200, "2026-06-07")
                .await
                .unwrap();

            assert!(completed.is_some());
            assert!(second.is_none());
            assert_eq!(
                snapshot.current_timer.unwrap().status,
                TimerStatus::Completed
            );
        });
    }

    #[test]
    fn pausing_running_timer_sets_paused_status() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();

            pause_timer(&pool, 1_500).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 3_000, "2026-06-07")
                .await
                .unwrap();
            let timer = snapshot.current_timer.unwrap();

            assert_eq!(timer.status, TimerStatus::Paused);
            assert_eq!(timer.elapsed_ms_at(3_000), 500);
        });
    }

    #[test]
    fn reset_timer_clears_current_timer_from_snapshot() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();
            add_timer_lap(&pool, 1_500).await.unwrap();

            reset_timer(&pool, 2_000).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();

            assert!(snapshot.current_timer.is_none());
            assert!(snapshot.timer_laps.is_empty());
        });
    }

    #[test]
    fn pomodoro_focus_completion_updates_daily_stats() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            let completed = complete_due_pomodoro_phase(&pool, "2026-06-07", 2_100)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_100, "2026-06-07")
                .await
                .unwrap();

            assert!(completed.is_some());
            assert_eq!(snapshot.today_completed_pomodoros, 1);
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::ShortBreak);
            assert_eq!(run.status, PomodoroStatus::Running);
            assert_eq!(run.phase_started_at, Some(2_100));
            assert_eq!(run.phase_paused_at, None);
        });
    }

    #[test]
    fn pause_then_resume_pomodoro_restarts_current_phase() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            pause_pomodoro(&pool, 1_400).await.unwrap();
            resume_pomodoro(&pool, 2_000).await.unwrap();

            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::Focus);
            assert_eq!(run.status, PomodoroStatus::Running);
            assert_eq!(run.phase_started_at, Some(2_000));
            assert_eq!(run.phase_paused_at, None);
            assert_eq!(run.phase_remaining_ms, Some(600));
        });
    }

    #[test]
    fn skip_pomodoro_phase_pauses_next_phase() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            skip_pomodoro_phase(&pool, "2026-06-07", 1_500)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 1_500, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.today_completed_pomodoros, 0);
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::ShortBreak);
            assert_eq!(run.status, PomodoroStatus::Paused);
            assert_eq!(run.phase_started_at, None);
            assert_eq!(run.phase_paused_at, Some(1_500));
        });
    }

    #[test]
    fn backup_restore_round_trips_tool_tables() {
        crate::engine::runtime_context::test_block_on(async {
            let pool = setup_test_db().await;
            create_reminder(&pool, "Check", 2_000, 1_000).await.unwrap();
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();
            add_timer_lap(&pool, 1_500).await.unwrap();
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();
            complete_due_pomodoro_phase(&pool, "2026-06-07", 2_100)
                .await
                .unwrap();

            let reminders = fetch_all_reminders_for_backup(&pool).await.unwrap();
            let timers = fetch_all_timers_for_backup(&pool).await.unwrap();
            let laps = fetch_all_timer_laps_for_backup(&pool).await.unwrap();
            let pomodoros = fetch_all_pomodoro_runs_for_backup(&pool).await.unwrap();
            let stats = fetch_all_daily_stats_for_backup(&pool).await.unwrap();

            let mut tx = pool.begin().await.unwrap();
            clear_for_restore(&mut tx).await.unwrap();
            insert_for_restore(&mut tx, &reminders, &timers, &laps, &pomodoros, &stats)
                .await
                .unwrap();
            tx.commit().await.unwrap();

            assert_eq!(
                fetch_all_reminders_for_backup(&pool).await.unwrap().len(),
                1
            );
            assert_eq!(fetch_all_timers_for_backup(&pool).await.unwrap().len(), 1);
            assert_eq!(
                fetch_all_timer_laps_for_backup(&pool).await.unwrap().len(),
                1
            );
            assert_eq!(
                fetch_all_pomodoro_runs_for_backup(&pool)
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                fetch_all_daily_stats_for_backup(&pool).await.unwrap().len(),
                1
            );
        });
    }
}

#[cfg(test)]
mod concurrency_tests;
